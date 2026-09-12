//! App state: the single presentation model.
//!
//! The TUI decides nothing (Contribuição §26): [`App::apply_event`] folds
//! `darb-core` events into display data, [`App::handle_key`] turns keys
//! into [`KeyOutcome`]s for `apps/darb` to execute, and [`render`] draws
//! it all. No provider, tool, or agent calls happen here — the panels are
//! filled from the outside (`set_files`, `set_diff`, `push_terminal`, …).

use crossterm::event::{KeyCode, KeyEvent};
use darb_core::events::{AgentState, DarbEvent};
use darb_core::i18n::{global_format, global_text};
use ratatui::Frame;

use crate::keybindings::{action_for, Action};
use crate::palette::{render_palette, Command, Palette};
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
    pub label: String,
    pub state: TaskState,
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
    Ignored,
}

pub struct App {
    pub agent_state: AgentState,
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
    pub permission: Option<PermissionPrompt>,
    pub focus: Focus,
    pub tab: Tab,
    /// Directory currently listed, relative to the project root.
    pub current_dir: String,
    pub explorer_index: usize,
    pub file_preview: Option<FilePreview>,
    pub diff_lines: Vec<String>,
    pub git_lines: Vec<String>,
    pub terminal_lines: Vec<String>,
    pub tasks: Vec<TaskItem>,
    /// `Some` while the Ctrl+K palette is open.
    pub palette: Option<Palette>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            agent_state: AgentState::Idle,
            messages: Vec::new(),
            streaming: String::new(),
            files: Vec::new(),
            context_lines: Vec::new(),
            input: String::new(),
            scroll: 0,
            explorer_visible: true,
            permission: None,
            focus: Focus::Input,
            tab: Tab::Chat,
            current_dir: String::new(),
            explorer_index: 0,
            file_preview: None,
            diff_lines: Vec::new(),
            git_lines: Vec::new(),
            terminal_lines: Vec::new(),
            tasks: Vec::new(),
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
                    label: global_format(
                        "agent.tool_requested",
                        &[("tool", tool), ("target", target)],
                    ),
                    state: TaskState::Pending,
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

    /// True when the workspace (the active tab) is the focused region.
    pub fn workspace_focused(&self) -> bool {
        self.focus == Focus::Workspace
    }

    /// The explorer shows a `..` row whenever we are below the root.
    pub fn has_parent_entry(&self) -> bool {
        !self.current_dir.is_empty()
    }

    fn explorer_rows(&self) -> usize {
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

    /// Up: move the explorer highlight up, or scroll the panel towards
    /// newer content (the offset is counted from the bottom).
    fn move_up(&mut self) {
        if self.focus == Focus::Explorer {
            self.explorer_index = self.explorer_index.saturating_sub(1);
        } else {
            self.scroll = self.scroll.saturating_add(1);
        }
    }

    /// Down: move the explorer highlight down, or scroll the panel back.
    fn move_down(&mut self) {
        if self.focus == Focus::Explorer {
            let last = self.explorer_rows().saturating_sub(1);
            self.explorer_index = (self.explorer_index + 1).min(last);
        } else {
            self.scroll = self.scroll.saturating_sub(1);
        }
    }

    /// Enter: open in the explorer, run in the terminal tab, send in the
    /// command line.
    fn submit(&mut self) -> KeyOutcome {
        match self.focus {
            Focus::Explorer => self.enter_explorer(),
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
        if self.has_parent_entry() && self.explorer_index == 0 {
            return KeyOutcome::GoUp;
        }
        let row = self.explorer_index - usize::from(self.has_parent_entry());
        match self.files.get(row) {
            Some(entry) => KeyOutcome::OpenEntry {
                path: join_path(&self.current_dir, &entry.name),
                is_dir: entry.is_dir,
            },
            None => KeyOutcome::Ignored,
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

    let layout = shell_layout(frame.area(), app.explorer_visible);
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
    context::render_context(frame, layout.context, app, theme);

    render_tab_bar(frame, layout.tabs, app, theme);
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
        .map(|tab| global_text(tab.title_key()).chars().count() as u16)
        .sum::<u16>()
        + 3 * (Tab::all().len() as u16 - 1);
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
        assert!(app.tasks[0].label.contains("read_file"));
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
}
