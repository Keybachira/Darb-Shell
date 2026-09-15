//! Exact-match file editing.
//!
//! Why exactly-one-match: replacing the wrong occurrence silently is how
//! agents corrupt code. Zero matches means stale context; several matches
//! means ambiguous context — both are errors the agent must fix by
//! re-reading, not guesses the tool makes for it (Negócio §20: understand
//! before modifying).

use std::path::Path;

use darb_core::errors::{DarbError, Result};

use crate::filesystem::{resolve_within, MAX_READ_BYTES};

/// Replace `old` with `new` in the file at `target` (project-relative).
/// Returns the number of bytes written.
pub fn edit_file(root: &Path, target: &str, old: &str, new: &str) -> Result<u64> {
    if old.is_empty() {
        return Err(DarbError::Tool(
            "edit requires non-empty 'old' text: read the file first, then match exactly"
                .to_string(),
        ));
    }
    let path = resolve_within(root, target)?;
    let metadata = std::fs::metadata(&path)
        .map_err(|e| DarbError::Tool(format!("could not stat '{}': {e}", path.display())))?;
    if metadata.is_dir() {
        return Err(DarbError::Tool(format!(
            "could not edit '{}': is a directory",
            path.display()
        )));
    }
    if metadata.len() > MAX_READ_BYTES {
        return Err(DarbError::Tool(format!(
            "could not edit '{}': {} bytes exceeds the {}-byte limit",
            path.display(),
            metadata.len(),
            MAX_READ_BYTES
        )));
    }
    let bytes = std::fs::read(&path)
        .map_err(|e| DarbError::Tool(format!("could not read '{}': {e}", path.display())))?;
    let content = String::from_utf8_lossy(&bytes).into_owned();

    let occurrences = content.matches(old).count();
    if occurrences == 0 {
        return Err(DarbError::Tool(format!(
            "could not edit '{}': 'old' text not found; re-read the file, it may have changed",
            path.display()
        )));
    }
    if occurrences > 1 {
        return Err(DarbError::Tool(format!(
            "could not edit '{}': 'old' text matches {occurrences} times; include more context to disambiguate",
            path.display()
        )));
    }

    let updated = content.replacen(old, new, 1);
    if updated.len() as u64 > MAX_READ_BYTES {
        return Err(DarbError::Tool(format!(
            "could not edit '{}': result would exceed the {MAX_READ_BYTES}-byte limit",
            path.display()
        )));
    }
    std::fs::write(&path, updated.as_bytes())
        .map_err(|e| DarbError::Tool(format!("could not write '{}': {e}", path.display())))?;
    Ok(updated.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::tests::temp_root;
    use std::path::PathBuf;

    fn fixture(content: &str) -> (PathBuf, String) {
        let root = temp_root();
        std::fs::write(root.join("code.rs"), content).expect("write");
        (root, "code.rs".to_string())
    }

    #[test]
    fn replaces_unique_match() {
        let (root, target) = fixture("fn main() {\n    todo!()\n}\n");
        let bytes = edit_file(&root, &target, "todo!()", "println!(\"hi\")").expect("edit");
        assert!(bytes > 0);
        let after = std::fs::read_to_string(root.join(&target)).expect("read");
        assert!(after.contains("println!(\"hi\")"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn whole_file_replace_powers_editor_saves() {
        // The TUI saves by replacing the open snapshot (old) with the
        // buffer (new): the whole content matches exactly once, so the
        // exact-match rule doubles as a stale-context guard.
        let (root, target) = fixture("ab\ncd\n");
        let bytes = edit_file(&root, &target, "ab\ncd\n", "Xab\ncd\n").expect("edit");
        assert!(bytes > 0);
        assert_eq!(
            std::fs::read_to_string(root.join(&target)).expect("read"),
            "Xab\ncd\n"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn stale_snapshot_fails_instead_of_overwriting() {
        let (root, target) = fixture("ab\ncd\n");
        // Someone else changed the file after the snapshot was taken.
        std::fs::write(root.join(&target), "ab\nCHANGED\n").expect("write");
        assert!(edit_file(&root, &target, "ab\ncd\n", "Xab\ncd\n").is_err());
        assert_eq!(
            std::fs::read_to_string(root.join(&target)).expect("read"),
            "ab\nCHANGED\n"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_old_text_fails() {
        let (root, target) = fixture("hello\n");
        assert!(edit_file(&root, &target, "bye", "x").is_err());
        // File untouched.
        assert_eq!(
            std::fs::read_to_string(root.join(&target)).expect("read"),
            "hello\n"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ambiguous_old_text_fails() {
        let (root, target) = fixture("a = 1\na = 1\n");
        let err = edit_file(&root, &target, "a = 1", "a = 2").unwrap_err();
        assert!(err.to_string().contains("2 times"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_old_and_escape_fail() {
        let (root, target) = fixture("hello\n");
        assert!(edit_file(&root, &target, "", "x").is_err());
        assert!(edit_file(&root, "../evil.txt", "a", "b").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
