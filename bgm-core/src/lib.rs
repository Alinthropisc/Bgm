//! # bgm-core
//!
//! The dependency-light heart of BGM. It defines the *vocabulary* every other
//! crate speaks — domain types ([`Status`], [`IterReport`], [`IterInfo`]),
//! the [`Protocol`] and [`LoadProfile`] strategy traits, the [`Interpolate`]
//! template machinery, and the shared error type.
//!
//! Keeping this crate free of `tokio`/`reqwest`/`ratatui` is deliberate
//! (Dependency Inversion): high-level policy lives here, concrete mechanisms
//! live in the satellite crates and depend *inward* on these abstractions.

pub mod error;
pub mod interpolate;
pub mod report;
pub mod status;
pub mod traits;

pub use error::{BgmError, IterError, IterResult, Result};
pub use interpolate::{Interpolate, Lookup, MapContext};
pub use report::{IterInfo, IterReport};
pub use status::{Status, StatusKind};
pub use traits::{LoadProfile, Protocol};
