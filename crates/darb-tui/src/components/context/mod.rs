//! Context panel (right sidebar, AI context): token budget with gauge,
//! per-call tokens, session stats, files, tools and model. Every value
//! is real state fed by `apps/darb`; unknown numbers render as `—`,
//! never as guesses (Contribuição §41).

use darb_core::i18n::global_text;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;
use crate::widgets;

pub fn render_context(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let mut lines: Vec<Line> = Vec::with_capacity(area.height as usize);

    lines.push(widgets::section_header(
        &global_text("context.session"),
        theme,
    ));
    if app.token_prompt > 0 {
        lines.push(widgets::kv_line(
            &global_text("context.tokens"),
            format_token(app.token_prompt),
            theme,
        ));
        lines.push(widgets::kv_line(
            &global_text("context.last_call"),
            format_token(app.token_completion),
            theme,
        ));
    } else {
        lines.push(Line::styled(
            global_text("panel.empty"),
            Style::default().fg(theme.muted),
        ));
    }
    if !app.model_label.is_empty() {
        lines.push(widgets::kv_line(
            &global_text("context.model"),
            app.model_label.clone(),
            theme,
        ));
    }
    // The router's reasoning, on its own line and dimmed: a model name
    // with no justification is the thing P2 set out to fix, so the reason
    // is shown next to it rather than buried in a log.
    if !app.route_note.is_empty() {
        lines.push(Line::styled(
            format!("  {}", app.route_note),
            Style::default().fg(theme.faint),
        ));
    }
    if app.tool_calls > 0 {
        lines.push(widgets::kv_line(
            &global_text("app.tools"),
            app.tool_calls.to_string(),
            theme,
        ));
    }

    let git = &app.workspace.git;
    if !git.branch.is_empty() || git.changes + git.staged + git.untracked > 0 {
        lines.push(widgets::section_header(
            &global_text("context.workspace"),
            theme,
        ));
        if !git.branch.is_empty() {
            lines.push(widgets::kv_line("branch", git.branch.clone(), theme));
        }
        lines.push(widgets::kv_line("changes", app.git_change_summary(), theme));
    }

    lines.push(widgets::section_header(
        &global_text("context.entries_header"),
        theme,
    ));
    lines.push(widgets::kv_line(
        &global_text("context.entries"),
        app.files.len().to_string(),
        theme,
    ));
    if !app.context_lines.is_empty() {
        for line in &app.context_lines {
            lines.push(Line::styled(
                line.clone(),
                Style::default().fg(theme.secondary),
            ));
        }
    }

    frame.render_widget(
        // Informational panel: never a focus target.
        Paragraph::new(lines).block(widgets::panel(global_text("panel.context"), false, theme)),
        area,
    );
}

/// Human token count: `1.2k`, `18.4k`, plain number below 1000.
fn format_token(value: u64) -> String {
    if value >= 1000 {
        format!("{:.1}k", value as f64 / 1000.0)
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_formatting_is_human() {
        assert_eq!(format_token(42), "42");
        assert_eq!(format_token(18432), "18.4k");
    }
}
