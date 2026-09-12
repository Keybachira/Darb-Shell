//! Filesystem reads, confined to the project root.
//!
//! Why sync `std::fs`: these are fast local ops; the async boundary lives
//! in the agent executor, not in each tool (Tokio only where needed).
//! Every path is resolved against the registry root first: absolute paths
//! and `..` escapes are refused before touching the disk.

use std::path::{Component, Path, PathBuf};

use darb_core::errors::{DarbError, Result};

/// Files larger than this are refused (low-memory rule: never load a
/// whole huge file into RAM just because the agent asked).
pub const MAX_READ_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
}

/// Join a project-relative `target` onto `root`, refusing absolute paths
/// and lexical `..` escapes. Checked lexically so it also protects
/// not-yet-existing paths.
pub fn resolve_within(root: &Path, target: &str) -> Result<PathBuf> {
    let target_path = Path::new(target);
    if target_path.is_absolute() {
        return Err(DarbError::Tool(format!(
            "refused '{target}': path must be relative to the project root"
        )));
    }
    let mut joined = root.to_path_buf();
    for component in target_path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                return Err(DarbError::Tool(format!(
                    "refused '{target}': path must be relative to the project root"
                )));
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !joined.pop() {
                    return Err(DarbError::Tool(format!(
                        "refused '{target}': path escapes the project root"
                    )));
                }
                if joined.strip_prefix(root).is_err() {
                    return Err(DarbError::Tool(format!(
                        "refused '{target}': path escapes the project root"
                    )));
                }
            }
            Component::Normal(part) => joined.push(part),
        }
    }
    Ok(joined)
}

/// Read a whole file as UTF-8 (lossy). Fails on directories, missing
/// files, and files above [`MAX_READ_BYTES`].
pub fn read_file(root: &Path, target: &str) -> Result<String> {
    let path = resolve_within(root, target)?;
    let metadata = std::fs::metadata(&path)
        .map_err(|e| DarbError::Tool(format!("could not stat '{}': {e}", path.display())))?;
    if metadata.is_dir() {
        return Err(DarbError::Tool(format!(
            "could not read '{}': is a directory",
            path.display()
        )));
    }
    if metadata.len() > MAX_READ_BYTES {
        return Err(DarbError::Tool(format!(
            "could not read '{}': {} bytes exceeds the {}-byte limit",
            path.display(),
            metadata.len(),
            MAX_READ_BYTES
        )));
    }
    let bytes = std::fs::read(&path)
        .map_err(|e| DarbError::Tool(format!("could not read '{}': {e}", path.display())))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// List a directory (default: the root). Entries are sorted: dirs first,
/// then files, each alphabetically.
pub fn list_directory(root: &Path, target: &str) -> Result<Vec<DirEntry>> {
    let path = if target.is_empty() {
        root.to_path_buf()
    } else {
        resolve_within(root, target)?
    };
    let read_dir = std::fs::read_dir(&path)
        .map_err(|e| DarbError::Tool(format!("could not list '{}': {e}", path.display())))?;
    let mut entries = Vec::new();
    for entry in read_dir {
        let entry = entry
            .map_err(|e| DarbError::Tool(format!("could not list '{}': {e}", path.display())))?;
        let file_type = entry.file_type().map_err(|e| {
            DarbError::Tool(format!("could not stat '{}': {e}", entry.path().display()))
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        entries.push(DirEntry {
            name,
            is_dir: file_type.is_dir(),
        });
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Fresh temp project root per test (no new deps: plain std).
    pub fn temp_root() -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root =
            std::env::temp_dir().join(format!("darb-tools-test-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("must create temp root");
        root
    }

    #[test]
    fn reads_relative_file() {
        let root = temp_root();
        std::fs::write(root.join("hello.txt"), "hi\n").expect("write");
        assert_eq!(read_file(&root, "hello.txt").expect("read"), "hi\n");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn refuses_escape_and_absolute_paths() {
        let root = temp_root();
        assert!(read_file(&root, "../evil.txt").is_err());
        assert!(read_file(&root, "sub/../../evil.txt").is_err());
        #[cfg(windows)]
        assert!(read_file(&root, "C:\\Windows\\x.txt").is_err());
        #[cfg(not(windows))]
        assert!(read_file(&root, "/etc/hostname").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn refuses_directories_and_missing_files() {
        let root = temp_root();
        std::fs::create_dir_all(root.join("sub")).expect("mkdir");
        assert!(read_file(&root, "sub").is_err());
        assert!(read_file(&root, "nope.txt").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn lists_dirs_first_sorted() {
        let root = temp_root();
        std::fs::write(root.join("b.txt"), "").expect("write");
        std::fs::write(root.join("a.txt"), "").expect("write");
        std::fs::create_dir_all(root.join("zdir")).expect("mkdir");
        let entries = list_directory(&root, "").expect("list");
        assert_eq!(entries[0].name, "zdir");
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].name, "a.txt");
        assert_eq!(entries[2].name, "b.txt");
        let _ = std::fs::remove_dir_all(&root);
    }
}
