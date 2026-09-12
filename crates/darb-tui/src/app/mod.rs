//! App state: the single presentation model.
//!
//! The TUI decides nothing (Contribuição §26): [`App::apply_event`]
//! folds `darb-core` events into display data, [`App::handle_key`]
//! turns keys into [`KeyOutcome`]s for `apps/darb` to execute, and
//! [`render`] draws it all. No provider, tool, or agent calls happen here.

use crossterm::event::{KeyCode, KeyEvent};
use darb_core::events::{AgentState, DarbEvent};
use ratatui::Frame;

use crate::keybindings::{action_for, Action};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyOutcome {
    Quit,
    Submitted(String),
    CancelRequested,
    PermissionAnswer(bool),
    Ignored,
}

pub struct App {
    pub agent_state: AgentState,
    pub messages: Vec<ChatMessage>,
    pub files: Vec<FileEntry>,
    pub context_lines: Vec<String>,
    pub input: String,
    pub chat_scroll: u16,
    pub explorer_visible: bool,
    pub permission: Option<PermissionPrompt>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            agent_state: AgentState::Idle,
            messages: Vec::new(),
            files: Vec::new(),
            context_lines: Vec::new(),
            input: String::new(),
            chat_scroll: 0,
            explorer_visible: true,
            permission: None,
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
                self.push(MessageRole::System, format!("◈ {task}"));
            }
            DarbEvent::AgentStateChanged { to, .. } => {
                self.agent_state = to.clone();
            }
            DarbEvent::ToolRequested { tool, target } => {
                self.push(MessageRole::System, format!("◉ {tool} {target}"));
            }
            DarbEvent::ToolCompleted { tool, success } => {
                if !success {
                    self.push(MessageRole::System, format!("✗ {tool} failed"));
                }
            }
            DarbEvent::PermissionRequested { tool, target } => {
                self.permission = Some(PermissionPrompt {
                    tool: tool.clone(),
                    target: target.clone(),
                });
            }
            DarbEvent::PermissionResolved { allowed, .. } => {
                if *allowed {
                    self.permission = None;
                } else {
                    self.permission = None;
                    self.push(MessageRole::System, "⌀ denied by user".to_string());
                }
            }
            DarbEvent::ContextReady { files } => {
                self.context_lines = vec![format!("Files: {files}")];
            }
            DarbEvent::AgentFinished { summary } => {
                self.push(MessageRole::Agent, summary.clone());
            }
            DarbEvent::ErrorOccurred { message } => {
                self.push(MessageRole::System, format!("⚠ {message}"));
            }
            DarbEvent::ConfigReloaded => {
                self.push(MessageRole::System, "✓ config reloaded".to_string());
            }
            DarbEvent::ContextRequested { .. } | DarbEvent::ReviewRequested { .. } => {}
        }
    }

    pub fn push(&mut self, role: MessageRole, text: String) {
        self.messages.push(ChatMessage { role, text });
        self.chat_scroll = 0;
    }

    pub fn set_files(&mut self, files: Vec<FileEntry>) {
        self.files = files;
    }

    pub fn set_context(&mut self, lines: Vec<String>) {
        self.context_lines = lines;
    }

    /// Handle one key. Character input only lands in the line when no
    /// modal dialog is open; `a`/`d` answer the dialog instead.
    pub fn handle_key(&mut self, key: KeyEvent) -> KeyOutcome {
        match action_for(key) {
            Action::Quit => KeyOutcome::Quit,
            Action::CancelRequested => KeyOutcome::CancelRequested,
            Action::ToggleExplorer => {
                self.explorer_visible = !self.explorer_visible;
                KeyOutcome::Ignored
            }
            Action::ScrollUp => {
                self.chat_scroll = self.chat_scroll.saturating_add(1);
                KeyOutcome::Ignored
            }
            Action::ScrollDown => {
                self.chat_scroll = self.chat_scroll.saturating_sub(1);
                KeyOutcome::Ignored
            }
            Action::FocusNext | Action::CommandPalette => KeyOutcome::Ignored,
            Action::Submit => {
                if self.permission.is_some() {
                    return KeyOutcome::Ignored;
                }
                let line = std::mem::take(&mut self.input);
                if line.trim().is_empty() {
                    KeyOutcome::Ignored
                } else {
                    KeyOutcome::Submitted(line)
                }
            }
            Action::PermissionAllow => {
                if self.permission.take().is_some() {
                    KeyOutcome::PermissionAnswer(true)
                } else {
                    self.insert_char(key);
                    KeyOutcome::Ignored
                }
            }
            Action::PermissionDeny => {
                if self.permission.take().is_some() {
                    KeyOutcome::PermissionAnswer(false)
                } else {
                    self.insert_char(key);
                    KeyOutcome::Ignored
                }
            }
            Action::Ignore => {
                self.insert_char(key);
                if key.code == KeyCode::Backspace && self.permission.is_none() {
                    self.input.pop();
                }
                KeyOutcome::Ignored
            }
        }
    }

    fn insert_char(&mut self, key: KeyEvent) {
        if self.permission.is_some() {
            return;
        }
        if let KeyCode::Char(c) = key.code {
            self.input.push(c);
        }
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

/// Draw the whole shell: header, three columns, input footer, modal dialog.
pub fn render(frame: &mut Frame, app: &App, theme: &Theme) {
    use crate::components::{chat, context, explorer};
    use crate::layout::shell_layout;
    use darb_core::i18n::global_text;
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
    chat::render_chat(frame, layout.workspace, app, theme);
    context::render_context(frame, layout.context, app, theme);

    frame.render_widget(
        Paragraph::new(format!("> {}", app.input)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border),
        ),
        layout.footer,
    );

    if let Some(prompt) = &app.permission {
        crate::dialogs::render_permission(frame, frame.area(), prompt, theme);
    }
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
        assert_eq!(app.chat_scroll, 1);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.chat_scroll, 0);
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

    #[test]
    fn renders_full_shell_on_test_backend() {
        let theme = Theme::default();
        let mut app = App::new();
        app.push(MessageRole::User, "hello".to_string());
        app.set_files(vec![FileEntry {
            name: "a.txt".to_string(),
            is_dir: false,
        }]);
        app.set_context(vec!["Files: 1".to_string()]);
        app.apply_event(&DarbEvent::PermissionRequested {
            tool: "shell".to_string(),
            target: "echo hi".to_string(),
        });

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(frame, &app, &theme))
            .expect("draw");
        let screen = buffer_text(&terminal);
        assert!(screen.contains("DARB SHELL"), "{screen}");
        assert!(screen.contains("hello"), "{screen}");
        assert!(screen.contains("a.txt"), "{screen}");
        assert!(screen.contains("Files: 1"), "{screen}");
        assert!(screen.contains("shell"), "{screen}");
    }
}
