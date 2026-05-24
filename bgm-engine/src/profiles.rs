//! Load profiles — the *shape* of concurrency over time (Strategy).
//!
//! Each profile answers one question for the engine: "how many workers should
//! be active at `elapsed`?". The worker pool polls that on every loop, so a new
//! curve (spike, sawtooth, stepped…) is just a new [`LoadProfile`] impl here —
//! the engine doesn't change. [`profile_from`] is the small Factory that turns a
//! declarative [`LoadSpec`] into the right strategy.

use std::time::Duration;

use bgm_core::LoadProfile;
use bgm_scenario::{LoadSpec, Profile};

/// Hold a fixed concurrency for the whole run.
#[derive(Debug, Clone, Copy)]
pub struct ConstantLoad {
    pub concurrency: usize,
    pub duration: Option<Duration>,
}

impl LoadProfile for ConstantLoad {
    fn concurrency_at(&self, _elapsed: Duration) -> usize {
        self.concurrency.max(1)
    }

    fn max_concurrency(&self) -> usize {
        self.concurrency.max(1)
    }

    fn duration(&self) -> Option<Duration> {
        self.duration
    }
}

/// Grow linearly from 1 to `target` over `ramp`, then hold at `target`.
#[derive(Debug, Clone, Copy)]
pub struct RampUpLoad {
    pub target: usize,
    pub ramp: Duration,
    pub duration: Option<Duration>,
}

impl LoadProfile for RampUpLoad {
    fn concurrency_at(&self, elapsed: Duration) -> usize {
        let target = self.target.max(1);
        if self.ramp.is_zero() {
            return target;
        }
        let frac = (elapsed.as_secs_f64() / self.ramp.as_secs_f64()).clamp(0.0, 1.0);
        // ceil so we reach `target` exactly at the end of the ramp, and never
        // sit at 0 active workers once the run has started.
        let n = (target as f64 * frac).ceil() as usize;
        n.clamp(1, target)
    }

    fn max_concurrency(&self) -> usize {
        self.target.max(1)
    }

    fn duration(&self) -> Option<Duration> {
        self.duration
    }
}

/// Climb to `target` in `steps` discrete stairs spread across `ramp`, then hold.
#[derive(Debug, Clone, Copy)]
pub struct StepsLoad {
    pub target: usize,
    pub steps: u32,
    pub ramp: Duration,
    pub duration: Option<Duration>,
}

impl LoadProfile for StepsLoad {
    fn concurrency_at(&self, elapsed: Duration) -> usize {
        let target = self.target.max(1);
        let steps = self.steps.max(1) as usize;
        if self.ramp.is_zero() || steps == 1 {
            return target;
        }
        let frac = (elapsed.as_secs_f64() / self.ramp.as_secs_f64()).clamp(0.0, 1.0);
        // Which stair we're on: 1..=steps.
        let stair = ((frac * steps as f64).floor() as usize).min(steps - 1) + 1;
        // Concurrency at this stair, rounded up so the last stair hits `target`.
        let conc = (target * stair).div_ceil(steps);
        conc.clamp(1, target)
    }

    fn max_concurrency(&self) -> usize {
        self.target.max(1)
    }

    fn duration(&self) -> Option<Duration> {
        self.duration
    }
}

/// Build the [`LoadProfile`] strategy described by a [`LoadSpec`].
pub fn profile_from(spec: &LoadSpec) -> Box<dyn LoadProfile> {
    match spec.profile {
        Profile::Constant => {
            Box::new(ConstantLoad { concurrency: spec.concurrency, duration: spec.duration })
        }
        Profile::RampUp => Box::new(RampUpLoad {
            target: spec.concurrency,
            ramp: spec.ramp.unwrap_or(Duration::ZERO),
            duration: spec.duration,
        }),
        Profile::Steps => Box::new(StepsLoad {
            target: spec.concurrency,
            steps: spec.steps,
            ramp: spec.ramp.unwrap_or(Duration::ZERO),
            duration: spec.duration,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_is_flat() {
        let p = ConstantLoad { concurrency: 8, duration: None };
        assert_eq!(p.concurrency_at(Duration::ZERO), 8);
        assert_eq!(p.concurrency_at(Duration::from_secs(99)), 8);
    }

    #[test]
    fn steps_climb_in_stairs() {
        let p = StepsLoad { target: 10, steps: 5, ramp: Duration::from_secs(10), duration: None };
        assert_eq!(p.concurrency_at(Duration::ZERO), 2); // stair 1 of 5 → 10/5
        assert_eq!(p.concurrency_at(Duration::from_secs(4)), 6); // stair 3 → 6
        assert_eq!(p.concurrency_at(Duration::from_secs(10)), 10); // top
        assert_eq!(p.concurrency_at(Duration::from_secs(99)), 10); // holds
    }

    #[test]
    fn ramp_grows_then_holds() {
        let p = RampUpLoad { target: 10, ramp: Duration::from_secs(10), duration: None };
        assert_eq!(p.concurrency_at(Duration::ZERO), 1); // never 0 once started
        assert_eq!(p.concurrency_at(Duration::from_secs(5)), 5);
        assert_eq!(p.concurrency_at(Duration::from_secs(10)), 10);
        assert_eq!(p.concurrency_at(Duration::from_secs(60)), 10); // holds
    }
}
