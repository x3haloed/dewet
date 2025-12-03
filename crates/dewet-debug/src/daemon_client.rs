//! WebSocket client for connecting to Dewet daemon

use anyhow::Result;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use crate::{ArbiterDecision, LogEntry};

/// Event emitted from daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonEvent {
    Connected,
    Disconnected,
    Log(LogEntry),
    ArbiterDecision(ArbiterDecision),
    VisionAnalysis(VisionAnalysis),
    ScreenCapture {
        image_base64: String,
        active_window: String,
        active_app: String,
    },
    Speak {
        character_id: String,
        text: String,
    },
}

/// Vision analysis from VLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionAnalysis {
    pub activity: String,
    pub warrants_response: bool,
    pub response_trigger: Option<String>,
    pub companion_interest: serde_json::Value,
    pub timestamp: i64,
}

/// Client for communicating with the Dewet daemon
pub struct DaemonClient {
    connected: bool,
    tx: Option<mpsc::UnboundedSender<String>>,
    recent_logs: Arc<RwLock<VecDeque<LogEntry>>>,
    recent_decisions: Arc<RwLock<VecDeque<ArbiterDecision>>>,
    event_handler: Option<Arc<dyn Fn(DaemonEvent) + Send + Sync>>,
}

impl DaemonClient {
    pub fn new() -> Self {
        Self {
            connected: false,
            tx: None,
            recent_logs: Arc::new(RwLock::new(VecDeque::with_capacity(100))),
            recent_decisions: Arc::new(RwLock::new(VecDeque::with_capacity(50))),
            event_handler: None,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn set_event_handler<F>(&mut self, handler: F)
    where
        F: Fn(DaemonEvent) + Send + Sync + 'static,
    {
        self.event_handler = Some(Arc::new(handler));
    }

    pub async fn connect(&mut self, url: &str) -> Result<()> {
        use tokio_tungstenite::connect_async;

        let (ws_stream, _) = connect_async(url).await?;
        let (mut write, mut read) = ws_stream.split();

        // Create channel for sending messages
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        self.tx = Some(tx);

        self.connected = true;

        if let Some(ref handler) = self.event_handler {
            handler(DaemonEvent::Connected);
        }

        // Spawn read task
        let event_handler = self.event_handler.clone();
        let log_store = self.recent_logs.clone();
        let decision_store = self.recent_decisions.clone();
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            if let Some(event) = map_wire_message(&value) {
                                if let DaemonEvent::Log(entry) = &event {
                                    push_bounded(log_store.clone(), entry.clone(), 200).await;
                                } else if let DaemonEvent::ArbiterDecision(entry) = &event {
                                    push_bounded(decision_store.clone(), entry.clone(), 50).await;
                                }

                                if let Some(ref handler) = event_handler {
                                    handler(event);
                                }
                            }
                        }
                    }
                    Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                        if let Some(ref handler) = event_handler {
                            handler(DaemonEvent::Disconnected);
                        }
                        break;
                    }
                    Err(_) => break,
                    _ => {}
                }
            }
        });

        // Spawn write task
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if write
                    .send(tokio_tungstenite::tungstenite::Message::Text(msg))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        Ok(())
    }

    pub async fn force_speak(&self, character_id: &str, text: Option<&str>) -> Result<()> {
        if let Some(ref tx) = self.tx {
            let mut msg = serde_json::json!({
                "type": "force_speak",
                "character_id": character_id,
            });
            if let Some(t) = text {
                msg["text"] = serde_json::Value::String(t.to_string());
            }
            tx.send(msg.to_string())?;
        }
        Ok(())
    }

    pub async fn reset_cooldowns(&self) -> Result<()> {
        if let Some(ref tx) = self.tx {
            let msg = serde_json::json!({"type": "reset_cooldowns"}).to_string();
            tx.send(msg)?;
        }
        Ok(())
    }

    pub async fn recent_logs(&self) -> Vec<LogEntry> {
        let store = self.recent_logs.read().await;
        store.iter().cloned().collect()
    }

    pub async fn recent_decisions(&self) -> Vec<ArbiterDecision> {
        let store = self.recent_decisions.read().await;
        store.iter().cloned().collect()
    }

}

async fn push_bounded<T: Clone>(
    store: Arc<RwLock<VecDeque<T>>>,
    entry: T,
    max_len: usize,
) {
    let mut guard = store.write().await;
    if guard.len() >= max_len {
        guard.pop_front();
    }
    guard.push_back(entry);
}

fn map_wire_message(value: &Value) -> Option<DaemonEvent> {
    let msg_type = value.get("type")?.as_str()?;
    match msg_type {
        "hello" => Some(DaemonEvent::Connected),
        "speak" => Some(DaemonEvent::Speak {
            character_id: value
                .get("character_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            text: value
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        }),
        "vision_analysis" => Some(DaemonEvent::VisionAnalysis(VisionAnalysis {
            activity: value
                .get("activity")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            warrants_response: value
                .get("warrants_response")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            response_trigger: value
                .get("response_trigger")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            companion_interest: value
                .get("companion_interest")
                .cloned()
                .unwrap_or(serde_json::json!({})),
            timestamp: value
                .get("timestamp")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(|| Utc::now().timestamp()),
        })),
        "decision_update" => {
            if let Some(decision) = value.get("decision") {
                if let Some(image) = decision.get("composite").and_then(|v| v.as_str()) {
                    return Some(DaemonEvent::ScreenCapture {
                        image_base64: image.to_string(),
                        active_window: String::new(),
                        active_app: String::new(),
                    });
                }

                if let Some(should) = decision.get("should_respond").and_then(|v| v.as_bool()) {
                    let responder = decision
                        .get("responder_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let reasoning = decision
                        .get("reasoning")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let urgency = decision
                        .get("urgency")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0) as f32;
                    return Some(DaemonEvent::ArbiterDecision(ArbiterDecision {
                        should_respond: should,
                        responder_id: responder,
                        reasoning,
                        urgency,
                        timestamp: Utc::now().timestamp(),
                    }));
                }
            }
            None
        }
        "log" => Some(DaemonEvent::Log(LogEntry {
            level: value
                .get("level")
                .and_then(|v| v.as_str())
                .unwrap_or("info")
                .to_string(),
            message: value
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            timestamp: value
                .get("timestamp")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(|| Utc::now().timestamp()),
        })),
        "observation_snapshot" => Some(DaemonEvent::ScreenCapture {
            image_base64: String::new(),
            active_window: value
                .get("active_window")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            active_app: value
                .get("active_app")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        }),
        _ => None,
    }
}

