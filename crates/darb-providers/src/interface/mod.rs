//! Provider interface — the Agent talks to `Provider`, never to a concrete API (§14).
//! HTTP transport is reqwest, hidden behind each adapter.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRequest {
    pub model: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResponse {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub streaming: bool,
    pub tool_calling: bool,
    pub vision: bool,
    pub reasoning: bool,
    pub structured_output: bool,
    pub embeddings: bool,
    pub max_context: u32,
}

pub trait Provider {
    fn capabilities(&self) -> ProviderCapabilities;
}
