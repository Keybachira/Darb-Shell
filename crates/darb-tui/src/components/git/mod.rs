//! Git panel: porcelain status lines, coloured by status code.
//!
//! Read-only. Anything that mutates the repository stays behind the
//! permission-gated tools (Contribuição §28) — this panel only displays.

use darb_core::i18n::global_text;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;
use crate::widgets;

pub fn render_git(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let lines: Vec<Line> = if app.git_lines.is_empty() {
        vec![Line::styled(
            global_text("git.empty"),
            Style::default().fg(theme.muted),
        )]
    } else {
        app.git_lines
            .iter()
            .map(|line| status_line(line, theme))
            .collect()
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(widgets::panel(
                global_text("panel.git"),
                app.workspace_focused(),
                theme,
            ))
            .scroll((app.scroll, 0)),
        area,
    );
}

/// `git status --porcelain=v1 --branch` lines: `## branch`, `?? untracked`,
/// `XY path`. The code column decides the colour.
fn status_line(raw: &str, theme: &Theme) -> Line<'static> {
    let style = if raw.starts_with("##") {
        Style::default().fg(theme.primary)
    } else if raw.starts_with("??") {
        Style::default().fg(theme.muted)
    } else if raw.starts_with('D') || raw.contains(" D ") {
        Style::default().fg(theme.danger)
    } else if raw.starts_with('A') {
        Style::default().fg(theme.success)
    } else {
        Style::default().fg(theme.warning)
    };
    Line::styled(raw.to_string(), style)
}
