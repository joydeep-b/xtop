use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::time::Duration;

/// User actions derived from key input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    TogglePause,
    OpenProfiles,
    MoveProfileUp,
    MoveProfileDown,
    SelectProfile,
    Cancel,
    None,
}

/// Poll for an input event up to `timeout`. Returns the mapped action, or
/// `Action::None` if there was no actionable input within the window.
pub fn poll(timeout: Duration) -> Result<Action> {
    if !event::poll(timeout)? {
        return Ok(Action::None);
    }
    match event::read()? {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            let action = match key.code {
                KeyCode::Char('q') => Action::Quit,
                KeyCode::Esc => Action::Cancel,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
                KeyCode::Char(' ') | KeyCode::Char('p') => Action::TogglePause,
                KeyCode::Char('l') => Action::OpenProfiles,
                KeyCode::Up | KeyCode::Char('k') => Action::MoveProfileUp,
                KeyCode::Down | KeyCode::Char('j') => Action::MoveProfileDown,
                KeyCode::Enter => Action::SelectProfile,
                _ => Action::None,
            };
            Ok(action)
        }
        _ => Ok(Action::None),
    }
}
