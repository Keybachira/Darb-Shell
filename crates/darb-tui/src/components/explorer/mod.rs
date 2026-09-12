//! Explorer panel: project file rows fed by `App::set_files`.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

pub fn render_explorer(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let items: Vec<ListItem> = app
        .files
        .iter()
        .map(|entry| {
            let label = if entry.is_dir {
                format!("{}/", entry.name)
            } else {
                entry.name.clone()
            };
            let style = if entry.is_dir {
                Style::default().fg(theme.primary)
            } else {
                Style::default()
            };
            ListItem::new(label).style(style)
        })
        .collect();
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .title("Explorer")
                .borders(Borders::ALL)
                .border_style(theme.border),
        ),
        area,
    );
}
