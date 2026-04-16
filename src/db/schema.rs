use anyhow::{Context, Result};
use rusqlite::Connection;

pub fn initialize(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS transcriptions (
            id            TEXT PRIMARY KEY,
            text          TEXT NOT NULL,
            word_count    INTEGER NOT NULL,
            char_count    INTEGER NOT NULL,
            duration_secs REAL NOT NULL,
            backend       TEXT NOT NULL,
            language      TEXT,
            created_at    TEXT NOT NULL DEFAULT (datetime('now')),
            deleted       INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_transcriptions_created_at
            ON transcriptions(created_at);

        CREATE TABLE IF NOT EXISTS daily_stats (
            date                   TEXT PRIMARY KEY,
            total_words            INTEGER NOT NULL DEFAULT 0,
            total_transcriptions   INTEGER NOT NULL DEFAULT 0,
            total_duration_secs    REAL NOT NULL DEFAULT 0
        );
        ",
    )
    .context("Failed to initialize database schema")?;

    Ok(())
}
