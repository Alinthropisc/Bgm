//! CSV reporter — a single summary row, easy to append into a spreadsheet or
//! a time series across runs.

use bgm_metrics::BenchReport;

use crate::reporter::Reporter;

/// Emits a two-line CSV: a header row and one values row.
#[derive(Debug, Default, Clone, Copy)]
pub struct CsvReporter;

impl Reporter for CsvReporter {
    fn render(&self, report: &BenchReport) -> String {
        let l = &report.latency;
        let header = "elapsed_secs,total,success,client_error,server_error,error,\
rps,throughput_bps,min_ms,mean_ms,p50_ms,p90_ms,p95_ms,p99_ms,max_ms";
        let row = format!(
            "{:.3},{},{},{},{},{},{:.2},{:.2},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}",
            report.elapsed_secs,
            report.total,
            report.success,
            report.client_error,
            report.server_error,
            report.error,
            report.rps,
            report.throughput_bps,
            l.min_ms,
            l.mean_ms,
            l.p50_ms,
            l.p90_ms,
            l.p95_ms,
            l.p99_ms,
            l.max_ms,
        );
        format!("{header}\n{row}\n")
    }

    fn extension(&self) -> &'static str {
        "csv"
    }
}
