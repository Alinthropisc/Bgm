//! The request template the protocol expands on every iteration.
//!
//! Static `{{ vars }}` from the scenario are baked in once at construction.
//! What may remain are *dynamic* placeholders that depend on the iteration —
//! `{{ seq }}`, `{{ worker_id }}`, `{{ worker_seq }}` — useful for generating
//! unique payloads. [`RequestTemplate::render`] resolves those per iteration via
//! [`IterContext`]; when no placeholders survive, it short-circuits (the common,
//! fast path) so steady-state runs don't re-interpolate every time.

use bgm_core::{Interpolate, IterInfo, Lookup};
use bgm_scenario::HttpRequestSpec;

/// A request with static vars already applied, plus a flag for whether any
/// dynamic placeholders remain.
#[derive(Debug, Clone)]
pub struct RequestTemplate {
    spec: HttpRequestSpec,
    dynamic: bool,
}

impl RequestTemplate {
    /// Bake in static vars and detect remaining dynamic placeholders.
    pub fn new(resolved: HttpRequestSpec) -> Self {
        let dynamic = spec_has_placeholder(&resolved);
        Self { spec: resolved, dynamic }
    }

    /// `true` when the template still contains `{{ … }}` to expand per iteration.
    pub fn is_dynamic(&self) -> bool {
        self.dynamic
    }

    /// The request for this iteration. Borrowed when static, freshly
    /// interpolated when dynamic — callers should treat the return as owned.
    pub fn render(&self, info: &IterInfo) -> std::borrow::Cow<'_, HttpRequestSpec> {
        if self.dynamic {
            std::borrow::Cow::Owned(self.spec.interpolate(&IterContext { info }))
        } else {
            std::borrow::Cow::Borrowed(&self.spec)
        }
    }
}

/// Cheap structural check for a surviving `{{` in any text field.
pub(crate) fn spec_has_placeholder(spec: &HttpRequestSpec) -> bool {
    let in_str = |s: &str| s.contains("{{");
    in_str(&spec.method)
        || in_str(&spec.url)
        || spec.body.as_deref().is_some_and(in_str)
        || spec.headers.iter().any(|h| in_str(&h.name) || in_str(&h.value))
}

/// [`Lookup`] exposing per-iteration counters as template variables.
struct IterContext<'a> {
    info: &'a IterInfo,
}

impl Lookup for IterContext<'_> {
    fn lookup(&self, key: &str) -> Option<String> {
        match key {
            "seq" | "runner_seq" => Some(self.info.runner_seq.to_string()),
            "worker_seq" => Some(self.info.worker_seq.to_string()),
            "worker_id" => Some(self.info.worker_id.to_string()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_template_is_borrowed_and_unchanged() {
        let t = RequestTemplate::new(HttpRequestSpec::get("https://x/health"));
        assert!(!t.is_dynamic());
        let info = IterInfo { worker_id: 3, worker_seq: 1, runner_seq: 7 };
        assert!(matches!(t.render(&info), std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn dynamic_template_expands_iter_counters() {
        let spec = HttpRequestSpec::get("https://x/item/{{ seq }}?w={{ worker_id }}");
        let t = RequestTemplate::new(spec);
        assert!(t.is_dynamic());
        let info = IterInfo { worker_id: 3, worker_seq: 1, runner_seq: 7 };
        assert_eq!(t.render(&info).url, "https://x/item/7?w=3");
    }
}
