//! SQLite storage: schema, connection, and low-level writes.
//!
//! One file per project (`.darb/memory.db`), created lazily on first
//! open. Writes are explicit calls from the session layer — no background
//! writer, no continuous disk activity (Negócio §31: avoid constant disk
//! writes). Timestamps are unix seconds (`INTEGER`): no date dependency.

use std::path::Path;

use darb_core::errors::{DarbError, Result};

/// Current schema version. Bumped only with a migration block below.
pub const SCHEMA_VERSION: i64 = 1;

const SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY);
CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at INTEGER NOT NULL,
    project_root TEXT NOT NULL,
    profile TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL REFERENCES sessions(id),
    role TEXT NOT NULL,
    text TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS tool_calls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL REFERENCES sessions(id),
    tool TEXT NOT NULL,
    target TEXT NOT NULL,
    success INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS decisions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL REFERENCES sessions(id),
    summary TEXT NOT NULL,
    outcome TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS memories (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
CREATE INDEX IF NOT EXISTS idx_tool_calls_session ON tool_calls(session_id);
CREATE INDEX IF NOT EXISTS idx_decisions_session ON decisions(session_id);
";

/// Single texts larger than this are refused: memory is an index of what
/// happened, not a second copy of the filesystem.
pub const MAX_TEXT_CHARS: usize = 256 * 1024;

pub struct MemoryDb {
    conn: rusqlite::Connection,
}

impl MemoryDb {
    /// Open (creating parents) and migrate to [`SCHEMA_VERSION`].
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    DarbError::Storage(format!(
                        "could not create memory dir '{}': {e}",
                        parent.display()
                    ))
                })?;
            }
        }
        let conn = rusqlite::Connection::open(path)
            .map_err(|e| DarbError::Storage(format!("could not open '{}': {e}", path.display())))?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn
            .execute_batch(SCHEMA_V1)
            .map_err(|e| DarbError::Storage(format!("could not create schema: {e}")))?;
        self.conn
            .execute(
                "INSERT OR IGNORE INTO schema_version (version) VALUES (?1)",
                rusqlite::params![SCHEMA_VERSION],
            )
            .map_err(|e| DarbError::Storage(format!("could not stamp schema: {e}")))?;
        Ok(())
    }

    pub(crate) fn conn(&self) -> &rusqlite::Connection {
        &self.conn
    }
}

/// Current unix time in seconds. Falls back to 0 when the clock is
/// unreadable (ordering still works; wall time is best-effort here).
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

/// Guardrail for stored texts (see [`MAX_TEXT_CHARS`]).
pub(crate) fn check_text(text: &str, what: &str) -> Result<()> {
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(DarbError::Storage(format!(
            "{what} exceeds the {MAX_TEXT_CHARS}-char memory limit"
        )));
    }
    Ok(())
}

pub(crate) fn map_err(e: rusqlite::Error) -> DarbError {
    DarbError::Storage(format!("sqlite error: {e}"))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Fresh database file per test (WAL-free default journal: nothing
    /// lingers beside the file itself).
    pub fn temp_db() -> MemoryDb {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("darb-mem-test-{}-{id}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        MemoryDb::open(&path).expect("must open temp db")
    }
}
