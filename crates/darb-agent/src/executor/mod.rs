//! Executor: parsed call → [`ToolResult`] via the registry.
//!
//! Deliberately thin: permission checks and execution live in
//! `darb-tools`; the Agent never implements tools itself
//! (Contribuição §27).

use darb_core::permissions::ToolRequest;
use darb_tools::registry::{ToolRegistry, ToolResult};

use crate::tool_calling::ToolCallSpec;

pub struct Executor<'a> {
    registry: &'a ToolRegistry,
}

impl<'a> Executor<'a> {
    pub fn new(registry: &'a ToolRegistry) -> Self {
        Self { registry }
    }

    /// Execute one parsed call. Never panics: refusals and failures come
    /// back as results for the reviewer to judge.
    pub fn execute(&self, call: ToolCallSpec) -> ToolResult {
        let request = ToolRequest {
            tool: call.tool,
            target: call.target,
            arguments: call.arguments,
        };
        self.registry.dispatch(&request)
    }
}
