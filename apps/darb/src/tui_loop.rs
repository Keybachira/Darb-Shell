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
use std::time::{Duration, Instant};

use darb_agent::{Agent, PermissionResponder};
use darb_core::config::DarbConfig;
use darb_core::events::{DarbEvent, EventBus};
use darb_core::i18n::{global_format, global_text, init_global, set_language, Locale};
use darb_core::permissions::{PermissionManager, ToolRequest};
use darb_core::t;
use darb_core::workspace::{AiMode, GitSnapshot, Workspace};
use darb_memory::project::ProjectPaths;
use darb_memory::storage::MemoryDb;
use darb_providers::interface::Provider;
use darb_providers::openai::OpenAIAdapter;
use darb_tools::registry::{ToolRegistry, ToolResult};
use darb_tui::app::{render, App, FileEntry, Focus, KeyOutcome, MessageRole, Tab, ToastKind};
use darb_tui::palette::Command;
use darb_tui::theme::Theme;
use ratatui::layout::Rect;
use sysinfo::System;
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

/// Confirmation the user is currently answering. Keeping every case in
/// one slot is what lets a single dialog serve the agent, the terminal
/// tab and the file editor without any of them gaining a way around the
/// permission manager.
enum Pending {
    Agent(oneshot::Sender<bool>),
    Direct(ToolRequest),
    Save(ToolRequest),
}

/// Presentation state plus the read-only sources that fill it. Grouping
/// them keeps the loop readable and stops the panel helpers from growing
/// a parameter per data source.
struct Ui {
    app: App,
    /// Event bus used to publish `WorkspaceChanged`. `None` when there is
    /// no bus to publish on; announcing is then skipped rather than faked.
    bus: Option<EventBus>,
    registry: ToolRegistry,
    config: DarbConfig,
    model_label: String,
    language: Locale,
    tool_count: u32,
    /// Throttled system telemetry (`sysinfo`). `None` when the refresh
    /// fails — the panels then simply show no gauges.
    sys: Option<System>,
    sys_refresh: Option<Instant>,
}

impl Ui {
    /// The application layer is the only writer of the workspace. The `App`
    /// holds the *same* value, not a copy: a second copy is exactly what
    /// P0 removed, and it would drift from the panels the moment anything
    /// changed one side and not the other.
    fn workspace(&self) -> &Workspace {
        &self.app.workspace
    }

    fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.app.workspace
    }

    fn new(
        app: App,
        bus: Option<EventBus>,
        registry: ToolRegistry,
        config: DarbConfig,
        model_label: String,
        language: Locale,
        project_root: &std::path::Path,
    ) -> Self {
        let mut app = app;
        app.workspace = Workspace::new(project_root);
        // The workspace already knows the project name; the header must not
        // invent its own, or the two can disagree.
        app.project_name = app.workspace.project_name.clone();
        // The mode starts at Mentor (least power), so the very first thing
        // the app does is tighten whatever the config allowed. Applying it
        // once here means there is no window where a tool could run under
        // the wrong policy.
        let mut policy = config.permissions.clone();
        app.workspace.mode.apply_to(&mut policy);
        registry.set_permissions(policy);
        Self {
            app,
            bus,
            registry,
            config,
            model_label,
            language,
            tool_count: 0,
            sys: None,
            sys_refresh: None,
        }
    }

    /// Switch the AI mode and make it real (P1).
    ///
    /// The workspace records it, the permission gate is rewritten, and the
    /// new state is announced. If the last part is missing, the feature is
    /// cosmetic: a Mentor that still writes files is worse than no Mentor
    /// at all, because the user believes it.
    fn set_ai_mode(&mut self, mode: AiMode) {
        self.workspace_mut().mode = mode;
        let mut policy = self.config.permissions.clone();
        mode.apply_to(&mut policy);
        self.registry.set_permissions(policy);
        self.announce_workspace();
    }

    /// Cycle Mentor → Assist → Autonomous → Mentor.
    fn cycle_ai_mode(&mut self) -> AiMode {
        let next = self.workspace_mut().cycle_mode();
        self.set_ai_mode(next);
        next
    }

    /// Refresh the CPU/RAM snapshot at most every ~2s (performance
    /// budget: telemetry must never drive the loop). First call seeds
    /// the sysinfo baseline (its first read is always ~0).
    fn refresh_system(&mut self) {
        let now = Instant::now();
        if self
            .sys_refresh
            .is_some_and(|last| now.duration_since(last) < Duration::from_secs(2))
        {
            return;
        }
        self.sys_refresh = Some(now);
        let sys = self.sys.get_or_insert_with(System::new);
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        self.app.cpu_percent = sys.global_cpu_usage();
        let total = sys.total_memory();
        let used = sys.used_memory();
        if total > 0 {
            self.app.mem_percent = (used as f32 / total as f32) * 100.0;
            self.app.mem_used = format_bytes(used);
            self.app.mem_total = format_bytes(total);
        }
    }

    /// Record the porcelain git status in the workspace, then announce it.
    /// The TUI reads the values straight from the workspace, so there is
    /// nothing to mirror: the old copy of branch/counts on `App` could drift
    /// from the Git panel, and the desktop would have needed a third copy.
    /// Parsing lives in `darb_core::workspace::GitSnapshot` so every renderer
    /// sees the same numbers from the same input.
    fn apply_git(&mut self, output: &str) {
        self.workspace_mut().git = GitSnapshot::parse(output);
        self.announce_workspace();
    }

    /// Publish the current workspace. The TUI already holds it (it writes
    /// it), so this exists for the *other* subscribers: the desktop shell
    /// and any future tool. Emitting the whole state rather than a diff
    /// means a late subscriber renders the truth instead of a patch it
    /// never saw.
    fn announce_workspace(&self) {
        if let Some(bus) = &self.bus {
            let _ = bus.emit(DarbEvent::WorkspaceChanged {
                workspace: self.workspace().clone(),
            });
        }
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
    }

    /// Show the repository status in the Git panel. A failure (no
    /// repository, no git binary) is reported *inside* the panel instead
    /// of as an error popup: that is where the user asked to look.
    /// Record the repository state. On failure the workspace is reset too:
    /// a panel that keeps showing a stale branch while the error text says
    /// "not a repository" is worse than showing nothing.
    fn refresh_git(&mut self) {
        let result = self.registry.dispatch(&ToolRequest::new("git_status", ""));
        if result.success {
            self.apply_git(&result.output);
            self.app.set_git(&result.output);
        } else {
            self.workspace_mut().git = GitSnapshot::default();
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

    /// Save the Files-tab buffer through `edit_file` (old = snapshot at
    /// open, so an external edit fails the match instead of being
    /// overwritten). Ask-gated exactly like the terminal path.
    fn run_save(&mut self, request: ToolRequest) -> Option<Pending> {
        let result = self.registry.dispatch(&request);
        if result.metadata.get("permission").map(String::as_str) == Some("ask") {
            self.app.ask_permission(&request.tool, &request.target);
            return Some(Pending::Save(request));
        }
        self.report_save(&request, &result);
        None
    }

    /// Surface the save in the chat: refresh the snapshot on success so
    /// the next save diffs against what is on disk, or explain the
    /// failure where the user asked to look (Negócio §42).
    fn report_save(&mut self, request: &ToolRequest, result: &ToolResult) {
        if result.success {
            let content = request.arguments.get("new").cloned().unwrap_or_default();
            self.app.confirm_saved(content);
            self.app.push(
                MessageRole::System,
                format!(
                    "✓ {}",
                    global_format("file.saved", &[("path", &request.target)])
                ),
            );
        } else {
            let reason = result.error.clone().unwrap_or_default();
            self.app.push(
                MessageRole::System,
                format!(
                    "⚠ {}",
                    global_format(
                        "file.save_error",
                        &[("path", &request.target), ("reason", &reason)]
                    )
                ),
            );
        }
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
            Command::ToggleTerminal => self.app.toggle_bottom(),
            Command::ToggleContext => self.app.toggle_context(),
            Command::CycleMode => self.app.cycle_mode(),
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
                self.app.language = self.language.as_str().to_string();
                // The globe plus the language tag is language-neutral: no
                // string has to be translated to announce the change.
                self.app.push(
                    MessageRole::System,
                    format!("🌐 {}", self.language.as_str()),
                );
                self.app
                    .toast(ToastKind::Info, format!("🌐 {}", self.language.as_str()));
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
                    // Cloned, not moved: the UI publishes
                    // `WorkspaceChanged` on this same channel.
                    bus.clone(),
                    config.agent.clone(),
                )
                .with_responder(Arc::new(UiResponder { tx: query_tx }))
                .with_context(root.clone(), config.context.max_tokens),
            ))
        }
        Err(reason) => {
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
    let mut ui = Ui::new(
        App::new(),
        // Cloned, not moved: the agent already holds a handle and the UI
        // publishes `WorkspaceChanged` on the same channel.
        Some(bus.clone()),
        view_registry,
        config,
        model_label,
        language,
        &root,
    );
    ui.app.ai_online = agent.is_some();
    ui.app.model_label = ui.model_label.clone();
    ui.app.profile = ui.config.performance.profile.clone();
    ui.app.language = language.as_str().to_string();
    ui.app.project_name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned());
    ui.app.user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    for (role, text) in startup_lines {
        ui.app.push(role, text);
    }
    ui.refresh_files();
    ui.refresh_git();
    ui.refresh_system();

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
                            ui.app.tool_calls = ui.tool_count;
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

        // 3. System snapshot (throttled internally to ~2s) and draw.
        ui.refresh_system();
        ui.app.expire_old();
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
        // The AI power mode is handled here, not in the view: switching it
        // rewrites the permission policy, and only this layer holds the
        // registry. Intercepting the action before `handle_key` means the
        // view can never widen its own permissions with a keystroke.
        if let Some(MouseOrKey::Key(key)) = event {
            if key.kind == crossterm::event::KeyEventKind::Press
                && darb_tui::keybindings::action_for(key)
                    == darb_tui::keybindings::Action::CycleAiMode
            {
                let mode = ui.cycle_ai_mode();
                ui.app.toast(ToastKind::Info, format!("[ {} ]", mode.name()));
                continue;
            }
        }
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
                Some(Pending::Save(request)) => {
                    if allowed {
                        ui.registry.grant_once(&request.tool, &request.target);
                        pending = ui.run_save(request);
                    } else {
                        let denied = global_format(
                            "permissions.denied",
                            &[
                                ("tool", request.tool.as_str()),
                                ("target", request.target.as_str()),
                            ],
                        );
                        ui.app.push(MessageRole::System, format!("⌀ {denied}"));
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
            KeyOutcome::SaveFile {
                path,
                original,
                content,
            } => {
                let request = ToolRequest::new("edit_file", path)
                    .with_arg("old", original)
                    .with_arg("new", content);
                pending = ui.run_save(request);
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

/// Human byte count for the system panel: `512 B`, `2.0 GB`.
fn format_bytes(value: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = value as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", value as u64, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darb_core::config::{PermissionDecision, PermissionsConfig};

    #[test]
    fn bytes_format_is_human() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(17_179_869_184), "16.0 GB");
    }

    #[test]
    fn git_counts_are_parsed_from_porcelain() {
        let mut ui = ui_with_edit(
            &std::env::temp_dir(),
            darb_core::config::PermissionDecision::Allow,
        );
        ui.apply_git("## main\n M src/a.rs\nM  src/b.rs\n?? new.txt\n D gone.rs");
        // Read through the workspace, not through a mirror on `App`: the
        // whole point of P0 is that there is only one copy of this state.
        let git = &ui.workspace().git;
        assert_eq!(git.branch, "main");
        assert_eq!(git.staged, 1);
        assert_eq!(git.changes, 2);
        assert_eq!(git.untracked, 1);
    }

    #[test]
    fn the_header_reads_the_same_branch_the_workspace_holds() {
        let mut ui = ui_with_edit(
            &std::env::temp_dir(),
            darb_core::config::PermissionDecision::Allow,
        );
        ui.apply_git("## main...origin/main\nM  src/a.rs\n");
        // What the panels draw and what the app exposes must not diverge.
        assert_eq!(ui.app.git_branch(), ui.workspace().git.branch);
        assert!(ui.app.git_change_summary().contains('+'));
    }

    /// Render the real frame and flatten it to text, so a test can assert on
    /// what the user actually sees rather than on internal fields.
    fn rendered(app: &App) -> String {
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(110, 34))
            .expect("test terminal");
        terminal
            .draw(|frame| darb_tui::app::render(frame, app, &darb_tui::theme::Theme::default()))
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<String>>()
            .join("\n")
    }

    #[test]
    fn a_workspace_change_reaches_the_rendered_header() {
        let mut app = App::new();
        let screen_before = rendered(&app);
        assert!(!screen_before.contains("feature-x"), "{screen_before}");
        // The application layer publishes; the view adopts. This is the
        // end-to-end proof that workspace state drives what is drawn.
        app.apply_event(&DarbEvent::WorkspaceChanged {
            workspace: Workspace {
                project_name: "feature-x".to_string(),
                ..Workspace::default()
            },
        });
        let screen = rendered(&app);
        assert!(screen.contains("feature-x"), "{screen}");
    }

    #[test]
    fn the_workspace_is_the_only_copy() {
        let ui = ui_with_edit(
            &std::env::temp_dir(),
            darb_core::config::PermissionDecision::Allow,
        );
        // `Ui` must not keep a second Workspace: two copies would drift.
        assert_eq!(ui.workspace().project_name, ui.app.project_name);
    }

    #[test]
    fn mentor_refuses_to_edit_even_when_the_config_allowed_it() {
        use darb_core::config::PermissionDecision::Allow;
        let mut ui = ui_with_edit(&std::env::temp_dir(), Allow);
        // The helper grants edit; Mentor must still take it away. This is
        // the whole feature: the mode is a ceiling on the user's config.
        ui.set_ai_mode(AiMode::Mentor);
        let outcome = ui
            .registry
            .dispatch(&darb_core::permissions::ToolRequest::new("edit_file", "a.rs"));
        assert!(
            !outcome.success,
            "mentor edited a file: {}",
            outcome.output
        );
    }

    #[test]
    fn switching_to_autonomous_really_enables_editing() {
        use darb_core::config::PermissionDecision::Allow;
        let mut ui = ui_with_edit(&std::env::temp_dir(), Allow);
        assert!(ui.workspace().mode == AiMode::Mentor, "starts at least power");
        ui.cycle_ai_mode();
        ui.cycle_ai_mode();
        assert_eq!(ui.workspace().mode, AiMode::Autonomous);
        // The same request that Mentor refused now reaches the tool. The
        // file may or may not exist; what matters is that the gate opened.
        let policy = ui.registry.permissions_config();
        assert_eq!(policy.edit, Allow, "autonomous allows edits");
    }

    #[test]
    fn the_mode_is_reflected_in_the_policy_in_force() {
        use darb_core::config::PermissionDecision::{Allow, Ask, Deny};
        let mut ui = ui_with_edit(&std::env::temp_dir(), Allow);
        ui.set_ai_mode(AiMode::Assist);
        let policy = ui.registry.permissions_config();
        assert_eq!(policy.edit, Ask, "assist asks before writing");
        assert_eq!(policy.read, Allow, "assist still reads freely");
        ui.set_ai_mode(AiMode::Autonomous);
        assert_eq!(ui.registry.permissions_config().edit, Allow);
        ui.set_ai_mode(AiMode::Mentor);
        let policy = ui.registry.permissions_config();
        assert_eq!(policy.edit, Deny);
        assert_eq!(policy.shell, Deny, "mentor never runs commands");
    }

    fn temp_root(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("darb-save-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp root");
        dir
    }

    fn ui_with_edit(root: &std::path::Path, decision: PermissionDecision) -> Ui {
        let config = DarbConfig::default();
        let registry = ToolRegistry::new(
            PermissionManager::new(PermissionsConfig {
                read: PermissionDecision::Allow,
                edit: decision,
                shell: PermissionDecision::Allow,
                delete: PermissionDecision::Deny,
                git_push: PermissionDecision::Deny,
            }),
            root,
        );
        Ui::new(
            App::new(),
            None,
            registry,
            config,
            "test-model".to_string(),
            Locale::En,
            root,
        )
    }

    /// Type one char through the real App path, then build the request
    /// exactly like the `SaveFile` arm does.
    fn save_request(ui: &mut Ui) -> ToolRequest {
        ui.app.set_tab(Tab::Files);
        ui.app.toggle_edit();
        ui.app.edit_insert('X');
        let (path, original, content) = ui.app.take_save().expect("dirty buffer");
        ToolRequest::new("edit_file", path)
            .with_arg("old", original)
            .with_arg("new", content)
    }

    #[test]
    fn save_round_trip_writes_the_buffer_to_disk() {
        let root = temp_root("round-trip");
        std::fs::write(root.join("a.txt"), "ab\ncd\n").expect("write");
        let mut ui = ui_with_edit(&root, PermissionDecision::Allow);
        ui.app
            .set_file_preview("a.txt".to_string(), "ab\ncd\n".to_string());

        let request = save_request(&mut ui);
        let pending = ui.run_save(request);
        assert!(pending.is_none(), "allowed edits save without asking");
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).expect("read"),
            "Xab\ncd\n"
        );
        assert!(!ui.app.edit_dirty, "snapshot refreshed after save");
        assert!(ui.app.take_save().is_none());
        assert!(
            ui.app.messages.iter().any(|m| m.text.contains("a.txt")),
            "success is reported where the user asked to look"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn save_under_ask_policy_waits_for_the_dialog() {
        let root = temp_root("ask-policy");
        std::fs::write(root.join("a.txt"), "ab\ncd\n").expect("write");
        let mut ui = ui_with_edit(&root, PermissionDecision::Ask);
        ui.app
            .set_file_preview("a.txt".to_string(), "ab\ncd\n".to_string());

        let request = save_request(&mut ui);
        let pending = ui.run_save(request);
        assert!(
            matches!(pending, Some(Pending::Save(_))),
            "ask-gated save must not touch the disk yet"
        );
        assert!(ui.app.permission.is_some());
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).expect("read"),
            "ab\ncd\n",
            "file untouched until the user allows"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
