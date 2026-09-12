//! Provider interface — the Agent talks to `Provider`, never to a concrete API.
//!
//! Why boxed futures instead of `async fn`: the trait must stay
//! object-safe so the runtime can hold `Box<dyn Provider>` and swap
//! providers without touching the Agent (Negócio §§13–14). HTTP transport
//! stays hidden behind each adapter; secrets never appear in errors.

use std::future::Future;
use std::pin::Pin;

use darb_core::errors::Result;
use serde::{Deserialize, Serialize};

/// What the Agent sends. Deliberately small: model selection and the
/// composed prompt. System instructions are composed into `prompt` by
/// the Agent so every adapter sees the same shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRequest {
    pub model: String,
    pub prompt: String,
}

/// Token accounting, for cost/latency observability (Agents §43).
/// Missing values default to zero (e.g. streamed replies without usage).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
}

impl TokenUsage {
    pub fn total(self) -> u64 {
        self.prompt_tokens + self.completion_tokens
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResponse {
    pub text: String,
    #[serde(default)]
    pub usage: TokenUsage,
}

/// One streamed item. The Agent renders `Delta` incrementally; `Done`
/// closes the stream and carries the final usage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    Delta(String),
    Done(TokenUsage),
}

/// Streaming receiver. A channel (not a `Stream` impl) so adapters need
/// nothing beyond tokio: the adapter pumps events in a background task
/// and the Agent drains with `recv().await` until `None`.
pub type ProviderStream = tokio::sync::mpsc::Receiver<Result<StreamEvent>>;

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

pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;

    fn capabilities(&self) -> ProviderCapabilities;

    fn complete(
        &self,
        request: ProviderRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProviderResponse>> + Send + '_>>;

    fn stream(
        &self,
        request: ProviderRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProviderStream>> + Send + '_>>;
}
