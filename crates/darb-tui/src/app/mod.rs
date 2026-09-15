//! App state: the single presentation model.
//!
//! The TUI decides nothing (Contribuição §26): [`App::apply_event`] folds
//! `darb-core` events into display data, [`App::handle_key`] turns keys
//! into [`KeyOutcome`]s for `apps/darb` to execute, and [`render`] draws
//! it all. No provider, tool, or agent calls happen here — the panels are
//! filled from the outside (`set_files`, `set_diff`, `push_terminal`, …).

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use darb_core::events::{AgentState, DarbEvent};
use darb_core::i18n::{global_format, global_text};
use ratatui::layout::Rect;
use ratatui::Frame;

use crate::keybindings::{action_for, Action};
use crate::mouse;
use crate::palette::{popup_rect, render_palette, Command, Palette};
use crate::theme::Theme;

/// One file row in the explorer. A presentation copy — `apps/darb` feeds
/// it from tool results; the TUI never reads the disk itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Agent,
    System,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub text: String,
}

/// Pending permission question shown modally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionPrompt {
    pub tool: String,
    pub target: String,
}

/// Content of the file the user opened in the explorer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePreview {
    pub path: String,
    pub text: String,
}

/// Progress of one agent step shown in the tasks panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Pending,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskItem {
    pub tool: String,
    pub target: String,
    pub state: TaskState,
    /// Expanded steps show the `tool → target` detail row (web reference:
    /// "click step to expand"). The header label is formatted at render
    /// time so a language switch re-translates old steps too.
    pub expanded: bool,
}

impl TaskItem {
    /// Visual rows this step occupies in the tasks list.
    fn rows(&self) -> usize {
        if self.expanded {
            2
        } else {
            1
        }
    }
}

/// Step index under visual `row` (0-based over list items, title row
/// excluded), or `None` past the end. Shared by the renderer and mouse
/// hit-testing so both agree (Contribuição §10).
pub fn task_at_row(tasks: &[TaskItem], row: usize) -> Option<usize> {
    let mut cursor = 0;
    for (index, task) in tasks.iter().enumerate() {
        if row < cursor + task.rows() {
            return Some(index);
        }
        cursor += task.rows();
    }
    None
}

/// Agent operating mode (web reference). Names are language-neutral
/// acronyms, so no i18n keys are needed for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentMode {
    #[default]
    Auto,
    Plan,
    Code,
    Debug,
    Review,
    Explain,
}

impl AgentMode {
    pub fn all() -> [AgentMode; 6] {
        [
            AgentMode::Auto,
            AgentMode::Plan,
            AgentMode::Code,
            AgentMode::Debug,
            AgentMode::Review,
            AgentMode::Explain,
        ]
    }

    pub fn name(self) -> &'static str {
        match self {
            AgentMode::Auto => "AUTO",
            AgentMode::Plan => "PLAN",
            AgentMode::Code => "CODE",
            AgentMode::Debug => "DEBUG",
            AgentMode::Review => "REVIEW",
            AgentMode::Explain => "EXPLAIN",
        }
    }

    fn next(self) -> Self {
        match self {
            AgentMode::Auto => AgentMode::Plan,
            AgentMode::Plan => AgentMode::Code,
            AgentMode::Code => AgentMode::Debug,
            AgentMode::Debug => AgentMode::Review,
            AgentMode::Review => AgentMode::Explain,
            AgentMode::Explain => AgentMode::Auto,
        }
    }
}

/// Which region receives scroll/Enter keys. Character input always goes
/// to the command line, whatever the focus is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Input,
    Explorer,
    Workspace,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Focus::Input => Focus::Explorer,
            Focus::Explorer => Focus::Workspace,
            Focus::Workspace => Focus::Input,
        }
    }
}

/// Workspace tabs (Arquitetura §33). Each one is a view over data the
/// application already has; switching tabs never triggers work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Chat,
    Files,
    Changes,
    Terminal,
    Git,
    Tasks,
}

impl Tab {
    pub fn all() -> [Tab; 6] {
        [
            Tab::Chat,
            Tab::Files,
            Tab::Changes,
            Tab::Terminal,
            Tab::Git,
            Tab::Tasks,
        ]
    }

    /// i18n key for the tab label (also the panel title).
    pub fn title_key(self) -> &'static str {
        match self {
            Tab::Chat => "panel.chat",
            Tab::Files => "panel.files",
            Tab::Changes => "panel.changes",
            Tab::Terminal => "panel.terminal",
            Tab::Git => "panel.git",
            Tab::Tasks => "panel.tasks",
        }
    }
}

/// Width of one translated tab label, exactly as the tab bar draws it.
/// A function shared by the renderer and mouse hit-testing so clicks can
/// never select a different tab than the one drawn under the cursor
/// (Contribuição §10: no duplicated geometry).
pub fn tab_label_width(tab: Tab) -> u16 {
    global_text(tab.title_key()).chars().count() as u16
}

/// Width of the ` │ ` separator spans the tab bar renders between tabs.
pub const TAB_SEPARATOR_WIDTH: u16 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyOutcome {
    Quit,
    Submitted(String),
    CancelRequested,
    PermissionAnswer(bool),
    /// A palette command the application layer must perform.
    Command(Command),
    /// Explorer: open this project-relative entry.
    OpenEntry {
        path: String,
        is_dir: bool,
    },
    /// Explorer: go to the parent directory.
    GoUp,
    /// Terminal tab: run this line (through permissions).
    TerminalCommand(String),
    /// Files tab: save the edit buffer through `edit_file`
    /// (old = snapshot at open, new = buffer). `apps/darb` dispatches it
    /// through the permission manager like any other tool.
    SaveFile {
        path: String,
        original: String,
        content: String,
    },
    Ignored,
}

pub struct App {
    pub agent_state: AgentState,
    /// Operating mode shown in the chat title (`Chat [AUTO]`). Displayed
    /// and cycled here; the agent request wiring lands in Fase 5.
    pub agent_mode: AgentMode,
    pub messages: Vec<ChatMessage>,
    /// In-progress streamed answer. Shown live, then replaced by the
    /// final message on `AgentFinished` (which always arrives after).
    pub streaming: String,
    pub files: Vec<FileEntry>,
    pub context_lines: Vec<String>,
    pub input: String,
    /// Scroll offset of the focused panel (chat or active tab).
    pub scroll: u16,
    pub explorer_visible: bool,
    /// Right AI-context panel (web: collapsible with `»`).
    pub context_visible: bool,
    pub permission: Option<PermissionPrompt>,
    pub focus: Focus,
    pub tab: Tab,
    /// Directory currently listed, relative to the project root.
    pub current_dir: String,
    pub explorer_index: usize,
    pub file_preview: Option<FilePreview>,
    /// Edit mode (Files tab): the preview becomes a buffer. `editing` is
    /// true only while a file is open; every mutation sets `edit_dirty`.
    pub editing: bool,
    pub edit_lines: Vec<String>,
    /// Cursor in characters, not display columns: wide chars and tabs may
    /// misalign the marker by a cell — documented, not silently wrong.
    pub edit_row: usize,
    pub edit_col: usize,
    /// Exact snapshot at open: the `old` argument of the save, which gives
    /// optimistic-concurrency for free (external edits fail the match).
    pub edit_origin: String,
    pub edit_trailing_newline: bool,
    pub edit_dirty: bool,
    pub diff_lines: Vec<String>,
    pub git_lines: Vec<String>,
    pub terminal_lines: Vec<String>,
    pub tasks: Vec<TaskItem>,
    /// Cursor over the tasks list. Moved with ↑/↓ while the Tasks tab is
    /// focused; Enter toggles the step under it.
    pub tasks_index: usize,
    /// Bottom terminal panel (web default: open). Closed it collapses to
    /// a one-line strip; the Terminal tab keeps the full scrollable view.
    pub bottom_open: bool,
    /// `Some` while the Ctrl+K palette is open.
    pub palette: Option<Palette>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            agent_state: AgentState::Idle,
            agent_mode: AgentMode::Auto,
            messages: Vec::new(),
            streaming: String::new(),
            files: Vec::new(),
            context_lines: Vec::new(),
            input: String::new(),
            scroll: 0,
            explorer_visible: true,
            context_visible: true,
            permission: None,
            focus: Focus::Input,
            tab: Tab::Chat,
            current_dir: String::new(),
            explorer_index: 0,
            file_preview: None,
            editing: false,
            edit_lines: Vec::new(),
            edit_row: 0,
            edit_col: 0,
            edit_origin: String::new(),
            edit_trailing_newline: false,
            edit_dirty: false,
            diff_lines: Vec::new(),
            git_lines: Vec::new(),
            terminal_lines: Vec::new(),
            tasks: Vec::new(),
            tasks_index: 0,
            bottom_open: true,
            palette: None,
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one domain event into display state.
    pub fn apply_event(&mut self, event: &DarbEvent) {
        match event {
            DarbEvent::AgentStarted { task } => {
                self.agent_state = AgentState::Analyzing;
                self.tasks.clear();
                self.push(MessageRole::System, format!("◈ {task}"));
            }
            DarbEvent::AgentStateChanged { to, .. } => {
                self.agent_state = to.clone();
            }
            DarbEvent::ToolRequested { tool, target } => {
                self.push(
                    MessageRole::System,
                    format!(
                        "◉ {}",
                        global_format(
                            "agent.tool_requested",
                            &[("tool", tool), ("target", target)]
                        )
                    ),
                );
                self.tasks.push(TaskItem {
                    tool: tool.clone(),
                    target: target.clone(),
                    state: TaskState::Pending,
                    expanded: false,
                });
            }
            DarbEvent::ToolCompleted { tool, success } => {
                if !success {
                    self.push(
                        MessageRole::System,
                        format!(
                            "✗ {}",
                            global_format("agent.tool_failed", &[("tool", tool)])
                        ),
                    );
                }
                self.finish_last_task(*success);
            }
            DarbEvent::PermissionRequested { tool, target } => {
                self.ask_permission(tool, target);
            }
            DarbEvent::PermissionResolved { allowed, .. } => {
                self.permission = None;
                if !*allowed {
                    self.push(
                        MessageRole::System,
                        format!("⌀ {}", global_text("agent.denied_by_user")),
                    );
                }
            }
            DarbEvent::ContextReady { files } => {
                self.context_lines = vec![global_format(
                    "context.files",
                    &[("count", &files.to_string())],
                )];
            }
            DarbEvent::AgentFinished { summary } => {
                self.streaming.clear();
                self.push(MessageRole::Agent, summary.clone());
            }
            DarbEvent::ProviderDelta { text } => {
                self.streaming.push_str(text);
                self.scroll = 0;
            }
            DarbEvent::ErrorOccurred { message } => {
                self.push(
                    MessageRole::System,
                    format!(
                        "⚠ {}",
                        global_format("agent.error", &[("message", message)])
                    ),
                );
            }
            DarbEvent::ConfigReloaded => {
                self.push(
                    MessageRole::System,
                    format!("✓ {}", global_text("app.config_reloaded")),
                );
            }
            DarbEvent::ContextRequested { .. } | DarbEvent::ReviewRequested { .. } => {}
        }
    }

    pub fn push(&mut self, role: MessageRole, text: String) {
        self.messages.push(ChatMessage { role, text });
        self.scroll = 0;
    }

    /// Drop the conversation, keeping the transcript on disk untouched
    /// (memory is written by the application layer, not here).
    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.streaming.clear();
        self.scroll = 0;
    }

    pub fn set_files(&mut self, files: Vec<FileEntry>) {
        self.files = files;
        self.clamp_explorer();
    }

    pub fn set_current_dir(&mut self, dir: String) {
        self.current_dir = dir;
        self.explorer_index = 0;
        self.clamp_explorer();
    }

    pub fn set_file_preview(&mut self, path: String, text: String) {
        self.edit_origin = text.clone();
        self.edit_trailing_newline = text.ends_with('\n');
        self.edit_lines = text.lines().map(str::to_string).collect();
        if self.edit_lines.is_empty() {
            // `str::lines` drops everything for empty input; the buffer
            // still needs one line so the cursor has somewhere to sit.
            self.edit_lines.push(String::new());
        }
        self.edit_row = 0;
        self.edit_col = 0;
        self.edit_dirty = false;
        self.editing = false;
        self.file_preview = Some(FilePreview { path, text });
        self.scroll = 0;
    }

    pub fn set_context(&mut self, lines: Vec<String>) {
        self.context_lines = lines;
    }

    pub fn set_diff(&mut self, text: &str) {
        self.diff_lines = text.lines().map(str::to_string).collect();
        self.scroll = 0;
    }

    pub fn set_git(&mut self, text: &str) {
        self.git_lines = text.lines().map(str::to_string).collect();
        self.scroll = 0;
    }

    /// Append captured command output to the terminal panel.
    pub fn push_terminal(&mut self, line: String) {
        self.terminal_lines.push(line);
        self.scroll = 0;
    }

    pub fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.scroll = 0;
    }

    /// Presentation mirror of `PermissionRequested`, for questions that do
    /// not come from the agent (a command typed in the terminal panel).
    pub fn ask_permission(&mut self, tool: &str, target: &str) {
        self.permission = Some(PermissionPrompt {
            tool: tool.to_string(),
            target: target.to_string(),
        });
    }

    pub fn toggle_explorer(&mut self) {
        self.explorer_visible = !self.explorer_visible;
    }

    /// Enter/exit edit mode. Entering needs an open file on the Files tab;
    /// exiting keeps the buffer — nothing reaches the disk until Ctrl+S.
    pub fn toggle_edit(&mut self) {
        if self.editing {
            self.editing = false;
        } else if self.tab == Tab::Files && self.file_preview.is_some() {
            self.editing = true;
        }
    }

    /// Current buffer exactly as it will be saved (trailing newline
    /// preserved from the snapshot, so saves are byte-faithful).
    fn edit_text(&self) -> String {
        let mut text = self.edit_lines.join("\n");
        if self.edit_trailing_newline {
            text.push('\n');
        }
        text
    }

    /// Save payload, or `None` when clean. An empty `edit_origin` is NOT
    /// filtered: saving an empty file is legitimate, and `edit_file`
    /// rejects it with a clear error the loop surfaces.
    pub fn take_save(&self) -> Option<(String, String, String)> {
        if !self.editing || !self.edit_dirty {
            return None;
        }
        let path = self.file_preview.as_ref()?.path.clone();
        Some((path, self.edit_origin.clone(), self.edit_text()))
    }

    /// Refresh the snapshot after a successful save. The buffer stays open
    /// (web parity: the editor does not close on save).
    pub fn confirm_saved(&mut self, content: String) {
        self.edit_origin = content.clone();
        self.edit_trailing_newline = content.ends_with('\n');
        self.edit_lines = content.lines().map(str::to_string).collect();
        if self.edit_lines.is_empty() {
            self.edit_lines.push(String::new());
        }
        self.clamp_edit_cursor();
        self.edit_dirty = false;
    }

    fn clamp_edit_cursor(&mut self) {
        self.edit_row = self.edit_row.min(self.edit_lines.len().saturating_sub(1));
        self.edit_col = self.edit_col.min(self.edit_line_len());
    }

    fn edit_line_len(&self) -> usize {
        self.edit_lines
            .get(self.edit_row)
            .map(|line| line.chars().count())
            .unwrap_or(0)
    }

    pub fn edit_insert(&mut self, c: char) {
        if self.edit_lines.is_empty() {
            self.edit_lines.push(String::new());
        }
        let col = self.edit_col.min(self.edit_line_len());
        let at = byte_idx(&self.edit_lines[self.edit_row], col);
        self.edit_lines[self.edit_row].insert(at, c);
        self.edit_col = col + 1;
        self.edit_dirty = true;
    }

    pub fn edit_backspace(&mut self) {
        if self.edit_col > 0 {
            let line = &mut self.edit_lines[self.edit_row];
            let prev = byte_idx(line, self.edit_col - 1);
            let cur = byte_idx(line, self.edit_col);
            line.drain(prev..cur);
            self.edit_col -= 1;
            self.edit_dirty = true;
        } else if self.edit_row > 0 {
            // Join with the previous line.
            let current = self.edit_lines.remove(self.edit_row);
            self.edit_row -= 1;
            self.edit_col = self.edit_line_len();
            self.edit_lines[self.edit_row].push_str(&current);
            self.edit_dirty = true;
        }
    }

    pub fn edit_newline(&mut self) {
        if self.edit_lines.is_empty() {
            self.edit_lines.push(String::new());
        }
        let at = byte_idx(&self.edit_lines[self.edit_row], self.edit_col);
        let tail = self.edit_lines[self.edit_row].split_off(at);
        self.edit_lines.insert(self.edit_row + 1, tail);
        self.edit_row += 1;
        self.edit_col = 0;
        self.edit_dirty = true;
    }

    pub fn edit_left(&mut self) {
        if self.edit_col > 0 {
            self.edit_col -= 1;
        } else if self.edit_row > 0 {
            self.edit_row -= 1;
            self.edit_col = self.edit_line_len();
        }
    }

    pub fn edit_right(&mut self) {
        if self.edit_col < self.edit_line_len() {
            self.edit_col += 1;
        } else if self.edit_row + 1 < self.edit_lines.len() {
            self.edit_row += 1;
            self.edit_col = 0;
        }
    }

    pub fn edit_up(&mut self) {
        self.edit_row = self.edit_row.saturating_sub(1);
        self.edit_col = self.edit_col.min(self.edit_line_len());
    }

    pub fn edit_down(&mut self) {
        if self.edit_row + 1 < self.edit_lines.len() {
            self.edit_row += 1;
            self.edit_col = self.edit_col.min(self.edit_line_len());
        }
    }

    /// Keys while the edit buffer owns the keyboard. Almost everything is
    /// consumed: a keystroke leaking into the command line mid-edit would
    /// corrupt both the buffer and the chat input.
    fn handle_edit_key(&mut self, key: KeyEvent) -> KeyOutcome {
        use crossterm::event::KeyModifiers;

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return match key.code {
                KeyCode::Char('s') | KeyCode::Char('S') => match self.take_save() {
                    Some((path, original, content)) => KeyOutcome::SaveFile {
                        path,
                        original,
                        content,
                    },
                    None => KeyOutcome::Ignored,
                },
                // Ctrl+E toggles both ways: entering is handled by the
                // normal path, but once editing owns the keyboard it must
                // also offer the way out (Esc works too).
                KeyCode::Char('e') | KeyCode::Char('E') => {
                    self.editing = false;
                    KeyOutcome::Ignored
                }
                KeyCode::Char('c') | KeyCode::Char('C') => KeyOutcome::CancelRequested,
                _ => KeyOutcome::Ignored,
            };
        }
        if key.modifiers != KeyModifiers::NONE {
            return KeyOutcome::Ignored;
        }
        match key.code {
            KeyCode::Esc => {
                self.editing = false;
                KeyOutcome::Ignored
            }
            KeyCode::Enter => {
                self.edit_newline();
                KeyOutcome::Ignored
            }
            KeyCode::Backspace => {
                self.edit_backspace();
                KeyOutcome::Ignored
            }
            KeyCode::Left => {
                self.edit_left();
                KeyOutcome::Ignored
            }
            KeyCode::Right => {
                self.edit_right();
                KeyOutcome::Ignored
            }
            KeyCode::Up => {
                self.edit_up();
                KeyOutcome::Ignored
            }
            KeyCode::Down => {
                self.edit_down();
                KeyOutcome::Ignored
            }
            KeyCode::Char(c) => {
                self.edit_insert(c);
                KeyOutcome::Ignored
            }
            // Tab never leaves the buffer mid-edit (focus would steal it);
            // two spaces is the visible, reversible fallback.
            KeyCode::Tab => {
                self.edit_insert(' ');
                self.edit_insert(' ');
                KeyOutcome::Ignored
            }
            _ => KeyOutcome::Ignored,
        }
    }

    /// True when the workspace (the active tab) is the focused region.
    pub fn workspace_focused(&self) -> bool {
        self.focus == Focus::Workspace
    }

    /// The explorer shows a `..` row whenever we are below the root.
    pub fn has_parent_entry(&self) -> bool {
        !self.current_dir.is_empty()
    }

    /// Total explorer rows (including the `..` entry). Public so mouse
    /// hit-testing clamps against the same count the renderer shows.
    pub fn explorer_rows(&self) -> usize {
        self.files.len() + usize::from(self.has_parent_entry())
    }

    fn clamp_explorer(&mut self) {
        self.explorer_index = self
            .explorer_index
            .min(self.explorer_rows().saturating_sub(1));
    }

    /// Handle one key. Overlays (palette, permission dialog) get the key
    /// first, then shortcuts, then plain text input.
    pub fn handle_key(&mut self, key: KeyEvent) -> KeyOutcome {
        if self.palette.is_some() {
            return self.handle_palette_key(key);
        }
        if self.permission.is_some() {
            return self.handle_permission_key(key);
        }
        if self.editing {
            return self.handle_edit_key(key);
        }
        match action_for(key) {
            Action::Quit => KeyOutcome::Quit,
            Action::CancelRequested => KeyOutcome::CancelRequested,
            Action::CommandPalette => {
                self.palette = Some(Palette::new());
                KeyOutcome::Ignored
            }
            Action::ToggleExplorer => {
                self.toggle_explorer();
                KeyOutcome::Ignored
            }
            Action::ToggleTerminal => {
                self.bottom_open = !self.bottom_open;
                KeyOutcome::Ignored
            }
            Action::ToggleContext => {
                self.context_visible = !self.context_visible;
                KeyOutcome::Ignored
            }
            Action::CycleMode => {
                self.agent_mode = self.agent_mode.next();
                KeyOutcome::Ignored
            }
            Action::ToggleEdit => {
                self.toggle_edit();
                KeyOutcome::Ignored
            }
            Action::ShowTab(tab) => {
                self.set_tab(tab);
                self.focus = Focus::Workspace;
                KeyOutcome::Ignored
            }
            Action::FocusNext => {
                self.focus = self.focus.next();
                KeyOutcome::Ignored
            }
            Action::ScrollUp => {
                self.move_up();
                KeyOutcome::Ignored
            }
            Action::ScrollDown => {
                self.move_down();
                KeyOutcome::Ignored
            }
            Action::Submit => self.submit(),
            // A/D only answer a dialog; without one they are normal text,
            // which is why they fall through to `type_key`.
            Action::PermissionAllow | Action::PermissionDeny | Action::Ignore => {
                self.type_key(key);
                KeyOutcome::Ignored
            }
        }
    }

    /// Up: move the explorer highlight or the tasks cursor up, or scroll
    /// the panel towards newer content (the offset counts from the bottom).
    fn move_up(&mut self) {
        if self.focus == Focus::Explorer {
            self.explorer_index = self.explorer_index.saturating_sub(1);
        } else if self.tasks_focused() {
            self.tasks_index = self.tasks_index.saturating_sub(1);
        } else {
            self.scroll = self.scroll.saturating_add(1);
        }
    }

    /// Down: move the explorer highlight or the tasks cursor down, or
    /// scroll the panel back.
    fn move_down(&mut self) {
        if self.focus == Focus::Explorer {
            let last = self.explorer_rows().saturating_sub(1);
            self.explorer_index = (self.explorer_index + 1).min(last);
        } else if self.tasks_focused() {
            let last = self.tasks.len().saturating_sub(1);
            self.tasks_index = (self.tasks_index + 1).min(last);
        } else {
            self.scroll = self.scroll.saturating_sub(1);
        }
    }

    /// True while ↑/↓/Enter should drive the tasks list instead of the
    /// generic panel scroll/submit.
    fn tasks_focused(&self) -> bool {
        self.focus == Focus::Workspace && self.tab == Tab::Tasks && !self.tasks.is_empty()
    }

    /// Enter on the Tasks tab expands the step under the cursor instead
    /// of submitting the command line (web parity). Shared with mouse
    /// clicks, so both expand the same step.
    pub(crate) fn toggle_task(&mut self) -> KeyOutcome {
        if self.tasks.is_empty() {
            return KeyOutcome::Ignored;
        }
        self.tasks_index = self.tasks_index.min(self.tasks.len() - 1);
        let task = &mut self.tasks[self.tasks_index];
        task.expanded = !task.expanded;
        KeyOutcome::Ignored
    }

    /// Enter: open in the explorer, toggle on the tasks tab, run in the
    /// terminal tab, send in the command line.
    fn submit(&mut self) -> KeyOutcome {
        match self.focus {
            Focus::Explorer => self.enter_explorer(),
            Focus::Workspace if self.tab == Tab::Tasks => self.toggle_task(),
            Focus::Workspace if self.tab == Tab::Terminal => {
                let line = std::mem::take(&mut self.input);
                if line.trim().is_empty() {
                    KeyOutcome::Ignored
                } else {
                    KeyOutcome::TerminalCommand(line)
                }
            }
            _ => {
                let line = std::mem::take(&mut self.input);
                if line.trim().is_empty() {
                    KeyOutcome::Ignored
                } else {
                    KeyOutcome::Submitted(line)
                }
            }
        }
    }

    fn enter_explorer(&mut self) -> KeyOutcome {
        self.enter_explorer_at(self.explorer_index)
    }

    /// Open the explorer row `row` (same indexing as `explorer_index`).
    /// Shared by the keyboard path and mouse clicks so both behave alike.
    pub fn enter_explorer_at(&mut self, row: usize) -> KeyOutcome {
        if self.has_parent_entry() && row == 0 {
            return KeyOutcome::GoUp;
        }
        let index = row - usize::from(self.has_parent_entry());
        match self.files.get(index) {
            Some(entry) => KeyOutcome::OpenEntry {
                path: join_path(&self.current_dir, &entry.name),
                is_dir: entry.is_dir,
            },
            None => KeyOutcome::Ignored,
        }
    }

    /// Handle one mouse event (Arquitetura §32: "clicável quando o
    /// terminal suportar mouse"). Left clicks map to the same
    /// `KeyOutcome`s keys produce; the wheel scrolls the focused region,
    /// mirroring ↑/↓. Overlays get the event first, exactly like
    /// `handle_key`.
    pub fn handle_mouse(&mut self, event: MouseEvent, area: Rect) -> KeyOutcome {
        let click = mouse::Click {
            x: event.column,
            y: event.row,
        };
        match event.kind {
            MouseEventKind::Up(MouseButton::Left) => {
                if self.palette.is_some() {
                    return self.handle_palette_click(click, area);
                }
                if self.permission.is_some() {
                    // Fail-safe: only [a]/[d]/Esc answer a permission ask,
                    // so no click can accidentally run a dangerous action
                    // (Negócio §40: safety before convenience).
                    return KeyOutcome::Ignored;
                }
                mouse::click_outcome(self, click, area)
            }
            MouseEventKind::ScrollUp => {
                self.move_up();
                KeyOutcome::Ignored
            }
            MouseEventKind::ScrollDown => {
                self.move_down();
                KeyOutcome::Ignored
            }
            // Drags, moves, other buttons: no meaning yet.
            _ => KeyOutcome::Ignored,
        }
    }

    /// Click inside the palette runs the command on that row (same as
    /// Enter); a click outside the popup closes it, like Esc.
    fn handle_palette_click(&mut self, click: mouse::Click, area: Rect) -> KeyOutcome {
        let matches: Vec<(Command, &'static str)> = self
            .palette
            .as_ref()
            .map(|palette| palette.filtered())
            .unwrap_or_default();
        let popup = popup_rect(area, matches.len());
        let inside = click.x >= popup.x
            && click.x < popup.x + popup.width
            && click.y >= popup.y
            && click.y < popup.y + popup.height;
        if !inside {
            self.palette = None;
            return KeyOutcome::Ignored;
        }
        // Inner rows: 0 = query header, 1 = blank, 2.. = matches, so the
        // first command sits three rows below the popup border.
        let row = (click.y - popup.y - 3) as usize;
        if click.y >= popup.y + 3 && row < matches.len() {
            self.palette = None;
            KeyOutcome::Command(matches[row].0)
        } else {
            KeyOutcome::Ignored
        }
    }

    fn handle_palette_key(&mut self, key: KeyEvent) -> KeyOutcome {
        let Some(palette) = self.palette.as_mut() else {
            return KeyOutcome::Ignored;
        };
        match key.code {
            KeyCode::Esc => {
                self.palette = None;
                KeyOutcome::Ignored
            }
            KeyCode::Enter => {
                let selected = palette.selected();
                self.palette = None;
                match selected {
                    Some(command) => KeyOutcome::Command(command),
                    None => KeyOutcome::Ignored,
                }
            }
            KeyCode::Up => {
                palette.move_selection(-1);
                KeyOutcome::Ignored
            }
            KeyCode::Down => {
                palette.move_selection(1);
                KeyOutcome::Ignored
            }
            KeyCode::Backspace => {
                palette.pop_char();
                KeyOutcome::Ignored
            }
            KeyCode::Char(c) => {
                palette.push_char(c);
                KeyOutcome::Ignored
            }
            _ => KeyOutcome::Ignored,
        }
    }

    fn handle_permission_key(&mut self, key: KeyEvent) -> KeyOutcome {
        match key.code {
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.permission = None;
                KeyOutcome::PermissionAnswer(true)
            }
            // Esc denies: an unanswered dangerous action must not run.
            KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Esc => {
                self.permission = None;
                KeyOutcome::PermissionAnswer(false)
            }
            _ => KeyOutcome::Ignored,
        }
    }

    fn type_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) => self.input.push(c),
            KeyCode::Backspace => {
                self.input.pop();
            }
            _ => {}
        }
    }

    fn finish_last_task(&mut self, success: bool) {
        if let Some(task) = self
            .tasks
            .iter_mut()
            .rev()
            .find(|task| task.state == TaskState::Pending)
        {
            task.state = if success {
                TaskState::Done
            } else {
                TaskState::Failed
            };
        }
    }
}

/// Byte index of the `col`-th character. Char-based cursor math needs
/// this on every edit; UTF-8 boundaries are never split.
fn byte_idx(line: &str, col: usize) -> usize {
    line.char_indices()
        .nth(col)
        .map(|(index, _)| index)
        .unwrap_or(line.len())
}

/// Join a project-relative directory with an entry name using `/`, which
/// the tools accept on every platform (`Component::Normal`).
fn join_path(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// i18n key for an agent state (labels live in the locales, never here).
pub fn status_key(state: &AgentState) -> &'static str {
    match state {
        AgentState::Idle => "app.ready",
        AgentState::Analyzing => "agent.thinking",
        AgentState::Planning => "agent.planning",
        AgentState::WaitingProvider => "agent.waiting",
        AgentState::WaitingPermission => "permissions.ask",
        AgentState::Executing => "agent.executing",
        AgentState::Reviewing => "agent.reviewing",
        AgentState::Retrying => "agent.retrying",
        AgentState::Completed => "agent.completed",
        AgentState::Failed => "agent.failed",
        AgentState::Cancelled => "agent.cancelled",
    }
}

/// Draw the whole shell: header, columns, tab bar, footer, overlays.
pub fn render(frame: &mut Frame, app: &App, theme: &Theme) {
    use crate::components::{chat, context, diff, explorer, git, tasks, terminal};
    use crate::layout::shell_layout;
    use ratatui::widgets::{Block, Borders, Paragraph};

    let layout = shell_layout(
        frame.area(),
        app.explorer_visible,
        app.context_visible,
        app.bottom_open,
    );
    let title = format!(
        "◈ DARB SHELL  ● {}  v0.1.0",
        global_text(status_key(&app.agent_state))
    );
    frame.render_widget(
        Paragraph::new(title).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border),
        ),
        layout.header,
    );

    if app.explorer_visible {
        explorer::render_explorer(frame, layout.explorer, app, theme);
    }
    match app.tab {
        Tab::Chat => chat::render_chat(frame, layout.workspace, app, theme),
        Tab::Files => explorer::render_preview(frame, layout.workspace, app, theme),
        Tab::Changes => diff::render_diff(frame, layout.workspace, app, theme),
        Tab::Terminal => terminal::render_terminal(frame, layout.workspace, app, theme),
        Tab::Git => git::render_git(frame, layout.workspace, app, theme),
        Tab::Tasks => tasks::render_tasks(frame, layout.workspace, app, theme),
    }
    if app.context_visible {
        context::render_context(frame, layout.context, app, theme);
    }
    if app.bottom_open {
        // Same buffer as the Terminal tab (no duplicated state); the tab
        // keeps the full scrollable view, this is the web's mini panel.
        // Focus glow lives here too until Fase 3 adds a terminal focus.
        terminal::render_terminal(frame, layout.terminal, app, theme);
    } else {
        render_terminal_strip(frame, layout.terminal, theme);
    }

    render_tab_bar(frame, layout.tabs, app, theme);
    render_statusbar(frame, layout.statusbar, app, theme);
    render_input(frame, layout.footer, app, theme);

    if let Some(palette) = &app.palette {
        render_palette(frame, frame.area(), palette, theme);
    }
    if let Some(prompt) = &app.permission {
        crate::dialogs::render_permission(frame, frame.area(), prompt, theme);
    }
}

/// Tab bar: active tab highlighted, shortcut hint on the right.
fn render_tab_bar(frame: &mut Frame, area: ratatui::layout::Rect, app: &App, theme: &Theme) {
    use ratatui::layout::{Constraint, Layout};
    use ratatui::style::Style;
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let mut spans: Vec<Span> = Vec::new();
    for (position, tab) in Tab::all().iter().enumerate() {
        if position > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(theme.muted)));
        }
        let style = if *tab == app.tab {
            Style::default().fg(theme.highlight)
        } else {
            Style::default().fg(theme.muted)
        };
        spans.push(Span::styled(global_text(tab.title_key()), style));
    }

    // Width the labels actually need (` │ ` separators included). The hint
    // only shows when it fits beside them: clipping "Tarefas" in half would
    // be worse than hiding the hint (§16 language/layout safety).
    let labels_width: u16 = Tab::all()
        .iter()
        .map(|tab| tab_label_width(*tab))
        .sum::<u16>()
        + TAB_SEPARATOR_WIDTH * (Tab::all().len() as u16 - 1);
    let hint = global_text("chat.hint");
    let hint_width = hint.chars().count() as u16;
    let show_hint = area.width > labels_width + hint_width + 3;
    let columns = Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            Constraint::Length(labels_width),
            Constraint::Min(if show_hint { hint_width } else { 1 }),
        ])
        .split(area);
    frame.render_widget(Paragraph::new(Line::from(spans)), columns[0]);
    if show_hint {
        frame.render_widget(
            Paragraph::new(hint).style(Style::default().fg(theme.muted)),
            columns[1],
        );
    }
}

/// Command line, with a placeholder and a focus-dependent border.
fn render_input(frame: &mut Frame, area: ratatui::layout::Rect, app: &App, theme: &Theme) {
    use ratatui::style::Style;
    use ratatui::widgets::{Block, Borders, Paragraph};

    let focused = app.focus == Focus::Input;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(theme.primary)
        } else {
            Style::default().fg(theme.border)
        });
    if app.input.is_empty() {
        frame.render_widget(
            Paragraph::new(global_text("chat.placeholder"))
                .style(Style::default().fg(theme.muted))
                .block(block),
            area,
        );
        return;
    }
    frame.render_widget(
        Paragraph::new(format!("> {}", app.input)).block(block),
        area,
    );
}

/// Collapsed bottom strip: one line, like the web's `▴ TERMINAL`.
fn render_terminal_strip(frame: &mut Frame, area: Rect, theme: &Theme) {
    use ratatui::style::Style;
    use ratatui::widgets::Paragraph;

    let label = format!("▴ {} (Ctrl+J)", global_text("panel.terminal"));
    frame.render_widget(
        Paragraph::new(label).style(Style::default().fg(theme.muted)),
        area,
    );
}

/// One-line status bar: agent state, task counts, pending permission.
/// Only real state — no invented telemetry (Contribuição §61).
fn render_statusbar(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::Style;
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let mut left = vec![Span::styled(
        global_text(status_key(&app.agent_state)),
        Style::default().fg(theme.accent),
    )];
    if !app.tasks.is_empty() {
        let (done, pending, failed) = task_counts(app);
        left.push(Span::styled(" │ ", Style::default().fg(theme.muted)));
        left.push(Span::styled(
            format!("✓ {done} ○ {pending} ✗ {failed}"),
            Style::default().fg(theme.secondary),
        ));
    }
    let right = if app.permission.is_some() {
        Line::styled(
            global_text("permissions.ask"),
            Style::default().fg(theme.warning),
        )
    } else {
        Line::styled(
            global_text(app.tab.title_key()),
            Style::default().fg(theme.muted),
        )
    };
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(right.width() as u16),
        ])
        .split(area);
    frame.render_widget(Paragraph::new(Line::from(left)), columns[0]);
    frame.render_widget(Paragraph::new(right), columns[1]);
}

fn task_counts(app: &App) -> (usize, usize, usize) {
    let (mut done, mut pending, mut failed) = (0, 0, 0);
    for task in &app.tasks {
        match task.state {
            TaskState::Done => done += 1,
            TaskState::Pending => pending += 1,
            TaskState::Failed => failed += 1,
        }
    }
    (done, pending, failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        let area = terminal.backend().buffer().area;
        let width = area.width as usize;
        terminal
            .backend()
            .buffer()
            .content
            .chunks(width)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn draw(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(frame, app, &Theme::default()))
            .expect("draw");
        buffer_text(&terminal)
    }

    #[test]
    fn events_fold_into_state() {
        let mut app = App::new();
        app.apply_event(&DarbEvent::AgentStarted {
            task: "fix login".to_string(),
        });
        assert_eq!(app.agent_state, AgentState::Analyzing);
        app.apply_event(&DarbEvent::AgentStateChanged {
            from: AgentState::Analyzing,
            to: AgentState::Executing,
        });
        assert_eq!(app.agent_state, AgentState::Executing);
        app.apply_event(&DarbEvent::AgentFinished {
            summary: "done".to_string(),
        });
        assert!(matches!(
            app.messages.last(),
            Some(ChatMessage {
                role: MessageRole::Agent,
                ..
            })
        ));
        app.apply_event(&DarbEvent::ErrorOccurred {
            message: "boom".to_string(),
        });
        assert!(app.messages.iter().any(|m| m.text.contains("boom")));
    }

    #[test]
    fn tool_events_build_the_task_list() {
        let mut app = App::new();
        app.apply_event(&DarbEvent::AgentStarted {
            task: "fix login".to_string(),
        });
        app.apply_event(&DarbEvent::ToolRequested {
            tool: "read_file".to_string(),
            target: "a.rs".to_string(),
        });
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].state, TaskState::Pending);
        assert_eq!(app.tasks[0].tool, "read_file");
        assert_eq!(app.tasks[0].target, "a.rs");
        assert!(!app.tasks[0].expanded);
        app.apply_event(&DarbEvent::ToolCompleted {
            tool: "read_file".to_string(),
            success: true,
        });
        assert_eq!(app.tasks[0].state, TaskState::Done);
        app.apply_event(&DarbEvent::ToolRequested {
            tool: "edit_file".to_string(),
            target: "a.rs".to_string(),
        });
        app.apply_event(&DarbEvent::ToolCompleted {
            tool: "edit_file".to_string(),
            success: false,
        });
        assert_eq!(app.tasks[1].state, TaskState::Failed);
    }

    #[test]
    fn permission_dialog_blocks_input_until_answered() {
        let mut app = App::new();
        app.apply_event(&DarbEvent::PermissionRequested {
            tool: "shell".to_string(),
            target: "rm -rf /tmp/x".to_string(),
        });
        assert!(app.permission.is_some());
        // Typing is captured by the modal…
        assert_eq!(app.handle_key(key(KeyCode::Char('x'))), KeyOutcome::Ignored);
        assert!(app.input.is_empty());
        // …until answered.
        assert_eq!(
            app.handle_key(key(KeyCode::Char('a'))),
            KeyOutcome::PermissionAnswer(true)
        );
        assert!(app.permission.is_none());
    }

    #[test]
    fn escape_denies_a_pending_question() {
        let mut app = App::new();
        app.ask_permission("shell", "echo hi");
        assert_eq!(
            app.handle_key(key(KeyCode::Esc)),
            KeyOutcome::PermissionAnswer(false)
        );
        assert!(app.permission.is_none());
    }

    #[test]
    fn typing_submit_and_scroll() {
        let mut app = App::new();
        for c in "hi".chars() {
            assert_eq!(app.handle_key(key(KeyCode::Char(c))), KeyOutcome::Ignored);
        }
        assert_eq!(app.input, "hi");
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            KeyOutcome::Submitted("hi".to_string())
        );
        assert!(app.input.is_empty());
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            KeyOutcome::Ignored,
            "empty line submits nothing"
        );
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.scroll, 1);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.scroll, 0);
        app.handle_key(key(KeyCode::Backspace));
        assert!(app.input.is_empty());
    }

    #[test]
    fn ctrl_k_opens_palette_and_enter_runs_a_command() {
        let mut app = App::new();
        assert_eq!(app.handle_key(ctrl('k')), KeyOutcome::Ignored);
        assert!(app.palette.is_some());
        // Filter down to the diff command and run it.
        for c in "diff".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        assert_eq!(app.input, "", "palette keystrokes never reach the line");
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            KeyOutcome::Command(Command::ShowDiff)
        );
        assert!(app.palette.is_none());
    }

    #[test]
    fn escaping_the_palette_keeps_the_session() {
        let mut app = App::new();
        app.handle_key(ctrl('k'));
        assert_eq!(app.handle_key(key(KeyCode::Esc)), KeyOutcome::Ignored);
        assert!(app.palette.is_none());
        // Esc with no overlay still quits.
        assert_eq!(app.handle_key(key(KeyCode::Esc)), KeyOutcome::Quit);
    }

    #[test]
    fn ctrl_letters_switch_tabs() {
        let mut app = App::new();
        app.handle_key(ctrl('p'));
        assert_eq!(app.tab, Tab::Files);
        assert_eq!(app.focus, Focus::Workspace);
        app.handle_key(ctrl('g'));
        assert_eq!(app.tab, Tab::Git);
        app.handle_key(ctrl('t'));
        assert_eq!(app.tab, Tab::Terminal);
    }

    #[test]
    fn focus_cycles_through_the_three_regions() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Explorer);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Workspace);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Input);
    }

    #[test]
    fn explorer_navigates_and_opens_entries() {
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
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Explorer);

        // Row 0 is `..` while we are below the root.
        assert_eq!(app.handle_key(key(KeyCode::Enter)), KeyOutcome::GoUp);

        app.handle_key(key(KeyCode::Down));
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            KeyOutcome::OpenEntry {
                path: "crates/core".to_string(),
                is_dir: true,
            }
        );
        app.handle_key(key(KeyCode::Down));
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            KeyOutcome::OpenEntry {
                path: "crates/lib.rs".to_string(),
                is_dir: false,
            }
        );
        // Selection stops at the last row instead of running past it.
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.explorer_index, 2);
    }

    #[test]
    fn terminal_tab_runs_the_typed_command() {
        let mut app = App::new();
        app.handle_key(ctrl('t'));
        for c in "cargo test".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            KeyOutcome::TerminalCommand("cargo test".to_string())
        );
    }

    #[test]
    fn status_keys_resolve_in_default_locale() {
        use darb_core::i18n::global_text;
        // Default global language is pt; every state must have a label.
        for state in [
            AgentState::Idle,
            AgentState::Analyzing,
            AgentState::Planning,
            AgentState::WaitingProvider,
            AgentState::WaitingPermission,
            AgentState::Executing,
            AgentState::Reviewing,
            AgentState::Retrying,
            AgentState::Completed,
            AgentState::Failed,
            AgentState::Cancelled,
        ] {
            let key = status_key(&state);
            let text = global_text(key);
            assert!(!text.is_empty() && text != key, "{state:?}");
        }
    }

    #[test]
    fn every_tab_has_a_translated_label() {
        use darb_core::i18n::global_text;
        for tab in Tab::all() {
            assert_ne!(global_text(tab.title_key()), tab.title_key());
        }
    }

    #[test]
    fn renders_full_shell_on_test_backend() {
        let mut app = App::new();
        app.push(MessageRole::User, "hello".to_string());
        app.set_files(vec![FileEntry {
            name: "a.txt".to_string(),
            is_dir: false,
        }]);
        app.set_context(vec!["Files: 1".to_string()]);
        app.ask_permission("shell", "echo hi");

        let screen = draw(&app, 110, 34);
        assert!(screen.contains("DARB SHELL"), "{screen}");
        assert!(screen.contains("hello"), "{screen}");
        assert!(screen.contains("a.txt"), "{screen}");
        assert!(screen.contains("Files: 1"), "{screen}");
        assert!(screen.contains("shell"), "{screen}");
        // Tab bar with every tab, and the palette hint line.
        for tab in Tab::all() {
            let label = darb_core::i18n::global_text(tab.title_key());
            assert!(
                screen.contains(label.as_str()),
                "{label} missing:\n{screen}"
            );
        }
    }

    #[test]
    fn each_tab_renders_its_panel() {
        let mut app = App::new();
        app.set_tab(Tab::Changes);
        app.set_diff("--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new");
        let screen = draw(&app, 110, 34);
        assert!(screen.contains("+new"), "{screen}");
        assert!(screen.contains("-old"), "{screen}");

        app.set_tab(Tab::Git);
        app.set_git("## main\n?? new.txt\n M a.txt");
        let screen = draw(&app, 110, 34);
        assert!(screen.contains("## main"), "{screen}");
        assert!(screen.contains("?? new.txt"), "{screen}");

        app.set_tab(Tab::Terminal);
        app.push_terminal("$ echo hi".to_string());
        app.push_terminal("hi".to_string());
        let screen = draw(&app, 110, 34);
        assert!(screen.contains("$ echo hi"), "{screen}");

        app.set_tab(Tab::Tasks);
        app.apply_event(&DarbEvent::ToolRequested {
            tool: "search".to_string(),
            target: "needle".to_string(),
        });
        let screen = draw(&app, 110, 34);
        assert!(screen.contains("search"), "{screen}");
        assert!(screen.contains('○'), "{screen}");

        app.set_tab(Tab::Files);
        app.set_file_preview("a.txt".to_string(), "content-here".to_string());
        let screen = draw(&app, 110, 34);
        assert!(screen.contains("content-here"), "{screen}");
    }

    #[test]
    fn palette_overlay_lists_commands() {
        let mut app = App::new();
        app.handle_key(ctrl('k'));
        let screen = draw(&app, 110, 34);
        let title = darb_core::i18n::global_text("palette.title");
        let quit = darb_core::i18n::global_text("cmd.quit");
        assert!(
            screen.contains(title.as_str()),
            "{title} missing:\n{screen}"
        );
        assert!(screen.contains(quit.as_str()), "{quit} missing:\n{screen}");
    }

    #[test]
    fn empty_state_shows_placeholders_not_blank_panels() {
        let app = App::new();
        let screen = draw(&app, 110, 34);
        let placeholder = darb_core::i18n::global_text("chat.placeholder");
        let empty = darb_core::i18n::global_text("panel.empty");
        assert!(screen.contains(placeholder.as_str()), "{screen}");
        assert!(screen.contains(empty.as_str()), "{screen}");
    }

    #[test]
    fn ctrl_j_and_l_toggle_panels() {
        let mut app = App::new();
        assert!(app.bottom_open && app.context_visible);
        app.handle_key(ctrl('j'));
        assert!(!app.bottom_open);
        app.handle_key(ctrl('j'));
        assert!(app.bottom_open);
        app.handle_key(ctrl('l'));
        assert!(!app.context_visible);
        app.handle_key(ctrl('l'));
        assert!(app.context_visible);
    }

    #[test]
    fn statusbar_shows_task_counts() {
        let mut app = App::new();
        app.apply_event(&DarbEvent::AgentStarted {
            task: "fix login".to_string(),
        });
        app.apply_event(&DarbEvent::ToolRequested {
            tool: "read_file".to_string(),
            target: "a.rs".to_string(),
        });
        app.apply_event(&DarbEvent::ToolCompleted {
            tool: "read_file".to_string(),
            success: true,
        });
        let screen = draw(&app, 110, 34);
        assert!(screen.contains("✓ 1"), "{screen}");
        assert!(screen.contains("○ 0"), "{screen}");
    }

    #[test]
    fn closed_terminal_renders_the_collapsed_strip() {
        let mut app = App::new();
        app.handle_key(ctrl('j'));
        let screen = draw(&app, 110, 34);
        assert!(screen.contains("Ctrl+J"), "{screen}");
    }

    #[test]
    fn ctrl_o_cycles_through_all_modes_and_wraps() {
        let mut app = App::new();
        assert_eq!(app.agent_mode, AgentMode::Auto);
        let mut seen = vec![app.agent_mode.name()];
        for _ in 0..6 {
            app.handle_key(ctrl('o'));
            seen.push(app.agent_mode.name());
        }
        assert_eq!(
            seen,
            ["AUTO", "PLAN", "CODE", "DEBUG", "REVIEW", "EXPLAIN", "AUTO"]
        );
    }

    #[test]
    fn chat_title_shows_the_current_mode() {
        let mut app = App::new();
        assert!(draw(&app, 110, 34).contains("[AUTO]"));
        app.handle_key(ctrl('o'));
        let screen = draw(&app, 110, 34);
        assert!(screen.contains("[PLAN]"), "{screen}");
    }

    #[test]
    fn tasks_cursor_moves_and_enter_toggles_expansion() {
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
        // Input → Explorer → Workspace.
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Workspace);
        // Cursor starts at 0; Down moves, Up clamps.
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.tasks_index, 1);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.tasks_index, 1, "cursor stops at the last step");
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.tasks_index, 0);
        // Enter expands the step under the cursor instead of submitting.
        assert_eq!(app.handle_key(key(KeyCode::Enter)), KeyOutcome::Ignored);
        assert!(app.tasks[0].expanded);
        assert!(!app.tasks[1].expanded);
        let screen = draw(&app, 110, 34);
        assert!(screen.contains("read_file → a.rs"), "{screen}");
    }

    #[test]
    fn task_at_row_counts_expanded_detail_lines() {
        let tasks = vec![
            TaskItem {
                tool: "read_file".to_string(),
                target: "a.rs".to_string(),
                state: TaskState::Done,
                expanded: false,
            },
            TaskItem {
                tool: "edit_file".to_string(),
                target: "b.rs".to_string(),
                state: TaskState::Pending,
                expanded: true,
            },
        ];
        assert_eq!(task_at_row(&tasks, 0), Some(0));
        assert_eq!(task_at_row(&tasks, 1), Some(1));
        assert_eq!(
            task_at_row(&tasks, 2),
            Some(1),
            "detail line maps to its step"
        );
        assert_eq!(task_at_row(&tasks, 3), None);
        assert_eq!(task_at_row(&[], 0), None);
    }

    fn open_for_edit(app: &mut App) {
        app.set_file_preview("a.txt".to_string(), "ab\ncd\n".to_string());
        app.set_tab(Tab::Files);
        app.handle_key(ctrl('e'));
        assert!(app.editing, "Ctrl+E enters edit mode on an open file");
    }

    #[test]
    fn edit_mode_needs_an_open_file() {
        let mut app = App::new();
        app.handle_key(ctrl('e'));
        assert!(!app.editing, "no preview open");
        app.set_tab(Tab::Files);
        app.handle_key(ctrl('e'));
        assert!(!app.editing, "Files tab but still no preview");
        app.set_file_preview("a.txt".to_string(), "x".to_string());
        app.handle_key(ctrl('e'));
        assert!(app.editing);
        app.handle_key(ctrl('e'));
        assert!(!app.editing, "toggles back off, buffer kept");
        assert!(app.file_preview.is_some());
    }

    #[test]
    fn typing_and_save_preserve_the_trailing_newline() {
        let mut app = App::new();
        open_for_edit(&mut app);
        app.handle_key(key(KeyCode::Char('X')));
        assert!(app.edit_dirty);
        let (path, original, content) = app.take_save().expect("dirty buffer saves");
        assert_eq!(path, "a.txt");
        assert_eq!(original, "ab\ncd\n");
        assert_eq!(content, "Xab\ncd\n");
        match app.handle_key(ctrl('s')) {
            KeyOutcome::SaveFile {
                path,
                original,
                content,
            } => {
                assert_eq!(path, "a.txt");
                assert_eq!(original, "ab\ncd\n");
                assert_eq!(content, "Xab\ncd\n");
            }
            other => panic!("expected SaveFile, got {other:?}"),
        }
    }

    #[test]
    fn clean_buffer_saves_nothing() {
        let mut app = App::new();
        open_for_edit(&mut app);
        assert!(app.take_save().is_none());
        assert_eq!(app.handle_key(ctrl('s')), KeyOutcome::Ignored);
    }

    #[test]
    fn backspace_at_line_start_joins_lines() {
        let mut app = App::new();
        open_for_edit(&mut app);
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.edit_lines, vec!["abcd".to_string()]);
        assert_eq!((app.edit_row, app.edit_col), (0, 2));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.edit_lines, vec!["ab".to_string(), "cd".to_string()]);
        assert_eq!((app.edit_row, app.edit_col), (1, 0));
    }

    #[test]
    fn arrows_wrap_across_lines_and_clamp() {
        let mut app = App::new();
        open_for_edit(&mut app);
        // Left at (0,0) stays; Right walks and wraps to the next line.
        app.handle_key(key(KeyCode::Left));
        assert_eq!((app.edit_row, app.edit_col), (0, 0));
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Right));
        assert_eq!((app.edit_row, app.edit_col), (1, 0));
        // Up keeps the column clamped to the shorter line.
        app.handle_key(key(KeyCode::Up));
        assert_eq!((app.edit_row, app.edit_col), (0, 0));
        // Down past the end stays.
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Down));
        assert_eq!((app.edit_row, app.edit_col), (1, 0));
    }

    #[test]
    fn esc_exits_edit_mode_without_saving() {
        let mut app = App::new();
        open_for_edit(&mut app);
        app.handle_key(key(KeyCode::Char('Z')));
        assert_eq!(app.handle_key(key(KeyCode::Esc)), KeyOutcome::Ignored);
        assert!(!app.editing);
        assert!(app.edit_dirty, "buffer kept for the next edit session");
        assert!(
            app.input.is_empty(),
            "keystrokes never reached the command line"
        );
    }

    #[test]
    fn cursor_math_is_char_based_not_byte_based() {
        let mut app = App::new();
        app.set_file_preview("u.txt".to_string(), "héllo\n".to_string());
        app.set_tab(Tab::Files);
        app.handle_key(ctrl('e'));
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Char('X')));
        assert_eq!(app.edit_lines[0], "hXéllo");
        assert_eq!((app.edit_row, app.edit_col), (0, 2));
    }

    #[test]
    fn confirm_saved_refreshes_the_snapshot() {
        let mut app = App::new();
        open_for_edit(&mut app);
        app.handle_key(key(KeyCode::Char('X')));
        let (_, _, content) = app.take_save().expect("save");
        app.confirm_saved(content);
        assert!(!app.edit_dirty);
        assert!(app.editing, "editor stays open after save");
        assert!(app.take_save().is_none());
    }

    #[test]
    fn editing_renders_cursor_and_hint() {
        let mut app = App::new();
        open_for_edit(&mut app);
        let screen = draw(&app, 110, 34);
        assert!(screen.contains('▍'), "cursor marker:\n{screen}");
        let hint = darb_core::i18n::global_text("file.edit_hint");
        assert!(screen.contains(hint.as_str()), "{screen}");
    }
}
