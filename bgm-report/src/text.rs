//! Plain-text reporter — the human summary printed at the end of a run.

use bgm_metrics::BenchReport;

use crate::reporter::Reporter;

/// Renders the report via its [`std::fmt::Display`] summary.
#[derive(Debug, Default, Clone, Copy)]
pub struct TextReporter;

impl Reporter for TextReporter {
    fn render(&self, report: &BenchReport) -> String {
        format!("{report}")
    }

    fn extension(&self) -> &'static str {
        "txt"
    }
}
