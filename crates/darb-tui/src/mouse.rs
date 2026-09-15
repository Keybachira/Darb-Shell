//! Mouse hit-testing: a click position → the same [`KeyOutcome`] a key
//! would produce. Presentation only (Contribuição §26): the mapping
//! decides *which intent* the click means; executing it (listing a
//! directory, running a command) stays with `apps/darb`, exactly like the
//! keyboard path. Pure geometry — unit-testable without a terminal.
//!
//! Docs §32/§34: the TUI must be "clicável quando o terminal suportar
//! mouse". Keeping this in one place stops click targets from drifting
//! from the keyboard actions.

use ratatui::layout::Rect;

use crate::app::{task_at_row, App, Focus, KeyOutcome, Tab};
use crate::layout::shell_layout;

/// Where the user clicked. Pure input: no app state is needed to build it,
/// so rendering and hit-testing can never disagree about geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Click {
    pub x: u16,
    pub y: u16,
}

/// Region hit by a click, in the same terms as [`crate::layout::ShellLayout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseRegion {
    Explorer,
    Workspace,
    Terminal,
    Context,
    TabBar,
    StatusBar,
    Footer,
    Header,
}

impl Click {
    /// Which shell region contains this click. Pure lookup on the same
    /// layout `render` uses, so the two can never disagree.
    pub fn region(&self, app: &App, area: Rect) -> MouseRegion {
        let layout = shell_layout(
            area,
            app.explorer_visible,
            app.context_visible,
            app.bottom_open,
        );
        if pos_in(self.x, self.y, layout.header) {
            return MouseRegion::Header;
        }
        if pos_in(self.x, self.y, layout.tabs) {
            return MouseRegion::TabBar;
        }
        if pos_in(self.x, self.y, layout.statusbar) {
            return MouseRegion::StatusBar;
        }
        if pos_in(self.x, self.y, layout.footer) {
            return MouseRegion::Footer;
        }
        if pos_in(self.x, self.y, layout.explorer) {
            return MouseRegion::Explorer;
        }
        if pos_in(self.x, self.y, layout.terminal) {
            return MouseRegion::Terminal;
        }
        if pos_in(self.x, self.y, layout.workspace) {
            return MouseRegion::Workspace;
        }
        if pos_in(self.x, self.y, layout.context) {
            return MouseRegion::Context;
        }
        MouseRegion::Header // Unreachable for in-area clicks; kept total.
    }

    /// A click on a tab bar cell selects that tab. Columns follow
    /// `render_tab_bar`: labels with ` │ ` separators, then the hint.
    pub fn tab_at(&self, app: &App, area: Rect) -> Option<Tab> {
        let layout = shell_layout(
            area,
            app.explorer_visible,
            app.context_visible,
            app.bottom_open,
        );
        if !pos_in(self.x, self.y, layout.tabs) {
            return None;
        }
        let mut cursor = layout.tabs.x;
        for tab in Tab::all() {
            let label_width = tab_label_width(tab);
            // Both bounds: a click on a separator (` │ `) must fall through
            // instead of matching the next label (the start cursor is
            // already past it).
            if self.x >= cursor && self.x < cursor + label_width {
                return Some(tab);
            }
            cursor += label_width + SEPARATOR_WIDTH;
        }
        None
    }

    /// Tasks-list row under the click, or `None` outside the list or on
    /// another tab. Rows count expanded detail lines exactly like the
    /// renderer does, via [`task_at_row`].
    pub fn task_row(&self, app: &App, area: Rect) -> Option<usize> {
        if app.tab != Tab::Tasks {
            return None;
        }
        let layout = shell_layout(
            area,
            app.explorer_visible,
            app.context_visible,
            app.bottom_open,
        );
        if !pos_in(self.x, self.y, layout.workspace) {
            return None;
        }
        // Inside the block border: row 0 is the title, steps start at 1.
        let inner_top = layout.workspace.y + 1;
        if self.y < inner_top {
            return None;
        }
        task_at_row(&app.tasks, (self.y - inner_top) as usize)
    }

    /// Explorer row under the click, or `None` outside the list. The row
    /// index counts the `..` entry exactly like `explorer_index` does.
    pub fn explorer_row(&self, app: &App, area: Rect) -> Option<usize> {
        let layout = shell_layout(
            area,
            app.explorer_visible,
            app.context_visible,
            app.bottom_open,
        );
        if !pos_in(self.x, self.y, layout.explorer) {
            return None;
        }
        // Inside the block border: row 0 is the title, entries start at 1.
        let inner_top = layout.explorer.y + 1;
        if self.y < inner_top {
            return None;
        }
        Some((self.y - inner_top) as usize)
    }
}

/// Separator span ` │ ` between tabs in `render_tab_bar`.
const SEPARATOR_WIDTH: u16 = 3;

fn tab_label_width(tab: Tab) -> u16 {
    crate::app::tab_label_width(tab)
}

fn pos_in(x: u16, y: u16, area: Rect) -> bool {
    area.width > 0
        && area.height > 0
        && x >= area.x
        && x < area.x + area.width
        && y >= area.y
        && y < area.y + area.height
}

/// Map one left-button click to an outcome. Overlay-free shell areas
/// first; overlays are handled by [`App::handle_mouse`] before this runs.
pub fn click_outcome(app: &mut App, click: Click, area: Rect) -> KeyOutcome {
    match click.region(app, area) {
        MouseRegion::TabBar => {
            match click.tab_at(app, area) {
                Some(tab) => {
                    app.set_tab(tab);
                    app.focus = Focus::Workspace;
                    KeyOutcome::Ignored
                }
                // Separator/empty part of the bar: nothing to select.
                None => KeyOutcome::Ignored,
            }
        }
        MouseRegion::Explorer => {
            let Some(row) = click.explorer_row(app, area) else {
                return KeyOutcome::Ignored;
            };
            if row >= app.explorer_rows() {
                // Below the last entry: move the highlight there is still
                // friendlier than ignoring, but never open a phantom row.
                return KeyOutcome::Ignored;
            }
            app.focus = Focus::Explorer;
            app.explorer_index = row;
            app.enter_explorer_at(row)
        }
        MouseRegion::Workspace => {
            app.focus = Focus::Workspace;
            // Tasks tab: a click moves the cursor and expands the step
            // under it (web reference: "click step to expand").
            if app.tab == Tab::Tasks {
                if let Some(index) = click.task_row(app, area) {
                    app.tasks_index = index;
                    return app.toggle_task();
                }
            }
            KeyOutcome::Ignored
        }
        // The mini terminal shares the workspace focus: typing still goes
        // to the command line and Enter follows the active tab's rule, so
        // clicking it must not invent a new input path.
        MouseRegion::Terminal => {
            app.focus = Focus::Workspace;
            KeyOutcome::Ignored
        }
        MouseRegion::Footer => {
            app.focus = Focus::Input;
            KeyOutcome::Ignored
        }
        // Header, status bar and context panel carry no actions; clicks
        // there do not steal focus from what the user was doing.
        MouseRegion::Header | MouseRegion::StatusBar | MouseRegion::Context => KeyOutcome::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, FileEntry};
    use crate::theme::Theme;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Same counting `render_tab_bar` uses, kept in sync by this test.
    fn labels_width() -> u16 {
        Tab::all()
            .iter()
            .map(|tab| tab_label_width(*tab))
            .sum::<u16>()
            + SEPARATOR_WIDTH * (Tab::all().len() as u16 - 1)
    }

    fn draw_to(app: &App, width: u16, height: u16) {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| crate::app::render(frame, app, &Theme::default()))
            .expect("draw");
    }

    /// The mapping and the renderer must agree on label widths, or clicks
    /// would select a different tab than the one drawn under the cursor.
    #[test]
    fn tab_label_widths_match_the_drawn_tab_bar() {
        draw_to(&App::new(), 110, 34);
        let expected: u16 = Tab::all()
            .iter()
            .map(|tab| {
                darb_core::i18n::global_text(tab.title_key())
                    .chars()
                    .count() as u16
            })
            .sum();
        let mapped: u16 = Tab::all().iter().map(|tab| tab_label_width(*tab)).sum();
        assert_eq!(mapped, expected);
        assert!(labels_width() > 0);
    }

    #[test]
    fn click_on_each_tab_selects_it() {
        let area = Rect::new(0, 0, 110, 34);
        let mut app = App::new();
        let layout = shell_layout(
            area,
            app.explorer_visible,
            app.context_visible,
            app.bottom_open,
        );
        let mut cursor = layout.tabs.x;
        for tab in Tab::all() {
            let click = Click {
                x: cursor,
                y: layout.tabs.y,
            };
            let outcome = click_outcome(&mut app, click, area);
            assert!(matches!(outcome, KeyOutcome::Ignored));
            assert_eq!(app.tab, tab, "clicked at x={}", click.x);
            assert_eq!(app.focus, Focus::Workspace);
            cursor += tab_label_width(tab) + SEPARATOR_WIDTH;
        }
    }

    #[test]
    fn click_on_separator_selects_nothing() {
        let area = Rect::new(0, 0, 110, 34);
        let mut app = App::new();
        let layout = shell_layout(
            area,
            app.explorer_visible,
            app.context_visible,
            app.bottom_open,
        );
        let first_width = tab_label_width(Tab::Chat);
        let click = Click {
            x: layout.tabs.x + first_width + 1, // middle of ` │ `
            y: layout.tabs.y,
        };
        let before = app.tab;
        assert!(matches!(
            click_outcome(&mut app, click, area),
            KeyOutcome::Ignored
        ));
        assert_eq!(app.tab, before);
    }

    #[test]
    fn click_on_explorer_entry_opens_it() {
        let area = Rect::new(0, 0, 110, 34);
        let mut app = App::new();
        app.set_current_dir("crates".to_string());
        app.set_files(vec![
            FileEntry {
                name: "core".to_string(),
                is_dir: true,
            },
            FileEntry {
                name: "lib.rs".to_string(),
                is_dir: false,
            },
        ]);
        let layout = shell_layout(
            area,
            app.explorer_visible,
            app.context_visible,
            app.bottom_open,
        );
        // Row 0 is `..` (we are below the root), so `core` is row 1.
        let click = Click {
            x: layout.explorer.x + 2,
            y: layout.explorer.y + 2,
        };
        let outcome = click_outcome(&mut app, click, area);
        assert_eq!(app.focus, Focus::Explorer);
        assert_eq!(app.explorer_index, 1);
        assert_eq!(
            outcome,
            KeyOutcome::OpenEntry {
                path: "crates/core".to_string(),
                is_dir: true,
            }
        );
    }

    #[test]
    fn click_below_last_entry_does_not_open() {
        let area = Rect::new(0, 0, 110, 34);
        let mut app = App::new();
        app.set_files(vec![FileEntry {
            name: "a.txt".to_string(),
            is_dir: false,
        }]);
        let layout = shell_layout(
            area,
            app.explorer_visible,
            app.context_visible,
            app.bottom_open,
        );
        // Row 0 = a.txt; click two rows further down.
        let click = Click {
            x: layout.explorer.x + 1,
            y: layout.explorer.y + 3,
        };
        let outcome = click_outcome(&mut app, click, area);
        assert!(matches!(outcome, KeyOutcome::Ignored));
        assert_eq!(
            app.explorer_index, 0,
            "selection must not move past the list"
        );
    }

    #[test]
    fn click_footer_focuses_input_click_workspace_focuses_workspace() {
        let area = Rect::new(0, 0, 110, 34);
        let mut app = App::new();
        let layout = shell_layout(
            area,
            app.explorer_visible,
            app.context_visible,
            app.bottom_open,
        );
        assert!(matches!(
            click_outcome(
                &mut app,
                Click {
                    x: layout.footer.x + 2,
                    y: layout.footer.y + 1,
                },
                area
            ),
            KeyOutcome::Ignored
        ));
        assert_eq!(app.focus, Focus::Input);
        assert!(matches!(
            click_outcome(
                &mut app,
                Click {
                    x: layout.workspace.x + 2,
                    y: layout.workspace.y + 1,
                },
                area
            ),
            KeyOutcome::Ignored
        ));
        assert_eq!(app.focus, Focus::Workspace);
    }

    #[test]
    fn header_and_context_clicks_do_not_steal_focus() {
        let area = Rect::new(0, 0, 110, 34);
        let mut app = App::new();
        app.focus = Focus::Input;
        let layout = shell_layout(
            area,
            app.explorer_visible,
            app.context_visible,
            app.bottom_open,
        );
        let _ = click_outcome(
            &mut app,
            Click {
                x: layout.header.x + 3,
                y: layout.header.y + 1,
            },
            area,
        );
        assert_eq!(app.focus, Focus::Input);
        let _ = click_outcome(
            &mut app,
            Click {
                x: layout.context.x + 2,
                y: layout.context.y + 1,
            },
            area,
        );
        assert_eq!(app.focus, Focus::Input);
    }

    #[test]
    fn region_detection_respects_hidden_explorer() {
        let area = Rect::new(0, 0, 110, 34);
        let mut app = App::new();
        app.explorer_visible = false;
        let click = Click { x: 2, y: 10 };
        assert_eq!(click.region(&app, area), MouseRegion::Workspace);
    }

    #[test]
    fn region_detection_respects_hidden_context() {
        let area = Rect::new(0, 0, 110, 34);
        let mut app = App::new();
        app.context_visible = false;
        // Far right used to be the context panel; now it is workspace.
        let click = Click { x: 105, y: 10 };
        assert_eq!(click.region(&app, area), MouseRegion::Workspace);
    }

    #[test]
    fn click_on_terminal_focuses_workspace() {
        let area = Rect::new(0, 0, 110, 34);
        let mut app = App::new();
        app.focus = Focus::Input;
        let layout = shell_layout(
            area,
            app.explorer_visible,
            app.context_visible,
            app.bottom_open,
        );
        let outcome = click_outcome(
            &mut app,
            Click {
                x: layout.terminal.x + 2,
                y: layout.terminal.y + 1,
            },
            area,
        );
        assert!(matches!(outcome, KeyOutcome::Ignored));
        assert_eq!(app.focus, Focus::Workspace);
    }

    #[test]
    fn statusbar_click_is_detected_and_ignored() {
        let area = Rect::new(0, 0, 110, 34);
        let mut app = App::new();
        app.focus = Focus::Explorer;
        let layout = shell_layout(
            area,
            app.explorer_visible,
            app.context_visible,
            app.bottom_open,
        );
        let click = Click {
            x: layout.statusbar.x + 2,
            y: layout.statusbar.y,
        };
        assert_eq!(click.region(&app, area), MouseRegion::StatusBar);
        let _ = click_outcome(&mut app, click, area);
        assert_eq!(app.focus, Focus::Explorer);
    }

    #[test]
    fn click_on_task_row_moves_cursor_and_expands() {
        use crate::app::TaskState;
        use darb_core::events::DarbEvent;

        let area = Rect::new(0, 0, 110, 34);
        let mut app = App::new();
        app.apply_event(&DarbEvent::AgentStarted {
            task: "fix login".to_string(),
        });
        for target in ["a.rs", "b.rs"] {
            app.apply_event(&DarbEvent::ToolRequested {
                tool: "read_file".to_string(),
                target: target.to_string(),
            });
        }
        app.set_tab(Tab::Tasks);
        let layout = shell_layout(
            area,
            app.explorer_visible,
            app.context_visible,
            app.bottom_open,
        );
        // Step 1 is the second visual row (title row + step 0).
        let outcome = click_outcome(
            &mut app,
            Click {
                x: layout.workspace.x + 2,
                y: layout.workspace.y + 2,
            },
            area,
        );
        assert!(matches!(outcome, KeyOutcome::Ignored));
        assert_eq!(app.focus, Focus::Workspace);
        assert_eq!(app.tasks_index, 1);
        assert!(app.tasks[1].expanded);
        assert!(!app.tasks[0].expanded);
        assert!(matches!(app.tasks[1].state, TaskState::Pending));
    }

    #[test]
    fn task_row_is_none_outside_the_tasks_tab() {
        let area = Rect::new(0, 0, 110, 34);
        let app = App::new();
        assert_eq!(app.tab, Tab::Chat);
        let layout = shell_layout(
            area,
            app.explorer_visible,
            app.context_visible,
            app.bottom_open,
        );
        let click = Click {
            x: layout.workspace.x + 2,
            y: layout.workspace.y + 1,
        };
        assert_eq!(click.task_row(&app, area), None);
    }
}
