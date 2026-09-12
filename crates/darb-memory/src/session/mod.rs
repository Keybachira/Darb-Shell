//! Session memory: one row per TUI run, with its messages, tool calls,
//! and agent decisions attached (§28 Arquitetura).
//!
//! Roles are validated (`user` | `agent` | `system`): anything else is a
//! caller bug, and the database should never learn new roles silently.

use darb_core::errors::{DarbError, Result};

use crate::storage::{check_text, map_err, now_unix, MemoryDb};

/// Roles the transcript understands.
pub const ROLES: &[&str] = &["user", "agent", "system"];

#[derive(Debug, Clone)]
pub struct Session {
    pub id: i64,
    pub started_at: i64,
    pub project_root: String,
    pub profile: String,
}

impl MemoryDb {
    /// Start a session for a project root. Cheap: one INSERT.
    pub fn create_session(&self, project_root: &str, profile: &str) -> Result<Session> {
        let started_at = now_unix();
        self.conn()
            .execute(
                "INSERT INTO sessions (started_at, project_root, profile) VALUES (?1, ?2, ?3)",
                rusqlite::params![started_at, project_root, profile],
            )
            .map_err(map_err)?;
        Ok(Session {
            id: self.conn().last_insert_rowid(),
            started_at,
            project_root: project_root.to_string(),
            profile: profile.to_string(),
        })
    }

    /// Append one transcript message. Empty roles/texts are rejected.
    pub fn record_message(&self, session_id: i64, role: &str, text: &str) -> Result<i64> {
        if !ROLES.contains(&role) {
            return Err(DarbError::Storage(format!(
                "invalid message role '{role}': expected one of {ROLES:?}"
            )));
        }
        if text.trim().is_empty() {
            return Err(DarbError::Storage(
                "cannot record an empty message".to_string(),
            ));
        }
        check_text(text, "message")?;
        self.conn()
            .execute(
                "INSERT INTO messages (session_id, role, text, created_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![session_id, role, text, now_unix()],
            )
            .map_err(map_err)?;
        Ok(self.conn().last_insert_rowid())
    }

    /// Append one tool execution record.
    pub fn record_tool_call(
        &self,
        session_id: i64,
        tool: &str,
        target: &str,
        success: bool,
    ) -> Result<i64> {
        if tool.trim().is_empty() {
            return Err(DarbError::Storage(
                "tool name must not be empty".to_string(),
            ));
        }
        self.conn().execute(
            "INSERT INTO tool_calls (session_id, tool, target, success, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![session_id, tool, target, success as i32, now_unix()],
        ).map_err(map_err)?;
        Ok(self.conn().last_insert_rowid())
    }

    /// Append one agent decision (task outcome).
    pub fn record_decision(&self, session_id: i64, summary: &str, outcome: &str) -> Result<i64> {
        if summary.trim().is_empty() {
            return Err(DarbError::Storage(
                "decision summary must not be empty".to_string(),
            ));
        }
        check_text(summary, "decision summary")?;
        self.conn().execute(
            "INSERT INTO decisions (session_id, summary, outcome, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![session_id, summary, outcome, now_unix()],
        ).map_err(map_err)?;
        Ok(self.conn().last_insert_rowid())
    }

    /// Newest session, if any. The TUI reads this before creating its own
    /// session to show the previous transcript on reopen.
    pub fn latest_session(&self) -> Result<Option<Session>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT id, started_at, project_root, profile FROM sessions ORDER BY id DESC LIMIT 1")
            .map_err(map_err)?;
        match stmt.query_row([], |row| {
            Ok(Session {
                id: row.get(0)?,
                started_at: row.get(1)?,
                project_root: row.get(2)?,
                profile: row.get(3)?,
            })
        }) {
            Ok(session) => Ok(Some(session)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::tests::temp_db;

    #[test]
    fn session_round_trip() {
        let db = temp_db();
        let session = db.create_session("/proj", "eco").expect("create");
        assert!(session.id > 0);
        assert_eq!(session.project_root, "/proj");
        let id = db
            .record_message(session.id, "user", "hi")
            .expect("message");
        assert!(id > 0);
        let tool = db
            .record_tool_call(session.id, "read_file", "a.txt", true)
            .expect("tool");
        assert!(tool > 0);
        let decision = db
            .record_decision(session.id, "fixed login", "completed")
            .expect("decision");
        assert!(decision > 0);
    }

    #[test]
    fn invalid_roles_and_empty_texts_fail() {
        let db = temp_db();
        let session = db.create_session("/proj", "eco").expect("create");
        assert!(db.record_message(session.id, "robot", "hi").is_err());
        assert!(db.record_message(session.id, "user", "   ").is_err());
        assert!(db.record_tool_call(session.id, "", "x", true).is_err());
        assert!(db.record_decision(session.id, "  ", "completed").is_err());
    }

    #[test]
    fn latest_session_tracks_newest() {
        let db = temp_db();
        assert!(db.latest_session().expect("read").is_none());
        let first = db.create_session("/proj", "eco").expect("create");
        let latest = db.latest_session().expect("read").expect("some");
        assert_eq!(latest.id, first.id);
        let second = db.create_session("/proj", "eco").expect("create");
        let latest = db.latest_session().expect("read").expect("some");
        assert_eq!(latest.id, second.id);
    }
}
