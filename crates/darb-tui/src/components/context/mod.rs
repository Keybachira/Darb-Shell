//! Context panel: token/file summary lines fed by `App::set_context`.

use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

pub fn render_context(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let text = if app.context_lines.is_empty() {
        "—".to_string()
    } else {
        app.context_lines.join("\n")
    };
    frame.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .title("Context")
                .borders(Borders::ALL)
                .border_style(theme.border),
        ),
        area,
    );
}
