//! Storage layer using Turso (libSQL)

mod turso;

pub use turso::TursoDb;

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{bridge::ChatPacket, config::StorageConfig};

/// Episode memory - the "what happened" log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub timestamp: i64,
    pub event_type: String,
    pub actor: Option<String>,
    pub content: String,
    pub emotional_valence: f32,
    pub importance: f32,
    pub screen_context: Option<ScreenContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}

/// Screen context at time of episode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenContext {
    pub active_window: String,
    pub active_app: String,
}

/// Spatial context for memory association
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpatialContext {
    pub id: String,
    pub context_type: String,
    pub context_value: String,
    pub last_seen: i64,
    pub visit_count: i64,
}

/// Character runtime state (not the static definition)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterState {
    pub character_id: String,
    pub current_mood: String,
    pub last_spoke_at: Option<i64>,
    pub relationship_score: f32,
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: i64,
    pub timestamp: i64,
    pub sender: String,
    pub content: String,
    pub in_response_to: Option<i64>,
}

/// Arbiter decision log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbiterDecisionLog {
    pub id: i64,
    pub timestamp: i64,
    pub should_respond: bool,
    pub responder_id: Option<String>,
    pub reasoning: String,
    pub urgency: f32,
    pub context_summary: String,
}

/// High-level storage wrapper that the daemon uses.
#[derive(Clone)]
pub struct Storage {
    db: TursoDb,
}

impl Storage {
    pub async fn connect(config: &StorageConfig) -> Result<Self> {
        let token = std::env::var(&config.auth_token_env).ok();
        let db = TursoDb::connect(&config.url, token.as_deref()).await?;
        db.initialize_schema().await?;
        Ok(Self { db })
    }

    pub async fn record_chat(&self, packet: &ChatPacket) -> Result<()> {
        self.db
            .add_chat_message(&packet.sender, &packet.content)
            .await?;
        Ok(())
    }

    pub async fn recent_chat(&self, limit: usize) -> Result<Vec<ChatPacket>> {
        use crate::bridge::MemoryTier;
        
        let messages = self.db.get_recent_chat(limit).await?;
        Ok(messages
            .into_iter()
            .map(|msg| ChatPacket {
                sender: msg.sender,
                content: msg.content,
                timestamp: msg.timestamp,
                relevance: 1.0,  // Fresh from DB = full relevance
                tier: MemoryTier::Hot,
            })
            .collect())
    }

    pub async fn record_decision(&self, decision: &StoredDecision) -> Result<()> {
        self.db
            .log_arbiter_decision(
                decision.should_respond,
                decision.responder_id.as_deref(),
                &decision.reasoning,
                decision.urgency,
                &decision.context_summary,
            )
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StoredDecision {
    pub timestamp: i64,
    pub should_respond: bool,
    pub responder_id: Option<String>,
    pub reasoning: String,
    pub urgency: f32,
    pub context_summary: String,
}

impl StoredDecision {
    pub fn now(
        should_respond: bool,
        responder_id: Option<String>,
        reasoning: impl Into<String>,
        urgency: f32,
    ) -> Self {
        Self {
            timestamp: Utc::now().timestamp(),
            should_respond,
            responder_id,
            reasoning: reasoning.into(),
            urgency,
            context_summary: String::new(),
        }
    }
}
