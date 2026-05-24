//! Error types shared across BGM.
//!
//! Two layers, on purpose:
//! - [`BgmError`] — fatal, setup-time failures (bad config, can't bind, …).
//!   These abort the run.
//! - [`IterError`] — a *single iteration* failed (timeout, connection reset).
//!   These are expected during a load test and are folded into the statistics
//!   rather than aborting the whole run.

use thiserror::Error;

/// Result alias for fatal BGM operations.
pub type Result<T> = std::result::Result<T, BgmError>;

/// Result alias for a single load-test iteration.
pub type IterResult<T> = std::result::Result<T, IterError>;

/// A fatal error that aborts the whole benchmark.
#[derive(Debug, Error)]
pub enum BgmError {
    /// Invalid or contradictory configuration.
    #[error("configuration error: {0}")]
    Config(String),

    /// A protocol could not be initialised (DNS, TLS, bind, …).
    #[error("protocol setup failed: {0}")]
    Setup(String),

    /// I/O failure outside of an iteration (writing reports, terminal, …).
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Anything else, with context.
    #[error("{0}")]
    Other(String),
}

/// A recoverable, per-iteration error. Counted, not fatal.
#[derive(Debug, Error)]
pub enum IterError {
    /// The iteration exceeded its deadline.
    #[error("timeout")]
    Timeout,

    /// Transport/connection level failure.
    #[error("connection error: {0}")]
    Connection(String),

    /// A response-level assertion failed.
    #[error("assertion failed: {0}")]
    Assertion(String),

    /// Any other per-iteration failure.
    #[error("{0}")]
    Other(String),
}
