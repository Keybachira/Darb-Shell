//! Tool calling: model text → structured [`ToolCallSpec`]s.
//!
//! Convention (documented in the prompt the loop sends): the model emits
//! fenced json blocks — `{"tool": "...", "target": "...", "arguments":
//! {...}}`. Only `tool` is required; `target` defaults to `""` and
//! `arguments` to empty. Malformed blocks are skipped, never fatal: a
//! model that rambles still yields zero calls instead of an error.

use std::collections::HashMap;

use darb_core::permissions::ToolRequest;

/// A parsed tool call, ready to become a [`ToolRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallSpec {
    pub tool: String,
    pub target: String,
    pub arguments: HashMap<String, String>,
}

impl ToolCallSpec {
    pub fn into_request(self) -> ToolRequest {
        ToolRequest {
            tool: self.tool,
            target: self.target,
            arguments: self.arguments,
        }
    }
}

/// Extract tool calls from model text, in order of appearance.
pub fn extract_tool_calls(text: &str) -> Vec<ToolCallSpec> {
    let mut calls = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("```") {
        let after_fence = &rest[start + 3..];
        // Skip the optional language tag up to the end of the line.
        let block_start = match after_fence.find('\n') {
            Some(index) => &after_fence[index + 1..],
            None => break,
        };
        let Some(end) = block_start.find("```") else {
            break;
        };
        let block = block_start[..end].trim();
        rest = &block_start[end + 3..];
        if let Some(call) = parse_call_block(block) {
            calls.push(call);
        }
    }
    calls
}

fn parse_call_block(block: &str) -> Option<ToolCallSpec> {
    let value: serde_json::Value = serde_json::from_str(block).ok()?;
    let tool = value.get("tool")?.as_str()?.to_string();
    if tool.trim().is_empty() {
        return None;
    }
    let target = value
        .get("target")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut arguments = HashMap::new();
    if let Some(map) = value.get("arguments").and_then(|v| v.as_object()) {
        for (key, val) in map {
            if let Some(text) = val.as_str() {
                arguments.insert(key.clone(), text.to_string());
            }
        }
    }
    Some(ToolCallSpec {
        tool,
        target,
        arguments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_fenced_json_call() {
        let text =
            "Let me read it.\n```json\n{\"tool\": \"read_file\", \"target\": \"a.txt\"}\n```\n";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool, "read_file");
        assert_eq!(calls[0].target, "a.txt");
    }

    #[test]
    fn extracts_arguments_and_multiple_calls() {
        let text = concat!(
            "```json\n{\"tool\": \"edit_file\", \"target\": \"a.txt\", ",
            "\"arguments\": {\"old\": \"x\", \"new\": \"y\"}}\n```\n",
            "```json\n{\"tool\": \"search\", \"target\": \"needle\"}\n```\n",
        );
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].arguments.get("old").map(String::as_str), Some("x"));
        assert_eq!(calls[1].tool, "search");
    }

    #[test]
    fn skips_malformed_blocks() {
        let text = "```json\nnot json\n```\n```json\n{\"target\": \"x\"}\n```\nplain text\n";
        assert!(extract_tool_calls(text).is_empty());
        assert!(extract_tool_calls("no fences here").is_empty());
    }
}
