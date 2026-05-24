//! Scenario parsing / validation errors.

use thiserror::Error;

/// Something went wrong turning text (YAML or flags) into a [`crate::Scenario`].
#[derive(Debug, Error)]
pub enum ScenarioError {
    /// The YAML was syntactically invalid or didn't match the schema.
    #[error("invalid scenario YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// The scenario parsed but is semantically invalid (e.g. empty URL).
    #[error("invalid scenario: {0}")]
    Invalid(String),
}

/// Convenience result for scenario operations.
pub type Result<T> = std::result::Result<T, ScenarioError>;
