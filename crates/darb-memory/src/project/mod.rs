//! Project layout: the `.darb/` directory (§30 Arquitetura).
//!
//! ```text
//! .darb/
//! ├── config.toml   (owned by `darb init`, never touched here)
//! ├── memory.db     (this crate, created lazily)
//! └── sessions/     (reserved for per-session transcripts)
//! ```
//!
//! `ensure_layout` only creates missing directories — it never writes or
//! overwrites files, so opening memory in a fresh project is side-effect
//! free except for the directories themselves.

use std::path::{Path, PathBuf};

use darb_core::errors::{DarbError, Result};

use crate::storage::MemoryDb;

#[derive(Debug, Clone)]
pub struct ProjectPaths {
    pub root: PathBuf,
    pub darb_dir: PathBuf,
    pub config_path: PathBuf,
    pub memory_path: PathBuf,
    pub sessions_dir: PathBuf,
}

impl ProjectPaths {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let darb_dir = root.join(".darb");
        Self {
            config_path: darb_dir.join("config.toml"),
            memory_path: darb_dir.join("memory.db"),
            sessions_dir: darb_dir.join("sessions"),
            darb_dir,
            root,
        }
    }

    /// Create `.darb/` and `.darb/sessions/` when missing.
    pub fn ensure_layout(&self) -> Result<()> {
        for dir in [&self.darb_dir, &self.sessions_dir] {
            std::fs::create_dir_all(dir).map_err(|e| {
                DarbError::Storage(format!("could not create '{}': {e}", dir.display()))
            })?;
        }
        Ok(())
    }

    /// Open the project's database (creating layout + file on demand).
    pub fn open_memory(&self) -> Result<MemoryDb> {
        self.ensure_layout()?;
        MemoryDb::open(&self.memory_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn layout_and_open() {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("darb-proj-test-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let paths = ProjectPaths::new(&root);
        assert!(!paths.darb_dir.exists());
        let db = paths.open_memory().expect("open");
        assert!(paths.memory_path.exists());
        assert!(paths.sessions_dir.exists());
        // Reopen is idempotent (migrations use IF NOT EXISTS).
        let session = db.create_session("/proj", "eco").expect("session");
        assert!(session.id > 0);
        drop(db);
        let _ = std::fs::remove_dir_all(&root);
    }
}
