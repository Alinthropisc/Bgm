//! The execution engine: a tokio worker pool that drives a [`Protocol`] under a
//! [`LoadProfile`] and streams results out.
//!
//! Patterns at work:
//! - **Builder** — [`EngineBuilder`] assembles an [`Engine`] from a protocol and
//!   a [`LoadSpec`], so the composition root reads as a sentence.
//! - **Observer** — workers don't aggregate; they emit [`IterReport`]s into an
//!   `mpsc` channel and whoever holds the receiver (TUI, silent collector) gets
//!   to decide what to do with them.
//! - **Mediator** — the engine coordinates workers, the rate limiter, the clock
//!   and the [`CancellationToken`]; the workers themselves stay simple loops.

use std::fmt;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bgm_core::{IterError, IterInfo, IterReport, LoadProfile, Protocol, Status};
use bgm_scenario::LoadSpec;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::profiles::profile_from;

/// How long a not-yet-active worker naps before re-checking the profile.
const IDLE_POLL: Duration = Duration::from_millis(20);

/// Fluent builder for an [`Engine`].
pub struct EngineBuilder<P: Protocol> {
    protocol: P,
    profile: Box<dyn LoadProfile>,
    rate: Option<u32>,
    iterations: Option<u64>,
    capacity: usize,
}

impl<P: Protocol> EngineBuilder<P> {
    /// Start from a protocol and the load described by `spec`.
    pub fn new(protocol: P, spec: &LoadSpec) -> Self {
        Self {
            protocol,
            profile: profile_from(spec),
            rate: spec.rate,
            iterations: spec.iterations,
            capacity: 1024,
        }
    }

    /// Override the report channel capacity (back-pressure bound).
    pub fn channel_capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity.max(1);
        self
    }

    /// Finalize into a runnable [`Engine`].
    pub fn build(self) -> Engine<P> {
        let limiter = self
            .rate
            .and_then(NonZeroU32::new)
            .map(|n| Arc::new(RateLimiter::direct(Quota::per_second(n))));
        Engine {
            protocol: Arc::new(self.protocol),
            profile: Arc::from(self.profile),
            limiter,
            iterations: self.iterations,
            capacity: self.capacity,
        }
    }
}

impl<P: Protocol> fmt::Debug for EngineBuilder<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EngineBuilder")
            .field("profile", &self.profile)
            .field("rate", &self.rate)
            .field("iterations", &self.iterations)
            .field("capacity", &self.capacity)
            .finish_non_exhaustive()
    }
}

/// A configured, ready-to-run engine.
pub struct Engine<P: Protocol> {
    protocol: Arc<P>,
    profile: Arc<dyn LoadProfile>,
    limiter: Option<Arc<DefaultDirectRateLimiter>>,
    iterations: Option<u64>,
    capacity: usize,
}

impl<P: Protocol> fmt::Debug for Engine<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine")
            .field("profile", &self.profile)
            .field("rate_limited", &self.limiter.is_some())
            .field("iterations", &self.iterations)
            .field("capacity", &self.capacity)
            .finish_non_exhaustive()
    }
}

impl<P: Protocol> Engine<P> {
    /// Start the run and return the stream of per-iteration reports.
    ///
    /// Workers are spawned on the current tokio runtime; the channel closes once
    /// every worker has finished (run completed or `cancel` fired), which is how
    /// the receiver learns the run is over. `cancel` lets the caller stop early
    /// (e.g. on Ctrl-C).
    pub fn run(self, cancel: CancellationToken) -> mpsc::Receiver<IterReport> {
        let (tx, rx) = mpsc::channel(self.capacity);
        tokio::spawn(self.drive(tx, cancel));
        rx
    }

    /// Supervisor task: spawn the pool, then wait for it to drain.
    async fn drive(self, tx: mpsc::Sender<IterReport>, cancel: CancellationToken) {
        let start = Instant::now();

        // A wall-clock budget translates into a deadline that cancels everyone.
        if let Some(limit) = self.profile.duration() {
            let cancel = cancel.clone();
            tokio::spawn(async move {
                sleep(limit).await;
                cancel.cancel();
            });
        }

        let max = self.profile.max_concurrency();
        let seq = Arc::new(AtomicU64::new(0));
        let mut handles = Vec::with_capacity(max);
        for id in 0..max {
            let ctx = WorkerCtx {
                protocol: Arc::clone(&self.protocol),
                profile: Arc::clone(&self.profile),
                limiter: self.limiter.clone(),
                iterations: self.iterations,
                seq: Arc::clone(&seq),
                tx: tx.clone(),
                cancel: cancel.clone(),
                start,
                id,
            };
            handles.push(tokio::spawn(worker_loop(ctx)));
        }
        // Drop our sender so the channel can close once the workers' clones do.
        drop(tx);
        for handle in handles {
            let _ = handle.await;
        }
    }
}

/// Everything one worker needs. Bundled so the supervisor loop stays readable.
struct WorkerCtx<P: Protocol> {
    protocol: Arc<P>,
    profile: Arc<dyn LoadProfile>,
    limiter: Option<Arc<DefaultDirectRateLimiter>>,
    iterations: Option<u64>,
    seq: Arc<AtomicU64>,
    tx: mpsc::Sender<IterReport>,
    cancel: CancellationToken,
    start: Instant,
    id: usize,
}

/// One worker's lifetime: `setup` once, loop `execute`, `teardown` at the end
/// (the Template Method shape, driven from the engine side).
async fn worker_loop<P: Protocol>(ctx: WorkerCtx<P>) {
    let mut state = match ctx.protocol.setup(ctx.id).await {
        Ok(state) => state,
        Err(_) => {
            // Setup is fatal for the whole run: stop everyone rather than limp on
            // with a partial pool.
            ctx.cancel.cancel();
            return;
        }
    };

    let mut info = IterInfo::new(ctx.id);
    loop {
        if ctx.cancel.is_cancelled() {
            break;
        }
        let elapsed = ctx.start.elapsed();
        if ctx.profile.duration().is_some_and(|d| elapsed >= d) {
            break;
        }

        // Honor the concurrency curve: workers beyond the current target nap.
        if ctx.id >= ctx.profile.concurrency_at(elapsed) {
            tokio::select! {
                () = ctx.cancel.cancelled() => break,
                () = sleep(IDLE_POLL) => continue,
            }
        }

        // Claim a global sequence number; enforce the iteration budget on it.
        let n = ctx.seq.fetch_add(1, Ordering::Relaxed);
        if ctx.iterations.is_some_and(|cap| n >= cap) {
            ctx.cancel.cancel();
            break;
        }
        info.runner_seq = n;

        if let Some(limiter) = &ctx.limiter {
            tokio::select! {
                () = ctx.cancel.cancelled() => break,
                () = limiter.until_ready() => {}
            }
        }

        // Time the call ourselves so a failed iteration still has a duration.
        let started = Instant::now();
        let report = match ctx.protocol.execute(&mut state, &info).await {
            Ok(report) => report,
            Err(err) => IterReport {
                duration: started.elapsed(),
                status: status_for(&err),
                bytes: 0,
                items: 0,
            },
        };

        if ctx.tx.send(report).await.is_err() {
            break; // Receiver dropped — nobody is listening; stop.
        }
        info.worker_seq += 1;
    }

    let _ = ctx.protocol.teardown(state).await;
}

/// Map a per-iteration error onto a coarse [`Status`] for the metrics stream.
fn status_for(err: &IterError) -> Status {
    match err {
        // 4xx-shaped: the request was well-formed but rejected by our assertion.
        IterError::Assertion(_) => Status::client_error(0),
        // Everything else is a transport/timeout failure.
        IterError::Timeout | IterError::Connection(_) | IterError::Other(_) => Status::error(0),
    }
}
