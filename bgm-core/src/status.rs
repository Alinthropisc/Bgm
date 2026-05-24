//! Outcome classification for a single iteration.

use serde::{Deserialize, Serialize};

/// Coarse category of an iteration outcome, protocol-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StatusKind {
    /// The iteration succeeded.
    Success,
    /// The peer reported a client-side problem (e.g. HTTP 4xx).
    ClientError,
    /// The peer reported a server-side problem (e.g. HTTP 5xx).
    ServerError,
    /// The iteration failed before/around a response (transport, timeout, …).
    Error,
}

impl StatusKind {
    /// `true` only for [`StatusKind::Success`].
    pub fn is_success(self) -> bool {
        matches!(self, StatusKind::Success)
    }
}

/// The outcome of one iteration: a [`StatusKind`] plus an optional numeric code
/// (e.g. the HTTP status). Kept tiny and `Copy` — millions of these flow
/// through the metrics channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Status {
    kind: StatusKind,
    code: i64,
}

impl Status {
    /// A successful outcome carrying a protocol code (use `0` if none).
    pub fn success(code: i64) -> Self {
        Self { kind: StatusKind::Success, code }
    }

    /// A client-side error outcome.
    pub fn client_error(code: i64) -> Self {
        Self { kind: StatusKind::ClientError, code }
    }

    /// A server-side error outcome.
    pub fn server_error(code: i64) -> Self {
        Self { kind: StatusKind::ServerError, code }
    }

    /// A transport/other error outcome (code is informational, often `0`).
    pub fn error(code: i64) -> Self {
        Self { kind: StatusKind::Error, code }
    }

    /// Build a [`Status`] from an HTTP status code using standard ranges.
    pub fn from_http(code: u16) -> Self {
        let code = i64::from(code);
        match code {
            100..=399 => Self::success(code),
            400..=499 => Self::client_error(code),
            500..=599 => Self::server_error(code),
            _ => Self::error(code),
        }
    }

    /// The outcome category.
    pub fn kind(self) -> StatusKind {
        self.kind
    }

    /// The numeric code (protocol specific; `0` when not applicable).
    pub fn code(self) -> i64 {
        self.code
    }

    /// Convenience: did this iteration succeed?
    pub fn is_success(self) -> bool {
        self.kind.is_success()
    }
}
