//! Per-iteration data structures exchanged between workers and collectors.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::status::Status;

/// Context handed to a [`crate::Protocol`] on every iteration so it can vary
/// behaviour per worker / per iteration (seeding RNGs, picking data rows, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IterInfo {
    /// Stable id of the worker running this iteration (`0..concurrency`).
    pub worker_id: usize,
    /// How many iterations this worker has completed so far.
    pub worker_seq: u64,
    /// Global iteration sequence across all workers.
    pub runner_seq: u64,
}

impl IterInfo {
    /// Create context for a worker's first iteration.
    pub fn new(worker_id: usize) -> Self {
        Self {
            worker_id,
            worker_seq: 0,
            runner_seq: 0,
        }
    }
}

/// The result of a single iteration. This is the unit of data the engine
/// streams to collectors (Observer pattern); keep it cheap to move.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct IterReport {
    /// Wall-clock time the iteration took.
    pub duration: Duration,
    /// Classified outcome.
    pub status: Status,
    /// Bytes transferred during the iteration (for throughput).
    pub bytes: u64,
    /// Logical items processed (requests, rows, messages…).
    pub items: u64,
}
