//! View-model: the immutable bundle the renderer reads each frame, plus the
//! small formatting helpers that keep `render.rs` declarative.

use std::time::Duration;

use bgm_metrics::BenchReport;

/// Static facts about the run, known before it starts.
#[derive(Debug, Clone)]
pub struct RunMeta {
    /// Scenario name, shown in the header.
    pub name: String,
    /// Iteration budget, if any (drives the progress gauge).
    pub target_iterations: Option<u64>,
    /// Time budget, if any (drives the progress gauge when no iteration cap).
    pub target_duration: Option<Duration>,
}

/// Everything one frame needs. Borrows so rendering allocates nothing extra.
#[derive(Debug)]
pub struct Snapshot<'a> {
    pub meta: &'a RunMeta,
    pub report: &'a BenchReport,
    pub elapsed: Duration,
    /// Recent instantaneous RPS samples, oldest first (for the sparkline).
    pub rps_history: &'a [u64],
    pub peak_rps: u64,
    /// Recent p50 latency samples in milliseconds (for the latency sparkline).
    pub lat_history: &'a [u64],
    pub peak_lat: u64,
    /// `true` once the engine's channel has closed (run finished).
    pub finished: bool,
    pub show_help: bool,
}

impl Snapshot<'_> {
    /// Completion fraction in `0.0..=1.0` for the progress gauge. Prefers the
    /// iteration budget, falls back to the time budget, else `None` (open-ended).
    pub fn progress(&self) -> Option<f64> {
        if let Some(target) = self.meta.target_iterations.filter(|t| *t > 0) {
            return Some((self.report.total as f64 / target as f64).clamp(0.0, 1.0));
        }
        if let Some(total) = self.meta.target_duration.filter(|d| !d.is_zero()) {
            return Some((self.elapsed.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0));
        }
        None
    }
}

/// `1234` → `1.2k`, `1_050_000` → `1.05M`. Compact, fixed-ish width for headers.
pub fn fmt_count(n: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let f = n as f64;
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1}k", f / 1_000.0),
        _ => format!("{:.2}M", f / 1_000_000.0),
    }
}

/// `90061s` → `25:01:01` style `mm:ss` (hours fold into minutes for brevity).
pub fn fmt_clock(d: Duration) -> String {
    let secs = d.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// Bytes → human MiB string.
pub fn fmt_mib(bytes: f64) -> String {
    format!("{:.2} MiB", bytes / 1_048_576.0)
}
