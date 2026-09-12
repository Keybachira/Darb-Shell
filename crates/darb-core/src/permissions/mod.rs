//! Permission manager: every tool call passes through here, no bypasses.
//!
//! Why this lives in core: the Agent requests, tools execute, but only the
//! manager decides (Negócio §§3–9, Contribuição §28). Rules in priority order:
//! 1. unknown tools fail closed (safety first);
//! 2. `sudo` is always denied (Negócio §9);
//! 3. destructive shell commands and `git push` need at least confirmation,
//!    even when `shell = "allow"` (Negócio §§8, 39);
//! 4. otherwise the configured `allow | ask | deny` applies.
//!
//! Denials carry structured data (what/why), never prose: the TUI renders
//! the explanation through i18n (Negócio §42).

use std::collections::HashMap;

use crate::config::{PermissionDecision, PermissionsConfig};

/// Risk level declared by each tool (Negócio §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Safe,
    Moderate,
    Dangerous,
}

/// A request to execute a tool. For shell-family tools, `target` carries
/// the command line so it can be screened for destructive patterns.
/// Tools needing extra parameters (e.g. `edit_file`'s old/new text) take
/// them from `arguments`; permission screening only uses tool + target,
/// so parameters can never change the risk class.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolRequest {
    pub tool: String,
    pub target: String,
    pub arguments: HashMap<String, String>,
}

impl ToolRequest {
    pub fn new(tool: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            target: target.into(),
            arguments: HashMap::new(),
        }
    }

    pub fn with_arg(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.arguments.insert(key.into(), value.into());
        self
    }
}

/// Why the user must be asked before executing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskReason {
    /// Policy says `ask` for this category.
    PolicyRequiresConfirmation,
    /// Command matches a destructive pattern (Negócio §39).
    DangerousCommand,
    /// `git push` is always high-risk (Negócio §8).
    GitPushRequiresConfirmation,
}

/// Why a request was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyReason {
    /// Policy says `deny` for this category.
    ForbiddenByPolicy,
    /// `sudo` is blocked (Negócio §9).
    SudoBlocked,
    /// Unknown tools fail closed: refusing is safer than guessing.
    UnknownTool,
    /// The user refused at the confirmation dialog.
    DeniedByUser,
}

/// Structured refusal: what was blocked and why (Negócio §42).
/// The TUI turns this into a user-visible explanation via i18n.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denial {
    pub tool: String,
    pub target: String,
    pub reason: DenyReason,
}

/// Outcome of a permission check (Agents §14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionOutcome {
    /// Execute immediately.
    Allowed,
    /// Caller must prompt the user (TUI dialog) before executing.
    AskUser(AskReason),
    /// Refused. Do not execute, explain instead.
    Denied(Denial),
}

impl PermissionOutcome {
    pub fn is_allowed(&self) -> bool {
        matches!(self, PermissionOutcome::Allowed)
    }
}

/// Tool-name families. Keep in sync with `darb-tools` registry names.
const READ_TOOLS: &[&str] = &[
    "read_file",
    "list_directory",
    "search",
    "git_status",
    "git_diff",
    "git_log",
];
const EDIT_TOOLS: &[&str] = &["write_file", "edit_file", "create_file", "install_package"];
const SHELL_TOOLS: &[&str] = &["shell", "run_shell", "run_command"];
const DELETE_TOOLS: &[&str] = &["delete_file", "delete"];

/// Substrings that mark a shell command as destructive (Negócio §39).
/// Matched case-insensitively against the whole command line so chained
/// commands (`git status; rm -rf /`) are caught too.
const DESTRUCTIVE_PATTERNS: &[&str] = &[
    "rm -rf",
    "reset --hard",
    "push --force",
    "drop database",
    ":(){:|:&};:",
];

/// First-token commands that are destructive or forbidden on their own.
/// `format` = disk formatting, `sudo` = privilege escalation.
const DANGEROUS_TOKENS: &[&str] = &["sudo", "format"];

fn is_shell_tool(name: &str) -> bool {
    SHELL_TOOLS.contains(&name)
}

fn is_git_push_command(command: &str) -> bool {
    command
        .split(&[';', '&', '|', '\n'])
        .any(|segment| segment.trim_start().to_lowercase().starts_with("git push"))
}

fn contains_destructive_pattern(command: &str) -> bool {
    let lower = command.to_lowercase();
    DESTRUCTIVE_PATTERNS
        .iter()
        .any(|pattern| lower.contains(pattern))
        || lower.split_whitespace().any(|token| {
            DANGEROUS_TOKENS
                .contains(&token.trim_matches(|c: char| !c.is_alphanumeric() && c != '-'))
        })
}

fn is_sudo_command(command: &str) -> bool {
    command.split_whitespace().any(|token| {
        token
            .trim_matches(|c: char| !c.is_alphanumeric())
            .eq_ignore_ascii_case("sudo")
    })
}

pub struct PermissionManager {
    config: PermissionsConfig,
    /// Single-use user confirmations, keyed `(tool, target)`. A grant turns
    /// one `AskUser` into `Allowed` and is then consumed. Grants never
    /// override `Denied`: policy denials, sudo, and unknown tools stay
    /// refused — the dialog can only confirm what policy marks askable.
    grants: std::sync::Mutex<std::collections::HashSet<(String, String)>>,
}

impl PermissionManager {
    pub fn new(config: PermissionsConfig) -> Self {
        Self {
            config,
            grants: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }

    pub fn config(&self) -> &PermissionsConfig {
        &self.config
    }

    /// Record a one-time user confirmation for an exact tool + target.
    pub fn grant_once(&self, tool: &str, target: &str) {
        if let Ok(mut grants) = self.grants.lock() {
            grants.insert((tool.to_string(), target.to_string()));
        }
    }

    /// Check a tool request against policy. Pure rules plus consumed
    /// grants: no I/O, no global state, so the Agent, tools, and tests
    /// all see the same decision path — there is no bypass around this.
    pub fn check(&self, request: &ToolRequest) -> PermissionOutcome {
        match self.evaluate(request) {
            PermissionOutcome::AskUser(_) => {
                let key = (request.tool.clone(), request.target.clone());
                let granted = self
                    .grants
                    .lock()
                    .map(|mut grants| grants.remove(&key))
                    .unwrap_or(false);
                if granted {
                    PermissionOutcome::Allowed
                } else {
                    self.evaluate(request)
                }
            }
            outcome => outcome,
        }
    }

    /// Policy evaluation without grant handling (single choke point above).
    fn evaluate(&self, request: &ToolRequest) -> PermissionOutcome {
        let tool = request.tool.as_str();

        if is_shell_tool(tool) {
            return self.check_shell(request);
        }
        if tool == "git_push" {
            return self.decide(
                request,
                self.config.git_push,
                AskReason::PolicyRequiresConfirmation,
            );
        }
        if READ_TOOLS.contains(&tool) {
            return self.decide(
                request,
                self.config.read,
                AskReason::PolicyRequiresConfirmation,
            );
        }
        if EDIT_TOOLS.contains(&tool) {
            return self.decide(
                request,
                self.config.edit,
                AskReason::PolicyRequiresConfirmation,
            );
        }
        if DELETE_TOOLS.contains(&tool) {
            return self.decide(
                request,
                self.config.delete,
                AskReason::PolicyRequiresConfirmation,
            );
        }
        PermissionOutcome::Denied(Denial {
            tool: request.tool.clone(),
            target: request.target.clone(),
            reason: DenyReason::UnknownTool,
        })
    }

    fn check_shell(&self, request: &ToolRequest) -> PermissionOutcome {
        // sudo is never allowed silently (Negócio §9).
        if is_sudo_command(&request.target) {
            return PermissionOutcome::Denied(Denial {
                tool: request.tool.clone(),
                target: request.target.clone(),
                reason: DenyReason::SudoBlocked,
            });
        }
        let base = self.config.shell;
        // git push via shell is still git push (Negócio §8).
        if is_git_push_command(&request.target) {
            return self.escalate(request, base, AskReason::GitPushRequiresConfirmation);
        }
        if contains_destructive_pattern(&request.target) {
            return self.escalate(request, base, AskReason::DangerousCommand);
        }
        self.decide(request, base, AskReason::PolicyRequiresConfirmation)
    }

    /// Apply a configured decision, mapping `ask` to the given reason.
    fn decide(
        &self,
        request: &ToolRequest,
        decision: PermissionDecision,
        ask: AskReason,
    ) -> PermissionOutcome {
        match decision {
            PermissionDecision::Allow => PermissionOutcome::Allowed,
            PermissionDecision::Ask => PermissionOutcome::AskUser(ask),
            PermissionDecision::Deny => PermissionOutcome::Denied(Denial {
                tool: request.tool.clone(),
                target: request.target.clone(),
                reason: DenyReason::ForbiddenByPolicy,
            }),
        }
    }

    /// Raise the floor to confirmation for high-risk commands: an `allow`
    /// policy becomes `ask`, `ask`/`deny` are preserved.
    fn escalate(
        &self,
        request: &ToolRequest,
        base: PermissionDecision,
        ask: AskReason,
    ) -> PermissionOutcome {
        match base {
            PermissionDecision::Allow => PermissionOutcome::AskUser(ask),
            _ => self.decide(request, base, ask),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_manager() -> PermissionManager {
        PermissionManager::new(PermissionsConfig::default())
    }

    #[test]
    fn safe_tools_are_allowed_by_default() {
        let manager = default_manager();
        for tool in [
            "read_file",
            "list_directory",
            "search",
            "git_status",
            "git_diff",
        ] {
            let outcome = manager.check(&ToolRequest::new(tool, "src/main.rs"));
            assert_eq!(outcome, PermissionOutcome::Allowed, "{tool}");
        }
    }

    #[test]
    fn edit_tools_ask_by_default() {
        let manager = default_manager();
        let outcome = manager.check(&ToolRequest::new("edit_file", "src/main.rs"));
        assert_eq!(
            outcome,
            PermissionOutcome::AskUser(AskReason::PolicyRequiresConfirmation)
        );
    }

    #[test]
    fn plain_shell_asks_by_default() {
        let manager = default_manager();
        let outcome = manager.check(&ToolRequest::new("shell", "cargo test"));
        assert_eq!(
            outcome,
            PermissionOutcome::AskUser(AskReason::PolicyRequiresConfirmation)
        );
    }

    #[test]
    fn sudo_is_denied_even_when_shell_allowed() {
        let manager = PermissionManager::new(PermissionsConfig {
            shell: PermissionDecision::Allow,
            ..PermissionsConfig::default()
        });
        let outcome = manager.check(&ToolRequest::new("shell", "sudo apt update"));
        assert_eq!(
            outcome,
            PermissionOutcome::Denied(Denial {
                tool: "shell".to_string(),
                target: "sudo apt update".to_string(),
                reason: DenyReason::SudoBlocked,
            })
        );
    }

    #[test]
    fn destructive_command_escalates_allow_to_ask() {
        let manager = PermissionManager::new(PermissionsConfig {
            shell: PermissionDecision::Allow,
            ..PermissionsConfig::default()
        });
        let outcome = manager.check(&ToolRequest::new("shell", "rm -rf /tmp/build"));
        assert_eq!(
            outcome,
            PermissionOutcome::AskUser(AskReason::DangerousCommand)
        );
    }

    #[test]
    fn chained_destructive_command_is_caught() {
        let manager = PermissionManager::new(PermissionsConfig {
            shell: PermissionDecision::Allow,
            ..PermissionsConfig::default()
        });
        let outcome = manager.check(&ToolRequest::new("shell", "git status; rm -rf /tmp/x"));
        assert_eq!(
            outcome,
            PermissionOutcome::AskUser(AskReason::DangerousCommand)
        );
    }

    #[test]
    fn git_push_via_shell_needs_confirmation_despite_allow() {
        let manager = PermissionManager::new(PermissionsConfig {
            shell: PermissionDecision::Allow,
            ..PermissionsConfig::default()
        });
        let outcome = manager.check(&ToolRequest::new("shell", "git push origin main"));
        assert_eq!(
            outcome,
            PermissionOutcome::AskUser(AskReason::GitPushRequiresConfirmation)
        );
    }

    #[test]
    fn git_push_tool_is_denied_by_default() {
        let manager = default_manager();
        let outcome = manager.check(&ToolRequest::new("git_push", "origin main"));
        assert_eq!(
            outcome,
            PermissionOutcome::Denied(Denial {
                tool: "git_push".to_string(),
                target: "origin main".to_string(),
                reason: DenyReason::ForbiddenByPolicy,
            })
        );
    }

    #[test]
    fn unknown_tool_fails_closed() {
        let manager = default_manager();
        let outcome = manager.check(&ToolRequest::new("format_disk", "/dev/sda"));
        assert_eq!(
            outcome,
            PermissionOutcome::Denied(Denial {
                tool: "format_disk".to_string(),
                target: "/dev/sda".to_string(),
                reason: DenyReason::UnknownTool,
            })
        );
    }

    #[test]
    fn explicit_allow_policy_is_honored() {
        let manager = PermissionManager::new(PermissionsConfig {
            edit: PermissionDecision::Allow,
            shell: PermissionDecision::Allow,
            ..PermissionsConfig::default()
        });
        assert!(manager
            .check(&ToolRequest::new("edit_file", "a.rs"))
            .is_allowed());
        assert!(manager
            .check(&ToolRequest::new("shell", "cargo check"))
            .is_allowed());
    }

    #[test]
    fn grant_turns_one_ask_into_allow_then_expires() {
        let manager = default_manager();
        let request = ToolRequest::new("edit_file", "a.rs");
        assert_eq!(
            manager.check(&request),
            PermissionOutcome::AskUser(AskReason::PolicyRequiresConfirmation)
        );
        manager.grant_once("edit_file", "a.rs");
        assert!(manager.check(&request).is_allowed());
        // Consumed: asks again.
        assert_eq!(
            manager.check(&request),
            PermissionOutcome::AskUser(AskReason::PolicyRequiresConfirmation)
        );
    }

    #[test]
    fn grant_never_overrides_denials() {
        let manager = default_manager();
        manager.grant_once("git_push", "origin main");
        manager.grant_once("shell", "sudo apt update");
        manager.grant_once("format_disk", "/dev/sda");
        assert!(matches!(
            manager.check(&ToolRequest::new("git_push", "origin main")),
            PermissionOutcome::Denied(_)
        ));
        assert!(matches!(
            manager.check(&ToolRequest::new("shell", "sudo apt update")),
            PermissionOutcome::Denied(_)
        ));
        assert!(matches!(
            manager.check(&ToolRequest::new("format_disk", "/dev/sda")),
            PermissionOutcome::Denied(_)
        ));
    }
}
