//! Selector: top files by relevance, contents included, inside a budget.
//!
//! Reads through [`resolve_within`](darb_tools::filesystem::resolve_within)
//! so selection enjoys the same root confinement as the tools (reuse
//! before creating). Unreadable files are skipped — the map is
//! best-effort, and one bad file must not starve the whole context.

use std::path::Path;

use darb_tools::filesystem::{resolve_within, MAX_READ_BYTES};

use crate::compression::truncate_middle;
use crate::project_map::ProjectMap;
use crate::relevance::score;
use crate::tokenizer::estimate;

/// Max estimated tokens per file: one huge file must not eat the budget.
pub const MAX_FILE_TOKENS: u32 = 2000;

#[derive(Debug, Clone)]
pub struct SelectedFile {
    pub path: String,
    pub content: String,
    pub tokens: u32,
}

/// Highest-relevance files whose contents fit `budget_tokens` (estimated).
/// Zero-score files are still included while budget lasts: an empty task
/// match shouldn't yield an empty context.
pub fn select(root: &Path, map: &ProjectMap, task: &str, budget_tokens: u32) -> Vec<SelectedFile> {
    let mut selected = Vec::new();
    let mut spent: u32 = 0;
    for scored in score(task, &map.files) {
        if spent >= budget_tokens {
            break;
        }
        let path = match resolve_within(root, &scored.file.path) {
            Ok(path) => path,
            Err(_) => continue,
        };
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        if bytes.len() as u64 > MAX_READ_BYTES {
            continue;
        }
        let content = String::from_utf8_lossy(&bytes).into_owned();
        let content = if estimate(&content) > MAX_FILE_TOKENS {
            truncate_middle(&content, (MAX_FILE_TOKENS * 4) as usize)
        } else {
            content
        };
        let tokens = estimate(&content);
        if spent + tokens > budget_tokens {
            break;
        }
        spent += tokens;
        selected.push(SelectedFile {
            path: scored.file.path.clone(),
            content,
            tokens,
        });
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::{tests::temp_project, Indexer};

    #[test]
    fn respects_budget_and_reads_top_file() {
        let root = temp_project();
        std::fs::write(root.join("login.rs"), "fn login() {}\n").expect("write");
        std::fs::write(root.join("other.rs"), "fn other() {}\n").expect("write");
        let map = Indexer::new(&root).scan();
        let picked = select(&root, &map, "fix login", 10_000);
        assert!(!picked.is_empty());
        assert_eq!(picked[0].path, "login.rs");
        assert!(picked[0].content.contains("fn login"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn tiny_budget_selects_nothing() {
        let root = temp_project();
        std::fs::write(root.join("login.rs"), "fn login() {}\n").expect("write");
        let map = Indexer::new(&root).scan();
        assert!(select(&root, &map, "fix login", 1).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
