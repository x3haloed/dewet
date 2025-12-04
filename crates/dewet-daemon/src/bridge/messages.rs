use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Ping {
        nonce: Option<String>,
    },
    UserChat {
        text: String,
    },
    OpticalRenderResult {
        memory: String,
        chat: String,
        status: String,
    },
    /// ARIAOS rendered image from Godot
    AriaosRenderResult {
        image: String,
    },
    DebugCommand {
        command: String,
        #[serde(default)]
        payload: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonMessage {
    Hello {
        version: String,
        capabilities: Vec<String>,
    },
    Speak {
        character_id: String,
        text: String,
        audio_base64: Option<String>,
        #[serde(default)]
        puppet: Value,
    },
    React {
        character_id: String,
        expression: String,
    },
    RenderOpticalMemory {
        chat_history: Vec<ChatPacket>,
        memory_nodes: Vec<MemoryNode>,
    },
    /// Request ARIAOS render from Godot
    RenderAriaos {
        ariaos_state: Value,
    },
    /// Execute ARIAOS tool commands
    AriaosCommand {
        commands: Value,
    },
    /// Initialize ARIAOS state (sent on startup)
    AriaosInit {
        notes_content: String,
        notes_scroll: f32,
    },
    DecisionUpdate {
        decision: Value,
        observation: Value,
    },
    ObservationSnapshot {
        active_app: String,
        active_window: String,
        screen_summary: String,
        timestamp: i64,
    },
    VisionAnalysis {
        activity: String,
        warrants_response: bool,
        response_trigger: Option<String>,
        companion_interest: Value,
        timestamp: i64,
    },
    Log {
        level: String,
        message: String,
        timestamp: i64,
    },
    /// Debug log of prompt/response for Arbiter or Response model
    PromptLog {
        /// "arbiter" or "response"
        model_type: String,
        /// The model name used
        model_name: String,
        /// The full prompt text (images stripped)
        prompt: String,
        /// The model's response
        response: String,
        timestamp: i64,
    },
}

/// Memory tier for chat messages (Aria's "forgetting without amnesia")
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MemoryTier {
    #[default]
    Hot,   // Recent, highly relevant
    Warm,  // Somewhat recent, still relevant
    Cold,  // Old or low relevance, candidate for eviction
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatPacket {
    pub sender: String,
    pub content: String,
    pub timestamp: i64,
    /// Relevance score (0.0-1.0), decays over time
    #[serde(default = "ChatPacket::default_relevance")]
    pub relevance: f32,
    /// Memory tier based on relevance and recency
    #[serde(default)]
    pub tier: MemoryTier,
}

impl ChatPacket {
    fn default_relevance() -> f32 {
        1.0
    }
    
    /// Calculate age in seconds
    pub fn age_seconds(&self) -> i64 {
        chrono::Utc::now().timestamp() - self.timestamp
    }
    
    /// Update tier based on current relevance score
    pub fn update_tier(&mut self, forget_threshold: f32) {
        self.tier = if self.relevance >= 0.7 {
            MemoryTier::Hot
        } else if self.relevance >= forget_threshold {
            MemoryTier::Warm
        } else {
            MemoryTier::Cold
        };
    }
    
    /// Apply time-based decay to relevance
    pub fn apply_decay(&mut self, decay_rate: f32, minutes_elapsed: f32) {
        self.relevance *= decay_rate.powf(minutes_elapsed);
        self.relevance = self.relevance.clamp(0.0, 1.0);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryNode {
    pub id: String,
    pub label: String,
    pub weight: f32,
    #[serde(default)]
    pub metadata: Value,
}
