//! The HTTP request a scenario drives.
//!
//! Every text field can carry `{{ vars }}`; `#[derive(Interpolate)]` walks the
//! struct and resolves them against a [`Lookup`] context (see
//! [`crate::Scenario::resolved`]). The method is kept as a `String` rather than
//! an enum precisely so it, too, can be templated (`method: "{{ verb }}"`);
//! validation into a real verb happens later, in `bgm-protocols`.

use bgm_macros::Interpolate;
use serde::{Deserialize, Serialize};

/// One request header. A separate struct (rather than a map) so it can derive
/// [`Interpolate`] and preserve author-specified ordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Interpolate)]
pub struct Header {
    pub name: String,
    pub value: String,
}

impl Header {
    /// Construct a header from any string-like pair.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self { name: name.into(), value: value.into() }
    }
}

/// A single HTTP request specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Interpolate)]
#[serde(default)]
pub struct HttpRequestSpec {
    /// HTTP method, e.g. `GET`, `POST`. Templatable; validated downstream.
    pub method: String,
    /// Target URL.
    pub url: String,
    /// Request headers, in order.
    pub headers: Vec<Header>,
    /// Optional request body (raw string; JSON, form data, …).
    pub body: Option<String>,
}

impl Default for HttpRequestSpec {
    fn default() -> Self {
        Self {
            method: "GET".to_owned(),
            url: String::new(),
            headers: Vec::new(),
            body: None,
        }
    }
}

impl HttpRequestSpec {
    /// A bare `GET url` request.
    pub fn get(url: impl Into<String>) -> Self {
        Self { url: url.into(), ..Self::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bgm_core::{Interpolate, MapContext};

    #[test]
    fn interpolates_nested_fields() {
        let req = HttpRequestSpec {
            method: "GET".into(),
            url: "https://api/{{ host }}/users/{{ id }}".into(),
            headers: vec![Header::new("Authorization", "Bearer {{ token }}")],
            body: Some("{\"id\": {{ id }}}".into()),
        };
        let ctx = MapContext::new()
            .with("host", "example.com")
            .with("id", "7")
            .with("token", "abc");

        let out = req.interpolate(&ctx);
        assert_eq!(out.url, "https://api/example.com/users/7");
        assert_eq!(out.headers[0].value, "Bearer abc");
        assert_eq!(out.body.as_deref(), Some("{\"id\": 7}"));
    }
}
