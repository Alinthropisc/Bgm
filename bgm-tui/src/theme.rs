//! One place for the dashboard's palette, so the look stays coherent and is
//! trivial to retheme later.

use ratatui::style::Color;

/// Brand accent (BGM) — used for the logo and progress.
pub const ACCENT: Color = Color::Rgb(0x5a, 0xd0, 0x8f);
/// Secondary accent for latency widgets.
pub const ACCENT_ALT: Color = Color::Rgb(0x46, 0xc6, 0xc6);
/// Healthy / success.
pub const OK: Color = Color::Rgb(0x6f, 0xd6, 0x7a);
/// Warnings (4xx).
pub const WARN: Color = Color::Rgb(0xe0, 0xbf, 0x5a);
/// Errors (5xx / transport).
pub const BAD: Color = Color::Rgb(0xe0, 0x6c, 0x6c);
/// Muted labels and chrome.
pub const MUTED: Color = Color::Rgb(0x8a, 0x8a, 0x8a);
/// Bar / sparkline fill.
pub const BAR: Color = Color::Rgb(0x4f, 0xb0, 0x77);
