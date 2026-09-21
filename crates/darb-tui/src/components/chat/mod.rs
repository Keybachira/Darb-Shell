//! Chat panel: the conversation, newest at the bottom, with the agent
//! timeline pinned to the panel's last rows (✓ done · ◉ running · ○
//! pending) so progress is always visible without switching tabs.
//!
//! The timeline rows are *derivable* from `App::tasks`: no separate
//! scroll state, no duplicated data — the panel shows the last steps
//! that fit in the available height (Contribuição §10).

use darb_core::i18n::{global_format, global_text};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{scroll_top, App, MessageRole, TaskItem, TaskState};
use crate::theme::Theme;
use crate::widgets;

fn role_label(role: MessageRole) -> String {
    match role {
        MessageRole::User => global_text("chat.you"),
        MessageRole::Agent => global_text("chat.darb"),
        MessageRole::System => "·".to_string(),
    }
}

fn role_style(role: MessageRole, theme: &Theme) -> Style {
    match role {
        MessageRole::User => Style::default().fg(theme.primary),
        MessageRole::Agent => Style::default().fg(theme.text),
        MessageRole::System => Style::default().fg(theme.muted),
    }
}

/// One timeline row for a step: mark + translated label. A pending step
/// is `◉` (running, with spinner) only while the agent is actually busy;
/// otherwise it is the quiet `○`.
fn timeline_line(task: &TaskItem, app: &App, theme: &Theme) -> Line<'static> {
    let running = task.state == TaskState::Pending && app.agent_busy();
    let (mark, style) = match task.state {
        TaskState::Done => ("✓", Style::default().fg(theme.success)),
        TaskState::Failed => ("✗", Style::default().fg(theme.danger)),
        TaskState::Pending if running => (
            "◉",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        TaskState::Pending => ("○", Style::default().fg(theme.muted)),
    };
    let label = global_format(
        "agent.tool_requested",
        &[("tool", &task.tool), ("target", &task.target)],
    );
    let mut spans = vec![
        Span::styled(format!(" {mark} "), style),
        Span::styled(label, style),
    ];
    if running {
        spans.push(Span::styled(
            format!(" {}", crate::app::header_spinner(app)),
            Style::default().fg(theme.accent),
        ));
    }
    Line::from(spans)
}

pub fn render_chat(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let mut title = global_text("panel.chat");
    title.push_str(&format!(" [{}]", app.agent_mode.name()));
    // Spinner rides on the title while the agent works (TUI §40).
    if app.agent_busy() {
        title.push_str(&format!(" {}", crate::app::header_spinner(app)));
    }

    // Timeline budget: 0 rows when collapsed or empty, else the last
    // steps that fit in a third of the panel (max 6). Derived from the
    // real height, so a resize simply shrinks the strip.
    let inner = area.height.saturating_sub(2);
    let timeline_rows = if app.tasks.is_empty() || inner < 4 {
        0
    } else {
        ((inner / 3) as usize).min(6).min(app.tasks.len())
    };

    let mut lines: Vec<Line> = app
        .messages
        .iter()
        .map(|message| {
            Line::from(vec![
                Span::styled(
                    format!("{}: ", role_label(message.role)),
                    role_style(message.role, theme),
                ),
                Span::raw(message.text.clone()),
            ])
        })
        .collect();
    // Live answer: provisional until AgentFinished replaces it.
    if !app.streaming.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{}: ", role_label(MessageRole::Agent)),
                role_style(MessageRole::Agent, theme),
            ),
            Span::raw(format!("{}▍", app.streaming)),
        ]));
    }

    // Bottom-pinned: `App::scroll` counts from the newest line, the
    // `Paragraph` from the oldest — see `scroll_top`.
    let visible = inner.saturating_sub(timeline_rows as u16);
    let top = scroll_top(lines.len(), visible, app.scroll);
    frame.render_widget(
        Paragraph::new(lines)
            .block(widgets::panel(title, app.workspace_focused(), theme))
            .wrap(Wrap { trim: false })
            .scroll((top, 0)),
        area,
    );

    // Timeline overlay: the last `timeline_rows` steps drawn over the
    // panel's final rows, above the bottom border.
    if timeline_rows > 0 {
        let start = app.tasks.len() - timeline_rows;
        for (row, task) in app.tasks[start..].iter().enumerate() {
            let y = area.y + area.height - 1 - (timeline_rows as u16 - row as u16);
            let line = timeline_line(task, app, theme);
            frame.render_widget(
                Paragraph::new(line).style(Style::default().bg(theme.panel)),
                Rect::new(area.x + 1, y, area.width.saturating_sub(2), 1),
            );
        }
    }
}
