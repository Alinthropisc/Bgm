//! Multi-step flows.
//!
//! A scenario may run a *sequence* of requests per iteration instead of a single
//! one, threading data between them: each [`Step`] can [`Extract`] values from
//! its response into the iteration's variable context, which later steps
//! interpolate via `{{ var }}`. This is the classic login-then-act flow.
//!
//! The model here is pure data; `bgm-protocols` executes it. A single-`request`
//! scenario is just the one-step case (see `Scenario::steps`).

use serde::{Deserialize, Serialize};

use crate::assert::Assertions;
use crate::http::HttpRequestSpec;

/// One request in a flow, with its own checks and value extractions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Step {
    /// Optional label for diagnostics.
    pub name: String,
    /// The request this step issues.
    pub request: HttpRequestSpec,
    /// Per-step response checks (YAML key: `assert`).
    #[serde(rename = "assert")]
    pub asserts: Assertions,
    /// Values to pull from the response into the variable context.
    pub extract: Vec<Extract>,
}

impl Default for Step {
    fn default() -> Self {
        Self {
            name: String::new(),
            request: HttpRequestSpec::default(),
            asserts: Assertions::default(),
            extract: Vec::new(),
        }
    }
}

/// Bind a response value to a variable name for use by later steps.
///
/// YAML:
/// ```yaml
/// extract:
///   - { var: token,  json: /data/token }
///   - { var: reqid,  header: X-Request-Id }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extract {
    /// Variable name to bind (referenced later as `{{ var }}`).
    pub var: String,
    /// Where the value comes from.
    #[serde(flatten)]
    pub from: ExtractFrom,
}

/// The source of an extracted value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtractFrom {
    /// A JSON Pointer (RFC 6901) into the response body, e.g. `/data/token`.
    Json(String),
    /// A response header name, e.g. `X-Request-Id`.
    Header(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_step_with_extracts() {
        let yaml = r#"
name: login
request:
  method: POST
  url: https://api/login
assert:
  status: 200
extract:
  - { var: token, json: /token }
  - { var: rid, header: X-Request-Id }
"#;
        let step: Step = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(step.name, "login");
        assert_eq!(step.asserts.status, Some(200));
        assert_eq!(step.extract.len(), 2);
        assert_eq!(step.extract[0].from, ExtractFrom::Json("/token".into()));
        assert_eq!(
            step.extract[1].from,
            ExtractFrom::Header("X-Request-Id".into())
        );
    }
}
