//! Lazy project walk: built on demand, never as a daemon.
//!
//! Why on demand (Negócio §18): continuous indexing is banned by default
//! on low-resource machines. The caller builds a fresh [`ProjectMap`]
//! when a task starts; caps on depth/files/bytes keep the walk cheap and
//! `truncated` flags partial results.

use std::path::{Path, PathBuf};

use crate::project_map::{ProjectFile, ProjectMap};

/// Directories never descended into (Agents §17).
pub const EXCLUDED_DIRS: &[&str] = &[
    ".git",
    "target",
    "dist",
    "build",
    ".cache",
    "coverage",
    "node_modules",
    ".darb",
];

/// Files never listed (Negócio §12: secrets stay out of context).
pub const EXCLUDED_FILES: &[&str] = &[".env"];

/// Walk limits: cheap by construction on weak hardware.
pub const MAX_DEPTH: usize = 8;
pub const MAX_FILES: usize = 2000;
pub const MAX_FILE_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone)]
pub struct Indexer {
    pub root: PathBuf,
    pub max_depth: usize,
    pub max_files: usize,
}

impl Indexer {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            max_depth: MAX_DEPTH,
            max_files: MAX_FILES,
        }
    }

    /// Scan now. Returns partial results with `truncated = true` instead
    /// of failing when caps hit — the caller reports the cut.
    pub fn scan(&self) -> ProjectMap {
        let mut files: Vec<ProjectFile> = Vec::new();
        let mut truncated = false;
        let mut stack = vec![(self.root.clone(), 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            if files.len() >= self.max_files {
                truncated = true;
                break;
            }
            if depth > self.max_depth {
                truncated = true;
                continue;
            }
            let entries = match std::fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(_) => continue, // Unreadable dir: skip, don't fail the map.
            };
            let mut paths: Vec<PathBuf> = entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .collect();
            paths.sort();
            for path in paths {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if path.is_dir() {
                    if !EXCLUDED_DIRS.contains(&name.as_str()) {
                        stack.push((path, depth + 1));
                    }
                    continue;
                }
                if name.is_empty()
                    || EXCLUDED_FILES
                        .iter()
                        .any(|base| name == *base || name.starts_with(".env."))
                    || name.ends_with(".pem")
                    || name.ends_with(".key")
                {
                    continue;
                }
                let bytes = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
                if bytes > MAX_FILE_BYTES {
                    continue;
                }
                let rel = path
                    .strip_prefix(&self.root)
                    .map(|rel| rel.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| name.clone());
                files.push(ProjectFile {
                    path: rel,
                    language: detect_language(&name),
                    bytes,
                });
                if files.len() >= self.max_files {
                    truncated = true;
                    break;
                }
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        ProjectMap {
            root: self.root.clone(),
            files,
            truncated,
        }
    }
}

/// Language by file extension. Unknown → `"text"`. Deliberately coarse:
/// the map needs a hint, not a grammar (tree-sitter symbols come later,
/// on demand per selected file).
pub fn detect_language(file_name: &str) -> String {
    let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" => "javascript",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "hpp" => "cpp",
        "toml" | "yaml" | "yml" | "json" => "config",
        "md" => "markdown",
        "sh" => "shell",
        _ => "text",
    }
    .to_string()
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    pub fn temp_project() -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("darb-ctx-test-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        root
    }

    #[test]
    fn skips_excluded_dirs_and_secrets() {
        let root = temp_project();
        std::fs::create_dir_all(root.join("target")).expect("mkdir");
        std::fs::write(root.join("target").join("x.rs"), "fn x() {}").expect("write");
        std::fs::write(root.join(".env"), "KEY=1").expect("write");
        std::fs::write(root.join("main.rs"), "fn main() {}").expect("write");
        let map = Indexer::new(&root).scan();
        assert_eq!(map.files.len(), 1);
        assert_eq!(map.files[0].path, "main.rs");
        assert_eq!(map.files[0].language, "rust");
        assert!(!map.truncated);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn caps_truncate_instead_of_failing() {
        let root = temp_project();
        for index in 0..10 {
            std::fs::write(root.join(format!("f{index}.txt")), "x").expect("write");
        }
        let indexer = Indexer {
            max_files: 3,
            ..Indexer::new(&root)
        };
        let map = indexer.scan();
        assert_eq!(map.files.len(), 3);
        assert!(map.truncated);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn detects_languages() {
        assert_eq!(detect_language("a.rs"), "rust");
        assert_eq!(detect_language("a.TS"), "typescript");
        assert_eq!(detect_language("Makefile"), "text");
    }
}
