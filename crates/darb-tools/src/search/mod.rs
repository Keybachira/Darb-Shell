//! Project search. Primary engine is the external `ripgrep` binary
//! (Stacks §12: prefer rg over a home-grown index); a small std-only
//! line scanner is the fallback when `rg` is missing, not a replacement.
//!
//! Exclusions follow Agents §17 (`target/`, `.git/`, …). Results are
//! capped so a broad query can't flood memory or the token budget.

use std::path::{Path, PathBuf};

use darb_core::errors::{DarbError, Result};
use serde::{Deserialize, Serialize};

/// Max matches returned per query; the rest is reported via `truncated`.
pub const MAX_MATCHES: usize = 100;

/// Files larger than this are skipped while scanning.
pub const MAX_SCAN_BYTES: u64 = 512 * 1024;

/// Directories never descended into (Agents §17).
const EXCLUDED_DIRS: &[&str] = &[
    ".git",
    "target",
    "dist",
    "build",
    ".cache",
    "coverage",
    "node_modules",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchMatch {
    /// Project-relative path with `/` separators.
    pub file: String,
    pub line: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchOutput {
    pub matches: Vec<SearchMatch>,
    /// True when more matches existed than returned.
    pub truncated: bool,
}

/// Search `root` for `query` (literal substring, case-sensitive).
/// Uses `rg` when available, otherwise the internal fallback scanner.
pub fn search(root: &Path, query: &str) -> Result<SearchOutput> {
    if query.is_empty() {
        return Err(DarbError::Tool(
            "search query must not be empty".to_string(),
        ));
    }
    if rg_available() {
        search_with_rg(root, query)
    } else {
        search_fallback(root, query)
    }
}

fn rg_available() -> bool {
    std::process::Command::new("rg")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn search_with_rg(root: &Path, query: &str) -> Result<SearchOutput> {
    let output = std::process::Command::new("rg")
        .arg("--json")
        .arg("--fixed-strings")
        .arg("--no-heading")
        .arg("--max-count")
        .arg(MAX_MATCHES.to_string())
        .arg("--glob")
        .arg("!{.git,target,dist,build,.cache,coverage,node_modules}/**")
        .arg(query)
        .arg(root)
        .output()
        .map_err(|e| DarbError::Tool(format!("could not run ripgrep: {e}")))?;
    // rg exits 1 on "no matches", which is a valid empty result.
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(DarbError::Tool(format!(
            "ripgrep failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    parse_rg_json(root, &output.stdout)
}

/// Parse `rg --json` (`match` lines only) into [`SearchMatch`]s.
fn parse_rg_json(root: &Path, stdout: &[u8]) -> Result<SearchOutput> {
    let text = String::from_utf8_lossy(stdout);
    let mut matches = Vec::new();
    for line in text.lines() {
        let event: serde_json::Value = match serde_json::from_str(line) {
            Ok(event) => event,
            Err(_) => continue,
        };
        if event.get("type").and_then(|t| t.as_str()) != Some("match") {
            continue;
        }
        let data = &event["data"];
        let abs_path = data["path"]["text"].as_str().unwrap_or("");
        let line_number = data["line_number"].as_u64().unwrap_or(0) as u32;
        let content = data["lines"]["text"]
            .as_str()
            .unwrap_or("")
            .trim_end()
            .to_string();
        let file = relative_display(root, abs_path);
        matches.push(SearchMatch {
            file,
            line: line_number,
            text: content,
        });
        if matches.len() >= MAX_MATCHES {
            break;
        }
    }
    Ok(SearchOutput {
        matches,
        truncated: false,
    })
}

fn relative_display(root: &Path, abs_path: &str) -> String {
    let abs = Path::new(abs_path);
    match abs.strip_prefix(root) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => abs_path.to_string(),
    }
}

/// Fallback scanner: recursive walk, literal substring per line.
/// Only used when `rg` is unavailable.
fn search_fallback(root: &Path, query: &str) -> Result<SearchOutput> {
    let mut matches = Vec::new();
    let mut truncated = false;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        // Collect + sort for deterministic output.
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        for path in paths {
            if matches.len() >= MAX_MATCHES {
                truncated = true;
                break;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if path.is_dir() {
                if !EXCLUDED_DIRS.contains(&name.as_str()) {
                    stack.push(path);
                }
                continue;
            }
            if name.is_empty() {
                continue;
            }
            let Ok(metadata) = std::fs::metadata(&path) else {
                continue;
            };
            if metadata.len() > MAX_SCAN_BYTES {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let content = String::from_utf8_lossy(&bytes);
            for (index, line) in content.lines().enumerate() {
                if line.contains(query) {
                    let rel = path
                        .strip_prefix(root)
                        .map(|r| r.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_else(|_| path.to_string_lossy().into_owned());
                    matches.push(SearchMatch {
                        file: rel,
                        line: (index + 1) as u32,
                        text: line.trim_end().to_string(),
                    });
                    if matches.len() >= MAX_MATCHES {
                        truncated = true;
                        break;
                    }
                }
            }
            if truncated {
                break;
            }
        }
        if truncated {
            break;
        }
    }
    Ok(SearchOutput { matches, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::tests::temp_root;

    #[test]
    fn finds_literal_match() {
        let root = temp_root();
        std::fs::write(root.join("a.txt"), "hello darb\nsecond\n").expect("write");
        std::fs::write(root.join("b.txt"), "nothing\n").expect("write");
        let out = search(&root, "darb").expect("search");
        assert_eq!(out.matches.len(), 1);
        assert_eq!(out.matches[0].file, "a.txt");
        assert_eq!(out.matches[0].line, 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_query_is_rejected() {
        let root = temp_root();
        assert!(search(&root, "").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_match_returns_empty() {
        let root = temp_root();
        std::fs::write(root.join("a.txt"), "hello\n").expect("write");
        let out = search(&root, "zzz-no-such-string").expect("search");
        assert!(out.matches.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn fallback_scanner_skips_excluded_dirs() {
        let root = temp_root();
        std::fs::create_dir_all(root.join("target")).expect("mkdir");
        std::fs::write(root.join("target").join("x.txt"), "needle\n").expect("write");
        std::fs::write(root.join("ok.txt"), "needle\n").expect("write");
        // Exercise the fallback directly: exclusions are its contract.
        let out = search_fallback(&root, "needle").expect("search");
        assert_eq!(out.matches.len(), 1);
        assert_eq!(out.matches[0].file, "ok.txt");
        let _ = std::fs::remove_dir_all(&root);
    }
}
