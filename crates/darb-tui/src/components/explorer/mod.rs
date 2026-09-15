//! Explorer panel (project tree) and the file editor shown by the
//! Files tab.
//!
//! Rows come from `App::set_files`; the TUI never reads the disk. Opening
//! an entry leaves as `KeyOutcome::OpenEntry` — `apps/darb` re-lists or
//! reads through the tool registry, so browsing is permission-gated like
//! everything else. Saving leaves as `KeyOutcome::SaveFile` for the same
//! reason: the TUI edits a buffer, the registry writes the file.

use darb_core::i18n::{global_format, global_text};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
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

/// Files tab: read-only preview, or the edit buffer with a gutter and a
/// cursor marker while `App::editing`. The cursor always stays visible:
/// the view centers on it, ignoring the manual scroll offset.
pub fn render_preview(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let Some(preview) = &app.file_preview else {
        frame.render_widget(
            Paragraph::new(global_text("file.empty"))
                .style(Style::default().fg(theme.text))
                .block(widgets::panel(
                    global_text("panel.files"),
                    app.workspace_focused(),
                    theme,
                )),
            area,
        );
        return;
    };
    let mut title = global_format("file.preview_title", &[("path", &preview.path)]);
    if app.edit_dirty {
        title.push_str(" ●");
    }
    // Block (2) + hint line (1): the text window keeps the cursor visible.
    let visible = area.height.saturating_sub(3) as usize;
    let top = cursor_top(app, visible.max(1));
    let mut lines: Vec<Line> = app
        .edit_lines
        .iter()
        .enumerate()
        .skip(top)
        .take(visible.max(1))
        .map(|(number, line)| edit_line(line, number, app, theme))
        .collect();
    if lines.is_empty() {
        lines.push(widgets::empty_line(theme));
    }
    if app.editing {
        lines.push(Line::styled(
            global_text("file.edit_hint"),
            Style::default().fg(theme.muted),
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(theme.text))
            .block(widgets::panel(title, app.workspace_focused(), theme)),
        area,
    );
}

/// First buffer row to show: the manual scroll offset out of edit mode,
/// a cursor-following window in edit mode (biased to keep context above
/// it — roughly one third of the window).
fn cursor_top(app: &App, visible: usize) -> usize {
    if !app.editing {
        return app.scroll as usize;
    }
    if visible == 0 {
        return 0;
    }
    let above = visible / 3;
    app.edit_row.saturating_sub(above)
}

/// One buffer row: right-aligned gutter + text + `▍` at the cursor while
/// editing. Cursor math is character-based (see `App::edit_col`).
fn edit_line(line: &str, number: usize, app: &App, theme: &Theme) -> Line<'static> {
    let gutter = Span::styled(
        format!("{:>4} ", number + 1),
        Style::default().fg(theme.faint),
    );
    if !app.editing || number != app.edit_row {
        return Line::from(vec![gutter, Span::raw(line.to_string())]);
    }
    let col = app.edit_col.min(line.chars().count());
    let mut text = String::with_capacity(line.len() + 3);
    for (index, c) in line.chars().enumerate() {
        if index == col {
            text.push('▍');
        }
        text.push(c);
    }
    if col == line.chars().count() {
        text.push('▍');
    }
    Line::from(vec![
        gutter,
        Span::styled(text, Style::default().fg(theme.accent)),
    ])
}
