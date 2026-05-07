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

        CREATE TABLE IF NOT EXISTS token_usage (
            id                 INTEGER PRIMARY KEY AUTOINCREMENT,
            transcription_id   TEXT NOT NULL,
            model              TEXT NOT NULL,
            prompt_tokens      INTEGER NOT NULL DEFAULT 0,
            completion_tokens  INTEGER NOT NULL DEFAULT 0,
            total_tokens       INTEGER NOT NULL DEFAULT 0,
            created_at         TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_token_usage_created_at
            ON token_usage(created_at);

        CREATE TABLE IF NOT EXISTS api_cost (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            transcription_id    TEXT NOT NULL,
            provider            TEXT NOT NULL,
            model               TEXT NOT NULL,
            audio_duration_secs REAL NOT NULL DEFAULT 0,
            prompt_tokens       INTEGER NOT NULL DEFAULT 0,
            completion_tokens   INTEGER NOT NULL DEFAULT 0,
            estimated_cost_usd  REAL NOT NULL DEFAULT 0,
            created_at          TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_api_cost_created_at
            ON api_cost(created_at);

        CREATE INDEX IF NOT EXISTS idx_api_cost_provider
            ON api_cost(provider);
        ",
    )
    .context("Failed to initialize database schema")?;

    // Migration: add post_processing_error column (idempotent)
    let has_column: bool = conn
        .prepare("PRAGMA table_info(transcriptions)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .any(|name| name == "post_processing_error");

    if !has_column {
        conn.execute_batch(
            "ALTER TABLE transcriptions ADD COLUMN post_processing_error TEXT;",
        )?;
    }

    // Migration: add raw_text column (idempotent)
    let has_raw_text: bool = conn
        .prepare("PRAGMA table_info(transcriptions)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .any(|name| name == "raw_text");

    if !has_raw_text {
        conn.execute_batch(
            "ALTER TABLE transcriptions ADD COLUMN raw_text TEXT;",
        )?;
    }

    // Migration: add stt_model column (idempotent)
    let has_stt_model: bool = conn
        .prepare("PRAGMA table_info(transcriptions)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .any(|name| name == "stt_model");

    if !has_stt_model {
        conn.execute_batch(
            "ALTER TABLE transcriptions ADD COLUMN stt_model TEXT;",
        )?;
    }

    // Migration: add pp_model column (idempotent)
    let has_pp_model: bool = conn
        .prepare("PRAGMA table_info(transcriptions)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .any(|name| name == "pp_model");

    if !has_pp_model {
        conn.execute_batch(
            "ALTER TABLE transcriptions ADD COLUMN pp_model TEXT;",
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();
        conn
    }

    #[test]
    fn test_schema_transcriptions_columns() {
        let conn = test_conn();
        let mut stmt = conn.prepare("PRAGMA table_info(transcriptions)").unwrap();
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        for expected in &["id", "text", "word_count", "char_count", "duration_secs", "backend", "language", "created_at", "deleted"] {
            assert!(columns.contains(&expected.to_string()), "missing column: {}", expected);
        }
    }

    #[test]
    fn test_schema_api_cost_columns() {
        let conn = test_conn();
        let mut stmt = conn.prepare("PRAGMA table_info(api_cost)").unwrap();
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        for expected in &["id", "transcription_id", "provider", "model", "audio_duration_secs", "estimated_cost_usd"] {
            assert!(columns.contains(&expected.to_string()), "missing column: {}", expected);
        }
    }

    #[test]
    fn test_schema_indexes_exist() {
        let conn = test_conn();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert!(count >= 4, "expected at least 4 indexes, found {}", count);
    }
}
