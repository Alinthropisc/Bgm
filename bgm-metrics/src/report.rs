//! The immutable end-of-run (or live snapshot) report.
//!
//! [`BenchReport`] is a flat, `Serialize`-able value object built from an
//! [`Aggregate`]. It is the single thing reporters consume: the text reporter
//! uses its [`Display`] impl, the JSON reporter just `serde_json`-serializes it.
//! Durations are flattened to milliseconds (`f64`) so the JSON is human-readable
//! rather than `{secs, nanos}` structs.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::aggregate::Aggregate;

/// Fixed bucket upper-bounds (ms) for the latency distribution. Roughly
/// log-spaced to cover sub-millisecond to multi-second responses.
const DIST_EDGES_MS: [f64; 11] = [
    1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1_000.0, 2_000.0,
];

/// One bar of the latency distribution: how many samples fell at or below
/// `le_ms`, but above the previous edge. `le_ms == None` is the overflow bucket
/// (slower than the largest edge). `None` keeps the JSON valid where `+inf`
/// would not.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bucket {
    /// Inclusive upper bound in ms, or `None` for the overflow bucket.
    pub le_ms: Option<f64>,
    pub count: u64,
}

/// Latency percentiles, all in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Latencies {
    pub min_ms: f64,
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p90_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}

/// A point-in-time summary of a benchmark run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchReport {
    /// Seconds the run has been active.
    pub elapsed_secs: f64,
    /// Total iterations completed.
    pub total: u64,
    pub success: u64,
    pub client_error: u64,
    pub server_error: u64,
    pub error: u64,
    /// Completed iterations per second.
    pub rps: f64,
    /// Bytes transferred per second.
    pub throughput_bps: f64,
    pub total_bytes: u64,
    pub total_items: u64,
    pub latency: Latencies,
    /// Latency distribution over fixed buckets (for the dashboard chart).
    pub distribution: Vec<Bucket>,
    /// Per protocol-code tally (e.g. HTTP `200 → n`).
    pub status_codes: BTreeMap<i64, u64>,
}

#[inline]
fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1_000.0
}

impl BenchReport {
    /// Build a report from `agg` given how long the run has been active.
    pub(crate) fn build(agg: &Aggregate, elapsed: Duration) -> Self {
        let (success, client_error, server_error, error) = agg.counts();
        let total = success + client_error + server_error + error;
        let h = agg.histogram();
        let secs = elapsed.as_secs_f64();
        // Guard the divide so a zero-duration snapshot reports 0, not infinity.
        let per_sec = |n: u64| if secs > 0.0 { n as f64 / secs } else { 0.0 };

        let edges_nanos: Vec<u64> = DIST_EDGES_MS
            .iter()
            .map(|ms| (ms * 1_000_000.0) as u64)
            .collect();
        let counts = h.bucket_counts(&edges_nanos);
        let distribution = counts
            .iter()
            .enumerate()
            .map(|(i, &count)| Bucket {
                le_ms: DIST_EDGES_MS.get(i).copied(),
                count,
            })
            .collect();

        Self {
            elapsed_secs: secs,
            total,
            success,
            client_error,
            server_error,
            error,
            rps: per_sec(total),
            throughput_bps: per_sec(agg.bytes()),
            total_bytes: agg.bytes(),
            total_items: agg.items(),
            latency: Latencies {
                min_ms: ms(h.min()),
                mean_ms: ms(h.mean()),
                p50_ms: ms(h.quantile(0.50)),
                p90_ms: ms(h.quantile(0.90)),
                p95_ms: ms(h.quantile(0.95)),
                p99_ms: ms(h.quantile(0.99)),
                max_ms: ms(h.max()),
            },
            distribution,
            status_codes: agg.by_code().clone(),
        }
    }

    /// Serialize to pretty JSON (for `--output json`).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("BenchReport is always serializable")
    }
}

/// Human-readable summary, the shape the CLI prints at the end of a run.
impl fmt::Display for BenchReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Summary")?;
        writeln!(f, "  elapsed      {:.2}s", self.elapsed_secs)?;
        writeln!(f, "  iterations   {} ({:.0}/s)", self.total, self.rps)?;
        writeln!(
            f,
            "  outcomes     ok {} · 4xx {} · 5xx {} · err {}",
            self.success, self.client_error, self.server_error, self.error
        )?;
        writeln!(
            f,
            "  data         {:.2} MiB ({:.2} MiB/s)",
            self.total_bytes as f64 / 1_048_576.0,
            self.throughput_bps / 1_048_576.0
        )?;
        writeln!(f, "Latency (ms)")?;
        writeln!(
            f,
            "  min {:.2}  mean {:.2}  max {:.2}",
            self.latency.min_ms, self.latency.mean_ms, self.latency.max_ms
        )?;
        writeln!(
            f,
            "  p50 {:.2}  p90 {:.2}  p95 {:.2}  p99 {:.2}",
            self.latency.p50_ms, self.latency.p90_ms, self.latency.p95_ms, self.latency.p99_ms
        )?;
        if !self.status_codes.is_empty() {
            write!(f, "Status codes")?;
            for (code, n) in &self.status_codes {
                write!(f, "  {code}:{n}")?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bgm_core::{IterReport, Status};

    #[test]
    fn build_computes_rates_and_roundtrips_json() {
        let mut agg = Aggregate::new();
        for _ in 0..10 {
            agg.record(&IterReport {
                duration: Duration::from_millis(20),
                status: Status::from_http(200),
                bytes: 1_024,
                items: 1,
            });
        }
        let rep = agg.snapshot(Duration::from_secs(2));
        assert_eq!(rep.total, 10);
        assert_eq!(rep.success, 10);
        assert!((rep.rps - 5.0).abs() < 1e-9);

        let json = rep.to_json();
        let back: BenchReport = serde_json::from_str(&json).unwrap();
        assert_eq!(rep, back);
    }
}
