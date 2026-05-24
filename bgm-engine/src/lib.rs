//! # bgm-engine
//!
//! The runtime that turns a [`Protocol`](bgm_core::Protocol) plus a
//! [`LoadSpec`](bgm_scenario::LoadSpec) into actual traffic. [`EngineBuilder`]
//! composes an [`Engine`]; [`Engine::run`] spawns a tokio worker pool and
//! streams [`IterReport`](bgm_core::IterReport)s back over an `mpsc` channel
//! (Observer), stopping on its duration/iteration budget or a
//! [`CancellationToken`](tokio_util::sync::CancellationToken).
//!
//! Load curves live behind the [`LoadProfile`](bgm_core::LoadProfile) strategy
//! in [`profiles`]; the engine only ever asks "how many workers now?".

pub mod engine;
pub mod profiles;

pub use engine::{Engine, EngineBuilder};
pub use profiles::{ConstantLoad, RampUpLoad, StepsLoad, profile_from};
