//! Terminal panel: output of commands the user ran from the TUI.
//!
//! The panel does not execute anything (Contribuição §26, §28). Commands
//! travel to `apps/darb` as `KeyOutcome::TerminalCommand`, go through the
//! permission-gated registry, and the captured output is pushed back here.

use darb_core::i18n::global_text;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;
use crate::widgets;

pub fn render_terminal(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let mut lines: Vec<Line> = if app.terminal_lines.is_empty() {
        vec![widgets::empty_line(theme)]
    } else {
        app.terminal_lines
            .iter()
            .map(|line| terminal_line(line, theme))
            .collect()
    };
    lines.push(Line::from(""));
    lines.push(Line::styled(
        global_text("terminal.hint"),
        Style::default().fg(theme.muted),
    ));
    frame.render_widget(
        Paragraph::new(lines)
            .block(widgets::panel(
                global_text("panel.terminal"),
                app.workspace_focused(),
                theme,
            ))
            .scroll((app.scroll, 0)),
        area,
    );
}

/// `$ command` echoes are accented; everything else is plain capture.
fn terminal_line(raw: &str, theme: &Theme) -> Line<'static> {
    let style = if raw.starts_with("$ ") {
        Style::default().fg(theme.primary)
    } else {
        Style::default().fg(theme.text)
    };
    Line::styled(raw.to_string(), style)
}
