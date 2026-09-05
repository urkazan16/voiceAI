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
            ("timecodes", "TEXT NOT NULL DEFAULT ''"),
        ] {
            let sql = format!("ALTER TABLE history ADD COLUMN {col} {ty}");
            let _ = self.conn.execute(&sql, []);
        }
        Ok(())
    }

    pub fn insert_history(&self, item: &HistoryItem) -> LfResult<()> {
        self.conn.execute(
            "INSERT INTO history (id, created_at, mode, transcript, output, application, profile, model, processing_time_ms, timecodes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                item.id,
                item.created_at,
                item.mode,
                item.transcript,
                item.output,
                item.application,
                item.profile,
                item.model,
                item.processing_time_ms as i64,
                item.timecodes
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
        self.query_history(None, None, None)
    }

    pub fn query_history(
        &self,
        query: Option<&str>,
        application: Option<&str>,
        since: Option<&str>,
    ) -> LfResult<Vec<HistoryItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, mode, transcript, output,
                    COALESCE(application, ''), COALESCE(profile, ''),
                    COALESCE(model, ''), COALESCE(processing_time_ms, 0),
                    COALESCE(timecodes, '')
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
                timecodes: row.get(9)?,
            })
        })?;
        let q = query.map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
        let app = application
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());
        let mut out = Vec::new();
        for row in rows {
            let item = row?;
            if let Some(since) = since {
                if item.created_at.as_str() < since {
                    continue;
                }
            }
            if let Some(app) = &app {
                if item.application.to_lowercase() != *app {
                    continue;
                }
            }
            if let Some(q) = &q {
                let hay = format!(
                    "{} {} {}",
                    item.transcript.to_lowercase(),
                    item.output.to_lowercase(),
                    item.application.to_lowercase()
                );
                if !hay.contains(q) {
                    continue;
                }
            }
            out.push(item);
        }
        Ok(out)
    }

    pub fn prune_history(&self, max_items: u32) -> LfResult<usize> {
        let keep = max_items.max(1) as i64;
        let deleted = self.conn.execute(
            "DELETE FROM history WHERE id NOT IN (
                SELECT id FROM history ORDER BY created_at DESC LIMIT ?1
             )",
            params![keep],
        )?;
        Ok(deleted)
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
                timecodes: "1\n00:00:00,000 --> 00:00:01,000\nHello.\n".into(),
            })
            .unwrap();
        assert_eq!(store.list_history().unwrap().len(), 1);
        store.delete_history().unwrap();
        assert!(store.list_history().unwrap().is_empty());
    }

    fn item(id: &str, at: &str, app: &str, text: &str) -> HistoryItem {
        HistoryItem {
            id: id.into(),
            created_at: at.into(),
            mode: "normal".into(),
            transcript: text.into(),
            output: text.into(),
            application: app.into(),
            profile: "".into(),
            model: "".into(),
            processing_time_ms: 1,
            timecodes: "".into(),
        }
    }

    #[test]
    fn query_filters_search_app_and_date() {
        let dir = tempdir().unwrap();
        let store = Store::open(&DataPaths::from_override(dir.path().to_path_buf())).unwrap();
        store
            .insert_history(&item("1", "2026-01-01T00:00:00Z", "Mail", "invoice paid"))
            .unwrap();
        store
            .insert_history(&item("2", "2026-08-01T00:00:00Z", "Safari", "search rust"))
            .unwrap();
        assert_eq!(store.query_history(Some("invoice"), None, None).unwrap().len(), 1);
        assert_eq!(
            store
                .query_history(None, Some("Safari"), None)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store
                .query_history(None, None, Some("2026-07-01T00:00:00Z"))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn prune_keeps_newest() {
        let dir = tempdir().unwrap();
        let store = Store::open(&DataPaths::from_override(dir.path().to_path_buf())).unwrap();
        store
            .insert_history(&item("1", "2026-01-01T00:00:00Z", "Mail", "old"))
            .unwrap();
        store
            .insert_history(&item("2", "2026-08-01T00:00:00Z", "Mail", "new"))
            .unwrap();
        assert_eq!(store.prune_history(1).unwrap(), 1);
        let rows = store.list_history().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "2");
    }
}
