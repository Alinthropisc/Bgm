//! Live aggregation of iteration results.
//!
//! [`Aggregate`] is the running tally a collector keeps as `IterReport`s stream
//! in from the workers (Observer): outcome counts, per-code tallies, transferred
//! bytes/items and a latency [`LatencyHistogram`]. It is deliberately decoupled
//! from wall-clock time — the engine owns the run clock and passes `elapsed` in
//! when it asks for a [`snapshot`](Aggregate::snapshot), so the same `Aggregate`
//! can be snapshotted repeatedly for a live dashboard and once at the end.

use std::collections::BTreeMap;
use std::time::Duration;

use bgm_core::{IterReport, StatusKind};

use crate::histogram::LatencyHistogram;
use crate::report::BenchReport;

/// A running tally of every iteration observed so far.
#[derive(Debug, Clone, Default)]
pub struct Aggregate {
    latency: LatencyHistogram,
    /// Outcome counts indexed by [`StatusKind`] (`success/client/server/error`).
    counts: Counts,
    /// Per protocol-code tally (e.g. HTTP 200 → n), kept ordered for display.
    by_code: BTreeMap<i64, u64>,
    bytes: u64,
    items: u64,
}

/// Outcome counts, one field per [`StatusKind`].
#[derive(Debug, Clone, Copy, Default)]
struct Counts {
    success: u64,
    client_error: u64,
    server_error: u64,
    error: u64,
}

impl Aggregate {
    /// An empty aggregate.
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold a single iteration result into the tally.
    pub fn record(&mut self, report: &IterReport) {
        self.latency.record(report.duration);
        self.bytes += report.bytes;
        self.items += report.items;

        let status = report.status;
        match status.kind() {
            StatusKind::Success => self.counts.success += 1,
            StatusKind::ClientError => self.counts.client_error += 1,
            StatusKind::ServerError => self.counts.server_error += 1,
            StatusKind::Error => self.counts.error += 1,
        }
        if status.code() != 0 {
            *self.by_code.entry(status.code()).or_insert(0) += 1;
        }
    }

    /// Total iterations recorded.
    pub fn total(&self) -> u64 {
        let c = &self.counts;
        c.success + c.client_error + c.server_error + c.error
    }

    /// Borrow the latency histogram (e.g. for a live sparkline).
    pub fn latency(&self) -> &LatencyHistogram {
        &self.latency
    }

    /// Build an immutable, serializable [`BenchReport`] for the run so far.
    /// `elapsed` is the wall-clock time the run has been active, used to derive
    /// throughput rates.
    pub fn snapshot(&self, elapsed: Duration) -> BenchReport {
        BenchReport::build(self, elapsed)
    }

    // --- accessors used by `BenchReport::build` (same crate) ---

    pub(crate) fn counts(&self) -> (u64, u64, u64, u64) {
        let c = &self.counts;
        (c.success, c.client_error, c.server_error, c.error)
    }

    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }

    pub(crate) fn items(&self) -> u64 {
        self.items
    }

    pub(crate) fn by_code(&self) -> &BTreeMap<i64, u64> {
        &self.by_code
    }

    pub(crate) fn histogram(&self) -> &LatencyHistogram {
        &self.latency
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bgm_core::Status;

    fn report(status: Status, ms: u64) -> IterReport {
        IterReport {
            duration: Duration::from_millis(ms),
            status,
            bytes: 100,
            items: 1,
        }
    }

    #[test]
    fn counts_outcomes_by_kind_and_code() {
        let mut agg = Aggregate::new();
        agg.record(&report(Status::from_http(200), 10));
        agg.record(&report(Status::from_http(200), 20));
        agg.record(&report(Status::from_http(404), 5));
        agg.record(&report(Status::from_http(503), 5));

        assert_eq!(agg.total(), 4);
        let (ok, ce, se, err) = agg.counts();
        assert_eq!((ok, ce, se, err), (2, 1, 1, 0));
        assert_eq!(agg.by_code().get(&200), Some(&2));
        assert_eq!(agg.bytes(), 400);
    }
}
