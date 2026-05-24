//! Non-blocking keyboard handling for the dashboard.
//!
//! The render loop polls this once per tick with a zero timeout, so reading
//! input never stalls drawing. Keys map to a small [`Action`] vocabulary rather
//! than letting raw events leak into the loop (Command pattern, lightly).

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// A user intent decoded from a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Stop the run and exit.
    Quit,
    /// Reset the accumulated statistics.
    Clear,
    /// Toggle the help line.
    ToggleHelp,
}

/// Drain pending key events, returning the first actionable one (if any).
/// Never blocks: if nothing is queued it returns `Ok(None)` immediately.
pub fn poll() -> io::Result<Option<Action>> {
    let mut action = None;
    while event::poll(Duration::ZERO)? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                action = action.or_else(|| decode(key));
            }
        }
    }
    Ok(action)
}

fn decode(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('q' | 'Q') | KeyCode::Esc => Some(Action::Quit),
        // Ctrl-C also quits.
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        KeyCode::Char('c' | 'C') => Some(Action::Clear),
        KeyCode::Char('h' | 'H') | KeyCode::Char('?') => Some(Action::ToggleHelp),
        _ => None,
    }
}
