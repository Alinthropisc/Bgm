//! The scenario aggregate: a request to drive plus how hard to drive it.
//!
//! Two construction paths converge on the same value (Adapter + Builder):
//! - **YAML** — [`Scenario::from_yaml`] deserializes the on-disk schema.
//! - **CLI flags** — [`ScenarioBuilder`] assembles one fluently.
//!
//! Either way the result is validated by [`Scenario::validate`]. Variables are
//! resolved on demand via [`Scenario::resolved`], which feeds the scenario's
//! `vars` into the [`Interpolate`] machinery.

use std::collections::BTreeMap;
use std::time::Duration;

use bgm_core::{Interpolate, MapContext};
use serde::{Deserialize, Serialize};

use crate::assert::Assertions;
use crate::error::{Result, ScenarioError};
use crate::http::HttpRequestSpec;
use crate::load::LoadSpec;
use crate::step::Step;

/// Which transport strategy drives the scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProtocolKind {
    /// HTTP(S) request per iteration (default).
    #[default]
    Http,
    /// WebSocket: persistent connection, one message round-trip per iteration.
    Ws,
}

/// A complete, runnable description of a benchmark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scenario {
    /// Human-facing label shown in the TUI / reports.
    pub name: String,
    /// Transport strategy (`http` or `ws`).
    pub protocol: ProtocolKind,
    /// Template variables, substituted into the request before each run.
    pub vars: BTreeMap<String, String>,
    /// How much load to apply.
    pub load: LoadSpec,
    /// The request to issue every iteration (single-step shorthand). Ignored
    /// when `steps` is non-empty.
    pub request: HttpRequestSpec,
    /// Expectations the single-step `request` must meet (YAML key: `assert`).
    #[serde(rename = "assert")]
    pub asserts: Assertions,
    /// A multi-step flow. When non-empty, this supersedes `request`/`assert`.
    pub steps: Vec<Step>,
}

impl Default for Scenario {
    fn default() -> Self {
        Self {
            name: "unnamed".to_owned(),
            protocol: ProtocolKind::Http,
            vars: BTreeMap::new(),
            load: LoadSpec::default(),
            request: HttpRequestSpec::default(),
            asserts: Assertions::default(),
            steps: Vec::new(),
        }
    }
}

impl Scenario {
    /// Parse and validate a scenario from a YAML document.
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        let scenario: Self = serde_yaml::from_str(yaml)?;
        scenario.validate()?;
        Ok(scenario)
    }

    /// The effective flow: the explicit `steps`, or the single `request`
    /// wrapped as a one-step flow. Always non-empty for a valid scenario.
    pub fn steps(&self) -> Vec<Step> {
        if self.steps.is_empty() {
            vec![Step {
                name: String::new(),
                request: self.request.clone(),
                asserts: self.asserts.clone(),
                extract: Vec::new(),
            }]
        } else {
            self.steps.clone()
        }
    }

    /// Reject semantically broken scenarios early, with a clear message.
    pub fn validate(&self) -> Result<()> {
        for (i, step) in self.steps().iter().enumerate() {
            if step.request.url.trim().is_empty() {
                return Err(ScenarioError::Invalid(format!(
                    "step {i}: request.url must not be empty"
                )));
            }
        }
        if self.load.concurrency == 0 {
            return Err(ScenarioError::Invalid("load.concurrency must be >= 1".into()));
        }
        Ok(())
    }

    /// The request with every `{{ var }}` resolved from [`Scenario::vars`].
    /// Variables the run doesn't define are left intact for visibility.
    pub fn resolved(&self) -> HttpRequestSpec {
        let mut ctx = MapContext::new();
        for (k, v) in &self.vars {
            ctx.set(k, v);
        }
        self.request.interpolate(&ctx)
    }
}

/// Fluent builder for the CLI path (Builder pattern).
///
/// ```
/// use bgm_scenario::ScenarioBuilder;
/// use std::time::Duration;
///
/// let scenario = ScenarioBuilder::new("smoke")
///     .get("https://example.com")
///     .concurrency(10)
///     .duration(Duration::from_secs(30))
///     .var("token", "secret")
///     .build()
///     .unwrap();
/// assert_eq!(scenario.load.concurrency, 10);
/// ```
#[derive(Debug, Clone)]
pub struct ScenarioBuilder {
    scenario: Scenario,
}

impl ScenarioBuilder {
    /// Start a builder with the given run name.
    pub fn new(name: impl Into<String>) -> Self {
        Self { scenario: Scenario { name: name.into(), ..Scenario::default() } }
    }

    /// Set a `GET` request to `url`.
    pub fn get(mut self, url: impl Into<String>) -> Self {
        self.scenario.request = HttpRequestSpec::get(url);
        self
    }

    /// Replace the request specification wholesale.
    pub fn request(mut self, request: HttpRequestSpec) -> Self {
        self.scenario.request = request;
        self
    }

    /// Target concurrency.
    pub fn concurrency(mut self, n: usize) -> Self {
        self.scenario.load.concurrency = n;
        self
    }

    /// Time bound for the run.
    pub fn duration(mut self, d: Duration) -> Self {
        self.scenario.load.duration = Some(d);
        self
    }

    /// Iteration bound for the run.
    pub fn iterations(mut self, n: u64) -> Self {
        self.scenario.load.iterations = Some(n);
        self
    }

    /// Add or overwrite a template variable.
    pub fn var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.scenario.vars.insert(key.into(), value.into());
        self
    }

    /// Replace the load specification wholesale.
    pub fn load(mut self, load: LoadSpec) -> Self {
        self.scenario.load = load;
        self
    }

    /// Validate and produce the [`Scenario`].
    pub fn build(self) -> Result<Scenario> {
        self.scenario.validate()?;
        Ok(self.scenario)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_yaml_and_resolves_vars() {
        let yaml = r#"
name: api-smoke
vars:
  host: example.com
load:
  concurrency: 25
  duration: 30s
  profile: ramp_up
  ramp: 5s
request:
  method: GET
  url: https://{{ host }}/health
"#;
        let s = Scenario::from_yaml(yaml).unwrap();
        assert_eq!(s.name, "api-smoke");
        assert_eq!(s.load.concurrency, 25);
        assert_eq!(s.load.duration, Some(Duration::from_secs(30)));
        assert_eq!(s.resolved().url, "https://example.com/health");
    }

    #[test]
    fn empty_url_is_rejected() {
        let err = ScenarioBuilder::new("bad").build().unwrap_err();
        assert!(matches!(err, ScenarioError::Invalid(_)));
    }
}
