//! Interactive TUI loop: terminal setup, key handling, agent wiring.
//!
//! Threading is deliberately boring: the UI owns the main thread and
//! blocks in `crossterm::event::poll`; agent runs go to the tokio runtime
//! via its `Handle` and report back through the [`EventBus`]. Permission
//! questions arrive as events (display) while the reply travels on a
//! oneshot channel — the dialog only ever answers, never executes.
//!
//! Every panel is filled from here, through the tool registry, so browsing
//! and shell commands pass the same permission gate as the agent
//! (Contribuição §28). The TUI itself never touches the disk.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use darb_agent::{Agent, PermissionResponder};
use darb_core::config::DarbConfig;
use darb_core::events::{DarbEvent, EventBus};
use darb_core::i18n::{global_format, global_text, init_global, set_language, Locale};
use darb_core::permissions::{PermissionManager, ToolRequest};
use darb_core::t;
use darb_memory::project::ProjectPaths;
use darb_memory::storage::MemoryDb;
use darb_providers::interface::Provider;
use darb_providers::openai::OpenAIAdapter;
use darb_tools::registry::{ToolRegistry, ToolResult};
use darb_tui::app::{render, App, FileEntry, Focus, KeyOutcome, MessageRole, Tab};
use darb_tui::palette::Command;
use darb_tui::theme::Theme;
use ratatui::layout::Rect;
use tokio::sync::{mpsc, oneshot};

/// Config file lookup: `$DARB_CONFIG`, else `./configs/default.toml`
/// (repo/dev layout). Shared with `doctor`.
pub(crate) fn config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("DARB_CONFIG") {
        return PathBuf::from(path);
    }
    std::env::current_dir()
        .map(|cwd| cwd.join("configs").join("default.toml"))
        .unwrap_or_else(|_| PathBuf::from("configs/default.toml"))
}

fn load_config() -> DarbConfig {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => match darb_core::config::load_from_str(&text) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("{}", t!("cli.config_fallback", reason = e.to_string()));
                DarbConfig::default()
            }
        },
        Err(_) => DarbConfig::default(),
    }
}

/// Load the configured UI language and install it as the global locale.
/// Called once by `main`, so CLI output and the TUI agree — including for
/// commands that never open the interface (`darb doctor`).
pub(crate) fn init_i18n() -> Locale {
    let language = Locale::parse(load_config().language());
    if let Err(e) = init_global(language) {
        eprintln!("{}", t!("cli.locale_fallback", reason = e.to_string()));
    }
    language
}

fn build_provider(config: &DarbConfig) -> Result<Box<dyn Provider>, String> {
    if config.provider.model.trim().is_empty() || config.provider.model.trim() == "..." {
        return Err("no model configured".to_string());
    }
    match config.provider.name.as_str() {
        "openai" => OpenAIAdapter::from_env()
            .map(|adapter| Box::new(adapter) as Box<dyn Provider>)
            .map_err(|e| e.to_string()),
        other => Err(format!("provider '{other}' is not implemented yet")),
    }
}

/// One input event after filtering: a key, or a mouse event when capture
/// is enabled. Everything else is dropped before it reaches the app.
enum MouseOrKey {
    Mouse(crossterm::event::MouseEvent),
    Key(crossterm::event::KeyEvent),
}

/// Reply half of a pending confirmation. Display comes from the
/// `PermissionRequested` event; this channel only carries the answer.
struct UiResponder {
    tx: mpsc::UnboundedSender<oneshot::Sender<bool>>,
}

impl PermissionResponder for UiResponder {
    fn ask(&self, _request: &ToolRequest) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let send = self.tx.send(reply_tx);
        Box::pin(async move {
            if send.is_err() {
                return false; // UI is gone: fail closed.
            }
            reply_rx.await.unwrap_or(false)
        })
    }
}

/// Confirmation the user is currently answering. Keeping both cases in one
/// slot is what lets a single dialog serve the agent and the terminal tab
/// without either gaining a way around the permission manager.
enum Pending {
    Agent(oneshot::Sender<bool>),
    Direct(ToolRequest),
}

/// Presentation state plus the read-only sources that fill it. Grouping
/// them keeps the loop readable and stops the panel helpers from growing
/// a parameter per data source.
struct Ui {
    app: App,
    registry: ToolRegistry,
    config: DarbConfig,
    model_label: String,
    language: Locale,
    tool_count: u32,
}

impl Ui {
    fn new(
        app: App,
        registry: ToolRegistry,
        config: DarbConfig,
        model_label: String,
        language: Locale,
    ) -> Self {
        Self {
            app,
            registry,
            config,
            model_label,
            language,
            tool_count: 0,
        }
    }

    /// Context panel lines. Only numbers we actually have: no invented
    /// token counts (Contribuição §41). "Entries" counts what the open
    /// directory holds — the agent's own context budget is not visible
    /// here, so it is not claimed either.
    fn refresh_context(&mut self) {
        let count = self.app.files.len().to_string();
        let tools = self.tool_count.to_string();
        self.app.set_context(vec![
            global_format("context.entries", &[("count", &count)]),
            global_format("context.model", &[("model", &self.model_label)]),
            global_format("app.profile", &[("name", &self.config.performance.profile)]),
            global_format("app.tools", &[("count", &tools)]),
        ]);
    }

    /// Re-list the current directory through the read-only view registry
    /// (same permission gate as the agent — display is not a bypass).
    fn refresh_files(&mut self) {
        let dir = self.app.current_dir.clone();
        let result: ToolResult = self
            .registry
            .dispatch(&ToolRequest::new("list_directory", dir));
        if !result.success {
            return;
        }
        let files = result
            .output
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| {
                let is_dir = line.ends_with('/');
                FileEntry {
                    name: line.trim_end_matches('/').to_string(),
                    is_dir,
                }
            })
            .collect();
        self.app.set_files(files);
        self.refresh_context();
    }

    /// Show the repository status in the Git panel. A failure (no
    /// repository, no git binary) is reported *inside* the panel instead
    /// of as an error popup: that is where the user asked to look.
    fn refresh_git(&mut self) {
        let result = self.registry.dispatch(&ToolRequest::new("git_status", ""));
        if result.success {
            self.app.set_git(&result.output);
        } else {
            self.app.set_git(result.error.as_deref().unwrap_or(""));
        }
    }

    /// Same for the Changes panel.
    fn refresh_diff(&mut self) {
        let result = self.registry.dispatch(&ToolRequest::new("git_diff", ""));
        if result.success {
            self.app.set_diff(&result.output);
        } else {
            self.app.set_diff(result.error.as_deref().unwrap_or(""));
        }
    }

    /// Open an explorer entry: directories are listed, files previewed.
    fn open_entry(&mut self, path: String, is_dir: bool) {
        if is_dir {
            self.app.set_current_dir(path);
            self.refresh_files();
            return;
        }
        let result = self
            .registry
            .dispatch(&ToolRequest::new("read_file", &path));
        if result.success {
            self.app.set_file_preview(path, result.output);
            self.app.set_tab(Tab::Files);
            self.app.focus = Focus::Workspace;
        } else {
            let reason = result.error.unwrap_or_default();
            self.app.push(
                MessageRole::System,
                format!(
                    "⚠ {}",
                    global_format("file.open_error", &[("path", &path), ("reason", &reason)])
                ),
            );
        }
    }

    /// Leave the current directory (explorer `..` row).
    fn go_up(&mut self) {
        let parent = self
            .app
            .current_dir
            .rsplit_once('/')
            .map(|(head, _)| head.to_string())
            .unwrap_or_default();
        self.app.set_current_dir(parent);
        self.refresh_files();
    }

    /// Run one command from the terminal panel. A command that policy
    /// marks `ask` comes back as a pending confirmation instead of running.
    fn run_terminal(&mut self, request: ToolRequest) -> Option<Pending> {
        let result = self.registry.dispatch(&request);
        if result.metadata.get("permission").map(String::as_str) == Some("ask") {
            self.app.ask_permission(&request.tool, &request.target);
            return Some(Pending::Direct(request));
        }
        self.report_terminal(&request, &result);
        None
    }

    /// Echo the command and its captured output into the terminal panel.
    fn report_terminal(&mut self, request: &ToolRequest, result: &ToolResult) {
        self.app.push_terminal(format!("$ {}", request.target));
        let output = result.output.trim_end();
        if !output.is_empty() {
            for line in output.lines() {
                self.app.push_terminal(line.to_string());
            }
        }
        if !result.success {
            if let Some(error) = &result.error {
                self.app.push_terminal(format!("✗ {error}"));
            }
        }
        if let Some(code) = result.metadata.get("exit_code") {
            self.app
                .push_terminal(global_format("terminal.exit_code", &[("code", code)]));
        }
    }

    /// Perform one palette command. Returns `true` when the app should quit.
    fn apply_command(&mut self, command: Command) -> bool {
        match command {
            Command::Quit => return true,
            Command::ToggleExplorer => self.app.toggle_explorer(),
            Command::RefreshFiles => self.refresh_files(),
            Command::RefreshGit => {
                self.refresh_git();
                self.app.set_tab(Tab::Git);
                self.app.focus = Focus::Workspace;
            }
            Command::ShowDiff => {
                self.refresh_diff();
                self.app.set_tab(Tab::Changes);
                self.app.focus = Focus::Workspace;
            }
            Command::NewChat => self.app.clear_messages(),
            Command::OpenTab(tab) => {
                self.app.set_tab(tab);
                self.app.focus = Focus::Workspace;
            }
            Command::ToggleLanguage => {
                self.language = match self.language {
                    Locale::Pt => Locale::En,
                    Locale::En => Locale::Pt,
                };
                set_language(self.language);
                // The globe plus the language tag is language-neutral: no
                // string has to be translated to announce the change.
                self.app.push(
                    MessageRole::System,
                    format!("🌐 {}", self.language.as_str()),
                );
            }
        }
        false
    }
}

/// Session transcript for this run. Every write is best-effort: a full
/// disk must degrade the transcript, never the TUI. Only the open failure
/// is surfaced (once); later write failures are ignored by callers.
struct MemoryCtx {
    db: MemoryDb,
    session_id: i64,
}

/// Open `.darb/memory.db`, replay the previous transcript for display,
/// and start a new session. Returns the context plus lines to show.
fn open_memory(
    root: &std::path::Path,
    profile: &str,
) -> (Option<MemoryCtx>, Vec<(MessageRole, String)>) {
    let mut lines = Vec::new();
    let db = match ProjectPaths::new(root).open_memory() {
        Ok(db) => db,
        Err(e) => {
            lines.push((
                MessageRole::System,
                global_format("app.memory_unavailable", &[("reason", &e.to_string())]),
            ));
            return (None, lines);
        }
    };
    if let Ok(Some(previous)) = db.latest_session() {
        if let Ok(messages) = db.recent_messages(previous.id, 50) {
            if !messages.is_empty() {
                lines.push((MessageRole::System, global_text("app.previous_session")));
                for message in messages {
                    if let Some(role) = parse_role(&message.role) {
                        lines.push((role, message.text));
                    }
                }
            }
        }
    }
    let root_str = root.to_string_lossy().into_owned();
    match db.create_session(&root_str, profile) {
        Ok(session) => (
            Some(MemoryCtx {
                db,
                session_id: session.id,
            }),
            lines,
        ),
        Err(e) => {
            lines.push((
                MessageRole::System,
                global_format("app.memory_unavailable", &[("reason", &e.to_string())]),
            ));
            (None, lines)
        }
    }
}

fn parse_role(role: &str) -> Option<MessageRole> {
    match role {
        "user" => Some(MessageRole::User),
        "agent" => Some(MessageRole::Agent),
        "system" => Some(MessageRole::System),
        _ => None,
    }
}

pub fn run() -> i32 {
    let config = load_config();
    let language = init_i18n();
    let root = match std::env::current_dir() {
        Ok(root) => root,
        Err(e) => {
            eprintln!("{}", t!("cli.no_cwd", reason = e.to_string()));
            return 1;
        }
    };
    let (memory, startup_lines) = open_memory(&root, &config.performance.profile);

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("{}", t!("cli.no_runtime", reason = e.to_string()));
            return 1;
        }
    };
    let handle = runtime.handle().clone();

    let bus = EventBus::default();
    let mut events = bus.subscribe();
    let (query_tx, mut query_rx) = mpsc::unbounded_channel::<oneshot::Sender<bool>>();

    let model_label = format!("{}/{}", config.provider.name, config.provider.model);
    let agent: Option<Arc<Agent>> = match build_provider(&config) {
        Ok(provider) => {
            let registry =
                ToolRegistry::new(PermissionManager::new(config.permissions.clone()), &root);
            Some(Arc::new(
                Agent::new(
                    provider,
                    config.provider.model.clone(),
                    registry,
                    bus,
                    config.agent.clone(),
                )
                .with_responder(Arc::new(UiResponder { tx: query_tx }))
                .with_context(root.clone(), config.context.max_tokens),
            ))
        }
        Err(reason) => {
            // No agent: keep the bus alive so the event drain below simply
            // idles instead of seeing a closed channel.
            let _keepalive = bus;
            drop(query_tx);
            eprintln!("{}", t!("app.provider_unavailable", reason = reason));
            None
        }
    };

    let view_registry =
        ToolRegistry::new(PermissionManager::new(config.permissions.clone()), &root);
    // Docs §32/§34: clickable when the terminal supports mouse. Read before
    // `config` moves into `Ui`; capture enable below is config-driven.
    let mouse_enabled = config.interface.mouse;
    let mut ui = Ui::new(App::new(), view_registry, config, model_label, language);
    for (role, text) in startup_lines {
        ui.app.push(role, text);
    }
    ui.refresh_files();
    ui.refresh_git();

    let mut terminal = ratatui::init();
    // Capture failure is not fatal — the keyboard keeps working.
    if mouse_enabled {
        if let Err(e) = crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)
        {
            eprintln!(
                "{}
",
                t!("app.mouse_unavailable", reason = e.to_string())
            );
        }
    }
    let theme = Theme::default();
    let mut busy = false;
    let mut pending: Option<Pending> = None;
    let mut last_request: Option<(String, String)> = None;
    // Terminal area for hit-testing. Falls back to an empty rect when the
    // size query fails; `handle_mouse` then finds no region and ignores the
    // click instead of acting on a wrong coordinate space.
    let area = || {
        ratatui::crossterm::terminal::size()
            .map(|(width, height)| Rect::new(0, 0, width, height))
            .unwrap_or_default()
    };

    loop {
        // 1. Agent → UI: fold every queued event.
        loop {
            match events.try_recv() {
                Ok(event) => {
                    match &event {
                        DarbEvent::AgentStarted { .. } => {
                            busy = true;
                            ui.tool_count = 0;
                        }
                        DarbEvent::ToolRequested { tool, target } => {
                            last_request = Some((tool.clone(), target.clone()));
                        }
                        DarbEvent::ToolCompleted { tool, success } => {
                            ui.tool_count += 1;
                            if let (Some(memory), Some((_, target))) = (&memory, &last_request) {
                                // Same tool name the event carries; the target
                                // is the last one requested (one agent at a
                                // time, so they can't interleave).
                                let _ = memory.db.record_tool_call(
                                    memory.session_id,
                                    tool,
                                    target,
                                    *success,
                                );
                            }
                        }
                        DarbEvent::AgentFinished { summary } => {
                            busy = false;
                            if let Some(memory) = &memory {
                                let _ =
                                    memory
                                        .db
                                        .record_message(memory.session_id, "agent", summary);
                                let outcome = format!("{:?}", ui.app.agent_state).to_lowercase();
                                let _ =
                                    memory
                                        .db
                                        .record_decision(memory.session_id, summary, &outcome);
                            }
                            // The run may have edited files: refresh what the
                            // panels show, never on a timer (Negócio §17).
                            ui.refresh_files();
                            ui.refresh_git();
                        }
                        _ => {}
                    }
                    ui.app.apply_event(&event);
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                Err(_) => break, // Lagged or closed: resync on the next tick.
            }
        }
        // 2. Pending confirmations: keep only the latest reply half.
        while let Ok(reply) = query_rx.try_recv() {
            pending = Some(Pending::Agent(reply));
        }

        // 3. Draw.
        if terminal
            .draw(|frame| render(frame, &ui.app, &theme))
            .is_err()
        {
            break;
        }

        // 4. Input.
        let poll = match crossterm::event::poll(Duration::from_millis(50)) {
            Ok(ready) => ready,
            Err(_) => break,
        };
        if !poll {
            continue;
        }
        let event = match crossterm::event::read() {
            Ok(crossterm::event::Event::Key(key)) => Some(MouseOrKey::Key(key)),
            Ok(crossterm::event::Event::Mouse(mouse)) if mouse_enabled => {
                Some(MouseOrKey::Mouse(mouse))
            }
            Ok(_) => None,
            Err(_) => break,
        };
        let outcome = match event {
            Some(MouseOrKey::Key(key)) => ui.app.handle_key(key),
            Some(MouseOrKey::Mouse(mouse)) => {
                let area = area();
                ui.app.handle_mouse(mouse, area)
            }
            None => continue,
        };
        match outcome {
            KeyOutcome::Quit => break,
            KeyOutcome::CancelRequested => {
                if let Some(agent) = &agent {
                    agent.cancel();
                    ui.app
                        .push(MessageRole::System, global_text("app.cancelling"));
                }
            }
            KeyOutcome::Submitted(task) => {
                ui.app.push(MessageRole::User, task.clone());
                if let Some(memory) = &memory {
                    let _ = memory.db.record_message(memory.session_id, "user", &task);
                }
                match &agent {
                    Some(agent) if !busy => {
                        busy = true;
                        let running = Arc::clone(agent);
                        handle.spawn(async move {
                            let _ = running.run(&task).await;
                        });
                    }
                    Some(_) => ui.app.push(MessageRole::System, global_text("app.busy")),
                    None => ui.app.push(MessageRole::System, global_text("app.offline")),
                }
            }
            KeyOutcome::PermissionAnswer(allowed) => match pending.take() {
                Some(Pending::Agent(reply)) => {
                    let _ = reply.send(allowed);
                }
                Some(Pending::Direct(request)) => {
                    if allowed {
                        ui.registry.grant_once(&request.tool, &request.target);
                        pending = ui.run_terminal(request);
                    } else {
                        let denied = global_format(
                            "permissions.denied",
                            &[
                                ("tool", request.tool.as_str()),
                                ("target", request.target.as_str()),
                            ],
                        );
                        ui.app.push_terminal(denied);
                    }
                }
                None => {}
            },
            KeyOutcome::Command(command) => {
                if ui.apply_command(command) {
                    break;
                }
            }
            KeyOutcome::OpenEntry { path, is_dir } => ui.open_entry(path, is_dir),
            KeyOutcome::GoUp => ui.go_up(),
            KeyOutcome::TerminalCommand(line) => {
                pending = ui.run_terminal(ToolRequest::new("shell", line));
            }
            KeyOutcome::Ignored => {}
        }
    }

    if mouse_enabled {
        let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
    }
    ratatui::restore();
    0
}
