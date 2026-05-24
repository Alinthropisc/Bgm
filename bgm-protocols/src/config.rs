//! Tunables for the HTTP client, kept separate from the request itself so the
//! transport policy (timeouts, redirects…) can evolve without touching the
//! request template. Defaults are deliberately conservative.

use std::time::Duration;

/// Transport-level configuration shared by every worker's client.
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    /// Per-request timeout. A breach is recorded as `IterError::Timeout`.
    pub timeout: Duration,
    /// Whether to follow 3xx redirects (off by default — a redirect is usually
    /// signal, not noise, in a load test).
    pub follow_redirects: bool,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            follow_redirects: false,
        }
    }
}
