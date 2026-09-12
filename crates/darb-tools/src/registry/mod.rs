//! Tool registry: Agent → ToolRequest → permission check → tool → ToolResult.
//!
//! Why this order: permission is checked *before* dispatch, so no tool
//! (implemented or not) can run without passing the manager (Contribuição
//! §28). Unknown/unimplemented tools fail as results, never as panics,
//! so the agent loop can reason about them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use darb_core::errors::DarbError;
use darb_core::permissions::{
    AskReason, Denial, PermissionManager, PermissionOutcome, ToolRequest,
};
use serde::{Deserialize, Serialize};

/// Structured tool result (Agents §13).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    pub duration_ms: u64,
}

impl ToolResult {
    pub fn ok(output: String) -> Self {
        Self {
            success: true,
            output,
            error: None,
            metadata: HashMap::new(),
            duration_ms: 0,
        }
    }

    pub fn fail(error: String) -> Self {
        Self {
            success: false,
            output: String::new(),
            error: Some(error),
            metadata: HashMap::new(),
            duration_ms: 0,
        }
    }

    fn with_meta(mut self, key: &str, value: String) -> Self {
        self.metadata.insert(key.to_string(), value);
        self
    }

    /// The request needs user confirmation first (agent enters WAITING_PERMISSION).
    pub fn permission_required(reason: AskReason, request: &ToolRequest) -> Self {
        Self::fail(format!(
            "permission required: '{}' on '{}'",
            request.tool, request.target
        ))
        .with_meta("permission", "ask".to_string())
        .with_meta("ask_reason", format!("{reason:?}"))
        .with_meta("tool", request.tool.clone())
        .with_meta("target", request.target.clone())
    }

    /// The request was refused; the agent must explain, not retry blindly.
    pub fn denied(denial: &Denial) -> Self {
        Self::fail(format!(
            "denied: '{}' on '{}' ({:?})",
            denial.tool, denial.target, denial.reason
        ))
        .with_meta("permission", "denied".to_string())
        .with_meta("deny_reason", format!("{:?}", denial.reason))
        .with_meta("tool", denial.tool.clone())
        .with_meta("target", denial.target.clone())
    }
}

/// Dispatches tool requests within a project root.
pub struct ToolRegistry {
    permissions: PermissionManager,
    root: PathBuf,
}

impl ToolRegistry {
    pub fn new(permissions: PermissionManager, root: impl AsRef<Path>) -> Self {
        Self {
            permissions,
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Check permission, then execute. The clock covers execution only,
    /// not the permission decision.
    pub fn dispatch(&self, request: &ToolRequest) -> ToolResult {
        match self.permissions.check(request) {
            PermissionOutcome::Allowed => {}
            PermissionOutcome::AskUser(reason) => {
                return ToolResult::permission_required(reason, request);
            }
            PermissionOutcome::Denied(denial) => return ToolResult::denied(&denial),
        }
        let started = Instant::now();
        let mut result = self.execute(request);
        result.duration_ms = started.elapsed().as_millis() as u64;
        result
    }

    /// Extract `edit_file` parameters. Both are required: defaulting them
    /// would let the agent edit blindly (Negócio §20).
    fn edit_args(request: &ToolRequest) -> Result<(String, String), DarbError> {
        match (request.arguments.get("old"), request.arguments.get("new")) {
            (Some(old), Some(new)) => Ok((old.clone(), new.clone())),
            _ => Err(DarbError::Tool(
                "edit_file needs 'old' and 'new' arguments".to_string(),
            )),
        }
    }

    fn execute(&self, request: &ToolRequest) -> ToolResult {
        let outcome: Result<ToolResult, DarbError> = match request.tool.as_str() {
            "read_file" => crate::filesystem::read_file(&self.root, &request.target)
                .map(|content| ToolResult::ok(content).with_meta("tool", "read_file".to_string())),
            "list_directory" => {
                crate::filesystem::list_directory(&self.root, &request.target).map(|entries| {
                    let mut lines = Vec::with_capacity(entries.len());
                    for entry in &entries {
                        lines.push(if entry.is_dir {
                            format!("{}/", entry.name)
                        } else {
                            entry.name.clone()
                        });
                    }
                    ToolResult::ok(lines.join("\n"))
                        .with_meta("tool", "list_directory".to_string())
                        .with_meta("entry_count", entries.len().to_string())
                })
            }
            "search" => crate::search::search(&self.root, &request.target).map(|out| {
                let lines: Vec<String> = out
                    .matches
                    .iter()
                    .map(|m| format!("{}:{}:{}", m.file, m.line, m.text))
                    .collect();
                ToolResult::ok(lines.join("\n"))
                    .with_meta("tool", "search".to_string())
                    .with_meta("match_count", out.matches.len().to_string())
                    .with_meta("truncated", out.truncated.to_string())
            }),
            "git_status" => crate::git::status(&self.root)
                .map(|out| ToolResult::ok(out).with_meta("tool", "git_status".to_string())),
            "git_diff" => crate::git::diff(&self.root, &request.target)
                .map(|out| ToolResult::ok(out).with_meta("tool", "git_diff".to_string())),
            "edit_file" => Self::edit_args(request).and_then(|(old, new)| {
                crate::editor::edit_file(&self.root, &request.target, &old, &new).map(|bytes| {
                    ToolResult::ok(format!("edited '{}' ({bytes} bytes)", request.target))
                        .with_meta("tool", "edit_file".to_string())
                        .with_meta("bytes_written", bytes.to_string())
                })
            }),
            "shell" | "run_shell" | "run_command" => crate::shell::run(&self.root, &request.target)
                .map(|out| {
                    let mut result = ToolResult {
                        success: out.succeeded(),
                        output: out.stdout.clone(),
                        error: if out.succeeded() {
                            None
                        } else {
                            Some(format!("exit {}: {}", out.exit_code, out.stderr.trim()))
                        },
                        metadata: HashMap::new(),
                        duration_ms: 0,
                    };
                    result = result
                        .with_meta("tool", "shell".to_string())
                        .with_meta("exit_code", out.exit_code.to_string())
                        .with_meta("truncated", out.truncated.to_string());
                    result
                }),
            other => Ok(ToolResult::fail(format!(
                "tool '{other}' is not implemented yet"
            ))),
        };
        match outcome {
            Ok(result) => result,
            Err(e) => ToolResult::fail(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darb_core::config::{PermissionDecision, PermissionsConfig};

    fn allow_all_registry(root: &Path) -> ToolRegistry {
        ToolRegistry::new(
            PermissionManager::new(PermissionsConfig {
                read: PermissionDecision::Allow,
                edit: PermissionDecision::Allow,
                shell: PermissionDecision::Allow,
                delete: PermissionDecision::Allow,
                git_push: PermissionDecision::Deny,
            }),
            root,
        )
    }

    #[test]
    fn read_file_round_trip() {
        let root = crate::filesystem::tests::temp_root();
        std::fs::write(root.join("a.txt"), "content\n").expect("write");
        let registry = allow_all_registry(&root);
        let result = registry.dispatch(&ToolRequest::new("read_file", "a.txt"));
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output, "content\n");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn escape_is_a_failure_result_not_a_panic() {
        let root = crate::filesystem::tests::temp_root();
        let registry = allow_all_registry(&root);
        let result = registry.dispatch(&ToolRequest::new("read_file", "../evil.txt"));
        assert!(!result.success);
        assert!(result.error.is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ask_policy_returns_permission_result() {
        let root = crate::filesystem::tests::temp_root();
        // Default config: read = allow.
        let registry =
            ToolRegistry::new(PermissionManager::new(PermissionsConfig::default()), &root);
        let result = registry.dispatch(&ToolRequest::new("shell", "cargo test"));
        assert!(!result.success);
        assert_eq!(
            result.metadata.get("permission").map(String::as_str),
            Some("ask")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn denied_tool_returns_denied_result() {
        let root = crate::filesystem::tests::temp_root();
        let registry = allow_all_registry(&root);
        let result = registry.dispatch(&ToolRequest::new("git_push", "origin main"));
        assert!(!result.success);
        assert_eq!(
            result.metadata.get("permission").map(String::as_str),
            Some("denied")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn search_and_list_through_registry() {
        let root = crate::filesystem::tests::temp_root();
        std::fs::write(root.join("a.txt"), "needle here\n").expect("write");
        let registry = allow_all_registry(&root);
        let search = registry.dispatch(&ToolRequest::new("search", "needle"));
        assert!(search.success, "{:?}", search.error);
        assert!(search.output.contains("a.txt:1"));
        let list = registry.dispatch(&ToolRequest::new("list_directory", ""));
        assert!(list.success, "{:?}", list.error);
        assert!(list.output.contains("a.txt"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn edit_file_through_registry() {
        let root = crate::filesystem::tests::temp_root();
        std::fs::write(root.join("a.txt"), "version = 1\n").expect("write");
        let registry = allow_all_registry(&root);
        let request = ToolRequest::new("edit_file", "a.txt")
            .with_arg("old", "version = 1")
            .with_arg("new", "version = 2");
        let result = registry.dispatch(&request);
        assert!(result.success, "{:?}", result.error);
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).expect("read"),
            "version = 2\n"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn edit_file_without_args_fails() {
        let root = crate::filesystem::tests::temp_root();
        std::fs::write(root.join("a.txt"), "x\n").expect("write");
        let registry = allow_all_registry(&root);
        let result = registry.dispatch(&ToolRequest::new("edit_file", "a.txt"));
        assert!(!result.success);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn shell_success_and_failure_through_registry() {
        let root = crate::filesystem::tests::temp_root();
        let registry = allow_all_registry(&root);
        let ok = registry.dispatch(&ToolRequest::new("shell", "echo hi"));
        assert!(ok.success, "{:?}", ok.error);
        assert_eq!(ok.metadata.get("exit_code").map(String::as_str), Some("0"));
        let fail = registry.dispatch(&ToolRequest::new("run_command", "exit 3"));
        assert!(!fail.success);
        assert_eq!(
            fail.metadata.get("exit_code").map(String::as_str),
            Some("3")
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
