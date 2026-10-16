//! Database and persistence module for the Clipboard Manager.
//!
//! Stores clipboard history (text, rich previews, images) using a bundled SQLite engine.
//! Provides fast deduplication, full-text search, pinning support, and automatic pruning.

use chrono::{DateTime, Local, Utc};
use rusqlite::{params, Connection, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Represents the type of data stored in a clipboard item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentType {
    Text,
    Image,
}

impl ContentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentType::Text => "text",
            ContentType::Image => "image",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "image" => ContentType::Image,
            _ => ContentType::Text,
        }
    }
}

/// A single clipboard history record.
#[derive(Debug, Clone)]
pub struct ClipboardItem {
    pub id: i64,
    pub content_type: ContentType,
    pub text_content: Option<String>,
    pub image_path: Option<String>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
    pub preview_text: String,
    pub is_pinned: bool,
    #[allow(dead_code)]
    pub hash: String,
    pub created_at: i64,
}

impl ClipboardItem {
    /// Formats the creation timestamp into a user-friendly relative or local time string.
    pub fn formatted_time(&self) -> String {
        let dt = DateTime::from_timestamp_millis(self.created_at).unwrap_or_else(Utc::now);
        let local_dt: DateTime<Local> = DateTime::from(dt);
        let now = Local::now();
        let diff = now.signed_duration_since(local_dt);

        if diff.num_seconds() < 60 {
            "Just now".to_string()
        } else if diff.num_minutes() < 60 {
            format!("{}m ago", diff.num_minutes())
        } else if diff.num_hours() < 24 {
            format!("{}h ago", diff.num_hours())
        } else {
            local_dt.format("%b %d, %H:%M").to_string()
        }
    }
}

/// Thread-safe database handle for persistence.
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
    images_dir: PathBuf,
}

impl Database {
    fn get_conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn init_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS clipboard_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content_type TEXT NOT NULL,
                text_content TEXT,
                image_path TEXT,
                image_width INTEGER,
                image_height INTEGER,
                preview_text TEXT NOT NULL,
                is_pinned INTEGER NOT NULL DEFAULT 0,
                hash TEXT NOT NULL UNIQUE,
                created_at INTEGER NOT NULL
            );",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_history_pinned_created
             ON clipboard_history (is_pinned DESC, created_at DESC);",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_history_hash
             ON clipboard_history (hash);",
            [],
        )?;

        Ok(())
    }

    /// Initializes the database in standard XDG local share directory (`~/.local/share/clipv`).
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let local_data = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
        let base_dir = local_data.join("clipv");
        let legacy_dir = local_data.join("clipboard-manager");

        let target_dir = if base_dir.exists() {
            base_dir
        } else if legacy_dir.exists() {
            legacy_dir
        } else {
            base_dir
        };

        fs::create_dir_all(&target_dir)?;
        let images_dir = target_dir.join("images");
        fs::create_dir_all(&images_dir)?;

        let db_path = target_dir.join("history.db");
        let conn = Connection::open(&db_path)?;
        Self::init_schema(&conn)?;

        Ok(Database {
            conn: Arc::new(Mutex::new(conn)),
            images_dir,
        })
    }

    #[cfg(test)]
    pub fn new_in_memory() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        let temp_dir =
            std::env::temp_dir().join(format!("clipboard-manager-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        Ok(Database {
            conn: Arc::new(Mutex::new(conn)),
            images_dir: temp_dir,
        })
    }

    /// Computes SHA256 hex digest for deduplication.
    pub fn compute_hash(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    /// Inserts or bumps a text entry. If it already exists, updates its timestamp and preserves pin status.
    /// Generates a clean preview truncated safely without UTF-8 boundary panics.
    fn generate_text_preview(text: &str) -> String {
        let lines: Vec<&str> = text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();
        let combined = if lines.len() > 3 {
            format!("{}\n{}\n{}...", lines[0], lines[1], lines[2])
        } else {
            text.trim().to_string()
        };

        // Safely truncate to at most 250 unicode characters
        let char_limit = 250;
        let char_count = combined.chars().count();
        if char_count > char_limit {
            let truncated: String = combined.chars().take(char_limit).collect();
            format!("{}...", truncated)
        } else {
            combined
        }
    }

    /// Inserts or bumps a text entry. If it already exists, updates its timestamp and preserves pin status.
    pub fn insert_or_update_text(
        &self,
        text: &str,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        if text.trim().is_empty() {
            return Ok(0);
        }

        let hash = Self::compute_hash(text.as_bytes());
        let now = Utc::now().timestamp_millis();
        let preview = Self::generate_text_preview(text);

        let conn = self.get_conn();

        // Upsert: On conflict with hash, update timestamp to move to top, preserve pin
        conn.execute(
            "INSERT INTO clipboard_history (
                content_type, text_content, image_path, image_width, image_height,
                preview_text, is_pinned, hash, created_at
             ) VALUES (?1, ?2, NULL, NULL, NULL, ?3, 0, ?4, ?5)
             ON CONFLICT(hash) DO UPDATE SET
                created_at = excluded.created_at,
                preview_text = excluded.preview_text;",
            params![ContentType::Text.as_str(), text, preview, hash, now,],
        )?;

        let id = conn.last_insert_rowid();

        // Automatically prune old unpinned entries to keep RAM & DB tiny
        self.prune_unpinned_locked(&conn, 60)?;

        Ok(id)
    }

    /// Inserts or bumps an RGBA image entry. Saves image to disk to keep SQLite lean and fast.
    pub fn insert_or_update_image(
        &self,
        width: u32,
        height: u32,
        rgba_bytes: &[u8],
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let hash = Self::compute_hash(rgba_bytes);
        let now = Utc::now().timestamp_millis();
        let image_file = self.images_dir.join(format!("{}.png", hash));

        // Save image to file if not already saved
        if !image_file.exists() {
            if let Some(img_buf) = image::RgbaImage::from_raw(width, height, rgba_bytes.to_vec()) {
                img_buf.save(&image_file)?;
            }
        }

        let image_path_str = image_file.to_string_lossy().to_string();
        let preview = format!("Image ({} × {})", width, height);

        let conn = self.get_conn();

        conn.execute(
            "INSERT INTO clipboard_history (
                content_type, text_content, image_path, image_width, image_height,
                preview_text, is_pinned, hash, created_at
             ) VALUES (?1, NULL, ?2, ?3, ?4, ?5, 0, ?6, ?7)
             ON CONFLICT(hash) DO UPDATE SET
                created_at = excluded.created_at;",
            params![
                ContentType::Image.as_str(),
                image_path_str,
                width,
                height,
                preview,
                hash,
                now,
            ],
        )?;

        let id = conn.last_insert_rowid();
        self.prune_unpinned_locked(&conn, 60)?;

        Ok(id)
    }

    /// Retrieves history items, filtered by optional search query and ordered by pinned DESC, created_at DESC.
    pub fn get_items(
        &self,
        query: &str,
    ) -> Result<Vec<ClipboardItem>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_conn();
        let query_trimmed = query.trim();

        let mut items = Vec::new();

        if query_trimmed.is_empty() {
            let mut stmt = conn.prepare(
                "SELECT id, content_type, text_content, image_path, image_width, image_height,
                        preview_text, is_pinned, hash, created_at
                 FROM clipboard_history
                 ORDER BY is_pinned DESC, created_at DESC
                 LIMIT 50;",
            )?;

            let rows = stmt.query_map([], |row| {
                let ct_str: String = row.get(1)?;
                Ok(ClipboardItem {
                    id: row.get(0)?,
                    content_type: ContentType::from_str(&ct_str),
                    text_content: row.get(2)?,
                    image_path: row.get(3)?,
                    image_width: row.get(4)?,
                    image_height: row.get(5)?,
                    preview_text: row.get(6)?,
                    is_pinned: row.get::<_, i64>(7)? != 0,
                    hash: row.get(8)?,
                    created_at: row.get(9)?,
                })
            })?;

            for item in rows {
                items.push(item?);
            }
        } else {
            let search_pattern = format!("%{}%", query_trimmed);
            let mut stmt = conn.prepare(
                "SELECT id, content_type, text_content, image_path, image_width, image_height,
                        preview_text, is_pinned, hash, created_at
                 FROM clipboard_history
                 WHERE text_content LIKE ?1 OR preview_text LIKE ?1
                 ORDER BY is_pinned DESC, created_at DESC
                 LIMIT 50;",
            )?;

            let rows = stmt.query_map(params![search_pattern], |row| {
                let ct_str: String = row.get(1)?;
                Ok(ClipboardItem {
                    id: row.get(0)?,
                    content_type: ContentType::from_str(&ct_str),
                    text_content: row.get(2)?,
                    image_path: row.get(3)?,
                    image_width: row.get(4)?,
                    image_height: row.get(5)?,
                    preview_text: row.get(6)?,
                    is_pinned: row.get::<_, i64>(7)? != 0,
                    hash: row.get(8)?,
                    created_at: row.get(9)?,
                })
            })?;

            for item in rows {
                items.push(item?);
            }
        }

        Ok(items)
    }

    /// Toggles the pinned status of a clipboard item. Returns the new pinned status.
    pub fn toggle_pin(&self, id: i64) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_conn();
        conn.execute(
            "UPDATE clipboard_history
             SET is_pinned = 1 - is_pinned
             WHERE id = ?1;",
            params![id],
        )?;

        let mut stmt = conn.prepare("SELECT is_pinned FROM clipboard_history WHERE id = ?1;")?;
        let is_pinned: i64 = stmt.query_row(params![id], |row| row.get(0))?;
        Ok(is_pinned != 0)
    }

    /// Deletes a specific item by ID.
    pub fn delete_item(&self, id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_conn();

        // Check if item has an image to delete from disk
        let mut stmt = conn.prepare("SELECT image_path FROM clipboard_history WHERE id = ?1;")?;
        let image_path: Option<String> = stmt
            .query_row(params![id], |row| row.get(0))
            .unwrap_or(None);

        conn.execute("DELETE FROM clipboard_history WHERE id = ?1;", params![id])?;

        if let Some(path) = image_path {
            let p = Path::new(&path);
            if p.exists() {
                let _ = fs::remove_file(p);
            }
        }

        Ok(())
    }

    /// Clears all unpinned history items. Pinned items are strictly preserved!
    pub fn clear_unpinned(&self) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_conn();

        // Collect images to clean up
        let mut stmt = conn.prepare("SELECT image_path FROM clipboard_history WHERE is_pinned = 0 AND image_path IS NOT NULL;")?;
        let image_paths = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for p in image_paths.flatten() {
            let path = Path::new(&p);
            if path.exists() {
                let _ = fs::remove_file(path);
            }
        }

        let deleted_count =
            conn.execute("DELETE FROM clipboard_history WHERE is_pinned = 0;", [])?;

        Ok(deleted_count)
    }

    /// Prunes oldest unpinned items if total unpinned items exceeds `max_items`.
    fn prune_unpinned_locked(
        &self,
        conn: &Connection,
        max_items: usize,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM clipboard_history WHERE is_pinned = 0;",
            [],
            |row| row.get(0),
        )?;

        if count > max_items as i64 {
            let excess = count - max_items as i64;
            let mut stmt = conn.prepare(
                "SELECT id, image_path FROM clipboard_history
                 WHERE is_pinned = 0
                 ORDER BY created_at ASC
                 LIMIT ?1;",
            )?;

            let old_rows = stmt.query_map(params![excess], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
            })?;

            let mut ids_to_delete = Vec::new();
            for item in old_rows.flatten() {
                ids_to_delete.push(item.0);
                if let Some(path) = item.1 {
                    let p = Path::new(&path);
                    if p.exists() {
                        let _ = fs::remove_file(p);
                    }
                }
            }

            for id in ids_to_delete {
                conn.execute("DELETE FROM clipboard_history WHERE id = ?1;", params![id])?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_text_preview_multiline() {
        let multiline = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        let preview = Database::generate_text_preview(multiline);
        assert_eq!(preview, "Line 1\nLine 2\nLine 3...");
    }

    #[test]
    fn test_generate_text_preview_unicode_boundary_safety() {
        // Multi-byte Unicode characters (Bengali, Japanese, Emojis)
        let unicode_text = "🦀 Rust 🚀 ".repeat(50);
        let preview = Database::generate_text_preview(&unicode_text);
        assert!(preview.ends_with("..."));
        assert!(preview.chars().count() <= 254);
    }

    #[test]
    fn test_db_insert_dedup_and_search() {
        let db = Database::new_in_memory().expect("Failed to create in-memory db");

        let id1 = db.insert_or_update_text("Hello Rust World").unwrap();
        assert!(id1 > 0);

        let items = db.get_items("").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text_content.as_deref(), Some("Hello Rust World"));

        // Upsert same text (deduplication)
        let _ = db.insert_or_update_text("Hello Rust World").unwrap();
        let items_after_dup = db.get_items("").unwrap();
        assert_eq!(items_after_dup.len(), 1);

        // Search matching
        let search_results = db.get_items("rust").unwrap();
        assert_eq!(search_results.len(), 1);

        // Search non-matching
        let no_match = db.get_items("python").unwrap();
        assert_eq!(no_match.len(), 0);
    }

    #[test]
    fn test_db_pinning_and_clear_unpinned() {
        let db = Database::new_in_memory().expect("Failed to create in-memory db");

        let id1 = db.insert_or_update_text("Keep me pinned").unwrap();
        let _id2 = db.insert_or_update_text("Unpinned item").unwrap();

        assert_eq!(db.get_items("").unwrap().len(), 2);

        // Pin first item
        let is_pinned = db.toggle_pin(id1).unwrap();
        assert!(is_pinned);

        // Clear unpinned
        let deleted = db.clear_unpinned().unwrap();
        assert_eq!(deleted, 1);

        let remaining = db.get_items("").unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, id1);
        assert!(remaining[0].is_pinned);
    }

    #[test]
    fn test_db_delete_item() {
        let db = Database::new_in_memory().expect("Failed to create in-memory db");

        let id = db.insert_or_update_text("Delete me").unwrap();
        assert_eq!(db.get_items("").unwrap().len(), 1);

        db.delete_item(id).unwrap();
        assert_eq!(db.get_items("").unwrap().len(), 0);
    }
}
