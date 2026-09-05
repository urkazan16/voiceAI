use crate::error::LfResult;
use crate::history::HistoryItem;
use crate::paths::DataPaths;
use rusqlite::{params, Connection};

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(paths: &DataPaths) -> LfResult<Self> {
        paths.ensure()?;
        let conn = Connection::open(paths.database_file())?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS history (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                mode TEXT NOT NULL,
                transcript TEXT NOT NULL,
                output TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS kv (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            ",
        )?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> LfResult<()> {
        for (col, ty) in [
            ("application", "TEXT NOT NULL DEFAULT ''"),
            ("profile", "TEXT NOT NULL DEFAULT ''"),
            ("model", "TEXT NOT NULL DEFAULT ''"),
            ("processing_time_ms", "INTEGER NOT NULL DEFAULT 0"),
        ] {
            let sql = format!("ALTER TABLE history ADD COLUMN {col} {ty}");
            let _ = self.conn.execute(&sql, []);
        }
        Ok(())
    }

    pub fn insert_history(&self, item: &HistoryItem) -> LfResult<()> {
        self.conn.execute(
            "INSERT INTO history (id, created_at, mode, transcript, output, application, profile, model, processing_time_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                item.id,
                item.created_at,
                item.mode,
                item.transcript,
                item.output,
                item.application,
                item.profile,
                item.model,
                item.processing_time_ms as i64
            ],
        )?;
        Ok(())
    }

    pub fn update_history_output(&self, id: &str, output: &str) -> LfResult<()> {
        self.conn.execute(
            "UPDATE history SET output=?1 WHERE id=?2",
            params![output, id],
        )?;
        Ok(())
    }

    pub fn delete_history_item(&self, id: &str) -> LfResult<()> {
        self.conn
            .execute("DELETE FROM history WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn list_history(&self) -> LfResult<Vec<HistoryItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, mode, transcript, output,
                    COALESCE(application, ''), COALESCE(profile, ''),
                    COALESCE(model, ''), COALESCE(processing_time_ms, 0)
             FROM history ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(HistoryItem {
                id: row.get(0)?,
                created_at: row.get(1)?,
                mode: row.get(2)?,
                transcript: row.get(3)?,
                output: row.get(4)?,
                application: row.get(5)?,
                profile: row.get(6)?,
                model: row.get(7)?,
                processing_time_ms: row.get::<_, i64>(8)? as u64,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn delete_history(&self) -> LfResult<()> {
        self.conn.execute("DELETE FROM history", [])?;
        Ok(())
    }

    pub fn put_kv(&self, key: &str, value: &str) -> LfResult<()> {
        self.conn.execute(
            "INSERT INTO kv(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_kv(&self, key: &str) -> LfResult<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM kv WHERE key=?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn delete_history_removes_rows() {
        let dir = tempdir().unwrap();
        let paths = DataPaths::from_override(dir.path().to_path_buf());
        let store = Store::open(&paths).unwrap();
        store
            .insert_history(&HistoryItem {
                id: "1".into(),
                created_at: "2026-01-01T00:00:00Z".into(),
                mode: "normal".into(),
                transcript: "hello".into(),
                output: "Hello.".into(),
                application: "Mail".into(),
                profile: "email".into(),
                model: "whisper-small".into(),
                processing_time_ms: 12,
            })
            .unwrap();
        assert_eq!(store.list_history().unwrap().len(), 1);
        store.delete_history().unwrap();
        assert!(store.list_history().unwrap().is_empty());
    }
}
