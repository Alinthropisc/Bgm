//! The reporter contract.
//!
//! A [`Reporter`] turns a finished [`BenchReport`] into a textual artifact —
//! plain summary, JSON, CSV or a self-contained HTML page. This is the Strategy
//! pattern: the CLI picks one at runtime and never cares which it got. Reporters
//! are pure (no I/O); the caller decides where the rendered bytes go.

use bgm_metrics::BenchReport;

/// Renders a [`BenchReport`] into a string artifact.
pub trait Reporter {
    /// Produce the rendered report.
    fn render(&self, report: &BenchReport) -> String;

    /// File-extension hint (no dot), used when writing to a generated filename.
    fn extension(&self) -> &'static str;
}
