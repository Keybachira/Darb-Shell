//! OpenAI adapter (`/chat/completions`, JSON + SSE).
//!
//! Key handling (Arquitetura §55): the key comes from
//! `DARB_OPENAI_API_KEY` (then `OPENAI_API_KEY`), never from config files,
//! and never appears in errors or logs. `base_url` is overridable so tests
//! and OpenAI-compatible gateways can point elsewhere.

use std::time::Duration;

use darb_core::errors::{DarbError, Result};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::interface::{
    Provider, ProviderCapabilities, ProviderRequest, ProviderResponse, ProviderStream, StreamEvent,
    TokenUsage,
};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
/// Per-request timeout for non-streamed completions. Streams have no total
/// timeout (they can run long); the Agent cancels by dropping the receiver.
const COMPLETE_TIMEOUT: Duration = Duration::from_secs(120);
/// Events buffered between the pump task and the Agent.
const STREAM_BUFFER: usize = 32;

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    stream: bool,
}

pub struct OpenAIAdapter {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl OpenAIAdapter {
    /// Build with an explicit key. Empty keys are rejected immediately —
    /// a late 401 after an agent loop started is worse than a fast error.
    pub fn new(api_key: String, base_url: Option<String>) -> Result<Self> {
        if api_key.trim().is_empty() {
            return Err(DarbError::Provider(
                "missing API key: set DARB_OPENAI_API_KEY".to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .build()
            .map_err(map_transport_err)?;
        Ok(Self {
            client,
            api_key,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
        })
    }

    /// Build from the environment (`DARB_OPENAI_API_KEY`, else `OPENAI_API_KEY`).
    pub fn from_env() -> Result<Self> {
        Self::from_env_vars("DARB_OPENAI_API_KEY", "OPENAI_API_KEY")
    }

    fn from_env_vars(primary: &str, fallback: &str) -> Result<Self> {
        let key = std::env::var(primary)
            .or_else(|_| std::env::var(fallback))
            .unwrap_or_default();
        Self::new(key, None)
    }

    fn completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    async fn post(&self, request: &ProviderRequest, stream: bool) -> Result<reqwest::Response> {
        let body = ChatRequest {
            model: &request.model,
            messages: vec![ChatMessage {
                role: "user",
                content: &request.prompt,
            }],
            stream,
        };
        let mut call = self
            .client
            .post(self.completions_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body);
        if !stream {
            call = call.timeout(COMPLETE_TIMEOUT);
        }
        let response = call.send().await.map_err(map_transport_err)?;
        let status = response.status();
        if !status.is_success() {
            // Read a bounded prefix: error bodies are small JSON, but a
            // misbehaving gateway could stream megabytes here.
            let snippet = response.text().await.unwrap_or_default();
            let snippet: String = snippet.chars().take(500).collect();
            return Err(DarbError::Provider(format!(
                "openai request failed with status {status}: {}",
                snippet.trim()
            )));
        }
        Ok(response)
    }
}

/// Transport failures (DNS, connect, TLS, local timeouts) are network
/// problems, not provider rejections — the Agent may retry or fall back
/// to a local provider (Negócio §36).
fn map_transport_err(e: reqwest::Error) -> DarbError {
    DarbError::Network(format!("http transport failed: {e}"))
}

impl Provider for OpenAIAdapter {
    fn name(&self) -> &'static str {
        "openai"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            streaming: true,
            tool_calling: true,
            vision: false,
            reasoning: false,
            structured_output: true,
            embeddings: false,
            max_context: 128_000,
        }
    }

    fn complete(
        &self,
        request: ProviderRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ProviderResponse>> + Send + '_>>
    {
        Box::pin(async move {
            let response = self.post(&request, false).await?;
            let value: serde_json::Value = response.json().await.map_err(map_transport_err)?;
            let text = value
                .pointer("/choices/0/message/content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if text.is_empty() {
                return Err(DarbError::Provider(
                    "openai returned an empty completion".to_string(),
                ));
            }
            let usage = value
                .get("usage")
                .and_then(|u| serde_json::from_value(u.clone()).ok())
                .unwrap_or_default();
            Ok(ProviderResponse { text, usage })
        })
    }

    fn stream(
        &self,
        request: ProviderRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ProviderStream>> + Send + '_>>
    {
        Box::pin(async move {
            let mut response = self.post(&request, true).await?;
            let (tx, rx) = mpsc::channel(STREAM_BUFFER);
            tokio::spawn(async move {
                let mut buffer = String::new();
                loop {
                    match response.chunk().await {
                        Ok(Some(chunk)) => {
                            buffer.push_str(&String::from_utf8_lossy(&chunk));
                            drain_sse_lines(&mut buffer, &tx).await;
                        }
                        Ok(None) => break,
                        Err(e) => {
                            let _ = tx.send(Err(map_transport_err(e))).await;
                            break;
                        }
                    }
                }
            });
            Ok(rx)
        })
    }
}

/// Move complete `data:` lines out of `buffer` into the channel.
/// A closed receiver just stops the pump; the Agent went away, so there
/// is nothing left to do.
async fn drain_sse_lines(buffer: &mut String, tx: &mpsc::Sender<Result<StreamEvent>>) {
    while let Some(newline) = buffer.find('\n') {
        let line: String = buffer.drain(..=newline).collect();
        let Some(event) = parse_sse_line(line.trim()) else {
            continue;
        };
        if tx.send(Ok(event)).await.is_err() {
            buffer.clear();
            break;
        }
    }
}

/// Parse one SSE line. Returns `None` for comments, empty lines, and the
/// terminal `data: [DONE]` marker.
fn parse_sse_line(line: &str) -> Option<StreamEvent> {
    let data = line.strip_prefix("data:")?.trim();
    if data.is_empty() || data == "[DONE]" {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(data).ok()?;
    let choice = value.get("choices")?.get(0)?;
    let delta = choice
        .pointer("/delta/content")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let usage: TokenUsage = value
        .get("usage")
        .and_then(|u| serde_json::from_value(u.clone()).ok())
        .unwrap_or_default();
    if !delta.is_empty() {
        return Some(StreamEvent::Delta(delta.to_string()));
    }
    // A choice with no text but usage/finish info ends this turn.
    if choice.get("finish_reason").is_some() || usage.total() > 0 {
        return Some(StreamEvent::Done(usage));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const COMPLETE_BODY: &str = r#"{"id":"x","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"hello"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":3,"total_tokens":8}}"#;

    const STREAM_BODY: &str = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"hel\"},\"index\":0}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"index\":0}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"index\":0,\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2}}\n\n",
        "data: [DONE]\n\n",
    );

    /// Minimal HTTP/1.1 mock: one connection, canned status + body.
    /// Returns the base URL (no external crates needed).
    async fn mock_server(status: u16, reason: &'static str, body: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock");
        let port = listener.local_addr().expect("addr").port();
        let body = body.to_string();
        tokio::spawn(async move {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            // Read until end of headers; the request body is irrelevant here.
            let mut seen = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                let Ok(n) = stream.read(&mut chunk).await else {
                    return;
                };
                if n == 0 {
                    break;
                }
                seen.extend_from_slice(&chunk[..n]);
                if seen.len() > 8192 || seen.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });
        format!("http://127.0.0.1:{port}")
    }

    fn test_adapter(base_url: String) -> OpenAIAdapter {
        OpenAIAdapter::new("sk-test-key".to_string(), Some(base_url)).expect("adapter")
    }

    fn test_request() -> ProviderRequest {
        ProviderRequest {
            model: "gpt-test".to_string(),
            prompt: "hi".to_string(),
        }
    }

    #[test]
    fn empty_key_is_rejected() {
        assert!(OpenAIAdapter::new("  ".to_string(), None).is_err());
    }

    #[test]
    fn missing_env_is_an_error() {
        // Unique names: parallel tests never share them.
        assert!(
            OpenAIAdapter::from_env_vars("DARB_TEST_MISSING_A_1", "DARB_TEST_MISSING_B_1").is_err()
        );
    }

    #[tokio::test]
    async fn complete_parses_text_and_usage() {
        let base = mock_server(200, "OK", COMPLETE_BODY).await;
        let response = test_adapter(base)
            .complete(test_request())
            .await
            .expect("complete");
        assert_eq!(response.text, "hello");
        assert_eq!(response.usage.prompt_tokens, 5);
        assert_eq!(response.usage.completion_tokens, 3);
    }

    #[tokio::test]
    async fn stream_reassembles_deltas() {
        let base = mock_server(200, "OK", STREAM_BODY).await;
        let mut stream = test_adapter(base)
            .stream(test_request())
            .await
            .expect("stream");
        let mut text = String::new();
        let mut usage = TokenUsage::default();
        while let Some(event) = stream.recv().await {
            match event.expect("stream item") {
                StreamEvent::Delta(part) => text.push_str(&part),
                StreamEvent::Done(final_usage) => usage = final_usage,
            }
        }
        assert_eq!(text, "hello");
        assert_eq!(usage.total(), 7);
    }

    #[tokio::test]
    async fn auth_failure_hides_the_key() {
        let base = mock_server(401, "Unauthorized", r#"{"error":{"message":"bad key"}}"#).await;
        let err = test_adapter(base)
            .complete(test_request())
            .await
            .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("401"), "{message}");
        assert!(!message.contains("sk-test-key"), "{message}");
    }

    #[test]
    fn sse_done_and_noise_are_ignored() {
        assert!(parse_sse_line("data: [DONE]").is_none());
        assert!(parse_sse_line(": comment").is_none());
        assert!(parse_sse_line("").is_none());
    }
}
