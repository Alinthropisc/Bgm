//! JSON reporter — machine-readable, the full report verbatim.

use bgm_metrics::BenchReport;

use crate::reporter::Reporter;

/// Pretty-prints the report as JSON.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonReporter;

impl Reporter for JsonReporter {
    fn render(&self, report: &BenchReport) -> String {
        report.to_json()
    }

    fn extension(&self) -> &'static str {
        "json"
    }
}
