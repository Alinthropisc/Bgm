//! `{{ variable }}` template substitution.
//!
//! [`Interpolate`] is the contract; `#[derive(Interpolate)]` (in `bgm-macros`)
//! generates the recursive field walk for structs. The leaf implementation
//! lives on [`String`]: it scans for `{{ key }}` spans and replaces them using
//! a [`Lookup`] context. Unknown keys are left verbatim so a half-resolved
//! template is debuggable rather than silently blanked.

use std::collections::HashMap;

/// A source of values for template substitution.
pub trait Lookup {
    /// Resolve `key` to a string value, or `None` if unknown.
    fn lookup(&self, key: &str) -> Option<String>;
}

/// Types that can produce a copy of themselves with `{{ vars }}` resolved.
pub trait Interpolate {
    /// Return `self` with every embedded `{{ key }}` replaced via `ctx`.
    fn interpolate(&self, ctx: &dyn Lookup) -> Self;
}

impl Interpolate for String {
    fn interpolate(&self, ctx: &dyn Lookup) -> Self {
        interpolate_str(self, ctx)
    }
}

impl<T: Interpolate> Interpolate for Option<T> {
    fn interpolate(&self, ctx: &dyn Lookup) -> Self {
        self.as_ref().map(|v| v.interpolate(ctx))
    }
}

impl<T: Interpolate> Interpolate for Vec<T> {
    fn interpolate(&self, ctx: &dyn Lookup) -> Self {
        self.iter().map(|v| v.interpolate(ctx)).collect()
    }
}

/// Implement [`Interpolate`] as a no-op (clone) for types that never contain
/// templates — numbers, bools, enums, durations and the like.
#[macro_export]
macro_rules! impl_interpolate_identity {
    ($($t:ty),* $(,)?) => {
        $(
            impl $crate::Interpolate for $t {
                fn interpolate(&self, _ctx: &dyn $crate::Lookup) -> Self {
                    self.clone()
                }
            }
        )*
    };
}

impl_interpolate_identity!(bool, u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, f32, f64);

/// Replace every `{{ key }}` in `input`. Whitespace inside the braces is
/// trimmed, so `{{ id }}` and `{{id}}` are equivalent. A `{{` without a
/// matching `}}` is emitted unchanged.
pub fn interpolate_str(input: &str, ctx: &dyn Lookup) -> String {
    // Fast path: nothing to do.
    if !input.contains("{{") {
        return input.to_owned();
    }

    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after_open = &rest[open + 2..];
        match after_open.find("}}") {
            Some(close) => {
                let key = after_open[..close].trim();
                match ctx.lookup(key) {
                    Some(val) => out.push_str(&val),
                    None => {
                        // Leave the placeholder intact for visibility.
                        out.push_str("{{");
                        out.push_str(&after_open[..close]);
                        out.push_str("}}");
                    }
                }
                rest = &after_open[close + 2..];
            }
            None => {
                // Unterminated; emit the remainder literally and stop.
                out.push_str("{{");
                out.push_str(after_open);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// A simple [`Lookup`] backed by a string map. Handy for tests and for the
/// CLI's `--var key=value` flags.
#[derive(Debug, Default, Clone)]
pub struct MapContext {
    vars: HashMap<String, String>,
}

impl MapContext {
    /// Empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or overwrite a variable. Returns `self` for chaining (Builder).
    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.vars.insert(key.into(), value.into());
        self
    }

    /// Insert or overwrite a variable in place.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(key.into(), value.into());
    }
}

impl Lookup for MapContext {
    fn lookup(&self, key: &str) -> Option<String> {
        self.vars.get(key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_known_keys_and_keeps_unknown() {
        let ctx = MapContext::new().with("id", "42");
        assert_eq!(interpolate_str("/users/{{ id }}", &ctx), "/users/42");
        assert_eq!(interpolate_str("/x/{{ id }}/{{ q }}", &ctx), "/x/42/{{ q }}");
    }

    #[test]
    fn handles_no_placeholder_and_unterminated() {
        let ctx = MapContext::new();
        assert_eq!(interpolate_str("/plain", &ctx), "/plain");
        assert_eq!(interpolate_str("a {{ oops", &ctx), "a {{ oops");
    }
}
