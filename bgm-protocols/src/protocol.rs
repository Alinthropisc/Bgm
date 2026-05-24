//! [`HttpProtocol`] — the HTTP implementation of [`bgm_core::Protocol`].
//!
//! - **Strategy**: the engine knows only the `Protocol` trait; this is one
//!   concrete strategy (gRPC/TCP/SQL could be others).
//! - **Factory**: [`HttpProtocol::from_scenario`] builds a ready protocol from a
//!   [`Scenario`], validating the static request(s) up front.
//! - **Template Method**: `setup` creates one [`reqwest::Client`] per worker
//!   (its own connection pool), `execute` runs an iteration, `teardown` is a
//!   no-op (the client drops itself).
//!
//! An iteration is a **flow**: the scenario's steps run in order, threading a
//! variable context — each step may [`Extract`] response values that later steps
//! interpolate (`{{ var }}`). The whole flow is reported as one [`IterReport`]
//! (summed latency/bytes, `items` = step count); the first failing step aborts
//! it.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use bgm_core::{
    BgmError, Interpolate, IterError, IterInfo, IterReport, IterResult, MapContext, Protocol,
    Result, Status,
};
use bgm_scenario::{Assertions, Extract, ExtractFrom, HttpRequestSpec, Scenario};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::redirect::Policy;
use reqwest::{Client, Method};
use serde_json::Value;

use crate::config::HttpClientConfig;
use crate::template::spec_has_placeholder;

/// A prepared step: request with static vars already baked in, plus its checks
/// and extractions.
#[derive(Debug)]
struct StepPlan {
    spec: HttpRequestSpec,
    asserts: Assertions,
    extract: Vec<Extract>,
}

/// An HTTP load strategy: run the scenario's flow once per iteration.
#[derive(Debug)]
pub struct HttpProtocol {
    steps: Vec<StepPlan>,
    config: HttpClientConfig,
}

impl HttpProtocol {
    /// Build from a scenario with the default client config.
    pub fn from_scenario(scenario: &Scenario) -> Result<Self> {
        Self::from_scenario_with(scenario, HttpClientConfig::default())
    }

    /// Build from a scenario with explicit client config (Factory).
    ///
    /// Static requests are validated now (bad method/URL → [`BgmError::Config`])
    /// so the run fails fast. Steps with dynamic placeholders can only be checked
    /// once expanded, so their validation is deferred to the first iteration.
    pub fn from_scenario_with(scenario: &Scenario, config: HttpClientConfig) -> Result<Self> {
        // Bake scenario-level vars in once; what survives are dynamic
        // ({{ seq }}…) or extracted ({{ token }}…) placeholders resolved later.
        let mut static_ctx = MapContext::new();
        for (key, value) in &scenario.vars {
            static_ctx.set(key, value);
        }

        let mut steps = Vec::new();
        for step in scenario.steps() {
            let spec = step.request.interpolate(&static_ctx);
            if !spec_has_placeholder(&spec) {
                parse_method(&spec.method).map_err(BgmError::Config)?;
                reqwest::Url::parse(&spec.url)
                    .map_err(|e| BgmError::Config(format!("invalid url {:?}: {e}", spec.url)))?;
            }
            steps.push(StepPlan { spec, asserts: step.asserts, extract: step.extract });
        }
        Ok(Self { steps, config })
    }
}

#[async_trait]
impl Protocol for HttpProtocol {
    type Worker = Client;

    async fn setup(&self, _worker_id: usize) -> Result<Self::Worker> {
        let redirect = if self.config.follow_redirects {
            Policy::default()
        } else {
            Policy::none()
        };
        Client::builder()
            .timeout(self.config.timeout)
            .redirect(redirect)
            .build()
            .map_err(|e| BgmError::Setup(format!("http client: {e}")))
    }

    async fn execute(
        &self,
        client: &mut Self::Worker,
        info: &IterInfo,
    ) -> IterResult<IterReport> {
        // Per-iteration variable context: dynamic counters first, then whatever
        // each step extracts gets layered on top for subsequent steps.
        let mut ctx = MapContext::new();
        ctx.set("seq", info.runner_seq.to_string());
        ctx.set("runner_seq", info.runner_seq.to_string());
        ctx.set("worker_seq", info.worker_seq.to_string());
        ctx.set("worker_id", info.worker_id.to_string());

        let mut total = Duration::ZERO;
        let mut total_bytes = 0u64;
        let mut last_code = 0u16;

        for plan in &self.steps {
            // Resolve any dynamic/extracted placeholders for this step; a no-op
            // clone when the step is fully static.
            let spec = plan.spec.interpolate(&ctx);
            let request = build_request(client, &spec)?;

            let started = Instant::now();
            let response = client.execute(request).await.map_err(classify)?;
            let code = response.status().as_u16();
            let headers = response.headers().clone();
            // Drain the body so we measure full transfer time and byte count.
            let body = response.bytes().await.map_err(classify)?;
            let elapsed = started.elapsed();

            if !plan.asserts.is_empty() {
                plan.asserts.check(code, elapsed, &body).map_err(IterError::Assertion)?;
            }
            for extract in &plan.extract {
                let value = extract_value(extract, &headers, &body)?;
                ctx.set(&extract.var, value);
            }

            total += elapsed;
            total_bytes += body.len() as u64;
            last_code = code;
        }

        Ok(IterReport {
            duration: total,
            status: Status::from_http(last_code),
            bytes: total_bytes,
            items: self.steps.len() as u64,
        })
    }
}

/// Pull a value out of a response per an [`Extract`] rule.
fn extract_value(extract: &Extract, headers: &HeaderMap, body: &[u8]) -> IterResult<String> {
    match &extract.from {
        ExtractFrom::Header(name) => headers
            .get(name.as_str())
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                IterError::Assertion(format!("extract {:?}: header {name:?} missing", extract.var))
            }),
        ExtractFrom::Json(pointer) => {
            let value: Value = serde_json::from_slice(body).map_err(|e| {
                IterError::Assertion(format!("extract {:?}: body is not JSON: {e}", extract.var))
            })?;
            value.pointer(pointer).map(json_to_string).ok_or_else(|| {
                IterError::Assertion(format!(
                    "extract {:?}: JSON pointer {pointer:?} not found",
                    extract.var
                ))
            })
        }
    }
}

/// Render a JSON value as a plain string (strings unquoted; others via JSON).
fn json_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Assemble a `reqwest::Request` from a (possibly per-iteration) spec.
fn build_request(client: &Client, spec: &HttpRequestSpec) -> IterResult<reqwest::Request> {
    let method = parse_method(&spec.method).map_err(IterError::Other)?;
    let mut builder = client.request(method, &spec.url);
    for header in &spec.headers {
        let name = HeaderName::from_bytes(header.name.as_bytes())
            .map_err(|e| IterError::Other(format!("bad header name {:?}: {e}", header.name)))?;
        let value = HeaderValue::from_str(&header.value)
            .map_err(|e| IterError::Other(format!("bad header value: {e}")))?;
        builder = builder.header(name, value);
    }
    if let Some(body) = &spec.body {
        builder = builder.body(body.clone());
    }
    builder.build().map_err(classify)
}

/// Parse an HTTP method, surfacing a readable message on failure.
fn parse_method(raw: &str) -> std::result::Result<Method, String> {
    Method::from_bytes(raw.trim().as_bytes()).map_err(|e| format!("invalid method {raw:?}: {e}"))
}

/// Map a `reqwest::Error` onto a per-iteration error category.
fn classify(err: reqwest::Error) -> IterError {
    if err.is_timeout() {
        IterError::Timeout
    } else {
        IterError::Connection(err.to_string())
    }
}
