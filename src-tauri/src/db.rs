use crate::model::{Board, Clip, ClipInsert, ClipKind, Tag};
use crate::util;
use rusqlite::params;
use std::path::Path;
use std::sync::Mutex;

pub struct Db {
    conn: Mutex<rusqlite::Connection>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS clips (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  kind TEXT NOT NULL,
  text TEXT,
  html TEXT,
  image_path TEXT,
  file_paths TEXT,
  byte_size INTEGER NOT NULL DEFAULT 0,
  hash TEXT NOT NULL,
  source_app TEXT,
  note TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  last_used_at INTEGER NOT NULL,
  use_count INTEGER NOT NULL DEFAULT 0,
  board_id INTEGER REFERENCES boards(id) ON DELETE SET NULL
);
CREATE INDEX IF NOT EXISTS idx_clips_created ON clips(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_clips_hash ON clips(hash);
CREATE TABLE IF NOT EXISTS boards (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE,
  position INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS tags (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE
);
CREATE TABLE IF NOT EXISTS clip_tags (
  clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
  tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (clip_id, tag_id)
);
CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
"#;

impl Db {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.pragma_update(None, "foreign_keys", true).ok();
        conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn ensure_defaults(&self) {
        for (k, v) in [
            ("hotkey", "CmdOrCtrl+Shift+V"),
            ("max_items", "0"),
            ("retention_days", "0"),
            ("theme", "system"),
            ("auto_paste", "true"),
            ("show_dock_icon", "false"),
            ("show_tray_icon", "true"),
            ("tray_left_action", "panel"),
            ("paste_plain_always", "false"),
            ("plain_modifier", "shift"),
            ("sound_enabled", "true"),
            ("excluded_apps", "[]"),
        ] {
            let conn = self.conn.lock().unwrap();
            let _ = conn.execute(
                "INSERT OR IGNORE INTO settings(key, value) VALUES (?1, ?2)",
                params![k, v],
            );
        }
    }

    // ---------- settings ----------

    pub fn get_setting(&self, key: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| {
            r.get(0)
        })
        .ok()
    }

    pub fn set_setting(&self, key: &str, value: &str) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        );
    }

    pub fn all_settings(&self) -> std::collections::HashMap<String, String> {
        let conn = self.conn.lock().unwrap();
        let Ok(mut stmt) = conn.prepare("SELECT key, value FROM settings") else {
            return Default::default();
        };
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn get_excluded_apps(&self) -> Vec<String> {
        self.get_setting("excluded_apps")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn add_excluded_app(&self, app: &str) {
        let mut v = self.get_excluded_apps();
        if !v.iter().any(|x| x.eq_ignore_ascii_case(app)) {
            v.push(app.to_string());
            self.set_setting("excluded_apps", &serde_json::to_string(&v).unwrap());
        }
    }

    pub fn remove_excluded_app(&self, app: &str) {
        let mut v = self.get_excluded_apps();
        v.retain(|x| !x.eq_ignore_ascii_case(app));
        self.set_setting("excluded_apps", &serde_json::to_string(&v).unwrap());
    }

    // ---------- clips ----------

    pub fn insert_or_bump(&self, c: &ClipInsert) -> i64 {
        let conn = self.conn.lock().unwrap();
        let now = now_ms();
        if let Ok(id) = conn.query_row(
            "SELECT id FROM clips WHERE hash = ?1 ORDER BY created_at DESC LIMIT 1",
            params![c.hash],
            |r| r.get::<_, i64>(0),
        ) {
            let _ = conn.execute(
                "UPDATE clips SET created_at = ?2, last_used_at = ?2 WHERE id = ?1",
                params![id, now],
            );
            if c.html.is_some() {
                let _ = conn.execute(
                    "UPDATE clips SET html = ?2 WHERE id = ?1 AND kind IN ('text','rich_text')",
                    params![id, c.html],
                );
            }
            return id;
        }
        let file_paths = c
            .file_paths
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap());
        let _ = conn.execute(
            "INSERT INTO clips (kind, text, html, image_path, file_paths, byte_size, hash, source_app, created_at, last_used_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![
                c.kind.as_str(),
                c.text,
                c.html,
                c.image_path,
                file_paths,
                c.byte_size,
                c.hash,
                c.source_app,
                now
            ],
        );
        conn.last_insert_rowid()
    }

    pub fn enforce_max(&self, max: i64) {
        if max <= 0 {
            return;
        }
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "DELETE FROM clips WHERE board_id IS NULL AND id NOT IN (
                SELECT id FROM clips WHERE board_id IS NULL ORDER BY created_at DESC LIMIT ?1
            )",
            params![max],
        );
    }

    /// 按时间清理历史(retention_days: 0=无限,30/90/365=保留月数折算的天数);
    /// 看板收藏(board_id 非空)不受影响
    pub fn enforce_retention(&self, days: i64) {
        if days <= 0 {
            return;
        }
        let cutoff = now_ms() - days.saturating_mul(86_400_000);
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "DELETE FROM clips WHERE board_id IS NULL AND created_at < ?1",
            params![cutoff],
        );
    }

    pub fn list_clips(
        &self,
        query: Option<&str>,
        kind: Option<ClipKind>,
        board_id: Option<i64>,
        tag: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Clip>, String> {
        let conn = self.conn.lock().unwrap();
        let mut sql = String::from("SELECT * FROM clips c WHERE 1=1");
        match board_id {
            Some(-1) => {} // 全部
            Some(id) => sql.push_str(&format!(" AND c.board_id = {id}")),
            None => sql.push_str(" AND c.board_id IS NULL"),
        }
        if let Some(k) = kind {
            sql.push_str(&format!(" AND c.kind = '{}'", k.as_str()));
        }

        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(q) = query.map(str::trim).filter(|q| !q.is_empty()) {
            let esc = q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            let like = format!("%{esc}%");
            sql.push_str(
                " AND (c.text LIKE ? ESCAPE '\\' OR c.note LIKE ? ESCAPE '\\'
                      OR EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON t.id = ct.tag_id
                                 WHERE ct.clip_id = c.id AND t.name LIKE ? ESCAPE '\\'))",
            );
            binds.push(Box::new(like.clone()));
            binds.push(Box::new(like.clone()));
            binds.push(Box::new(like));
        }
        if let Some(t) = tag.map(str::trim).filter(|t| !t.is_empty()) {
            sql.push_str(
                " AND EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON t.id = ct.tag_id
                              WHERE ct.clip_id = c.id AND t.name = ?)",
            );
            binds.push(Box::new(t.to_string()));
        }
        sql.push_str(&format!(
            " ORDER BY c.created_at DESC LIMIT {limit} OFFSET {offset}"
        ));

        let binds_ref: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(binds_ref.as_slice(), Self::row_to_clip)
            .map_err(|e| e.to_string())?;
        let mut clips = Vec::new();
        for r in rows {
            let mut c = r.map_err(|e| e.to_string())?;
            c.tags = Self::tags_for_clip(&conn, c.id);
            clips.push(c);
        }
        Ok(clips)
    }

    pub fn get_clip(&self, id: i64) -> Option<Clip> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT * FROM clips WHERE id = ?1").ok()?;
        stmt.query_row(params![id], Self::row_to_clip)
            .ok()
            .map(|mut c| {
                c.tags = Self::tags_for_clip(&conn, c.id);
                c
            })
    }

    fn row_to_clip(row: &rusqlite::Row) -> rusqlite::Result<Clip> {
        let kind_s: String = row.get("kind")?;
        let file_paths: Option<String> = row.get("file_paths")?;
        Ok(Clip {
            id: row.get("id")?,
            kind: ClipKind::parse(&kind_s).unwrap_or(ClipKind::Text),
            text: row.get("text")?,
            html: row.get("html")?,
            image_path: row.get("image_path")?,
            file_paths: file_paths.and_then(|s| serde_json::from_str(&s).ok()),
            source_app: row.get("source_app")?,
            note: row.get("note")?,
            created_at: row.get("created_at")?,
            last_used_at: row.get("last_used_at")?,
            use_count: row.get("use_count")?,
            board_id: row.get("board_id")?,
            tags: Vec::new(),
        })
    }

    fn tags_for_clip(conn: &rusqlite::Connection, clip_id: i64) -> Vec<Tag> {
        let Ok(mut stmt) = conn.prepare(
            "SELECT t.id, t.name FROM tags t
             JOIN clip_tags ct ON ct.tag_id = t.id
             WHERE ct.clip_id = ?1 ORDER BY t.name",
        ) else {
            return Vec::new();
        };
        stmt.query_map(params![clip_id], |r| {
            Ok(Tag { id: r.get(0)?, name: r.get(1)?, count: 0 })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn bump_usage(&self, id: i64) {
        let conn = self.conn.lock().unwrap();
        let now = now_ms();
        let _ = conn.execute(
            "UPDATE clips SET created_at = ?2, last_used_at = ?2, use_count = use_count + 1 WHERE id = ?1",
            params![id, now],
        );
    }

    pub fn delete_clip(&self, id: i64) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("DELETE FROM clips WHERE id = ?1", params![id]);
    }

    pub fn clear_history(&self) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("DELETE FROM clips WHERE board_id IS NULL", []);
    }

    /// 编辑文本内容:编辑后视为纯文本并重算指纹
    pub fn edit_clip(&self, id: i64, text: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE clips SET text = ?2, html = NULL, kind = 'text', byte_size = ?3,
             hash = ?4, last_used_at = ?5
             WHERE id = ?1 AND kind IN ('text','rich_text')",
            params![id, text, text.len() as i64, util::hash_text(text), now_ms()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_note(&self, id: i64, note: &str) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("UPDATE clips SET note = ?2 WHERE id = ?1", params![id, note]);
    }

    pub fn move_clip_to_board(&self, id: i64, board_id: Option<i64>) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        if let Some(bid) = board_id {
            let exists: Option<i64> = conn
                .query_row("SELECT id FROM boards WHERE id = ?1", params![bid], |r| r.get(0))
                .ok();
            if exists.is_none() {
                return Err("看板不存在".into());
            }
        }
        conn.execute(
            "UPDATE clips SET board_id = ?2 WHERE id = ?1",
            params![id, board_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ---------- boards ----------

    pub fn get_boards(&self) -> Vec<Board> {
        let conn = self.conn.lock().unwrap();
        let Ok(mut stmt) =
            conn.prepare("SELECT id, name, position FROM boards ORDER BY position, id")
        else {
            return Vec::new();
        };
        stmt.query_map([], |r| {
            Ok(Board { id: r.get(0)?, name: r.get(1)?, position: r.get(2)? })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn create_board(&self, name: &str) -> Result<Board, String> {
        let conn = self.conn.lock().unwrap();
        let pos: i64 = conn
            .query_row("SELECT COALESCE(MAX(position), 0) + 1 FROM boards", [], |r| r.get(0))
            .unwrap_or(1);
        conn.execute(
            "INSERT INTO boards(name, position, created_at) VALUES (?1, ?2, ?3)",
            params![name, pos, now_ms()],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                "看板名称已存在".to_string()
            } else {
                e.to_string()
            }
        })?;
        Ok(Board { id: conn.last_insert_rowid(), name: name.to_string(), position: pos })
    }

    pub fn rename_board(&self, id: i64, name: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE boards SET name = ?2 WHERE id = ?1", params![id, name])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_board(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM boards WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ---------- tags ----------

    pub fn get_tags(&self) -> Vec<Tag> {
        let conn = self.conn.lock().unwrap();
        let Ok(mut stmt) = conn.prepare(
            "SELECT t.id, t.name, COUNT(ct.clip_id) FROM tags t
             LEFT JOIN clip_tags ct ON ct.tag_id = t.id
             GROUP BY t.id ORDER BY t.name",
        ) else {
            return Vec::new();
        };
        stmt.query_map([], |r| {
            Ok(Tag { id: r.get(0)?, name: r.get(1)?, count: r.get(2)? })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn add_tag(&self, clip_id: i64, name: &str) -> Result<Tag, String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", params![name])
            .map_err(|e| e.to_string())?;
        let tag_id: i64 = conn
            .query_row("SELECT id FROM tags WHERE name = ?1", params![name], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR IGNORE INTO clip_tags(clip_id, tag_id) VALUES (?1, ?2)",
            params![clip_id, tag_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(Tag { id: tag_id, name: name.to_string(), count: 0 })
    }

    pub fn remove_tag(&self, clip_id: i64, tag_id: i64) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "DELETE FROM clip_tags WHERE clip_id = ?1 AND tag_id = ?2",
            params![clip_id, tag_id],
        );
    }

    pub fn get_source_apps(&self) -> Vec<String> {
        let conn = self.conn.lock().unwrap();
        let Ok(mut stmt) = conn.prepare(
            "SELECT source_app, COUNT(*) AS c FROM clips
             WHERE source_app IS NOT NULL AND source_app <> ''
             GROUP BY source_app ORDER BY c DESC LIMIT 50",
        ) else {
            return Vec::new();
        };
        stmt.query_map([], |r| r.get(0))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }
}
