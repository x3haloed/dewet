//! Turso (libSQL) database client

use anyhow::{Context, Result};
use libsql::{Builder, Connection, params};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

use super::{CharacterState, ChatMessage, Episode, ScreenContext, SpatialContext};

/// Turso database client
#[derive(Clone)]
pub struct TursoDb {
    conn: Arc<Mutex<Connection>>,
}

impl TursoDb {
    /// Connect to a Turso database
    pub async fn connect(url: &str, auth_token: Option<&str>) -> Result<Self> {
        let db = if url.starts_with("libsql://") || url.starts_with("https://") {
            // Remote Turso database
            let token = auth_token
                .map(|s| s.to_string())
                .or_else(|| std::env::var("TURSO_AUTH_TOKEN").ok())
                .context("TURSO_AUTH_TOKEN required for remote database")?;

            Builder::new_remote(url.to_string(), token)
                .build()
                .await
                .context("Failed to connect to remote Turso database")?
        } else {
            // Local file database
            let path = url.strip_prefix("file:").unwrap_or(url);
            Builder::new_local(path)
                .build()
                .await
                .context("Failed to open local database")?
        };

        let conn = db.connect().context("Failed to get database connection")?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Initialize the database schema
    pub async fn initialize_schema(&self) -> Result<()> {
        let conn = self.conn.lock().await;

        // Episodes table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS episodes (
                id TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                actor TEXT,
                content TEXT NOT NULL,
                emotional_valence REAL DEFAULT 0.0,
                importance REAL DEFAULT 0.5,
                screen_context TEXT,
                embedding BLOB
            )
            "#,
            (),
        )
        .await?;

        // Spatial contexts table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS spatial_contexts (
                id TEXT PRIMARY KEY,
                context_type TEXT NOT NULL,
                context_value TEXT NOT NULL,
                last_seen INTEGER,
                visit_count INTEGER DEFAULT 1
            )
            "#,
            (),
        )
        .await?;

        // Memory-spatial links table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS memory_spatial_links (
                episode_id TEXT REFERENCES episodes(id),
                context_id TEXT REFERENCES spatial_contexts(id),
                strength REAL DEFAULT 1.0,
                PRIMARY KEY (episode_id, context_id)
            )
            "#,
            (),
        )
        .await?;

        // Character states table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS character_states (
                character_id TEXT PRIMARY KEY,
                current_mood TEXT DEFAULT 'neutral',
                last_spoke_at INTEGER,
                relationship_score REAL DEFAULT 0.5
            )
            "#,
            (),
        )
        .await?;

        // Chat messages table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS chat_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                sender TEXT NOT NULL,
                content TEXT NOT NULL,
                in_response_to INTEGER REFERENCES chat_messages(id)
            )
            "#,
            (),
        )
        .await?;

        // Arbiter decisions table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS arbiter_decisions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                should_respond INTEGER NOT NULL,
                responder_id TEXT,
                reasoning TEXT NOT NULL,
                urgency REAL,
                context_summary TEXT
            )
            "#,
            (),
        )
        .await?;

        // Create indices
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_episodes_timestamp ON episodes(timestamp DESC)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chat_messages_timestamp ON chat_messages(timestamp DESC)",
            (),
        )
        .await?;

        info!("Database schema initialized");
        Ok(())
    }

    /// Add an episode to memory
    pub async fn add_episode(&self, episode: &Episode) -> Result<()> {
        let conn = self.conn.lock().await;

        let screen_context_json = episode
            .screen_context
            .as_ref()
            .map(|sc| serde_json::to_string(sc))
            .transpose()?;

        conn.execute(
            r#"
            INSERT INTO episodes (id, timestamp, event_type, actor, content, emotional_valence, importance, screen_context)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                episode.id.clone(),
                episode.timestamp,
                episode.event_type.clone(),
                episode.actor.clone(),
                episode.content.clone(),
                episode.emotional_valence,
                episode.importance,
                screen_context_json,
            ],
        )
        .await?;

        debug!("Added episode: {}", episode.id);
        Ok(())
    }

    /// Get recent episodes
    pub async fn get_recent_episodes(&self, limit: usize) -> Result<Vec<Episode>> {
        let conn = self.conn.lock().await;

        let mut rows = conn
            .query(
                r#"
                SELECT id, timestamp, event_type, actor, content, emotional_valence, importance, screen_context
                FROM episodes
                ORDER BY timestamp DESC
                LIMIT ?1
                "#,
                params![limit as i64],
            )
            .await?;

        let mut episodes = Vec::new();
        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            let timestamp: i64 = row.get(1)?;
            let event_type: String = row.get(2)?;
            let actor: Option<String> = row.get(3)?;
            let content: String = row.get(4)?;
            let emotional_valence: f64 = row.get(5)?;
            let importance: f64 = row.get(6)?;
            let screen_context_str: Option<String> = row.get(7)?;

            let screen_context: Option<ScreenContext> =
                screen_context_str.and_then(|s| serde_json::from_str(&s).ok());

            episodes.push(Episode {
                id,
                timestamp,
                event_type,
                actor,
                content,
                emotional_valence: emotional_valence as f32,
                importance: importance as f32,
                screen_context,
                embedding: None,
            });
        }

        Ok(episodes)
    }

    /// Add a chat message
    pub async fn add_chat_message(&self, sender: &str, content: &str) -> Result<i64> {
        let conn = self.conn.lock().await;
        let timestamp = chrono::Utc::now().timestamp();

        conn.execute(
            r#"
            INSERT INTO chat_messages (timestamp, sender, content)
            VALUES (?1, ?2, ?3)
            "#,
            params![timestamp, sender.to_string(), content.to_string()],
        )
        .await?;

        // Get the inserted ID
        let mut rows = conn.query("SELECT last_insert_rowid()", ()).await?;

        let id: i64 = if let Some(row) = rows.next().await? {
            row.get(0)?
        } else {
            0
        };

        debug!("Added chat message from {}: {}", sender, content);
        Ok(id)
    }

    /// Get recent chat messages
    pub async fn get_recent_chat(&self, limit: usize) -> Result<Vec<ChatMessage>> {
        let conn = self.conn.lock().await;

        let mut rows = conn
            .query(
                r#"
                SELECT id, timestamp, sender, content, in_response_to
                FROM chat_messages
                ORDER BY timestamp DESC
                LIMIT ?1
                "#,
                params![limit as i64],
            )
            .await?;

        let mut messages = Vec::new();
        while let Some(row) = rows.next().await? {
            let id: i64 = row.get(0)?;
            let timestamp: i64 = row.get(1)?;
            let sender: String = row.get(2)?;
            let content: String = row.get(3)?;
            let in_response_to: Option<i64> = row.get(4)?;

            messages.push(ChatMessage {
                id,
                timestamp,
                sender,
                content,
                in_response_to,
            });
        }

        // Reverse to get chronological order
        messages.reverse();
        Ok(messages)
    }

    /// Log an arbiter decision
    pub async fn log_arbiter_decision(
        &self,
        should_respond: bool,
        responder_id: Option<&str>,
        reasoning: &str,
        urgency: f32,
        context_summary: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().await;
        let timestamp = chrono::Utc::now().timestamp();

        conn.execute(
            r#"
            INSERT INTO arbiter_decisions (timestamp, should_respond, responder_id, reasoning, urgency, context_summary)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                timestamp,
                should_respond as i32,
                responder_id.map(|s| s.to_string()),
                reasoning.to_string(),
                urgency as f64,
                context_summary.to_string(),
            ],
        )
        .await?;

        Ok(())
    }

    /// Get character state
    pub async fn get_character_state(&self, character_id: &str) -> Result<Option<CharacterState>> {
        let conn = self.conn.lock().await;

        let mut rows = conn
            .query(
                r#"
                SELECT character_id, current_mood, last_spoke_at, relationship_score
                FROM character_states
                WHERE character_id = ?1
                "#,
                params![character_id.to_string()],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            let character_id: String = row.get(0)?;
            let current_mood: String = row.get(1)?;
            let last_spoke_at: Option<i64> = row.get(2)?;
            let relationship_score: f64 = row.get(3)?;

            Ok(Some(CharacterState {
                character_id,
                current_mood,
                last_spoke_at,
                relationship_score: relationship_score as f32,
            }))
        } else {
            Ok(None)
        }
    }

    /// Update character state
    pub async fn update_character_state(&self, state: &CharacterState) -> Result<()> {
        let conn = self.conn.lock().await;

        conn.execute(
            r#"
            INSERT INTO character_states (character_id, current_mood, last_spoke_at, relationship_score)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(character_id) DO UPDATE SET
                current_mood = excluded.current_mood,
                last_spoke_at = excluded.last_spoke_at,
                relationship_score = excluded.relationship_score
            "#,
            params![
                state.character_id.clone(),
                state.current_mood.clone(),
                state.last_spoke_at,
                state.relationship_score as f64,
            ],
        )
        .await?;

        Ok(())
    }

    /// Decay importance of old memories
    pub async fn decay_importance(&self, decay_factor: f32, min_age_hours: i64) -> Result<u64> {
        let conn = self.conn.lock().await;
        let cutoff = chrono::Utc::now().timestamp() - (min_age_hours * 3600);

        let result = conn
            .execute(
                r#"
                UPDATE episodes 
                SET importance = importance * ?1 
                WHERE timestamp < ?2 AND importance > 0.01
                "#,
                params![decay_factor as f64, cutoff],
            )
            .await?;

        Ok(result)
    }

    /// Prune forgotten memories
    pub async fn prune_forgotten(&self, threshold: f32) -> Result<u64> {
        let conn = self.conn.lock().await;

        let result = conn
            .execute(
                "DELETE FROM episodes WHERE importance < ?1",
                params![threshold as f64],
            )
            .await?;

        Ok(result)
    }

    /// Get or create spatial context
    pub async fn get_or_create_spatial_context(
        &self,
        context_type: &str,
        context_value: &str,
    ) -> Result<SpatialContext> {
        let conn = self.conn.lock().await;
        let now = chrono::Utc::now().timestamp();

        // Try to get existing
        let mut rows = conn
            .query(
                r#"
                SELECT id, context_type, context_value, last_seen, visit_count
                FROM spatial_contexts
                WHERE context_type = ?1 AND context_value = ?2
                "#,
                params![context_type.to_string(), context_value.to_string()],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            let ctx_type: String = row.get(1)?;
            let ctx_value: String = row.get(2)?;
            let last_seen: i64 = row.get(3)?;
            let visit_count: i64 = row.get(4)?;

            // Drop the rows to release the connection for the update
            drop(rows);

            // Update visit count and last seen
            conn.execute(
                r#"
                UPDATE spatial_contexts 
                SET last_seen = ?1, visit_count = visit_count + 1
                WHERE id = ?2
                "#,
                params![now, id.clone()],
            )
            .await?;

            Ok(SpatialContext {
                id,
                context_type: ctx_type,
                context_value: ctx_value,
                last_seen,
                visit_count,
            })
        } else {
            // Drop the rows iterator
            drop(rows);

            // Create new
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                r#"
                INSERT INTO spatial_contexts (id, context_type, context_value, last_seen, visit_count)
                VALUES (?1, ?2, ?3, ?4, 1)
                "#,
                params![id.clone(), context_type.to_string(), context_value.to_string(), now],
            )
            .await?;

            Ok(SpatialContext {
                id,
                context_type: context_type.to_string(),
                context_value: context_value.to_string(),
                last_seen: now,
                visit_count: 1,
            })
        }
    }
}
