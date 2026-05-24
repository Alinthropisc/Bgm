//! # bgm-scenario
//!
//! The declarative model of *what to run*: an [`HttpRequestSpec`] plus a
//! [`LoadSpec`], bundled into a [`Scenario`]. Two adapters build it — YAML
//! ([`Scenario::from_yaml`]) and the fluent [`ScenarioBuilder`] for CLI flags —
//! and [`Scenario::resolved`] applies `{{ var }}` interpolation.
//!
//! Depends only on [`bgm_core`] and `bgm-macros`: no runtime, no I/O beyond
//! parsing. `bgm-engine` consumes the [`LoadSpec`]; `bgm-protocols` consumes the
//! resolved [`HttpRequestSpec`].

pub mod assert;
pub mod error;
pub mod http;
pub mod load;
pub mod scenario;
pub mod step;

pub use assert::Assertions;
pub use error::{Result, ScenarioError};
pub use http::{Header, HttpRequestSpec};
pub use load::{LoadSpec, Profile};
pub use scenario::{ProtocolKind, Scenario, ScenarioBuilder};
pub use step::{Extract, ExtractFrom, Step};
