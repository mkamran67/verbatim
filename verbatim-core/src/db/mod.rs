pub mod schema;

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Transcription {
    pub id: String,
    pub text: String,
    pub word_count: i64,
    pub char_count: i64,
    pub duration_secs: f64,
    pub backend: String,
    pub language: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub prompt_tokens: i64,
    #[serde(default)]
    pub completion_tokens: i64,
    #[serde(default)]
    pub post_processing_error: Option<String>,
    #[serde(default)]
    pub raw_text: Option<String>,
    #[serde(default)]
    pub stt_model: Option<String>,
    #[serde(default)]
    pub pp_model: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DailyTokenUsage {
    pub date: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DailyWordStats {
    pub date: String,
    pub total_words: i64,
    pub total_transcriptions: i64,
    pub total_duration_secs: f64,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Stats {
    pub today_words: i64,
    pub today_transcriptions: i64,
    pub week_words: i64,
    pub week_transcriptions: i64,
    pub total_words: i64,
    pub total_transcriptions: i64,
    pub today_tokens: i64,
    pub week_tokens: i64,
    pub total_tokens: i64,
    pub today_cost_usd: f64,
    pub week_cost_usd: f64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DailyCostSummary {
    pub date: String,
    pub provider: String,
    pub total_cost_usd: f64,
    pub total_duration_secs: f64,
    pub total_requests: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderCostSummary {
    pub provider: String,
    pub total_cost_usd: f64,
    pub total_duration_secs: f64,
    pub total_requests: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DailyProviderUsage {
    pub date: String,
    pub provider: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelTokenUsage {
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
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
        tracing::debug!("WAL mode enabled");

        schema::initialize(&conn)?;
        tracing::debug!("database schema initialized");

        tracing::info!("Database opened at {}", db_path.display());
        Ok(Self { conn })
    }

    /// Open an in-memory database for testing.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .context("Failed to open in-memory database")?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        schema::initialize(&conn)?;
        Ok(Self { conn })
    }

    pub fn open_shared() -> Result<SharedDatabase> {
        tracing::debug!("creating shared database handle");
        Ok(Arc::new(Mutex::new(Self::open()?)))
    }

    pub fn insert_transcription(&self, t: &Transcription) -> Result<()> {
        tracing::debug!(
            id = %t.id,
            word_count = t.word_count,
            char_count = t.char_count,
            duration_secs = t.duration_secs,
            backend = %t.backend,
            language = ?t.language,
            "inserting transcription"
        );
        self.conn.execute(
            "INSERT INTO transcriptions (id, text, word_count, char_count, duration_secs, backend, language, created_at, post_processing_error, raw_text, stt_model, pp_model)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                t.id,
                t.text,
                t.word_count,
                t.char_count,
                t.duration_secs,
                t.backend,
                t.language,
                t.created_at,
                t.post_processing_error,
                t.raw_text,
                t.stt_model,
                t.pp_model,
            ],
        )?;

        // Update daily stats
        let date = &t.created_at[..10]; // YYYY-MM-DD
        tracing::trace!(date, "updating daily_stats");
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
        tracing::trace!(limit, "querying recent transcriptions");
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.text, t.word_count, t.char_count, t.duration_secs, t.backend, t.language, t.created_at,
                    COALESCE(u.prompt_tokens, 0), COALESCE(u.completion_tokens, 0), t.post_processing_error, t.raw_text,
                    t.stt_model, t.pp_model
             FROM transcriptions t
             LEFT JOIN token_usage u ON u.transcription_id = t.id
             WHERE t.deleted = 0
             ORDER BY t.created_at DESC
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
                prompt_tokens: row.get(8)?,
                completion_tokens: row.get(9)?,
                post_processing_error: row.get(10)?,
                raw_text: row.get(11)?,
                stt_model: row.get(12)?,
                pp_model: row.get(13)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        tracing::trace!(count = results.len(), "returning recent transcriptions");
        Ok(results)
    }

    pub fn search(&self, query: &str, limit: usize, offset: usize) -> Result<Vec<Transcription>> {
        tracing::debug!(query, limit, offset, "searching transcriptions");
        let pattern = format!("%{}%", query);
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.text, t.word_count, t.char_count, t.duration_secs, t.backend, t.language, t.created_at,
                    COALESCE(u.prompt_tokens, 0), COALESCE(u.completion_tokens, 0), t.post_processing_error, t.raw_text,
                    t.stt_model, t.pp_model
             FROM transcriptions t
             LEFT JOIN token_usage u ON u.transcription_id = t.id
             WHERE t.deleted = 0 AND t.text LIKE ?1
             ORDER BY t.created_at DESC
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
                prompt_tokens: row.get(8)?,
                completion_tokens: row.get(9)?,
                post_processing_error: row.get(10)?,
                raw_text: row.get(11)?,
                stt_model: row.get(12)?,
                pp_model: row.get(13)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        tracing::debug!(count = results.len(), "search returned results");
        Ok(results)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        tracing::debug!(id, "soft-deleting transcription");
        self.conn.execute(
            "UPDATE transcriptions SET deleted = 1 WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    pub fn insert_token_usage(
        &self,
        transcription_id: &str,
        model: &str,
        prompt_tokens: i64,
        completion_tokens: i64,
        total_tokens: i64,
    ) -> Result<()> {
        tracing::debug!(
            transcription_id,
            model,
            prompt_tokens,
            completion_tokens,
            total_tokens,
            "inserting token usage"
        );
        self.conn.execute(
            "INSERT INTO token_usage (transcription_id, model, prompt_tokens, completion_tokens, total_tokens, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
            rusqlite::params![transcription_id, model, prompt_tokens, completion_tokens, total_tokens],
        )?;
        Ok(())
    }

    pub fn get_stats(&self) -> Result<Stats> {
        tracing::trace!("querying stats");
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

        let today_tokens: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total_tokens), 0) FROM token_usage WHERE date(created_at) = ?1",
                [&today],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let week_tokens: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total_tokens), 0) FROM token_usage WHERE date(created_at) >= ?1",
                [&week_ago],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let total_tokens: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total_tokens), 0) FROM token_usage",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let today_cost: f64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(estimated_cost_usd), 0) FROM api_cost WHERE date(created_at) = ?1",
                [&today],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        let week_cost: f64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(estimated_cost_usd), 0) FROM api_cost WHERE date(created_at) >= ?1",
                [&week_ago],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        let total_cost: f64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(estimated_cost_usd), 0) FROM api_cost",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        let stats = Stats {
            today_words: today_stats.0,
            today_transcriptions: today_stats.1,
            week_words: week_stats.0,
            week_transcriptions: week_stats.1,
            total_words: total_stats.0,
            total_transcriptions: total_stats.1,
            today_tokens,
            week_tokens,
            total_tokens,
            today_cost_usd: today_cost,
            week_cost_usd: week_cost,
            total_cost_usd: total_cost,
        };
        tracing::trace!(
            today_words = stats.today_words,
            today_transcriptions = stats.today_transcriptions,
            total_words = stats.total_words,
            total_transcriptions = stats.total_transcriptions,
            "stats computed"
        );
        Ok(stats)
    }

    pub fn get_transcriptions_for_date(&self, date: &str) -> Result<Vec<Transcription>> {
        tracing::debug!(date, "querying transcriptions for date");
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.text, t.word_count, t.char_count, t.duration_secs, t.backend, t.language, t.created_at,
                    COALESCE(u.prompt_tokens, 0), COALESCE(u.completion_tokens, 0), t.post_processing_error, t.raw_text,
                    t.stt_model, t.pp_model
             FROM transcriptions t
             LEFT JOIN token_usage u ON u.transcription_id = t.id
             WHERE t.deleted = 0 AND date(t.created_at) = ?1
             ORDER BY t.created_at DESC",
        )?;

        let rows = stmt.query_map([date], |row| {
            Ok(Transcription {
                id: row.get(0)?,
                text: row.get(1)?,
                word_count: row.get(2)?,
                char_count: row.get(3)?,
                duration_secs: row.get(4)?,
                backend: row.get(5)?,
                language: row.get(6)?,
                created_at: row.get(7)?,
                prompt_tokens: row.get(8)?,
                completion_tokens: row.get(9)?,
                post_processing_error: row.get(10)?,
                raw_text: row.get(11)?,
                stt_model: row.get(12)?,
                pp_model: row.get(13)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        tracing::debug!(count = results.len(), date, "returning transcriptions for date");
        Ok(results)
    }

    pub fn get_daily_word_stats(&self, days: i64) -> Result<Vec<DailyWordStats>> {
        tracing::trace!(days, "querying daily word stats");
        let since = (chrono::Local::now() - chrono::Duration::days(days))
            .format("%Y-%m-%d")
            .to_string();

        let mut stmt = self.conn.prepare(
            "SELECT date, total_words, total_transcriptions, total_duration_secs
             FROM daily_stats
             WHERE date >= ?1
             ORDER BY date ASC",
        )?;

        let rows = stmt.query_map([&since], |row| {
            Ok(DailyWordStats {
                date: row.get(0)?,
                total_words: row.get(1)?,
                total_transcriptions: row.get(2)?,
                total_duration_secs: row.get(3)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        tracing::trace!(count = results.len(), "returning daily word stats");
        Ok(results)
    }

    pub fn get_daily_token_usage(&self, days: i64) -> Result<Vec<DailyTokenUsage>> {
        tracing::trace!(days, "querying daily token usage");
        let since = (chrono::Local::now() - chrono::Duration::days(days))
            .format("%Y-%m-%d")
            .to_string();

        let mut stmt = self.conn.prepare(
            "SELECT date(created_at) as day,
                    COALESCE(SUM(prompt_tokens), 0),
                    COALESCE(SUM(completion_tokens), 0)
             FROM token_usage
             WHERE date(created_at) >= ?1
             GROUP BY day
             ORDER BY day ASC",
        )?;

        let rows = stmt.query_map([&since], |row| {
            Ok(DailyTokenUsage {
                date: row.get(0)?,
                prompt_tokens: row.get(1)?,
                completion_tokens: row.get(2)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        tracing::trace!(count = results.len(), "returning daily token usage");
        Ok(results)
    }

    pub fn get_daily_provider_usage(&self, days: i64) -> Result<Vec<DailyProviderUsage>> {
        tracing::trace!(days, "querying daily provider usage");
        let since = (chrono::Local::now() - chrono::Duration::days(days))
            .format("%Y-%m-%d")
            .to_string();

        let mut stmt = self.conn.prepare(
            "SELECT date(created_at) as day,
                    provider,
                    COALESCE(SUM(prompt_tokens), 0),
                    COALESCE(SUM(completion_tokens), 0),
                    COALESCE(SUM(audio_duration_secs), 0)
             FROM api_cost
             WHERE date(created_at) >= ?1
             GROUP BY day, provider
             ORDER BY day ASC",
        )?;

        let rows = stmt.query_map([&since], |row| {
            Ok(DailyProviderUsage {
                date: row.get(0)?,
                provider: row.get(1)?,
                prompt_tokens: row.get(2)?,
                completion_tokens: row.get(3)?,
                duration_secs: row.get(4)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        tracing::trace!(count = results.len(), "returning daily provider usage");
        Ok(results)
    }

    pub fn get_token_usage_by_model(&self) -> Result<Vec<ModelTokenUsage>> {
        tracing::trace!("querying token usage by model");
        let mut stmt = self.conn.prepare(
            "SELECT model,
                    COALESCE(SUM(prompt_tokens), 0),
                    COALESCE(SUM(completion_tokens), 0),
                    COALESCE(SUM(total_tokens), 0)
             FROM token_usage
             GROUP BY model
             ORDER BY SUM(total_tokens) DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(ModelTokenUsage {
                model: row.get(0)?,
                prompt_tokens: row.get(1)?,
                completion_tokens: row.get(2)?,
                total_tokens: row.get(3)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        tracing::trace!(count = results.len(), "returning token usage by model");
        Ok(results)
    }

    // ── API Cost Tracking ───────────────────────────────────────────

    /// Estimate cost in USD based on provider/model and usage.
    fn estimate_cost(provider: &str, model: &str, audio_duration_secs: f64, prompt_tokens: i64, completion_tokens: i64) -> f64 {
        match provider {
            "deepgram" => {
                let minutes = audio_duration_secs / 60.0;
                match model {
                    "nova-3" => minutes * 0.0059,
                    _ => minutes * 0.0043, // nova-2 and others
                }
            }
            "openai-stt" => {
                let minutes = audio_duration_secs / 60.0;
                minutes * 0.006 // whisper-1 pricing
            }
            "smallest" => {
                // Smallest Pulse STT — $0.005/minute (smallest.ai/pricing,
                // "Pay as you go" tier).
                let minutes = audio_duration_secs / 60.0;
                minutes * 0.005
            }
            "openai-postproc" => {
                let input_cost = match model {
                    "gpt-4o" => prompt_tokens as f64 * 2.50 / 1_000_000.0,
                    "gpt-4o-mini" | _ => prompt_tokens as f64 * 0.15 / 1_000_000.0,
                };
                let output_cost = match model {
                    "gpt-4o" => completion_tokens as f64 * 10.00 / 1_000_000.0,
                    "gpt-4o-mini" | _ => completion_tokens as f64 * 0.60 / 1_000_000.0,
                };
                input_cost + output_cost
            }
            _ => 0.0,
        }
    }

    pub fn insert_api_cost(
        &self,
        transcription_id: &str,
        provider: &str,
        model: &str,
        audio_duration_secs: f64,
        prompt_tokens: i64,
        completion_tokens: i64,
    ) -> Result<()> {
        let estimated_cost = Self::estimate_cost(provider, model, audio_duration_secs, prompt_tokens, completion_tokens);
        tracing::debug!(
            transcription_id,
            provider,
            model,
            audio_duration_secs,
            prompt_tokens,
            completion_tokens,
            estimated_cost,
            "inserting API cost"
        );
        self.conn.execute(
            "INSERT INTO api_cost (transcription_id, provider, model, audio_duration_secs, prompt_tokens, completion_tokens, estimated_cost_usd, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now', 'localtime'))",
            rusqlite::params![
                transcription_id,
                provider,
                model,
                audio_duration_secs,
                prompt_tokens,
                completion_tokens,
                estimated_cost,
            ],
        )?;
        Ok(())
    }

    pub fn get_daily_cost_summary(&self, days: i64) -> Result<Vec<DailyCostSummary>> {
        tracing::trace!(days, "querying daily cost summary");
        let since = (chrono::Local::now() - chrono::Duration::days(days))
            .format("%Y-%m-%d")
            .to_string();

        let mut stmt = self.conn.prepare(
            "SELECT date(created_at) as day, provider,
                    COALESCE(SUM(estimated_cost_usd), 0),
                    COALESCE(SUM(audio_duration_secs), 0),
                    COUNT(*)
             FROM api_cost
             WHERE date(created_at) >= ?1
             GROUP BY day, provider
             ORDER BY day ASC, provider ASC",
        )?;

        let rows = stmt.query_map([&since], |row| {
            Ok(DailyCostSummary {
                date: row.get(0)?,
                provider: row.get(1)?,
                total_cost_usd: row.get(2)?,
                total_duration_secs: row.get(3)?,
                total_requests: row.get(4)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        tracing::trace!(count = results.len(), "returning daily cost summary");
        Ok(results)
    }

    /// Sum estimated costs from the api_cost table since a given datetime string,
    /// optionally filtered by provider prefix (e.g. "deepgram", "openai").
    pub fn get_estimated_costs_since(&self, since: &str, provider_prefix: Option<&str>) -> Result<f64> {
        let cost: f64 = match provider_prefix {
            Some(prefix) => {
                let pattern = format!("{}%", prefix);
                self.conn.query_row(
                    "SELECT COALESCE(SUM(estimated_cost_usd), 0) FROM api_cost WHERE created_at >= ?1 AND provider LIKE ?2",
                    rusqlite::params![since, pattern],
                    |row| row.get(0),
                )?
            }
            None => {
                self.conn.query_row(
                    "SELECT COALESCE(SUM(estimated_cost_usd), 0) FROM api_cost WHERE created_at >= ?1",
                    [since],
                    |row| row.get(0),
                )?
            }
        };
        Ok(cost)
    }

    pub fn get_cost_by_provider(&self) -> Result<Vec<ProviderCostSummary>> {
        tracing::trace!("querying cost by provider");
        let mut stmt = self.conn.prepare(
            "SELECT provider,
                    COALESCE(SUM(estimated_cost_usd), 0),
                    COALESCE(SUM(audio_duration_secs), 0),
                    COUNT(*)
             FROM api_cost
             GROUP BY provider
             ORDER BY SUM(estimated_cost_usd) DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(ProviderCostSummary {
                provider: row.get(0)?,
                total_cost_usd: row.get(1)?,
                total_duration_secs: row.get(2)?,
                total_requests: row.get(3)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        tracing::trace!(count = results.len(), "returning cost by provider");
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn make_transcription(id: &str, text: &str, created_at: &str) -> Transcription {
        Transcription {
            id: id.to_string(),
            text: text.to_string(),
            word_count: text.split_whitespace().count() as i64,
            char_count: text.len() as i64,
            duration_secs: 1.5,
            backend: "test".to_string(),
            language: Some("en".to_string()),
            created_at: created_at.to_string(),
            prompt_tokens: 0,
            completion_tokens: 0,
            post_processing_error: None,
            raw_text: None,
            stt_model: None,
            pp_model: None,
        }
    }

    #[test]
    fn test_schema_creates_tables() {
        let db = test_db();
        // Verify tables exist by querying sqlite_master
        let count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('transcriptions', 'daily_stats', 'token_usage', 'api_cost')",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 4);
    }

    #[test]
    fn test_schema_idempotent() {
        let db = test_db();
        // Initialize again -- should not error
        schema::initialize(&db.conn).unwrap();
    }

    #[test]
    fn test_insert_and_get_recent() {
        let db = test_db();
        let t1 = make_transcription("1", "hello world", "2024-01-01 10:00:00");
        let t2 = make_transcription("2", "foo bar baz", "2024-01-01 11:00:00");
        let t3 = make_transcription("3", "third entry", "2024-01-01 12:00:00");
        db.insert_transcription(&t1).unwrap();
        db.insert_transcription(&t2).unwrap();
        db.insert_transcription(&t3).unwrap();

        let recent = db.get_recent(2).unwrap();
        assert_eq!(recent.len(), 2);
        // Most recent first
        assert_eq!(recent[0].id, "3");
        assert_eq!(recent[1].id, "2");
    }

    #[test]
    fn test_get_recent_empty_db() {
        let db = test_db();
        let recent = db.get_recent(10).unwrap();
        assert!(recent.is_empty());
    }

    #[test]
    fn test_soft_delete_excludes_from_recent() {
        let db = test_db();
        let t = make_transcription("del-1", "to be deleted", "2024-01-01 10:00:00");
        db.insert_transcription(&t).unwrap();
        assert_eq!(db.get_recent(10).unwrap().len(), 1);

        db.delete("del-1").unwrap();
        assert!(db.get_recent(10).unwrap().is_empty());
    }

    #[test]
    fn test_search_by_substring() {
        let db = test_db();
        db.insert_transcription(&make_transcription("1", "hello world", "2024-01-01 10:00:00")).unwrap();
        db.insert_transcription(&make_transcription("2", "goodbye moon", "2024-01-01 11:00:00")).unwrap();

        let results = db.search("hello", 10, 0).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1");
    }

    #[test]
    fn test_search_case_insensitive() {
        let db = test_db();
        db.insert_transcription(&make_transcription("1", "Hello World", "2024-01-01 10:00:00")).unwrap();

        let results = db.search("hello", 10, 0).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_with_offset() {
        let db = test_db();
        for i in 0..5 {
            db.insert_transcription(&make_transcription(
                &format!("{}", i),
                &format!("entry {}", i),
                &format!("2024-01-01 {:02}:00:00", 10 + i),
            )).unwrap();
        }

        let results = db.search("entry", 2, 2).unwrap();
        assert_eq!(results.len(), 2);
        // Ordered DESC by created_at, so offset 2 skips the two most recent
        assert_eq!(results[0].id, "2");
        assert_eq!(results[1].id, "1");
    }

    #[test]
    fn test_insert_token_usage() {
        let db = test_db();
        let t = make_transcription("tok-1", "test", "2024-01-01 10:00:00");
        db.insert_transcription(&t).unwrap();
        db.insert_token_usage("tok-1", "gpt-4o-mini", 100, 50, 150).unwrap();

        let recent = db.get_recent(1).unwrap();
        assert_eq!(recent[0].prompt_tokens, 100);
        assert_eq!(recent[0].completion_tokens, 50);
    }

    #[test]
    fn test_stats_empty_db() {
        let db = test_db();
        let stats = db.get_stats().unwrap();
        assert_eq!(stats.today_words, 0);
        assert_eq!(stats.today_transcriptions, 0);
        assert_eq!(stats.total_words, 0);
        assert_eq!(stats.total_transcriptions, 0);
        assert_eq!(stats.today_tokens, 0);
        assert_eq!(stats.total_tokens, 0);
    }

    #[test]
    fn test_stats_today_counts() {
        let db = test_db();
        let today = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let t = make_transcription("today-1", "hello world today", &today);
        db.insert_transcription(&t).unwrap();

        let stats = db.get_stats().unwrap();
        assert_eq!(stats.today_words, 3);
        assert_eq!(stats.today_transcriptions, 1);
        assert_eq!(stats.total_words, 3);
        assert_eq!(stats.total_transcriptions, 1);
    }

    #[test]
    fn test_daily_word_stats_aggregation() {
        let db = test_db();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let yesterday = (chrono::Local::now() - chrono::Duration::days(1))
            .format("%Y-%m-%d").to_string();

        db.insert_transcription(&make_transcription("1", "hello world", &format!("{} 10:00:00", today))).unwrap();
        db.insert_transcription(&make_transcription("2", "foo bar baz", &format!("{} 11:00:00", today))).unwrap();
        db.insert_transcription(&make_transcription("3", "yesterday test", &format!("{} 10:00:00", yesterday))).unwrap();

        let stats = db.get_daily_word_stats(7).unwrap();
        assert!(stats.len() >= 2);

        let today_stat = stats.iter().find(|s| s.date == today).unwrap();
        assert_eq!(today_stat.total_words, 5); // "hello world" (2) + "foo bar baz" (3)
        assert_eq!(today_stat.total_transcriptions, 2);
    }

    #[test]
    fn test_daily_token_usage() {
        let db = test_db();
        let t = make_transcription("tu-1", "test", "2024-01-15 10:00:00");
        db.insert_transcription(&t).unwrap();
        db.insert_token_usage("tu-1", "gpt-4o-mini", 100, 50, 150).unwrap();

        // Query with a wide range
        let usage = db.get_daily_token_usage(365 * 3).unwrap();
        assert!(!usage.is_empty());
    }

    #[test]
    fn test_get_transcriptions_for_date() {
        let db = test_db();
        db.insert_transcription(&make_transcription("d1", "jan first", "2024-01-01 10:00:00")).unwrap();
        db.insert_transcription(&make_transcription("d2", "jan second", "2024-01-02 10:00:00")).unwrap();

        let results = db.get_transcriptions_for_date("2024-01-01").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "d1");
    }

    #[test]
    fn test_insert_api_cost_deepgram() {
        let db = test_db();
        let t = make_transcription("cost-1", "test", "2024-01-01 10:00:00");
        db.insert_transcription(&t).unwrap();
        db.insert_api_cost("cost-1", "deepgram", "nova-2", 60.0, 0, 0).unwrap();

        let providers = db.get_cost_by_provider().unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider, "deepgram");
        // 1 minute at $0.0043/min
        assert!((providers[0].total_cost_usd - 0.0043).abs() < 0.0001);
        assert_eq!(providers[0].total_requests, 1);
    }

    #[test]
    fn test_insert_api_cost_openai_stt() {
        let db = test_db();
        let t = make_transcription("cost-2", "test", "2024-01-01 10:00:00");
        db.insert_transcription(&t).unwrap();
        db.insert_api_cost("cost-2", "openai-stt", "whisper-1", 60.0, 0, 0).unwrap();

        let providers = db.get_cost_by_provider().unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider, "openai-stt");
        // 1 minute at $0.006/min
        assert!((providers[0].total_cost_usd - 0.006).abs() < 0.0001);
    }

    #[test]
    fn test_insert_api_cost_openai_postproc() {
        let db = test_db();
        let t = make_transcription("cost-3", "test", "2024-01-01 10:00:00");
        db.insert_transcription(&t).unwrap();
        db.insert_api_cost("cost-3", "openai-postproc", "gpt-4o-mini", 0.0, 100, 50).unwrap();

        let providers = db.get_cost_by_provider().unwrap();
        assert_eq!(providers.len(), 1);
        // 100 * 0.15/1M + 50 * 0.60/1M = 0.000015 + 0.00003 = 0.000045
        assert!(providers[0].total_cost_usd > 0.0);
    }

    #[test]
    fn test_stats_include_cost() {
        let db = test_db();
        let today = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let t = make_transcription("cost-s", "hello", &today);
        db.insert_transcription(&t).unwrap();
        db.insert_api_cost("cost-s", "deepgram", "nova-2", 60.0, 0, 0).unwrap();

        let stats = db.get_stats().unwrap();
        assert!(stats.today_cost_usd > 0.0);
        assert!(stats.total_cost_usd > 0.0);
    }

    #[test]
    fn test_estimate_cost_unknown_provider() {
        let cost = Database::estimate_cost("unknown", "model", 60.0, 0, 0);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_daily_cost_summary() {
        let db = test_db();
        let t = make_transcription("dcs-1", "test", "2024-01-15 10:00:00");
        db.insert_transcription(&t).unwrap();
        db.insert_api_cost("dcs-1", "deepgram", "nova-2", 30.0, 0, 0).unwrap();

        let summary = db.get_daily_cost_summary(365 * 3).unwrap();
        assert!(!summary.is_empty());
        assert_eq!(summary[0].provider, "deepgram");
    }

    // ── Edge case & boundary tests ──────────────────────────────────

    #[test]
    fn test_search_sql_special_char_percent() {
        let db = test_db();
        db.insert_transcription(&make_transcription("p1", "100% done", "2024-01-01 10:00:00")).unwrap();
        db.insert_transcription(&make_transcription("p2", "nothing here", "2024-01-01 11:00:00")).unwrap();

        // Searching for "%" currently matches everything because format!("%{}%", "%") => "%%%"
        // which LIKE treats as "match anything". This documents the unescaped-wildcard behavior.
        let results = db.search("%", 10, 0).unwrap();
        // BUG: returns all rows, not just the one containing literal "%"
        assert_eq!(results.len(), 2, "unescaped LIKE wildcard '%' matches all rows");
    }

    #[test]
    fn test_search_sql_special_char_single_quote() {
        let db = test_db();
        db.insert_transcription(&make_transcription("q1", "it's a test", "2024-01-01 10:00:00")).unwrap();
        db.insert_transcription(&make_transcription("q2", "no quote", "2024-01-01 11:00:00")).unwrap();

        // Single quotes are safely handled by parameterized queries
        let results = db.search("it's", 10, 0).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "q1");
    }

    #[test]
    fn test_search_sql_special_char_underscore() {
        let db = test_db();
        db.insert_transcription(&make_transcription("u1", "snake_case", "2024-01-01 10:00:00")).unwrap();
        db.insert_transcription(&make_transcription("u2", "two words", "2024-01-01 11:00:00")).unwrap();

        // "_" in LIKE matches any single character, so searching "_" without escaping
        // will match rows that have at least one character. Documents unescaped behavior.
        let results = db.search("_", 10, 0).unwrap();
        assert_eq!(results.len(), 2, "unescaped LIKE wildcard '_' matches all rows with any char");
    }

    #[test]
    fn test_search_empty_query_matches_all() {
        let db = test_db();
        db.insert_transcription(&make_transcription("e1", "hello", "2024-01-01 10:00:00")).unwrap();
        db.insert_transcription(&make_transcription("e2", "world", "2024-01-01 11:00:00")).unwrap();

        let results = db.search("", 10, 0).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_insert_zero_length_text() {
        let db = test_db();
        let t = Transcription {
            id: "empty-1".into(),
            text: String::new(),
            word_count: 0,
            char_count: 0,
            duration_secs: 0.0,
            backend: "test".into(),
            language: None,
            created_at: "2024-01-01 10:00:00".into(),
            prompt_tokens: 0,
            completion_tokens: 0,
            post_processing_error: None,
            raw_text: None,
            stt_model: None,
            pp_model: None,
        };
        db.insert_transcription(&t).unwrap();

        let recent = db.get_recent(1).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].text, "");
        assert_eq!(recent[0].word_count, 0);
        assert_eq!(recent[0].char_count, 0);
    }

    #[test]
    fn test_search_offset_beyond_results() {
        let db = test_db();
        db.insert_transcription(&make_transcription("o1", "hello", "2024-01-01 10:00:00")).unwrap();

        let results = db.search("hello", 10, 1000).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_limit_zero() {
        let db = test_db();
        db.insert_transcription(&make_transcription("l1", "hello", "2024-01-01 10:00:00")).unwrap();

        let results = db.search("hello", 0, 0).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_get_recent_limit_zero() {
        let db = test_db();
        db.insert_transcription(&make_transcription("r1", "hello", "2024-01-01 10:00:00")).unwrap();

        let results = db.get_recent(0).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_duplicate_id_insert_fails() {
        let db = test_db();
        let t = make_transcription("dup-1", "first", "2024-01-01 10:00:00");
        db.insert_transcription(&t).unwrap();

        let t2 = make_transcription("dup-1", "second", "2024-01-01 11:00:00");
        let result = db.insert_transcription(&t2);
        assert!(result.is_err(), "duplicate primary key should fail");
    }

    #[test]
    fn test_delete_nonexistent_id_is_noop() {
        let db = test_db();
        // Should succeed without error even though no row matches
        db.delete("no-such-id").unwrap();
    }

    #[test]
    fn test_estimate_cost_zero_duration() {
        let cost = Database::estimate_cost("deepgram", "nova-2", 0.0, 0, 0);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_estimate_cost_nova3_vs_nova2() {
        let nova2 = Database::estimate_cost("deepgram", "nova-2", 60.0, 0, 0);
        let nova3 = Database::estimate_cost("deepgram", "nova-3", 60.0, 0, 0);
        assert!(nova3 > nova2, "nova-3 should be more expensive than nova-2");
    }

    #[test]
    fn test_estimate_cost_gpt4o_vs_gpt4o_mini() {
        let mini = Database::estimate_cost("openai-postproc", "gpt-4o-mini", 0.0, 1000, 1000);
        let full = Database::estimate_cost("openai-postproc", "gpt-4o", 0.0, 1000, 1000);
        assert!(full > mini, "gpt-4o should be more expensive than gpt-4o-mini");
    }

    #[test]
    fn test_insert_api_cost_zero_values() {
        let db = test_db();
        let t = make_transcription("zero-1", "test", "2024-01-01 10:00:00");
        db.insert_transcription(&t).unwrap();
        db.insert_api_cost("zero-1", "deepgram", "nova-2", 0.0, 0, 0).unwrap();

        let providers = db.get_cost_by_provider().unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].total_cost_usd, 0.0);
    }

    #[test]
    fn test_get_token_usage_by_model_empty_db() {
        let db = test_db();
        let usage = db.get_token_usage_by_model().unwrap();
        assert!(usage.is_empty());
    }

    #[test]
    fn test_stats_multiple_days_aggregation() {
        let db = test_db();
        let today = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let three_days_ago = (chrono::Local::now() - chrono::Duration::days(3))
            .format("%Y-%m-%d %H:%M:%S").to_string();

        db.insert_transcription(&make_transcription("m1", "today words", &today)).unwrap();
        db.insert_transcription(&make_transcription("m2", "older entry here", &three_days_ago)).unwrap();

        let stats = db.get_stats().unwrap();
        assert_eq!(stats.today_words, 2, "today should have 2 words");
        assert_eq!(stats.today_transcriptions, 1);
        assert_eq!(stats.week_words, 5, "week should include both entries (2 + 3)");
        assert_eq!(stats.week_transcriptions, 2);
        assert_eq!(stats.total_words, 5);
        assert_eq!(stats.total_transcriptions, 2);
    }

    #[test]
    fn test_get_estimated_costs_since() {
        let db = test_db();
        let t1 = make_transcription("cs-1", "test", "2024-01-01 10:00:00");
        let t2 = make_transcription("cs-2", "test", "2024-01-01 14:00:00");
        db.insert_transcription(&t1).unwrap();
        db.insert_transcription(&t2).unwrap();
        db.insert_api_cost("cs-1", "deepgram", "nova-2", 60.0, 0, 0).unwrap();
        db.insert_api_cost("cs-2", "openai-stt", "whisper-1", 60.0, 0, 0).unwrap();

        // All costs since epoch
        let total = db.get_estimated_costs_since("2000-01-01 00:00:00", None).unwrap();
        assert!(total > 0.0);

        // Only deepgram costs
        let dg = db.get_estimated_costs_since("2000-01-01 00:00:00", Some("deepgram")).unwrap();
        assert!((dg - 0.0043).abs() < 0.0001);

        // Only openai costs
        let oai = db.get_estimated_costs_since("2000-01-01 00:00:00", Some("openai")).unwrap();
        assert!((oai - 0.006).abs() < 0.0001);

        // No costs after a future date
        let future = db.get_estimated_costs_since("2099-01-01 00:00:00", None).unwrap();
        assert_eq!(future, 0.0);
    }
}
