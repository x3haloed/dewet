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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatPacket {
    pub sender: String,
    pub content: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryNode {
    pub id: String,
    pub label: String,
    pub weight: f32,
    #[serde(default)]
    pub metadata: Value,
}
