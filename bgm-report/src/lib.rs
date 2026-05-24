//! # bgm-report
//!
//! Output strategies for a finished run. A [`Reporter`] renders a
//! [`BenchReport`](bgm_metrics::BenchReport) into text, JSON, CSV or a
//! self-contained HTML page; [`baseline`] persists a run and diffs the next one
//! against it ([`Comparison`]) to catch regressions.
//!
//! Depends only on [`bgm_metrics`] and serde — pure rendering, no I/O except the
//! small baseline file helpers.

pub mod baseline;
pub mod csv;
pub mod html;
pub mod json;
pub mod reporter;
pub mod text;

pub use baseline::{Comparison, load, save};
pub use csv::CsvReporter;
pub use html::HtmlReporter;
pub use json::JsonReporter;
pub use reporter::Reporter;
pub use text::TextReporter;
