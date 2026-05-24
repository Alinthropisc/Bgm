//! # bgm-metrics
//!
//! Statistics for a BGM run: a latency [`LatencyHistogram`], the live
//! [`Aggregate`] that folds in `IterReport`s as workers produce them, and the
//! serializable [`BenchReport`] snapshot that reporters render.
//!
//! This crate depends only on [`bgm_core`] (for the domain types) and
//! `hdrhistogram` — no runtime, no I/O. Collectors elsewhere own *when* to
//! record and snapshot; this crate only knows *how* to tally.

pub mod aggregate;
pub mod histogram;
pub mod report;

pub use aggregate::Aggregate;
pub use histogram::LatencyHistogram;
pub use report::{BenchReport, Bucket, Latencies};
