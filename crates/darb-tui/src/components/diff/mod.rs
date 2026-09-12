//! Changes panel: unified diff output, coloured by line prefix.
//!
//! The colouring is presentation only — the diff itself comes from the
//! `git_diff` tool, fed in by `apps/darb`.

use darb_core::i18n::global_text;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;
use crate::widgets;

pub fn render_diff(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let lines: Vec<Line> = if app.diff_lines.is_empty() {
        vec![widgets::empty_line(theme)]
    } else {
        app.diff_lines
            .iter()
            .map(|line| diff_line(line, theme))
            .collect()
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(widgets::panel(
                global_text("panel.changes"),
                app.workspace_focused(),
                theme,
            ))
            .scroll((app.scroll, 0)),
        area,
    );
}

/// Style one diff line by its prefix: additions, removals, hunk headers,
/// file headers, and context.
fn diff_line(raw: &str, theme: &Theme) -> Line<'static> {
    let style = if raw.starts_with("+++") || raw.starts_with("---") {
        Style::default().fg(theme.muted)
    } else if raw.starts_with('+') {
        Style::default().fg(theme.success)
    } else if raw.starts_with('-') {
        Style::default().fg(theme.danger)
    } else if raw.starts_with("@@") {
        Style::default().fg(theme.primary)
    } else if raw.starts_with("diff --git") {
        Style::default().fg(theme.warning)
    } else {
        Style::default().fg(theme.text)
    };
    Line::styled(raw.to_string(), style)
}
