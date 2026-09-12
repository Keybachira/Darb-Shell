//! Chat panel: the conversation, newest at the bottom.

use darb_core::i18n::global_text;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, MessageRole};
use crate::theme::Theme;

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
    let lines: Vec<ratatui::text::Line> = app
        .messages
        .iter()
        .map(|message| {
            ratatui::text::Line::from(vec![
                ratatui::text::Span::styled(
                    format!("{}: ", role_label(message.role)),
                    role_style(message.role, theme),
                ),
                ratatui::text::Span::raw(message.text.clone()),
            ])
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(theme.border),
            )
            .wrap(Wrap { trim: false })
            .scroll((app.chat_scroll, 0)),
        area,
    );
}
