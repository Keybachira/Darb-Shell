//! Shared widget helpers for the panels.
//!
//! Every panel is the same shape — titled block, content, empty marker —
//! so that shape lives here once (Contribuição §10) instead of being
//! re-typed in each component.

use darb_core::i18n::global_text;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders};

use crate::theme::Theme;

/// Bordered panel block. The focused panel gets the accent border so the
/// user can see where keys will land.
pub fn panel(title: String, focused: bool, theme: &Theme) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(theme.primary)
        } else {
            Style::default().fg(theme.border)
        })
}

/// The marker shown by a panel that has nothing to display.
pub fn empty_line(theme: &Theme) -> Line<'static> {
    Line::styled(global_text("panel.empty"), Style::default().fg(theme.muted))
}

/// Split captured output into ratatui lines.
pub fn text_lines(text: &str) -> Vec<Line<'static>> {
    text.lines()
        .map(|line| Line::raw(line.to_string()))
        .collect()
}
