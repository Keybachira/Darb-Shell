//! Tasks panel: the steps the agent took, folded from domain events.
//!
//! `App::apply_event` builds the list; this module only draws it, so the
//! panel stays honest about what actually happened instead of guessing a
//! plan (Contribuição §26). Headers are translated at render time so a
//! language switch re-translates old steps; expanded steps show the raw
//! `tool → target` detail row (web reference: "click step to expand").

use darb_core::i18n::{global_format, global_text};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{List, ListItem};
use ratatui::Frame;

use crate::app::{App, Focus, Tab, TaskState};
use crate::theme::Theme;
use crate::widgets;

pub fn render_tasks(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let items: Vec<ListItem> = if app.tasks.is_empty() {
        vec![ListItem::new(widgets::empty_line(theme))]
    } else {
        let cursor_visible = app.focus == Focus::Workspace && app.tab == Tab::Tasks;
        let mut items = Vec::new();
        for (index, task) in app.tasks.iter().enumerate() {
            let (mark, style) = match task.state {
                TaskState::Pending => ("○", Style::default().fg(theme.muted)),
                TaskState::Done => ("✓", Style::default().fg(theme.success)),
                TaskState::Failed => ("✗", Style::default().fg(theme.danger)),
            };
            let label = global_format(
                "agent.tool_requested",
                &[("tool", &task.tool), ("target", &task.target)],
            );
            let mut header = format!("{mark} {label}");
            let mut header_style = style;
            if cursor_visible && index == app.tasks_index.min(app.tasks.len() - 1) {
                header = format!("▸ {header}");
                header_style = header_style.fg(theme.accent).add_modifier(Modifier::BOLD);
            }
            items.push(ListItem::new(header).style(header_style));
            if task.expanded {
                items.push(
                    ListItem::new(format!("  {} → {}", task.tool, task.target))
                        .style(Style::default().fg(theme.secondary)),
                );
            }
        }
        items
    };
    frame.render_widget(
        List::new(items).block(widgets::panel(
            global_text("panel.tasks"),
            app.workspace_focused(),
            theme,
        )),
        area,
    );
}
