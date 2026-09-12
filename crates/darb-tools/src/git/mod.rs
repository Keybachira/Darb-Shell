//! Git integration through the Git CLI (Stacks §13).
//!
//! Read-only to start: `status` and `diff`. A native git binding may
//! come later; the CLI keeps this crate light and always compatible
//! with the user's git version.

use std::path::Path;
use std::process::Command;

use darb_core::errors::{DarbError, Result};

/// Porcelain status with branch info (`git status --porcelain=v1 --branch`).
pub fn status(root: &Path) -> Result<String> {
    run_git(root, &["status", "--porcelain=v1", "--branch"])
}

/// Unstaged diff, or scoped to `target` when non-empty
/// (`git diff -- <target>`). Empty output means "no changes".
pub fn diff(root: &Path, target: &str) -> Result<String> {
    if target.is_empty() {
        run_git(root, &["diff"])
    } else {
        run_git(root, &["diff", "--", target])
    }
}

fn run_git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| DarbError::Git(format!("could not run git: {e}")))?;
    if !output.status.success() {
        return Err(DarbError::Git(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::tests::temp_root;

    #[test]
    fn status_outside_repo_fails() {
        if !git_available() {
            return;
        }
        let root = temp_root();
        let err = status(&root).unwrap_err();
        assert!(matches!(err, DarbError::Git(_)), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn status_inside_repo_succeeds() {
        if !git_available() {
            return;
        }
        // The crate itself lives inside a git checkout when developed normally.
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let grandparent = here.join("..").join("..");
        if !grandparent.join(".git").exists() {
            return;
        }
        let out = status(&grandparent).expect("status must work in repo");
        assert!(out.contains("## "), "{out}");
    }
}
