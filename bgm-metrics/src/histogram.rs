//! Latency histogram.
//!
//! A thin wrapper around [`hdrhistogram::Histogram`] that speaks in
//! [`Duration`] instead of raw integers. We record nanoseconds with three
//! significant figures and an auto-resizing backing store, so a run that drifts
//! from microseconds to seconds stays accurate without pre-sizing.
//!
//! The wrapper exists so the rest of BGM never imports `hdrhistogram` directly
//! (Adapter): if we ever swap the backend, only this file changes.

use std::time::Duration;

use hdrhistogram::Histogram;

/// Records iteration latencies and answers percentile / summary queries.
#[derive(Debug, Clone)]
pub struct LatencyHistogram {
    inner: Histogram<u64>,
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl LatencyHistogram {
    /// A fresh, empty histogram (3 significant figures, auto-resizing).
    pub fn new() -> Self {
        // `new(3)` cannot fail for a valid sigfig count; resizing is enabled by
        // default so we never need an upper bound up front.
        let inner = Histogram::new(3).expect("3 is a valid significant-figure count");
        Self { inner }
    }

    /// Record one latency sample. Sub-nanosecond and zero durations are clamped
    /// to 1 ns (the histogram's lowest trackable value); absurdly long samples
    /// saturate at `u64::MAX` ns rather than panicking.
    pub fn record(&mut self, latency: Duration) {
        let nanos = u64::try_from(latency.as_nanos()).unwrap_or(u64::MAX).max(1);
        // Auto-resizing means the only error is an out-of-range value, which the
        // clamp above prevents; ignore the Result deliberately.
        let _ = self.inner.record(nanos);
    }

    /// Fold another histogram into this one (used to merge per-worker tallies).
    /// Auto-resizing makes the add infallible here; ignore the Result.
    pub fn merge(&mut self, other: &Self) {
        let _ = self.inner.add(&other.inner);
    }

    /// Number of samples recorded.
    pub fn len(&self) -> u64 {
        self.inner.len()
    }

    /// `true` until the first sample is recorded.
    pub fn is_empty(&self) -> bool {
        self.inner.len() == 0
    }

    /// Latency at the given quantile (`0.0..=1.0`).
    pub fn quantile(&self, q: f64) -> Duration {
        Duration::from_nanos(self.inner.value_at_quantile(q))
    }

    /// Smallest recorded latency.
    pub fn min(&self) -> Duration {
        Duration::from_nanos(self.inner.min())
    }

    /// Largest recorded latency.
    pub fn max(&self) -> Duration {
        Duration::from_nanos(self.inner.max())
    }

    /// Arithmetic mean of recorded latencies.
    pub fn mean(&self) -> Duration {
        Duration::from_nanos(self.inner.mean() as u64)
    }

    /// Count samples falling into each bucket defined by ascending `edges_nanos`
    /// (inclusive upper bounds). Returns one count per edge plus a final
    /// "overflow" count for samples above the last edge — so the result has
    /// `edges_nanos.len() + 1` entries. Implemented via cumulative differences
    /// so no sample is double-counted at a shared boundary.
    pub fn bucket_counts(&self, edges_nanos: &[u64]) -> Vec<u64> {
        let mut out = Vec::with_capacity(edges_nanos.len() + 1);
        let total = self.inner.len();
        let mut prev_cumulative = 0;
        for &edge in edges_nanos {
            let cumulative = self.inner.count_between(0, edge);
            out.push(cumulative.saturating_sub(prev_cumulative));
            prev_cumulative = cumulative;
        }
        out.push(total.saturating_sub(prev_cumulative));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_reports_percentiles() {
        let mut h = LatencyHistogram::new();
        for ms in 1..=100u64 {
            h.record(Duration::from_millis(ms));
        }
        assert_eq!(h.len(), 100);
        // p50 of 1..=100 ms should land near 50 ms (within histogram error).
        let p50 = h.quantile(0.5).as_millis();
        assert!((45..=55).contains(&p50), "p50 was {p50}ms");
        assert!(h.max().as_millis() >= 99);
    }

    #[test]
    fn clamps_zero_and_merges() {
        let mut a = LatencyHistogram::new();
        a.record(Duration::ZERO);
        let mut b = LatencyHistogram::new();
        b.record(Duration::from_millis(5));
        a.merge(&b);
        assert_eq!(a.len(), 2);
    }
}
