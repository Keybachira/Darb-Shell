//! Project map: the lightweight structural sketch (§18 Arquitetura).
//!
//! Directories and files with sizes and detected languages — no contents.
//! Rendering produces the tree text that goes into prompts so the model
//! navigates by structure instead of receiving the whole project.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// One file in the map. `path` is project-relative with `/` separators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFile {
    pub path: String,
    pub language: String,
    pub bytes: u64,
}

/// The scanned structure. `truncated` means caps cut the walk short and
/// the map is partial (callers say so in the prompt, never silently).
#[derive(Debug, Clone, Default)]
pub struct ProjectMap {
    pub root: PathBuf,
    pub files: Vec<ProjectFile>,
    pub truncated: bool,
}

#[derive(Default)]
struct TreeNode {
    dirs: BTreeMap<String, TreeNode>,
    files: Vec<String>,
}

impl ProjectMap {
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Render as an indented tree, e.g. the §18 example. Deterministic:
    /// files are stored sorted, so prompts are stable across runs.
    pub fn render(&self) -> String {
        let mut root = TreeNode::default();
        for file in &self.files {
            let mut node = &mut root;
            let mut parts: Vec<&str> = file.path.split('/').collect();
            let name = parts.pop().unwrap_or("").to_string();
            for part in parts {
                node = node.dirs.entry(part.to_string()).or_default();
            }
            node.files.push(name);
        }
        let mut out = String::new();
        render_node(&mut out, &root, 0);
        out
    }
}

fn render_node(out: &mut String, node: &TreeNode, depth: usize) {
    let pad = "  ".repeat(depth);
    for (name, child) in &node.dirs {
        out.push_str(&format!("{pad}{name}/\n"));
        render_node(out, child, depth + 1);
    }
    let mut files = node.files.clone();
    files.sort();
    for name in files {
        out.push_str(&format!("{pad}{name}\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> ProjectMap {
        ProjectMap {
            root: PathBuf::from("/proj"),
            files: vec![
                ProjectFile {
                    path: "src/auth/login.ts".to_string(),
                    language: "typescript".to_string(),
                    bytes: 100,
                },
                ProjectFile {
                    path: "src/auth/session.ts".to_string(),
                    language: "typescript".to_string(),
                    bytes: 200,
                },
                ProjectFile {
                    path: "README.md".to_string(),
                    language: "markdown".to_string(),
                    bytes: 50,
                },
            ],
            truncated: false,
        }
    }

    #[test]
    fn renders_tree() {
        let text = map().render();
        assert!(text.contains("src/\n"), "{text}");
        assert!(text.contains("  auth/\n"), "{text}");
        assert!(text.contains("    login.ts\n"), "{text}");
        assert!(text.contains("README.md\n"), "{text}");
    }
}
