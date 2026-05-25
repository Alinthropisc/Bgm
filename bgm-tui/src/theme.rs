//! One place for the dashboard's palette, so the look stays coherent and is
//! trivial to retheme later. Includes height-based gradient helpers so bars can
//! glow from calm to hot, like the reference dashboard.

use ratatui::style::Color;

/// Brand accent (BGM) — bright green, used for the logo and progress.
pub const ACCENT: Color = Color::Rgb(0x6f, 0xe6, 0x9f);
/// Secondary accent — bright teal, for latency widgets.
pub const ACCENT_ALT: Color = Color::Rgb(0x5a, 0xe0, 0xe0);
/// Healthy / success.
pub const OK: Color = Color::Rgb(0x7c, 0xe8, 0x88);
/// Warnings (4xx).
pub const WARN: Color = Color::Rgb(0xf0, 0xcf, 0x5a);
/// Errors (5xx / transport).
pub const BAD: Color = Color::Rgb(0xf0, 0x76, 0x76);
/// Muted labels and chrome.
pub const MUTED: Color = Color::Rgb(0x9a, 0x9a, 0x9a);
/// Ink for text on a bright brand block.
pub const INK: Color = Color::Rgb(0x0d, 0x0f, 0x12);

type Rgb = (u8, u8, u8);

fn lerp(a: u8, b: u8, t: f64) -> u8 {
    (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round() as u8
}

fn mix(from: Rgb, to: Rgb, t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::Rgb(
        lerp(from.0, to.0, t),
        lerp(from.1, to.1, t),
        lerp(from.2, to.2, t),
    )
}

/// "More is good" gradient (RPS / throughput): deep green → bright lime as the
/// ratio `t` (0..=1) climbs.
pub fn cool(t: f64) -> Color {
    mix((0x2e, 0x8b, 0x57), (0xb6, 0xf5, 0x6a), t)
}

/// "More is bad" gradient (latency): green → amber → red as `t` climbs.
pub fn heat(t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        mix((0x5a, 0xd0, 0x8f), (0xf0, 0xcf, 0x5a), t * 2.0)
    } else {
        mix((0xf0, 0xcf, 0x5a), (0xf0, 0x6c, 0x6c), (t - 0.5) * 2.0)
    }
}
