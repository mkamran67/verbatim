pub mod schema;

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Transcription {
    pub id: String,
    pub text: String,
    pub word_count: i64,
    pub char_count: i64,
    pub duration_secs: f64,
    pub backend: String,
    pub language: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct Stats {
    pub today_words: i64,
    pub today_transcriptions: i64,
    pub week_words: i64,
    pub week_transcriptions: i64,
    pub total_words: i64,
    pub total_transcriptions: i64,
}

pub struct Database {
    conn: Connection,
}

/// Thread-safe wrapper for database access.
pub type SharedDatabase = Arc<Mutex<Database>>;

impl Database {
    pub fn open() -> Result<Self> {
        let db_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("~/.local/share"))
            .join("verbatim");

        std::fs::create_dir_all(&db_dir)
            .with_context(|| format!("Failed to create data directory {}", db_dir.display()))?;

        let db_path = db_dir.join("verbatim.db");
        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open database at {}", db_path.display()))?;

        // Enable WAL mode for better concurrent read performance
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;

        schema::initialize(&conn)?;

        tracing::info!("Database opened at {}", db_path.display());
        Ok(Self { conn })
    }

    pub fn open_shared() -> Result<SharedDatabase> {
        Ok(Arc::new(Mutex::new(Self::open()?)))
    }

    pub fn insert_transcription(&self, t: &Transcription) -> Result<()> {
        self.conn.execute(
            "INSERT INTO transcriptions (id, text, word_count, char_count, duration_secs, backend, language, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                t.id,
                t.text,
                t.word_count,
                t.char_count,
                t.duration_secs,
                t.backend,
                t.language,
                t.created_at,
            ],
        )?;

        // Update daily stats
        let date = &t.created_at[..10]; // YYYY-MM-DD
        self.conn.execute(
            "INSERT INTO daily_stats (date, total_words, total_transcriptions, total_duration_secs)
             VALUES (?1, ?2, 1, ?3)
             ON CONFLICT(date) DO UPDATE SET
                total_words = total_words + ?2,
                total_transcriptions = total_transcriptions + 1,
                total_duration_secs = total_duration_secs + ?3",
            rusqlite::params![date, t.word_count, t.duration_secs],
        )?;

        Ok(())
    }

    pub fn get_recent(&self, limit: usize) -> Result<Vec<Transcription>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, text, word_count, char_count, duration_secs, backend, language, created_at
             FROM transcriptions
             WHERE deleted = 0
             ORDER BY created_at DESC
             LIMIT ?1",
        )?;

        let rows = stmt.query_map([limit as i64], |row| {
            Ok(Transcription {
                id: row.get(0)?,
                text: row.get(1)?,
                word_count: row.get(2)?,
                char_count: row.get(3)?,
                duration_secs: row.get(4)?,
                backend: row.get(5)?,
                language: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn search(&self, query: &str, limit: usize, offset: usize) -> Result<Vec<Transcription>> {
        let pattern = format!("%{}%", query);
        let mut stmt = self.conn.prepare(
            "SELECT id, text, word_count, char_count, duration_secs, backend, language, created_at
             FROM transcriptions
             WHERE deleted = 0 AND text LIKE ?1
             ORDER BY created_at DESC
             LIMIT ?2 OFFSET ?3",
        )?;

        let rows = stmt.query_map(rusqlite::params![pattern, limit as i64, offset as i64], |row| {
            Ok(Transcription {
                id: row.get(0)?,
                text: row.get(1)?,
                word_count: row.get(2)?,
                char_count: row.get(3)?,
                duration_secs: row.get(4)?,
                backend: row.get(5)?,
                language: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE transcriptions SET deleted = 1 WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    pub fn get_stats(&self) -> Result<Stats> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let week_ago = (chrono::Local::now() - chrono::Duration::days(7))
            .format("%Y-%m-%d")
            .to_string();

        let today_stats: (i64, i64) = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total_words), 0), COALESCE(SUM(total_transcriptions), 0)
                 FROM daily_stats WHERE date = ?1",
                [&today],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap_or((0, 0));

        let week_stats: (i64, i64) = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total_words), 0), COALESCE(SUM(total_transcriptions), 0)
                 FROM daily_stats WHERE date >= ?1",
                [&week_ago],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap_or((0, 0));

        let total_stats: (i64, i64) = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total_words), 0), COALESCE(SUM(total_transcriptions), 0)
                 FROM daily_stats",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap_or((0, 0));

        Ok(Stats {
            today_words: today_stats.0,
            today_transcriptions: today_stats.1,
            week_words: week_stats.0,
            week_transcriptions: week_stats.1,
            total_words: total_stats.0,
            total_transcriptions: total_stats.1,
        })
    }
}
