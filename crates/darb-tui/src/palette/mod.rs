//! Command palette (Ctrl+K): a filtered list of actions.
//!
//! Presentation only (Contribuição §26): the palette decides *which id*
//! the user picked and hands it back as a `KeyOutcome::Command`. Knowing
//! what a command does — reloading files, running git — belongs to
//! `apps/darb`, which owns the tool registry.

use darb_core::i18n::global_text;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::Tab;
use crate::layout::centered_rect;
use crate::theme::Theme;

/// Popup geometry for `match_count` filtered commands, shared by the
/// renderer and mouse hit-testing (Contribuição §10): clicks must hit the
/// rows actually drawn, so both sides use this one function.
pub fn popup_rect(area: Rect, match_count: usize) -> Rect {
    let height = (match_count as u16 + 5).min(area.height).max(6);
    centered_rect(area, 64, height)
}

/// What the user asked for. Deliberately tiny: every variant maps to one
/// action `apps/darb` can perform without inventing new machinery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    ToggleExplorer,
    RefreshFiles,
    RefreshGit,
    ShowDiff,
    NewChat,
    ToggleLanguage,
    OpenTab(Tab),
    Quit,
}

/// Static catalog, in palette order: `(command, i18n label key)`.
pub const COMMANDS: &[(Command, &str)] = &[
    (Command::OpenTab(Tab::Chat), "cmd.open_chat"),
    (Command::OpenTab(Tab::Files), "cmd.open_files"),
    (Command::OpenTab(Tab::Changes), "cmd.open_changes"),
    (Command::OpenTab(Tab::Git), "cmd.open_git"),
    (Command::OpenTab(Tab::Terminal), "cmd.open_terminal"),
    (Command::OpenTab(Tab::Tasks), "cmd.open_tasks"),
    (Command::RefreshFiles, "cmd.refresh_files"),
    (Command::RefreshGit, "cmd.git_status"),
    (Command::ShowDiff, "cmd.show_diff"),
    (Command::NewChat, "cmd.new_chat"),
    (Command::ToggleExplorer, "cmd.toggle_explorer"),
    (Command::ToggleLanguage, "cmd.toggle_language"),
    (Command::Quit, "cmd.quit"),
];

/// Palette state: the query typed so far and the highlighted row.
#[derive(Debug, Clone, Default)]
pub struct Palette {
    pub query: String,
    pub index: usize,
}

impl Palette {
    pub fn new() -> Self {
        Self::default()
    }

    /// Commands matching the query, in catalog order. Matching happens on
    /// the *translated* label, so it works the same in pt and en.
    pub fn filtered(&self) -> Vec<(Command, &'static str)> {
        let query = self.query.trim().to_lowercase();
        COMMANDS
            .iter()
            .copied()
            .filter(|(_, key)| query.is_empty() || global_text(key).to_lowercase().contains(&query))
            .collect()
    }

    /// Currently highlighted command, or `None` when nothing matches.
    pub fn selected(&self) -> Option<Command> {
        let matches = self.filtered();
        matches
            .get(self.index.min(matches.len().saturating_sub(1)))
            .map(|(command, _)| *command)
    }

    /// Move the highlight, clamped to the filtered list (wrapping around).
    pub fn move_selection(&mut self, delta: i32) {
        let len = self.filtered().len();
        if len == 0 {
            self.index = 0;
            return;
        }
        let next = self.index as i32 + delta;
        self.index = next.rem_euclid(len as i32) as usize;
    }

    pub fn push_char(&mut self, c: char) {
        self.query.push(c);
        self.index = 0;
    }

    pub fn pop_char(&mut self) {
        self.query.pop();
        self.index = 0;
    }
}

/// Draw the palette centered over `area`.
pub fn render_palette(frame: &mut Frame, area: Rect, palette: &Palette, theme: &Theme) {
    let matches = palette.filtered();
    let popup = popup_rect(area, matches.len());
    frame.render_widget(Clear, popup);

    let header = Line::from(format!("> {}", palette.query));
    let hint = Line::styled(
        global_text("palette.hint"),
        Style::default().fg(theme.muted),
    );

    let mut lines = vec![header, Line::from("")];
    if matches.is_empty() {
        lines.push(Line::styled(
            global_text("palette.no_match"),
            Style::default().fg(theme.muted),
        ));
    } else {
        for (position, (_, key)) in matches.iter().enumerate() {
            let label = global_text(key);
            let style = if position == palette.index.min(matches.len() - 1) {
                Style::default().fg(theme.highlight)
            } else {
                Style::default().fg(theme.text)
            };
            lines.push(Line::styled(format!("  {label}"), style));
        }
    }
    lines.push(hint);

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(global_text("palette.title"))
                .borders(Borders::ALL)
                .border_style(theme.warning),
        ),
        popup,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Tab;

    #[test]
    fn empty_query_lists_every_command() {
        let palette = Palette::new();
        assert_eq!(palette.filtered().len(), COMMANDS.len());
        assert_eq!(palette.selected(), Some(Command::OpenTab(Tab::Chat)));
    }

    #[test]
    fn query_filters_on_the_translated_label() {
        let mut palette = Palette::new();
        // "env" is not in any label; "dif" is (pt: "Ver alterações (diff)").
        for c in "diff".chars() {
            palette.push_char(c);
        }
        let matches = palette.filtered();
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(palette.selected(), Some(Command::ShowDiff));
    }

    #[test]
    fn selection_wraps_and_clamps_on_empty_results() {
        let mut palette = Palette::new();
        palette.move_selection(-1);
        assert_eq!(palette.index, COMMANDS.len() - 1);
        for c in "zzzz".chars() {
            palette.push_char(c);
        }
        assert!(palette.filtered().is_empty());
        assert_eq!(palette.selected(), None);
        palette.move_selection(3);
        assert_eq!(palette.index, 0);
        palette.pop_char();
        assert_eq!(palette.query, "zzz");
    }

    #[test]
    fn every_catalog_key_is_translated() {
        // A label key that is missing would render as the raw key.
        for (_, key) in COMMANDS {
            let label = global_text(key);
            assert_ne!(label, *key, "{key}");
        }
    }
}
