//! Response assertions.
//!
//! A scenario may declare expectations a response must meet; a failed assertion
//! turns an otherwise-successful iteration into a counted failure (not a fatal
//! error). The model is pure data here — protocols evaluate it where they have
//! the response in hand (`bgm-protocols` for HTTP), so the engine and core stay
//! oblivious. Keeping the check on the scenario side (rather than scattering
//! `if`s through the protocol) keeps the policy declarative and testable.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Expectations checked against each response.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Assertions {
    /// Exact status code the response must carry (e.g. `200`).
    pub status: Option<u16>,
    /// Maximum acceptable latency; slower iterations fail.
    #[serde(with = "humantime_serde", default)]
    pub max_latency: Option<Duration>,
    /// Substring that must appear in the response body.
    pub body_contains: Option<String>,
}

impl Assertions {
    /// `true` when nothing is asserted (the common, zero-cost case).
    pub fn is_empty(&self) -> bool {
        self.status.is_none() && self.max_latency.is_none() && self.body_contains.is_none()
    }

    /// Check a response. Returns the first failure's message, or `Ok(())`.
    pub fn check(&self, status: u16, latency: Duration, body: &[u8]) -> Result<(), String> {
        if let Some(expected) = self.status {
            if status != expected {
                return Err(format!("expected status {expected}, got {status}"));
            }
        }
        self.check_body_latency(latency, body)
    }

    /// Like [`Assertions::check`] but without the status code — for protocols
    /// that have no status concept (e.g. WebSocket messages).
    pub fn check_body_latency(&self, latency: Duration, body: &[u8]) -> Result<(), String> {
        if let Some(max) = self.max_latency {
            if latency > max {
                return Err(format!("latency {latency:?} exceeded limit {max:?}"));
            }
        }
        if let Some(needle) = &self.body_contains {
            if !contains(body, needle.as_bytes()) {
                return Err(format!("body did not contain {needle:?}"));
            }
        }
        Ok(())
    }
}

/// Naive substring search over bytes — fine for the small needles assertions use.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_when_all_match() {
        let a = Assertions {
            status: Some(200),
            max_latency: Some(Duration::from_secs(1)),
            body_contains: Some("ok".into()),
        };
        assert!(
            a.check(200, Duration::from_millis(10), b"all ok here")
                .is_ok()
        );
    }

    #[test]
    fn reports_first_failure() {
        let a = Assertions {
            status: Some(200),
            ..Default::default()
        };
        let err = a.check(503, Duration::ZERO, b"").unwrap_err();
        assert!(err.contains("503"));
    }

    #[test]
    fn empty_is_cheap_and_always_ok() {
        let a = Assertions::default();
        assert!(a.is_empty());
        assert!(a.check(500, Duration::from_secs(99), b"").is_ok());
    }
}
