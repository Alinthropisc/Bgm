//! How much load to apply, expressed declaratively.
//!
//! [`LoadSpec`] is the *intent* ("50 workers for 30s, capped at 500 rps, ramped
//! over 5s"). It is intentionally a plain data record — `bgm-engine` is the one
//! that turns it into a concrete `LoadProfile` strategy. Keeping the spec here,
//! free of any runtime, lets the CLI and the YAML parser produce it without
//! pulling in tokio.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Shape of the concurrency curve over the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    /// Hold [`LoadSpec::concurrency`] for the whole run.
    #[default]
    Constant,
    /// Linearly grow from 1 to [`LoadSpec::concurrency`] over [`LoadSpec::ramp`],
    /// then hold.
    RampUp,
    /// Climb to [`LoadSpec::concurrency`] in [`LoadSpec::steps`] discrete
    /// stairs spread evenly across [`LoadSpec::ramp`], then hold (staircase).
    Steps,
}

/// Declarative description of the load to generate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoadSpec {
    /// Target number of concurrent workers (the ceiling for `RampUp`).
    pub concurrency: usize,
    /// Concurrency curve.
    pub profile: Profile,
    /// Stop after this wall-clock duration, if set.
    #[serde(with = "humantime_serde", default)]
    pub duration: Option<Duration>,
    /// Stop after this many total iterations, if set.
    pub iterations: Option<u64>,
    /// Global rate cap in iterations per second, if set.
    pub rate: Option<u32>,
    /// Ramp window for [`Profile::RampUp`] / [`Profile::Steps`]; ignored
    /// otherwise.
    #[serde(with = "humantime_serde", default)]
    pub ramp: Option<Duration>,
    /// Number of stairs for [`Profile::Steps`] (ignored otherwise).
    pub steps: u32,
}

impl Default for LoadSpec {
    fn default() -> Self {
        Self {
            concurrency: 1,
            profile: Profile::Constant,
            duration: None,
            iterations: None,
            rate: None,
            ramp: None,
            steps: 5,
        }
    }
}

impl LoadSpec {
    /// A run that holds `concurrency` workers for `duration`.
    pub fn constant(concurrency: usize, duration: Duration) -> Self {
        Self { concurrency, duration: Some(duration), ..Self::default() }
    }

    /// `true` if neither a duration nor an iteration budget bounds the run.
    /// Such a run is open-ended and only stops on Ctrl-C / cancellation.
    pub fn is_open_ended(&self) -> bool {
        self.duration.is_none() && self.iterations.is_none()
    }
}
