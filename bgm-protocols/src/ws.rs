//! [`WsProtocol`] — a WebSocket load strategy.
//!
//! Same Strategy/Factory/Template-Method shape as [`crate::HttpProtocol`], so
//! the engine drives it identically. The load model is *message round-trips on
//! a persistent connection*: `setup` opens one WebSocket per worker, each
//! `execute` sends a message and (optionally) awaits one reply — measuring the
//! message RTT — and `teardown` closes it. A connection that errors mid-run is
//! transparently re-opened so a transient drop doesn't doom the worker.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use bgm_core::{BgmError, IterError, IterInfo, IterReport, IterResult, Protocol, Result, Status};
use bgm_scenario::{Assertions, Scenario};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::template::RequestTemplate;

/// The connected stream a worker owns for its lifetime.
type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Tunables for the WebSocket strategy.
#[derive(Debug, Clone)]
pub struct WsConfig {
    /// Per-iteration deadline for the send + receive round-trip.
    pub timeout: Duration,
    /// Whether to wait for a reply after sending (off = fire-and-forget).
    pub expect_reply: bool,
}

impl Default for WsConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            expect_reply: true,
        }
    }
}

/// A WebSocket strategy: send the scenario's (templated) body as a message each
/// iteration, optionally awaiting a reply.
#[derive(Debug)]
pub struct WsProtocol {
    /// Resolved connection URL (`ws://` / `wss://`); static across iterations.
    url: String,
    /// Carries the per-iteration message body (supports `{{ seq }}` etc.).
    template: RequestTemplate,
    asserts: Assertions,
    config: WsConfig,
}

impl WsProtocol {
    /// Build from a scenario with the default config.
    pub fn from_scenario(scenario: &Scenario) -> Result<Self> {
        Self::from_scenario_with(scenario, WsConfig::default())
    }

    /// Build from a scenario with explicit config (Factory). Validates the URL
    /// scheme up front so a misconfigured run fails fast.
    pub fn from_scenario_with(scenario: &Scenario, config: WsConfig) -> Result<Self> {
        let resolved = scenario.resolved();
        let url = resolved.url.clone();
        if !(url.starts_with("ws://") || url.starts_with("wss://")) {
            return Err(BgmError::Config(format!(
                "websocket url must start with ws:// or wss://, got {url:?}"
            )));
        }
        Ok(Self {
            url,
            template: RequestTemplate::new(resolved),
            asserts: scenario.asserts.clone(),
            config,
        })
    }

    /// (Re)open a connection to the target.
    async fn connect(&self) -> std::result::Result<WsStream, String> {
        connect_async(&self.url)
            .await
            .map(|(stream, _response)| stream)
            .map_err(|e| e.to_string())
    }
}

#[async_trait]
impl Protocol for WsProtocol {
    type Worker = WsStream;

    async fn setup(&self, _worker_id: usize) -> Result<Self::Worker> {
        self.connect()
            .await
            .map_err(|e| BgmError::Setup(format!("websocket connect: {e}")))
    }

    async fn execute(&self, worker: &mut Self::Worker, info: &IterInfo) -> IterResult<IterReport> {
        let spec = self.template.render(info);
        let message = spec.body.as_deref().unwrap_or("").to_owned();

        let started = Instant::now();
        let outcome = round_trip(
            worker,
            message,
            self.config.expect_reply,
            self.config.timeout,
        )
        .await;

        let reply = match outcome {
            Ok(reply) => reply,
            Err(err) => {
                // Re-open for the next iteration; this one still counts as failed.
                if let Ok(fresh) = self.connect().await {
                    *worker = fresh;
                }
                return Err(err);
            }
        };
        let duration = started.elapsed();

        if !self.asserts.is_empty() {
            self.asserts
                .check_body_latency(duration, &reply)
                .map_err(IterError::Assertion)?;
        }

        Ok(IterReport {
            duration,
            status: Status::success(0),
            bytes: reply.len() as u64,
            items: 1,
        })
    }

    async fn teardown(&self, mut worker: Self::Worker) -> Result<()> {
        // Best-effort close handshake; ignore errors on the way out.
        let _ = worker.send(Message::Close(None)).await;
        Ok(())
    }
}

/// Send one message and (optionally) wait for the next data frame, all under a
/// single deadline. Returns the reply bytes (empty when not expecting one).
async fn round_trip(
    worker: &mut WsStream,
    message: String,
    expect_reply: bool,
    deadline: Duration,
) -> IterResult<Vec<u8>> {
    let work = async {
        worker
            .send(Message::Text(message.into()))
            .await
            .map_err(|e| IterError::Connection(e.to_string()))?;

        if !expect_reply {
            return Ok(Vec::new());
        }
        // Skip control frames (ping/pong) until a text/binary message arrives.
        loop {
            match worker.next().await {
                Some(Ok(Message::Text(text))) => return Ok(text.as_bytes().to_vec()),
                Some(Ok(Message::Binary(data))) => return Ok(data.to_vec()),
                Some(Ok(Message::Close(_))) | None => {
                    return Err(IterError::Connection("connection closed".to_owned()));
                }
                Some(Ok(_)) => {} // ping/pong/frame — keep reading
                Some(Err(e)) => return Err(IterError::Connection(e.to_string())),
            }
        }
    };

    match timeout(deadline, work).await {
        Ok(result) => result,
        Err(_elapsed) => Err(IterError::Timeout),
    }
}
