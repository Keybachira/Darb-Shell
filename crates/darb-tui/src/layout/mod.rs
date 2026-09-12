//! Shell geometry: header / body / tabs / footer, and the three body
//! columns.
//!
//! Pure function of the terminal area — no widgets, no state — so the
//! layout is unit-testable without a terminal.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Regions of the Studio layout (Arquitetura §33).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellLayout {
    pub header: Rect,
    pub explorer: Rect,
    pub workspace: Rect,
    pub context: Rect,
    /// One-line tab bar under the body (`Chat │ Files │ …`).
    pub tabs: Rect,
    pub footer: Rect,
}

/// Split `area` into the shell regions. The explorer column is skipped
/// (zero-sized) when hidden so workspace + context take the full width.
pub fn shell_layout(area: Rect, explorer_visible: bool) -> ShellLayout {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(area);
    let (explorer, workspace, context) = if explorer_visible {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Fill(1),
                Constraint::Percentage(25),
            ])
            .split(rows[1]);
        (columns[0], columns[1], columns[2])
    } else {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Fill(1), Constraint::Percentage(25)])
            .split(rows[1]);
        (Rect::new(0, 0, 0, 0), columns[0], columns[1])
    };
    ShellLayout {
        header: rows[0],
        explorer,
        workspace,
        context,
        tabs: rows[2],
        footer: rows[3],
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
        let layout = shell_layout(area, true);
        assert_eq!(layout.header.height, 3);
        assert_eq!(layout.tabs.height, 1);
        assert_eq!(layout.footer.height, 3);
        // Columns tile the body row exactly.
        assert_eq!(layout.explorer.x, 0);
        assert_eq!(
            layout.explorer.width + layout.workspace.width + layout.context.width,
            100
        );
        assert_eq!(layout.workspace.y, layout.explorer.y);
        // Rows tile the terminal area: nothing overlaps, nothing is lost.
        assert_eq!(layout.tabs.y, layout.workspace.y + layout.workspace.height);
        assert_eq!(layout.footer.y, layout.tabs.y + layout.tabs.height);
        assert_eq!(layout.footer.y + layout.footer.height, 30);
    }

    #[test]
    fn hidden_explorer_gives_full_width() {
        let area = Rect::new(0, 0, 100, 30);
        let layout = shell_layout(area, false);
        assert_eq!(layout.explorer.width, 0);
        assert_eq!(layout.workspace.width + layout.context.width, 100);
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
