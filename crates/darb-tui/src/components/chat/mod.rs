//! Chat panel: the conversation, newest at the bottom.

use darb_core::i18n::global_text;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, MessageRole};
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

pub fn render_chat(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
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
    frame.render_widget(
        Paragraph::new(lines)
            .block(widgets::panel(
                global_text("panel.chat"),
                app.workspace_focused(),
                theme,
            ))
            .wrap(Wrap { trim: false })
            .scroll((app.scroll, 0)),
        area,
    );
}
