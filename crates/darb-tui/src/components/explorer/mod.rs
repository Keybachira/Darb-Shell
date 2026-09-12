//! Explorer panel (project tree) and the file preview shown by the
//! Files tab.
//!
//! Rows come from `App::set_files`; the TUI never reads the disk. Opening
//! an entry leaves as `KeyOutcome::OpenEntry` — `apps/darb` re-lists or
//! reads through the tool registry, so browsing is permission-gated like
//! everything else.

use darb_core::i18n::{global_format, global_text};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;
use crate::widgets;

pub fn render_explorer(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let mut items: Vec<ListItem> = Vec::with_capacity(app.files.len() + 1);
    if app.has_parent_entry() {
        items.push(
            ListItem::new(global_text("explorer.parent")).style(Style::default().fg(theme.muted)),
        );
    }
    if app.files.is_empty() {
        items.push(ListItem::new(widgets::empty_line(theme)));
    }
    for entry in &app.files {
        let label = if entry.is_dir {
            format!("{}/", entry.name)
        } else {
            entry.name.clone()
        };
        let style = if entry.is_dir {
            Style::default().fg(theme.primary)
        } else {
            Style::default().fg(theme.text)
        };
        items.push(ListItem::new(label).style(style));
    }

    let title = if app.current_dir.is_empty() {
        global_text("panel.explorer")
    } else {
        // Showing the open directory avoids the classic "where am I?".
        format!("{} · {}", global_text("panel.explorer"), app.current_dir)
    };

    let mut state = ListState::default();
    state.select(Some(app.explorer_index));
    frame.render_stateful_widget(
        List::new(items)
            .block(widgets::panel(
                title,
                app.focus == crate::app::Focus::Explorer,
                theme,
            ))
            .highlight_style(Style::default().fg(theme.highlight))
            .highlight_symbol("› "),
        area,
        &mut state,
    );
}

/// Files tab: the content of the entry the user opened.
pub fn render_preview(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let (title, body) = match &app.file_preview {
        Some(preview) => (
            global_format("file.preview_title", &[("path", &preview.path)]),
            preview.text.clone(),
        ),
        None => (global_text("panel.files"), global_text("file.empty")),
    };
    frame.render_widget(
        Paragraph::new(body)
            .style(Style::default().fg(theme.text))
            .block(widgets::panel(title, app.workspace_focused(), theme))
            .scroll((app.scroll, 0)),
        area,
    );
}
