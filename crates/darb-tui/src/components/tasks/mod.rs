//! Tasks panel: the steps the agent took, folded from domain events.
//!
//! `App::apply_event` builds the list; this module only draws it, so the
//! panel stays honest about what actually happened instead of guessing a
//! plan (Contribuição §26).

use darb_core::i18n::global_text;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{List, ListItem};
use ratatui::Frame;

use crate::app::{App, TaskState};
use crate::theme::Theme;
use crate::widgets;

pub fn render_tasks(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let items: Vec<ListItem> = if app.tasks.is_empty() {
        vec![ListItem::new(widgets::empty_line(theme))]
    } else {
        app.tasks
            .iter()
            .map(|task| {
                let (mark, style) = match task.state {
                    TaskState::Pending => ("○", Style::default().fg(theme.muted)),
                    TaskState::Done => ("✓", Style::default().fg(theme.success)),
                    TaskState::Failed => ("✗", Style::default().fg(theme.danger)),
                };
                ListItem::new(format!("{mark} {}", task.label)).style(style)
            })
            .collect()
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
