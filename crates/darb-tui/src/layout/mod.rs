//! Shell geometry: header / body / tabs / statusbar / footer, the three
//! body columns, and the bottom terminal strip inside the center column.
//!
//! Pure function of the terminal area — no widgets, no state — so the
//! layout is unit-testable without a terminal. Mirrors the web reference
//! (`Criar interface editável`): top bar, collapsible side panels, bottom
//! terminal (open panel or one-line collapsed strip), status bar.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Height of the bottom terminal panel while open: title + ~6 content
/// lines. Fixed (not proportional) so small terminals stay usable.
pub const TERMINAL_OPEN_HEIGHT: u16 = 8;
/// Collapsed strip: one line, like the web's `▴ TERMINAL`.
pub const TERMINAL_CLOSED_HEIGHT: u16 = 1;

/// Regions of the Studio layout (Arquitetura §33). Hidden panels are
/// zero-sized so the visible columns take the full width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellLayout {
    pub header: Rect,
    pub explorer: Rect,
    pub workspace: Rect,
    /// Bottom terminal inside the center column. Never zero: 1 row when
    /// closed, so the collapsed strip always has somewhere to draw.
    pub terminal: Rect,
    pub context: Rect,
    /// One-line tab bar under the body (`Chat │ Files │ …`).
    pub tabs: Rect,
    /// One-line status bar: agent state, task counts, pending permission.
    pub statusbar: Rect,
    pub footer: Rect,
}

/// Split `area` into the shell regions.
pub fn shell_layout(
    area: Rect,
    explorer_visible: bool,
    context_visible: bool,
    bottom_open: bool,
) -> ShellLayout {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(area);
    let mut constraints = Vec::with_capacity(3);
    if explorer_visible {
        constraints.push(Constraint::Percentage(20));
    }
    constraints.push(Constraint::Fill(1));
    if context_visible {
        constraints.push(Constraint::Percentage(25));
    }
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(rows[1]);
    let (explorer, center, context) = match (explorer_visible, context_visible) {
        (true, true) => (columns[0], columns[1], columns[2]),
        (true, false) => (columns[0], columns[1], Rect::new(0, 0, 0, 0)),
        (false, true) => (Rect::new(0, 0, 0, 0), columns[0], columns[1]),
        (false, false) => (Rect::new(0, 0, 0, 0), columns[0], Rect::new(0, 0, 0, 0)),
    };
    let terminal_height = if bottom_open {
        TERMINAL_OPEN_HEIGHT
    } else {
        TERMINAL_CLOSED_HEIGHT
    };
    let center_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(terminal_height)])
        .split(center);
    ShellLayout {
        header: rows[0],
        explorer,
        workspace: center_rows[0],
        terminal: center_rows[1],
        context,
        tabs: rows[2],
        statusbar: rows[3],
        footer: rows[4],
    }
}

/// Centered rectangle for modal dialogs, clamped to `area`.
pub fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_full_shell() {
        let area = Rect::new(0, 0, 100, 30);
        let layout = shell_layout(area, true, true, true);
        assert_eq!(layout.header.height, 3);
        assert_eq!(layout.tabs.height, 1);
        assert_eq!(layout.statusbar.height, 1);
        assert_eq!(layout.footer.height, 3);
        assert_eq!(layout.terminal.height, TERMINAL_OPEN_HEIGHT);
        // Columns tile the body row exactly.
        assert_eq!(layout.explorer.x, 0);
        assert_eq!(
            layout.explorer.width + layout.workspace.width + layout.context.width,
            100
        );
        // Center column tiles workspace + terminal exactly.
        assert_eq!(layout.terminal.x, layout.workspace.x);
        assert_eq!(layout.terminal.width, layout.workspace.width);
        assert_eq!(
            layout.workspace.height + layout.terminal.height,
            layout.explorer.height
        );
        // Rows tile the terminal area: nothing overlaps, nothing is lost.
        assert_eq!(
            layout.tabs.y,
            layout.workspace.y + layout.workspace.height + layout.terminal.height
        );
        assert_eq!(layout.statusbar.y, layout.tabs.y + layout.tabs.height);
        assert_eq!(
            layout.footer.y,
            layout.statusbar.y + layout.statusbar.height
        );
        assert_eq!(layout.footer.y + layout.footer.height, 30);
    }

    #[test]
    fn closed_terminal_keeps_a_one_line_strip() {
        let area = Rect::new(0, 0, 100, 30);
        let layout = shell_layout(area, true, true, false);
        assert_eq!(layout.terminal.height, TERMINAL_CLOSED_HEIGHT);
        assert_eq!(layout.terminal.x, layout.workspace.x);
        assert_eq!(layout.terminal.width, layout.workspace.width);
    }

    #[test]
    fn hidden_panels_give_full_width() {
        let area = Rect::new(0, 0, 100, 30);
        let both_hidden = shell_layout(area, false, false, true);
        assert_eq!(both_hidden.explorer.width, 0);
        assert_eq!(both_hidden.context.width, 0);
        assert_eq!(both_hidden.workspace.width, 100);
        let no_explorer = shell_layout(area, false, true, true);
        assert_eq!(no_explorer.explorer.width, 0);
        assert_eq!(no_explorer.workspace.width + no_explorer.context.width, 100);
        let no_context = shell_layout(area, true, false, true);
        assert_eq!(no_context.context.width, 0);
        assert_eq!(no_context.explorer.width + no_context.workspace.width, 100);
    }

    #[test]
    fn dialog_is_centered_and_clamped() {
        let area = Rect::new(0, 0, 100, 30);
        let popup = centered_rect(area, 60, 10);
        assert_eq!(popup, Rect::new(20, 10, 60, 10));
        // Larger than the terminal: clamps instead of overflowing.
        assert_eq!(centered_rect(area, 200, 100), area);
    }
}
