//! Agent loop: task → provider → tools → review → done.
//!
//! The Agent holds `Box<dyn Provider>` — it never knows which API answers
//! (Negócio §13). Budgets from [`AgentConfig`] bound autonomy: iterations,
//! tool calls, consecutive failures, and wall-clock time (Agents §§21–22).
//! Every state change emits an event; the TUI observes, never drives.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use darb_context::compression::truncate_middle;
use darb_context::indexer::Indexer;
use darb_context::selector::select;
use darb_core::config::AgentConfig;
use darb_core::errors::{DarbError, Result};
use darb_core::events::{AgentState, DarbEvent, EventBus};
use darb_core::permissions::{Denial, DenyReason, ToolRequest};
use darb_providers::interface::{Provider, ProviderRequest, ProviderResponse, StreamEvent};
use darb_tools::registry::{ToolRegistry, ToolResult};

use crate::executor::Executor;
use crate::planner::Plan;
use crate::reviewer::{review, ReviewVerdict};
use crate::state::{is_legal_transition, is_terminal};

/// Tool-result lines longer than this are cut before going back into the
/// prompt (context/token budget, Negócio §19).
const MAX_RESULT_CHARS: usize = 4000;

/// Optional project context: built once per run (lazy, never a daemon)
/// and injected into every prompt inside the token budget.
#[derive(Debug, Clone)]
pub struct ContextInput {
    pub root: std::path::PathBuf,
    pub budget_tokens: u32,
}

/// Outcome of one finished task.
#[derive(Debug, Clone)]
pub struct FinalResult {
    /// Always terminal: Completed, Failed, or Cancelled.
    pub outcome: AgentState,
    pub summary: String,
    pub iterations: u32,
    pub tool_calls: u32,
    pub changed_files: Vec<String>,
    pub warnings: Vec<String>,
}

/// Mutable counters carried through one run.
#[derive(Debug, Default)]
struct RunStats {
    iterations: u32,
    tool_calls: u32,
    failures: u32,
    changed_files: Vec<String>,
    warnings: Vec<String>,
    last_text: String,
}

pub struct Agent {
    provider: Box<dyn Provider>,
    model: String,
    registry: ToolRegistry,
    events: EventBus,
    limits: AgentConfig,
    cancelled: Arc<AtomicBool>,
    responder: Option<Arc<dyn PermissionResponder>>,
    context: Option<ContextInput>,
}

/// Answers one confirmation question. Implemented by the UI layer; when
/// unset, permission questions fail the batch instead of pausing the
/// loop (headless behavior, unchanged).
pub trait PermissionResponder: Send + Sync {
    fn ask(
        &self,
        request: &ToolRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + '_>>;
}

impl Agent {
    pub fn new(
        provider: Box<dyn Provider>,
        model: String,
        registry: ToolRegistry,
        events: EventBus,
        limits: AgentConfig,
    ) -> Self {
        Self {
            provider,
            model,
            registry,
            events,
            limits,
            cancelled: Arc::new(AtomicBool::new(false)),
            responder: None,
            context: None,
        }
    }

    /// Wire the interactive confirmation path (TUI dialog). Without it,
    /// `ask` results fail the batch as before.
    pub fn with_responder(mut self, responder: Arc<dyn PermissionResponder>) -> Self {
        self.responder = Some(responder);
        self
    }

    /// Feed project context (map + relevant files) into every prompt,
    /// inside `budget_tokens`. Without it, prompts carry history only.
    pub fn with_context(mut self, root: std::path::PathBuf, budget_tokens: u32) -> Self {
        self.context = Some(ContextInput {
            root,
            budget_tokens,
        });
        self
    }

    /// Ask the loop to stop at the next iteration boundary (Ctrl+C path).
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Run one task to a terminal state, bounded by the configured timeout.
    pub async fn run(&self, task: &str) -> Result<FinalResult> {
        let timeout = Duration::from_secs(self.limits.timeout_seconds.max(1));
        match tokio::time::timeout(timeout, self.run_inner(task)).await {
            Ok(result) => result,
            Err(_) => {
                // Stuck inside `run_inner` (almost always awaiting the
                // provider), so WaitingProvider is the honest `from` state.
                let mut stats = RunStats::default();
                stats.warnings.push(format!(
                    "exceeded the {}s timeout",
                    self.limits.timeout_seconds
                ));
                Ok(self.finish(
                    AgentState::WaitingProvider,
                    AgentState::Failed,
                    "agent timed out".to_string(),
                    stats,
                ))
            }
        }
    }

    async fn run_inner(&self, task: &str) -> Result<FinalResult> {
        self.events.emit(DarbEvent::AgentStarted {
            task: task.to_string(),
        })?;
        let mut state = AgentState::Idle;
        let plan = Plan::single(task);

        state = self.transition(state, AgentState::Analyzing)?;
        state = self.transition(state, AgentState::Planning)?;

        // Project context is built once per run: a fresh lazy scan per
        // task, never a background daemon (Negócio §18).
        let context = self.build_context(task);

        let executor = Executor::new(&self.registry);
        let mut history: Vec<String> = Vec::new();
        let mut stats = RunStats::default();

        loop {
            if self.cancelled.load(Ordering::SeqCst) {
                return Ok(self.finish(
                    state,
                    AgentState::Cancelled,
                    "cancelled by user".to_string(),
                    stats,
                ));
            }
            if stats.iterations >= self.limits.max_iterations {
                stats.warnings.push(format!(
                    "stopped after {} iterations (limit {})",
                    stats.iterations, self.limits.max_iterations
                ));
                let summary = stats.last_text.clone();
                return Ok(self.finish(state, AgentState::Failed, summary, stats));
            }
            stats.iterations += 1;

            state = self.transition(state, AgentState::WaitingProvider)?;
            let prompt = build_prompt(task, &plan, &history, &context);
            let response = match self
                .fetch_response(ProviderRequest {
                    model: self.model.clone(),
                    prompt,
                })
                .await
            {
                Ok((response, interrupted)) => {
                    if interrupted {
                        stats
                            .warnings
                            .push("stream interrupted — used the partial response".to_string());
                    }
                    response
                }
                Err(e) => {
                    stats.failures += 1;
                    stats.warnings.push(format!("provider request failed: {e}"));
                    if stats.failures > self.limits.max_retries {
                        let summary = stats.last_text.clone();
                        return Ok(self.finish(state, AgentState::Failed, summary, stats));
                    }
                    // A retry is a visible state change (Agents §10): the next
                    // iteration then re-enters WaitingProvider legally.
                    state = self.transition(state, AgentState::Retrying)?;
                    continue;
                }
            };
            stats.last_text = response.text.clone();

            let calls = crate::tool_calling::extract_tool_calls(&response.text);
            if calls.is_empty() {
                let summary = stats.last_text.clone();
                return Ok(self.finish(state, AgentState::Completed, summary, stats));
            }

            state = self.transition(state, AgentState::Executing)?;
            let mut batch: Vec<ToolResult> = Vec::new();
            for call in calls {
                if stats.tool_calls >= self.limits.max_tool_calls {
                    stats.warnings.push(format!(
                        "stopped after {} tool calls (limit {})",
                        stats.tool_calls, self.limits.max_tool_calls
                    ));
                    let summary = stats.last_text.clone();
                    return Ok(self.finish(state, AgentState::Failed, summary, stats));
                }
                self.events.emit(DarbEvent::ToolRequested {
                    tool: call.tool.clone(),
                    target: call.target.clone(),
                })?;
                let request = call.into_request();
                // Remembered before executing: only successful edits count
                // as changed files.
                let edited_target = (request.tool == "edit_file").then(|| request.target.clone());
                let mut result = executor.execute_request(&request);
                // Interactive confirmation (Agents §14 User branch): an ask
                // pauses for the UI instead of failing the batch.
                if is_permission_ask(&result) {
                    if let Some(responder) = &self.responder {
                        result = self
                            .confirm_interactively(&mut state, &request, responder)
                            .await?;
                    }
                }
                stats.tool_calls += 1;
                self.events.emit(DarbEvent::ToolCompleted {
                    tool: result
                        .metadata
                        .get("tool")
                        .cloned()
                        .unwrap_or_else(|| "unknown tool".to_string()),
                    success: result.success,
                })?;
                if result.success {
                    if let Some(target) = edited_target {
                        if !stats.changed_files.contains(&target) {
                            stats.changed_files.push(target);
                        }
                    }
                }
                history.push(summarize_result(&result));
                batch.push(result);
            }

            state = self.transition(state, AgentState::Reviewing)?;
            match review(&batch) {
                ReviewVerdict::Pass => {
                    // Progress resets the consecutive-failure count; the
                    // model sees the results and concludes.
                    stats.failures = 0;
                    state = self.transition(state, AgentState::Retrying)?;
                }
                ReviewVerdict::Fail(reasons) => {
                    stats.failures += 1;
                    history.push(format!("review failed: {}", reasons.join("; ")));
                    if stats.failures > self.limits.max_retries {
                        stats
                            .warnings
                            .push(format!("gave up after {} failed reviews", stats.failures));
                        let summary = stats.last_text.clone();
                        return Ok(self.finish(state, AgentState::Failed, summary, stats));
                    }
                    state = self.transition(state, AgentState::Retrying)?;
                }
            }
        }
    }

    /// Get one model response, streaming when the provider supports it.
    /// Returns the response plus whether the stream broke midway (partial
    /// text is still usable — the reviewer judges it like any answer).
    /// A stream that fails before the first delta is a provider error, and
    /// so is an answer with no content at all: reporting Completed with an
    /// empty summary would look like success with nothing to show.
    async fn fetch_response(&self, request: ProviderRequest) -> Result<(ProviderResponse, bool)> {
        if !self.provider.capabilities().streaming {
            let response = self.provider.complete(request).await?;
            if response.text.trim().is_empty() {
                return Err(DarbError::Provider(
                    "provider returned an empty response".to_string(),
                ));
            }
            return Ok((response, false));
        }
        let mut stream = self.provider.stream(request).await?;
        let mut text = String::new();
        let mut usage = darb_providers::interface::TokenUsage::default();
        while let Some(item) = stream.recv().await {
            match item {
                Ok(StreamEvent::Delta(part)) => {
                    text.push_str(&part);
                    self.events.emit(DarbEvent::ProviderDelta { text: part })?;
                }
                Ok(StreamEvent::Done(final_usage)) => {
                    usage = final_usage;
                }
                Err(e) => {
                    if text.trim().is_empty() {
                        return Err(e);
                    }
                    return Ok((ProviderResponse { text, usage }, true));
                }
            }
        }
        if text.trim().is_empty() {
            return Err(DarbError::Provider(
                "provider stream produced no content".to_string(),
            ));
        }
        Ok((ProviderResponse { text, usage }, false))
    }

    /// Pause for one user confirmation, then resume. An approval stores a
    /// single-use grant and re-dispatches through the registry — the
    /// permission manager still decides, so this is confirmation, not a
    /// bypass (Contribuição §28). A refusal becomes a structured denial
    /// for the reviewer.
    async fn confirm_interactively(
        &self,
        state: &mut AgentState,
        request: &ToolRequest,
        responder: &Arc<dyn PermissionResponder>,
    ) -> Result<ToolResult> {
        self.events.emit(DarbEvent::PermissionRequested {
            tool: request.tool.clone(),
            target: request.target.clone(),
        })?;
        let current = state.clone();
        *state = self.transition(current, AgentState::WaitingPermission)?;
        let allowed = responder.ask(request).await;
        let current = state.clone();
        *state = self.transition(current, AgentState::Executing)?;
        self.events.emit(DarbEvent::PermissionResolved {
            tool: request.tool.clone(),
            allowed,
        })?;
        if allowed {
            self.registry.grant_once(&request.tool, &request.target);
            Ok(self.registry.dispatch(request))
        } else {
            Ok(ToolResult::denied(&Denial {
                tool: request.tool.clone(),
                target: request.target.clone(),
                reason: DenyReason::DeniedByUser,
            }))
        }
    }

    /// Assemble the prompt context once per run. Budget split of the
    /// configured token budget: ~10% map, ~60% files, ~30% history
    /// (token budget, Negócio §19 — all estimated, never exact).
    fn build_context(&self, task: &str) -> BuiltContext {
        let empty = BuiltContext {
            map: String::new(),
            files: String::new(),
            history_chars: usize::MAX,
        };
        let Some(input) = &self.context else {
            return empty;
        };
        if input.budget_tokens == 0 {
            return empty;
        }
        let map = Indexer::new(&input.root).scan();
        let map_chars = (input.budget_tokens / 10 * 4) as usize;
        let mut map_text = truncate_middle(&map.render(), map_chars.max(64));
        if map.truncated {
            map_text.push_str("\n(note: map is partial — walk caps hit)");
        }
        let files_budget = input.budget_tokens * 6 / 10;
        let mut files_text = String::new();
        for file in select(&input.root, &map, task, files_budget) {
            files_text.push_str(&format!(
                "--- {} ({} tokens) ---\n{}\n",
                file.path, file.tokens, file.content
            ));
        }
        BuiltContext {
            map: map_text,
            files: files_text,
            history_chars: (input.budget_tokens * 3 / 10 * 4) as usize,
        }
    }

    /// Emit a state change. Debug-asserts legality so tests catch loop bugs.
    fn transition(&self, from: AgentState, to: AgentState) -> Result<AgentState> {
        debug_assert!(
            is_legal_transition(&from, &to),
            "illegal agent transition: {from:?} -> {to:?}"
        );
        self.events.emit(DarbEvent::AgentStateChanged {
            from: from.clone(),
            to: to.clone(),
        })?;
        Ok(to)
    }

    /// Record the terminal transition plus the finished event.
    fn finish(
        &self,
        from: AgentState,
        outcome: AgentState,
        summary: String,
        stats: RunStats,
    ) -> FinalResult {
        debug_assert!(is_terminal(&outcome));
        debug_assert!(is_legal_transition(&from, &outcome));
        let _ = self.events.emit(DarbEvent::AgentStateChanged {
            from,
            to: outcome.clone(),
        });
        let _ = self.events.emit(DarbEvent::AgentFinished {
            summary: summary.clone(),
        });
        FinalResult {
            outcome,
            summary,
            iterations: stats.iterations,
            tool_calls: stats.tool_calls,
            changed_files: stats.changed_files,
            warnings: stats.warnings,
        }
    }
}

fn build_prompt(task: &str, plan: &Plan, history: &[String], context: &BuiltContext) -> String {
    let mut prompt = format!(
        "Task: {task}\nPlan: {} ({}/{} steps done)\n",
        plan.goal,
        plan.steps.len() - plan.pending_count(),
        plan.steps.len()
    );
    if !context.map.is_empty() {
        prompt.push_str("Project map:\n");
        prompt.push_str(&context.map);
        prompt.push('\n');
    }
    if !context.files.is_empty() {
        prompt.push_str("Relevant files:\n");
        prompt.push_str(&context.files);
        prompt.push('\n');
    }
    if !history.is_empty() {
        let joined = history.join("\n- ");
        prompt.push_str("Previous tool results:\n- ");
        prompt.push_str(&truncate_middle(&joined, context.history_chars));
        prompt.push('\n');
    }
    prompt.push_str(
        "Respond with a short summary, or emit tool calls as fenced json blocks \
         ({\"tool\": \"...\", \"target\": \"...\", \"arguments\": {...}}). \
         Available tools: read_file, list_directory, search, git_status, git_diff, edit_file, shell.",
    );
    prompt
}

/// Pre-built prompt context. Empty when the agent runs without context.
struct BuiltContext {
    map: String,
    files: String,
    history_chars: usize,
}

fn is_permission_ask(result: &ToolResult) -> bool {
    result.metadata.get("permission").map(String::as_str) == Some("ask")
}

fn summarize_result(result: &ToolResult) -> String {
    let tool = result
        .metadata
        .get("tool")
        .map(String::as_str)
        .unwrap_or("tool");
    let status = if result.success { "ok" } else { "FAILED" };
    let detail = if result.success {
        truncate(&result.output)
    } else {
        result.error.clone().unwrap_or_default()
    };
    format!("{tool}: {status} {detail}")
}

fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_RESULT_CHARS {
        return text.to_string();
    }
    let kept: String = text.chars().take(MAX_RESULT_CHARS).collect();
    format!("{kept}…[truncated]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use darb_core::config::{AgentConfig, PermissionDecision, PermissionsConfig};
    use darb_core::permissions::PermissionManager;
    use darb_providers::interface::{ProviderCapabilities, ProviderResponse, TokenUsage};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    /// Replies in order; repeats the last one forever. Records prompts.
    struct StubProvider {
        replies: std::sync::Mutex<Vec<String>>,
        seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl StubProvider {
        fn new(replies: Vec<&str>) -> Self {
            Self::new_shared(
                replies,
                std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            )
        }

        fn new_shared(
            replies: Vec<&str>,
            seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        ) -> Self {
            Self {
                replies: std::sync::Mutex::new(replies.into_iter().map(str::to_string).collect()),
                seen,
            }
        }
    }

    impl Provider for StubProvider {
        fn name(&self) -> &'static str {
            "stub"
        }

        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                streaming: false,
                tool_calling: false,
                vision: false,
                reasoning: false,
                structured_output: false,
                embeddings: false,
                max_context: 4096,
            }
        }

        fn complete(
            &self,
            request: ProviderRequest,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<ProviderResponse>> + Send + '_>,
        > {
            let text = {
                self.seen
                    .lock()
                    .expect("stub lock")
                    .push(request.prompt.clone());
                let mut replies = self.replies.lock().expect("stub lock");
                if replies.len() > 1 {
                    replies.remove(0)
                } else {
                    replies.first().cloned().unwrap_or_default()
                }
            };
            Box::pin(async move {
                Ok(ProviderResponse {
                    text,
                    usage: TokenUsage::default(),
                })
            })
        }

        fn stream(
            &self,
            _request: ProviderRequest,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ProviderStream>> + Send + '_>>
        {
            Box::pin(async move {
                Err(darb_core::errors::DarbError::Provider(
                    "streaming not supported in stub".to_string(),
                ))
            })
        }
    }

    use darb_providers::interface::ProviderStream;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_root() -> PathBuf {
        let id = COUNTER.fetch_add(1, AtomicOrdering::SeqCst);
        let root =
            std::env::temp_dir().join(format!("darb-agent-test-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("must create temp root");
        root
    }

    fn test_agent(replies: Vec<&str>, root: &PathBuf, limits: AgentConfig) -> Agent {
        let registry = ToolRegistry::new(
            PermissionManager::new(PermissionsConfig {
                read: PermissionDecision::Allow,
                edit: PermissionDecision::Allow,
                shell: PermissionDecision::Allow,
                delete: PermissionDecision::Allow,
                git_push: PermissionDecision::Deny,
            }),
            root,
        );
        Agent::new(
            Box::new(StubProvider::new(replies)),
            "stub-model".to_string(),
            registry,
            EventBus::default(),
            limits,
        )
    }

    fn quick_limits() -> AgentConfig {
        AgentConfig {
            max_iterations: 10,
            max_tool_calls: 10,
            max_retries: 2,
            timeout_seconds: 30,
        }
    }

    /// Answers every confirmation with a fixed verdict.
    struct FixedResponder(bool);

    impl PermissionResponder for FixedResponder {
        fn ask(
            &self,
            _request: &ToolRequest,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + '_>> {
            let answer = self.0;
            Box::pin(async move { answer })
        }
    }

    /// Agent with `edit = ask` plus a responder: the interactive path.
    fn ask_agent(replies: Vec<&str>, root: &PathBuf, answer: bool) -> Agent {
        let registry = ToolRegistry::new(
            PermissionManager::new(PermissionsConfig {
                read: PermissionDecision::Allow,
                edit: PermissionDecision::Ask,
                shell: PermissionDecision::Ask,
                delete: PermissionDecision::Ask,
                git_push: PermissionDecision::Deny,
            }),
            root,
        );
        Agent::new(
            Box::new(StubProvider::new(replies)),
            "stub-model".to_string(),
            registry,
            EventBus::default(),
            quick_limits(),
        )
        .with_responder(Arc::new(FixedResponder(answer)))
    }

    const EDIT_CALL: &str = "```json\n{\"tool\": \"edit_file\", \"target\": \"a.txt\", \
        \"arguments\": {\"old\": \"v = 1\", \"new\": \"v = 2\"}}\n```";

    #[tokio::test]
    async fn approval_runs_the_tool() {
        let root = temp_root();
        std::fs::write(root.join("a.txt"), "v = 1\n").expect("write");
        let agent = ask_agent(vec![EDIT_CALL, "Updated."], &root, true);
        let result = agent.run("bump version").await.expect("run");
        assert_eq!(result.outcome, AgentState::Completed);
        assert_eq!(result.tool_calls, 1);
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).expect("read"),
            "v = 2\n"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn refusal_fails_the_batch() {
        let root = temp_root();
        std::fs::write(root.join("a.txt"), "v = 1\n").expect("write");
        let agent = ask_agent(vec![EDIT_CALL], &root, false);
        let result = agent.run("bump version").await.expect("run");
        assert_eq!(result.outcome, AgentState::Failed);
        // Denied, never written.
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).expect("read"),
            "v = 1\n"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn no_responder_keeps_fail_fast() {
        let root = temp_root();
        std::fs::write(root.join("a.txt"), "v = 1\n").expect("write");
        // Same ask policy, but no responder: the ask-result fails the batch.
        let registry = ToolRegistry::new(
            PermissionManager::new(PermissionsConfig {
                read: PermissionDecision::Allow,
                edit: PermissionDecision::Ask,
                shell: PermissionDecision::Ask,
                delete: PermissionDecision::Ask,
                git_push: PermissionDecision::Deny,
            }),
            &root,
        );
        let agent = Agent::new(
            Box::new(StubProvider::new(vec![EDIT_CALL])),
            "stub-model".to_string(),
            registry,
            EventBus::default(),
            quick_limits(),
        );
        let result = agent.run("bump version").await.expect("run");
        assert_eq!(result.outcome, AgentState::Failed);
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).expect("read"),
            "v = 1\n"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn completes_without_tools() {
        let root = temp_root();
        let agent = test_agent(vec!["All done"], &root, quick_limits());
        let result = agent.run("say hi").await.expect("run");
        assert_eq!(result.outcome, AgentState::Completed);
        assert_eq!(result.iterations, 1);
        assert_eq!(result.tool_calls, 0);
        assert_eq!(result.summary, "All done");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn edits_file_then_concludes() {
        let root = temp_root();
        std::fs::write(root.join("a.txt"), "v = 1\n").expect("write");
        let edit = "```json\n{\"tool\": \"edit_file\", \"target\": \"a.txt\", \
            \"arguments\": {\"old\": \"v = 1\", \"new\": \"v = 2\"}}\n```";
        let agent = test_agent(vec![edit, "Updated the version."], &root, quick_limits());
        let result = agent.run("bump version").await.expect("run");
        assert_eq!(result.outcome, AgentState::Completed);
        assert_eq!(result.tool_calls, 1);
        assert_eq!(result.changed_files, vec!["a.txt".to_string()]);
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).expect("read"),
            "v = 2\n"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn failing_tools_give_up_after_retries() {
        let root = temp_root();
        let edit = "```json\n{\"tool\": \"edit_file\", \"target\": \"missing.txt\", \
            \"arguments\": {\"old\": \"a\", \"new\": \"b\"}}\n```";
        let limits = AgentConfig {
            max_retries: 1,
            ..quick_limits()
        };
        let agent = test_agent(vec![edit], &root, limits);
        let result = agent.run("edit ghost").await.expect("run");
        assert_eq!(result.outcome, AgentState::Failed);
        assert_eq!(result.tool_calls, 2);
        assert!(!result.warnings.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn iteration_budget_stops_the_loop() {
        let root = temp_root();
        std::fs::write(root.join("a.txt"), "x\n").expect("write");
        let read = "```json\n{\"tool\": \"read_file\", \"target\": \"a.txt\"}\n```";
        let limits = AgentConfig {
            max_iterations: 1,
            ..quick_limits()
        };
        let agent = test_agent(vec![read], &root, limits);
        let result = agent.run("read forever").await.expect("run");
        assert_eq!(result.outcome, AgentState::Failed);
        assert_eq!(result.iterations, 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn cancel_stops_before_starting() {
        let root = temp_root();
        let agent = test_agent(vec!["never"], &root, quick_limits());
        agent.cancel();
        let result = agent.run("cancelled task").await.expect("run");
        assert_eq!(result.outcome, AgentState::Cancelled);
        assert_eq!(result.iterations, 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn prompt_carries_project_context() {
        use std::sync::Mutex;

        let root = temp_root();
        std::fs::write(root.join("login.rs"), "fn login() {}\n").expect("write");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let registry = ToolRegistry::new(
            PermissionManager::new(PermissionsConfig {
                read: PermissionDecision::Allow,
                edit: PermissionDecision::Allow,
                shell: PermissionDecision::Allow,
                delete: PermissionDecision::Allow,
                git_push: PermissionDecision::Deny,
            }),
            &root,
        );
        let agent = Agent::new(
            Box::new(StubProvider::new_shared(vec!["done"], Arc::clone(&seen))),
            "stub-model".to_string(),
            registry,
            EventBus::default(),
            quick_limits(),
        )
        .with_context(root.clone(), 4000);
        let result = agent.run("fix login").await.expect("run");
        assert_eq!(result.outcome, AgentState::Completed);
        let prompts = seen.lock().expect("seen lock");
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains("Project map"), "{}", prompts[0]);
        assert!(prompts[0].contains("login.rs"), "{}", prompts[0]);
        assert!(prompts[0].contains("fn login() {}"), "{}", prompts[0]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Scripted stream chunks for the streaming tests.
    #[derive(Clone, Copy)]
    enum ScriptItem {
        Delta(&'static str),
        Done,
        Fail,
    }

    /// Provider that streams a script instead of completing.
    struct StreamStubProvider {
        script: std::sync::Mutex<Vec<ScriptItem>>,
    }

    impl Provider for StreamStubProvider {
        fn name(&self) -> &'static str {
            "stream-stub"
        }

        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                streaming: true,
                tool_calling: false,
                vision: false,
                reasoning: false,
                structured_output: false,
                embeddings: false,
                max_context: 4096,
            }
        }

        fn complete(
            &self,
            _request: ProviderRequest,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<ProviderResponse>> + Send + '_>,
        > {
            Box::pin(async move {
                Err(darb_core::errors::DarbError::Provider(
                    "complete must not be called on the streaming stub".to_string(),
                ))
            })
        }

        fn stream(
            &self,
            _request: ProviderRequest,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ProviderStream>> + Send + '_>>
        {
            let items: Vec<ScriptItem> = {
                let mut script = self.script.lock().expect("stub lock");
                std::mem::take(&mut *script)
            };
            Box::pin(async move {
                let (tx, rx) = tokio::sync::mpsc::channel(16);
                for item in items {
                    let event = match item {
                        ScriptItem::Delta(text) => Ok(
                            darb_providers::interface::StreamEvent::Delta(text.to_string()),
                        ),
                        ScriptItem::Done => Ok(darb_providers::interface::StreamEvent::Done(
                            TokenUsage::default(),
                        )),
                        ScriptItem::Fail => {
                            Err(darb_core::errors::DarbError::Provider("boom".to_string()))
                        }
                    };
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                Ok(rx)
            })
        }
    }

    fn stream_agent(script: Vec<ScriptItem>, root: &PathBuf) -> (Agent, EventBusReceiver) {
        use tokio::sync::broadcast::Receiver as BroadcastReceiver;
        let bus = EventBus::default();
        let rx: BroadcastReceiver<DarbEvent> = bus.subscribe();
        let registry = ToolRegistry::new(
            PermissionManager::new(PermissionsConfig {
                read: PermissionDecision::Allow,
                edit: PermissionDecision::Allow,
                shell: PermissionDecision::Allow,
                delete: PermissionDecision::Allow,
                git_push: PermissionDecision::Deny,
            }),
            root,
        );
        let agent = Agent::new(
            Box::new(StreamStubProvider {
                script: std::sync::Mutex::new(script),
            }),
            "stub-model".to_string(),
            registry,
            bus,
            quick_limits(),
        );
        (agent, rx)
    }

    type EventBusReceiver = tokio::sync::broadcast::Receiver<DarbEvent>;

    #[tokio::test]
    async fn streams_deltas_then_completes() {
        let root = temp_root();
        let (agent, mut rx) = stream_agent(
            vec![
                ScriptItem::Delta("hel"),
                ScriptItem::Delta("lo"),
                ScriptItem::Done,
            ],
            &root,
        );
        let result = agent.run("say hi").await.expect("run");
        assert_eq!(result.outcome, AgentState::Completed);
        assert_eq!(result.summary, "hello");
        let mut deltas = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let DarbEvent::ProviderDelta { text } = event {
                deltas.push(text);
            }
        }
        assert_eq!(deltas, vec!["hel".to_string(), "lo".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn interrupted_stream_uses_partial_text() {
        let root = temp_root();
        let (agent, _rx) = stream_agent(vec![ScriptItem::Delta("par"), ScriptItem::Fail], &root);
        let result = agent.run("say hi").await.expect("run");
        assert_eq!(result.outcome, AgentState::Completed);
        assert_eq!(result.summary, "par");
        assert!(result.warnings.iter().any(|w| w.contains("interrupt")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn stream_failing_before_first_delta_retries_out() {
        let root = temp_root();
        let (agent, _rx) = stream_agent(vec![ScriptItem::Fail], &root);
        let result = agent.run("say hi").await.expect("run");
        assert_eq!(result.outcome, AgentState::Failed);
        let _ = std::fs::remove_dir_all(&root);
    }
}
