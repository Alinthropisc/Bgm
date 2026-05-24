//! Baseline comparison — persist a run and diff the next one against it to catch
//! performance regressions (think CI gate).
//!
//! A baseline is just a saved [`BenchReport`] JSON. [`Comparison`] computes the
//! deltas that matter — throughput, tail latency, error rate — and decides
//! whether the new run *regressed* against simple, explicit thresholds.

use std::fs;
use std::io;
use std::path::Path;

use bgm_metrics::BenchReport;

/// Regression thresholds. Crossing any one flags the run as regressed.
const RPS_DROP_PCT: f64 = 5.0; // throughput fell by more than this %
const P99_RISE_PCT: f64 = 10.0; // tail latency rose by more than this %
const ERROR_RISE_PP: f64 = 1.0; // error rate rose by more than this (points)

/// Save a report as a baseline JSON file.
pub fn save(report: &BenchReport, path: &Path) -> io::Result<()> {
    fs::write(path, report.to_json())
}

/// Load a previously-saved baseline report.
pub fn load(path: &Path) -> io::Result<BenchReport> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// The delta between a current run and a baseline.
#[derive(Debug, Clone, Copy)]
pub struct Comparison {
    /// Change in RPS, percent (positive = faster).
    pub rps_delta_pct: f64,
    /// Change in p50 latency, percent (positive = slower).
    pub p50_delta_pct: f64,
    /// Change in p99 latency, percent (positive = slower).
    pub p99_delta_pct: f64,
    /// Change in error rate, percentage points (positive = worse).
    pub error_rate_delta_pp: f64,
    /// `true` if any threshold was crossed in the bad direction.
    pub regressed: bool,
}

impl Comparison {
    /// Compare `current` against `baseline`.
    pub fn new(current: &BenchReport, baseline: &BenchReport) -> Self {
        let rps_delta_pct = pct_change(baseline.rps, current.rps);
        let p50_delta_pct = pct_change(baseline.latency.p50_ms, current.latency.p50_ms);
        let p99_delta_pct = pct_change(baseline.latency.p99_ms, current.latency.p99_ms);
        let error_rate_delta_pp = error_rate(current) - error_rate(baseline);

        let regressed = rps_delta_pct < -RPS_DROP_PCT
            || p99_delta_pct > P99_RISE_PCT
            || error_rate_delta_pp > ERROR_RISE_PP;

        Self { rps_delta_pct, p50_delta_pct, p99_delta_pct, error_rate_delta_pp, regressed }
    }
}

impl std::fmt::Display for Comparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let verdict = if self.regressed { "REGRESSED" } else { "OK" };
        writeln!(f, "Baseline comparison: {verdict}")?;
        writeln!(f, "  RPS         {:+.1}%", self.rps_delta_pct)?;
        writeln!(f, "  p50 latency {:+.1}%", self.p50_delta_pct)?;
        writeln!(f, "  p99 latency {:+.1}%", self.p99_delta_pct)?;
        write!(f, "  error rate  {:+.2} pp", self.error_rate_delta_pp)
    }
}

/// Percent change from `from` to `to`, guarding division by zero.
fn pct_change(from: f64, to: f64) -> f64 {
    if from.abs() < f64::EPSILON {
        return 0.0;
    }
    (to - from) / from * 100.0
}

/// Error rate of a report as a percentage (0..=100).
fn error_rate(report: &BenchReport) -> f64 {
    if report.total == 0 {
        return 0.0;
    }
    (report.total - report.success) as f64 / report.total as f64 * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(rps: f64, p99: f64, total: u64, success: u64) -> BenchReport {
        let json = format!(
            r#"{{"elapsed_secs":1.0,"total":{total},"success":{success},"client_error":0,
            "server_error":0,"error":{},"rps":{rps},"throughput_bps":0.0,"total_bytes":0,
            "total_items":{total},"latency":{{"min_ms":0,"mean_ms":0,"p50_ms":1,"p90_ms":1,
            "p95_ms":1,"p99_ms":{p99},"max_ms":1}},"distribution":[],"status_codes":{{}}}}"#,
            total - success
        );
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn flags_throughput_regression() {
        let base = report(1000.0, 10.0, 1000, 1000);
        let now = report(800.0, 10.0, 1000, 1000); // -20% rps
        let cmp = Comparison::new(&now, &base);
        assert!(cmp.regressed);
        assert!(cmp.rps_delta_pct < -5.0);
    }

    #[test]
    fn stable_run_is_ok() {
        let base = report(1000.0, 10.0, 1000, 1000);
        let now = report(990.0, 10.5, 1000, 1000);
        assert!(!Comparison::new(&now, &base).regressed);
    }
}
