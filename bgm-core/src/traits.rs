//! The two strategy traits that make BGM extensible.
//!
//! - [`Protocol`] — *what work an iteration does* (HTTP today; gRPC/TCP/SQL
//!   tomorrow). Implemented in `bgm-protocols`.
//! - [`LoadProfile`] — *how much concurrency to apply over time* (constant,
//!   ramp-up, spike…). Implemented in `bgm-engine`.
//!
//! The engine is generic over `Protocol` (static dispatch, zero-cost) and holds
//! a `dyn LoadProfile` (dynamic — it's polled rarely, flexibility wins).

use std::time::Duration;

use async_trait::async_trait;

use crate::error::{IterResult, Result};
use crate::report::{IterInfo, IterReport};

/// Defines the unit of work executed on every iteration.
///
/// This is the *Template Method* shape: `setup` once per worker, `execute`
/// per iteration, `teardown` once at the end. Per-worker state (an HTTP client,
/// a DB connection) lives in [`Protocol::Worker`].
#[async_trait]
pub trait Protocol: Send + Sync + 'static {
    /// Per-worker mutable state, created in [`Protocol::setup`].
    type Worker: Send;

    /// Initialise per-worker state. Called once per worker before the run.
    async fn setup(&self, worker_id: usize) -> Result<Self::Worker>;

    /// Run a single iteration and report its outcome.
    ///
    /// Returning `Err(IterError)` records a failed iteration; it does **not**
    /// abort the benchmark.
    async fn execute(
        &self,
        worker: &mut Self::Worker,
        info: &IterInfo,
    ) -> IterResult<IterReport>;

    /// Release per-worker resources. Default: nothing to do.
    async fn teardown(&self, _worker: Self::Worker) -> Result<()> {
        Ok(())
    }
}

/// Decides the desired concurrency at any point during the run.
pub trait LoadProfile: Send + Sync + std::fmt::Debug {
    /// Target number of active workers at `elapsed` since start.
    fn concurrency_at(&self, elapsed: Duration) -> usize;

    /// Upper bound on concurrency, used to pre-size the worker pool.
    fn max_concurrency(&self) -> usize;

    /// Total planned duration, if the profile is time-bounded.
    fn duration(&self) -> Option<Duration>;
}
