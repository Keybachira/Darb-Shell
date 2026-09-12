//! Keybindings: raw keys → [`Action`]. Pure mapping, no state.
//!
//! Character input is NOT an action: when [`Action::Ignore`] comes back
//! with a `KeyCode::Char` (no modifiers), the app inserts the character
//! into the input line — unless a modal dialog or the command palette is
//! open, which capture keys themselves.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::Tab;

/// What a key press means. Shortcuts follow Arquitetura §59.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Submit,
    CancelRequested,
    FocusNext,
    ScrollUp,
    ScrollDown,
    ToggleExplorer,
    CommandPalette,
    ShowTab(Tab),
    PermissionAllow,
    PermissionDeny,
    Ignore,
}

pub fn action_for(key: KeyEvent) -> Action {
    // Ctrl combinations first: they win over plain characters.
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') | KeyCode::Char('C') => Action::CancelRequested,
            KeyCode::Char('b') | KeyCode::Char('B') => Action::ToggleExplorer,
            KeyCode::Char('k') | KeyCode::Char('K') => Action::CommandPalette,
            KeyCode::Char('p') | KeyCode::Char('P') => Action::ShowTab(Tab::Files),
            KeyCode::Char('g') | KeyCode::Char('G') => Action::ShowTab(Tab::Git),
            KeyCode::Char('t') | KeyCode::Char('T') => Action::ShowTab(Tab::Terminal),
            _ => Action::Ignore,
        };
    }
    if key.modifiers != KeyModifiers::NONE {
        return Action::Ignore;
    }
    match key.code {
        KeyCode::Esc => Action::Quit,
        KeyCode::Enter => Action::Submit,
        KeyCode::Tab => Action::FocusNext,
        KeyCode::Up => Action::ScrollUp,
        KeyCode::Down => Action::ScrollDown,
        KeyCode::Char('a') | KeyCode::Char('A') => Action::PermissionAllow,
        KeyCode::Char('d') | KeyCode::Char('D') => Action::PermissionDeny,
        _ => Action::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn maps_special_keys() {
        assert_eq!(action_for(key(KeyCode::Esc)), Action::Quit);
        assert_eq!(action_for(key(KeyCode::Enter)), Action::Submit);
        assert_eq!(action_for(key(KeyCode::Tab)), Action::FocusNext);
        assert_eq!(action_for(key(KeyCode::Up)), Action::ScrollUp);
        assert_eq!(
            action_for(ctrl(KeyCode::Char('c'))),
            Action::CancelRequested
        );
        assert_eq!(action_for(ctrl(KeyCode::Char('b'))), Action::ToggleExplorer);
        assert_eq!(action_for(ctrl(KeyCode::Char('k'))), Action::CommandPalette);
    }

    #[test]
    fn ctrl_letters_open_tabs() {
        assert_eq!(
            action_for(ctrl(KeyCode::Char('p'))),
            Action::ShowTab(Tab::Files)
        );
        assert_eq!(
            action_for(ctrl(KeyCode::Char('g'))),
            Action::ShowTab(Tab::Git)
        );
        assert_eq!(
            action_for(ctrl(KeyCode::Char('t'))),
            Action::ShowTab(Tab::Terminal)
        );
    }

    #[test]
    fn plain_characters_are_input_not_actions() {
        // 'q' must not quit while typing: only Esc quits.
        assert_eq!(action_for(key(KeyCode::Char('q'))), Action::Ignore);
        assert_eq!(action_for(key(KeyCode::Backspace)), Action::Ignore);
    }
}
