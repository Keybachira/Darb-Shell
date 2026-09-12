//! Configuration loading (TOML).
//!
//! Why this shape: human-edited config stays in TOML (§9 Stacks), structs
//! mirror `configs/default.toml` so defaults are safe and old files keep
//! parsing (unknown/missing keys fall back to defaults, §32 Regras de Negócio).
//! Secrets never live here — they come from env vars / OS stores.

use serde::Deserialize;
use std::path::Path;

use crate::errors::{DarbError, Result};

/// A single permission decision: allow, ask the user, or deny.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionDecision {
    Allow,
    Ask,
    Deny,
}

fn default_allow() -> PermissionDecision {
    PermissionDecision::Allow
}

fn default_ask() -> PermissionDecision {
    PermissionDecision::Ask
}

fn default_deny() -> PermissionDecision {
    PermissionDecision::Deny
}

fn default_app_language() -> String {
    "pt".to_string()
}

fn default_profile() -> String {
    "eco".to_string()
}

fn default_provider_name() -> String {
    "openai".to_string()
}

fn default_provider_model() -> String {
    "...".to_string()
}

fn default_max_tokens() -> u32 {
    16000
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    #[serde(default = "default_app_language")]
    pub language: String,
    #[serde(default = "default_profile")]
    pub profile: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            language: default_app_language(),
            profile: default_profile(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    #[serde(default = "default_provider_name")]
    pub name: String,
    #[serde(default = "default_provider_model")]
    pub model: String,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            name: default_provider_name(),
            model: default_provider_model(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct InterfaceConfig {
    #[serde(default = "default_app_language")]
    pub language: String,
    #[serde(default = "default_true")]
    pub mouse: bool,
    pub animations: bool,
    pub compact: bool,
}

impl Default for InterfaceConfig {
    fn default() -> Self {
        Self {
            language: default_app_language(),
            mouse: true,
            animations: false,
            compact: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ContextConfig {
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_true")]
    pub lazy_indexing: bool,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_tokens: default_max_tokens(),
            lazy_indexing: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PerformanceConfig {
    #[serde(default = "default_profile")]
    pub profile: String,
    /// `None` = default behavior for the profile (remote allowed, except `local`).
    pub remote_requests: Option<bool>,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            profile: default_profile(),
            remote_requests: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PermissionsConfig {
    #[serde(default = "default_allow")]
    pub read: PermissionDecision,
    #[serde(default = "default_ask")]
    pub edit: PermissionDecision,
    #[serde(default = "default_ask")]
    pub shell: PermissionDecision,
    #[serde(default = "default_ask")]
    pub delete: PermissionDecision,
    #[serde(default = "default_deny")]
    pub git_push: PermissionDecision,
}

impl Default for PermissionsConfig {
    fn default() -> Self {
        Self {
            read: PermissionDecision::Allow,
            edit: PermissionDecision::Ask,
            shell: PermissionDecision::Ask,
            delete: PermissionDecision::Ask,
            git_push: PermissionDecision::Deny,
        }
    }
}

fn default_max_iterations() -> u32 {
    20
}

fn default_max_tool_calls() -> u32 {
    100
}

fn default_max_retries() -> u32 {
    3
}

fn default_timeout_seconds() -> u64 {
    900
}

/// Agent execution limits (Agents §22). They bound autonomy: no infinite loops.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    #[serde(default = "default_max_tool_calls")]
    pub max_tool_calls: u32,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            max_tool_calls: default_max_tool_calls(),
            max_retries: default_max_retries(),
            timeout_seconds: default_timeout_seconds(),
        }
    }
}

/// Root configuration. Every section has safe defaults so old/partial
/// files keep working instead of breaking silently.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DarbConfig {
    pub app: AppConfig,
    pub provider: ProviderConfig,
    pub interface: InterfaceConfig,
    pub context: ContextConfig,
    pub performance: PerformanceConfig,
    pub permissions: PermissionsConfig,
    pub agent: AgentConfig,
}

impl DarbConfig {
    /// Effective UI language. `interface.language` wins, `app.language` is legacy.
    pub fn language(&self) -> &str {
        if self.interface.language.is_empty() {
            &self.app.language
        } else {
            &self.interface.language
        }
    }
}

/// Parse TOML text into [`DarbConfig`].
pub fn load_from_str(text: &str) -> Result<DarbConfig> {
    toml::from_str(text).map_err(|e| DarbError::Config(format!("failed to parse config: {e}")))
}

/// Load [`DarbConfig`] from a TOML file.
///
/// The error always carries the file path and the reason (§13 Contribuição).
pub fn load_from_file(path: impl AsRef<Path>) -> Result<DarbConfig> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|e| {
        DarbError::Config(format!(
            "could not read config file '{}': {}",
            path.display(),
            e
        ))
    })?;
    load_from_str(&text).map_err(|e| {
        DarbError::Config(format!(
            "could not parse config file '{}': {}",
            path.display(),
            e
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_uses_safe_defaults() {
        let cfg = load_from_str("").expect("empty config must parse");
        assert_eq!(cfg.language(), "pt");
        assert_eq!(cfg.permissions.read, PermissionDecision::Allow);
        assert_eq!(cfg.permissions.git_push, PermissionDecision::Deny);
        assert_eq!(cfg.agent.max_iterations, 20);
        assert_eq!(cfg.agent.max_retries, 3);
    }

    #[test]
    fn partial_config_keeps_defaults_for_missing_keys() {
        let cfg = load_from_str("[provider]\nname = \"ollama\"\n").expect("must parse");
        assert_eq!(cfg.provider.name, "ollama");
        // Untouched sections keep defaults (backwards compatibility).
        assert_eq!(cfg.language(), "pt");
        assert_eq!(cfg.context.max_tokens, 16000);
    }

    #[test]
    fn invalid_toml_reports_error() {
        let err = load_from_str("[unclosed").unwrap_err();
        assert!(matches!(err, DarbError::Config(_)));
    }

    #[test]
    fn missing_file_reports_path() {
        let err = load_from_file("does-not-exist-darb.toml").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("does-not-exist-darb.toml"), "{msg}");
    }
}
