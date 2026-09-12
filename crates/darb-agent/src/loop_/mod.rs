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

use darb_core::config::AgentConfig;
use darb_core::errors::Result;
use darb_core::events::{AgentState, DarbEvent, EventBus};
use darb_providers::interface::{Provider, ProviderRequest};
use darb_tools::registry::{ToolRegistry, ToolResult};

use crate::executor::Executor;
use crate::planner::Plan;
use crate::reviewer::{review, ReviewVerdict};
use crate::state::{is_legal_transition, is_terminal};

/// Tool-result lines longer than this are cut before going back into the
/// prompt (context/token budget, Negócio §19).
const MAX_RESULT_CHARS: usize = 4000;

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
        }
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
            let prompt = build_prompt(task, &plan, &history);
            let response = match self
                .provider
                .complete(ProviderRequest {
                    model: self.model.clone(),
                    prompt,
                })
                .await
            {
                Ok(response) => response,
                Err(e) => {
                    stats.failures += 1;
                    stats.warnings.push(format!("provider request failed: {e}"));
                    if stats.failures > self.limits.max_retries {
                        let summary = stats.last_text.clone();
                        return Ok(self.finish(state, AgentState::Failed, summary, stats));
                    }
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
                // Remembered before the move: only successful edits count
                // as changed files.
                let edited_target = (call.tool == "edit_file").then(|| call.target.clone());
                let result = executor.execute(call);
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

fn build_prompt(task: &str, plan: &Plan, history: &[String]) -> String {
    let mut prompt = format!(
        "Task: {task}\nPlan: {} ({}/{} steps done)\n",
        plan.goal,
        plan.steps.len() - plan.pending_count(),
        plan.steps.len()
    );
    if !history.is_empty() {
        prompt.push_str("Previous tool results:\n");
        for line in history {
            prompt.push_str("- ");
            prompt.push_str(line);
            prompt.push('\n');
        }
    }
    prompt.push_str(
        "Respond with a short summary, or emit tool calls as fenced json blocks \
         ({\"tool\": \"...\", \"target\": \"...\", \"arguments\": {...}}). \
         Available tools: read_file, list_directory, search, git_status, git_diff, edit_file, shell.",
    );
    prompt
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

    /// Replies in order; repeats the last one forever.
    struct StubProvider {
        replies: std::sync::Mutex<Vec<String>>,
    }

    impl StubProvider {
        fn new(replies: Vec<&str>) -> Self {
            Self {
                replies: std::sync::Mutex::new(replies.into_iter().map(str::to_string).collect()),
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
            _request: ProviderRequest,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<ProviderResponse>> + Send + '_>,
        > {
            let text = {
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
}
