//! # bgm-protocols
//!
//! Concrete [`Protocol`](bgm_core::Protocol) strategies: [`HttpProtocol`] over
//! `reqwest` and [`WsProtocol`] (WebSocket message round-trips) over
//! `tokio-tungstenite`. The engine depends only on the `Protocol` trait, so
//! adding a gRPC/TCP/SQL strategy later means a new type here and nothing
//! changes upstream (Open/Closed).
//!
//! Each protocol is built through a Factory (`from_scenario`) that validates
//! the static request up front and defers dynamic-template validation to the
//! first iteration.

pub mod config;
pub mod protocol;
pub mod template;
pub mod ws;

pub use config::HttpClientConfig;
pub use protocol::HttpProtocol;
pub use template::RequestTemplate;
pub use ws::{WsConfig, WsProtocol};
