//! History reads + project memory (§29 Arquitetura).
//!
//! Two halves: transcript queries (recent messages/decisions per session,
//! newest last, capped by `limit`) and the project key/value store
//! (stack, conventions, preferences — upsert by key).

use darb_core::errors::Result;

use crate::storage::{check_text, map_err, now_unix, MemoryDb};

#[derive(Debug, Clone)]
pub struct Message {
    pub id: i64,
    pub role: String,
    pub text: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct Decision {
    pub id: i64,
    pub summary: String,
    pub outcome: String,
    pub created_at: i64,
}

impl MemoryDb {
    /// Last `limit` messages of a session, oldest first.
    pub fn recent_messages(&self, session_id: i64, limit: u32) -> Result<Vec<Message>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT id, role, text, created_at FROM messages WHERE session_id = ?1 ORDER BY id DESC LIMIT ?2")
            .map_err(map_err)?;
        let rows = stmt
            .query_map(rusqlite::params![session_id, limit], |row| {
                Ok(Message {
                    id: row.get(0)?,
                    role: row.get(1)?,
                    text: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(map_err)?;
        let mut messages: Vec<Message> = rows
            .collect::<std::result::Result<Vec<_>, rusqlite::Error>>()
            .map_err(map_err)?;
        messages.reverse();
        Ok(messages)
    }

    /// Last `limit` decisions of a session, oldest first.
    pub fn recent_decisions(&self, session_id: i64, limit: u32) -> Result<Vec<Decision>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT id, summary, outcome, created_at FROM decisions WHERE session_id = ?1 ORDER BY id DESC LIMIT ?2")
            .map_err(map_err)?;
        let rows = stmt
            .query_map(rusqlite::params![session_id, limit], |row| {
                Ok(Decision {
                    id: row.get(0)?,
                    summary: row.get(1)?,
                    outcome: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(map_err)?;
        let mut decisions: Vec<Decision> = rows
            .collect::<std::result::Result<Vec<_>, rusqlite::Error>>()
            .map_err(map_err)?;
        decisions.reverse();
        Ok(decisions)
    }

    /// Store a project fact (`stack`, `conventions/python`, …). Overwrites.
    pub fn set_memory(&self, key: &str, value: &str) -> Result<()> {
        if key.trim().is_empty() {
            return Err(darb_core::errors::DarbError::Storage(
                "memory key must not be empty".to_string(),
            ));
        }
        check_text(value, "memory value")?;
        self.conn().execute(
            "INSERT INTO memories (key, value, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![key, value, now_unix()],
        ).map_err(map_err)?;
        Ok(())
    }

    /// Read a project fact. `None` means "never stored", not an error.
    pub fn get_memory(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT value FROM memories WHERE key = ?1")
            .map_err(map_err)?;
        match stmt.query_row(rusqlite::params![key], |row| row.get::<_, String>(0)) {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::tests::temp_db;

    #[test]
    fn messages_come_back_oldest_first_capped() {
        let db = temp_db();
        let session = db.create_session("/proj", "eco").expect("create");
        for text in ["one", "two", "three"] {
            db.record_message(session.id, "user", text).expect("record");
        }
        let recent = db.recent_messages(session.id, 2).expect("read");
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].text, "two");
        assert_eq!(recent[1].text, "three");
        assert_eq!(recent[0].role, "user");
    }

    #[test]
    fn decisions_and_memories() {
        let db = temp_db();
        let session = db.create_session("/proj", "eco").expect("create");
        db.record_decision(session.id, "did it", "completed")
            .expect("record");
        let decisions = db.recent_decisions(session.id, 10).expect("read");
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].outcome, "completed");

        assert_eq!(db.get_memory("stack").expect("read"), None);
        db.set_memory("stack", "rust + ratatui").expect("write");
        assert_eq!(
            db.get_memory("stack").expect("read"),
            Some("rust + ratatui".to_string())
        );
        db.set_memory("stack", "rust").expect("overwrite");
        assert_eq!(
            db.get_memory("stack").expect("read"),
            Some("rust".to_string())
        );
        assert!(db.set_memory("  ", "x").is_err());
    }
}
