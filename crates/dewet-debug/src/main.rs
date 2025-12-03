//! Dewet Debug Window
//!
//! A Tauri-based developer tool for introspecting and controlling the Dewet daemon.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Emitter, State};
use tokio::sync::RwLock;

mod daemon_client;

use daemon_client::DaemonClient;

/// Application state shared across commands
struct AppState {
    client: Arc<RwLock<DaemonClient>>,
}

/// Log entry from daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub level: String,
    pub message: String,
    pub timestamp: i64,
}

/// Arbiter decision from daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbiterDecision {
    pub should_respond: bool,
    pub responder_id: Option<String>,
    pub reasoning: String,
    pub urgency: f32,
    pub timestamp: i64,
}

/// Get connection status
#[tauri::command]
async fn get_connection_status(state: State<'_, AppState>) -> Result<bool, String> {
    let client = state.client.read().await;
    Ok(client.is_connected())
}

/// Connect to daemon
#[tauri::command]
async fn connect_to_daemon(state: State<'_, AppState>, url: String) -> Result<(), String> {
    let mut client = state.client.write().await;
    client.connect(&url).await.map_err(|e| e.to_string())
}

/// Force a character to speak
#[tauri::command]
async fn force_speak(
    state: State<'_, AppState>,
    character_id: String,
    text: Option<String>,
) -> Result<(), String> {
    let client = state.client.read().await;
    client
        .force_speak(&character_id, text.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Reset character cooldowns
#[tauri::command]
async fn reset_cooldowns(state: State<'_, AppState>) -> Result<(), String> {
    let client = state.client.read().await;
    client.reset_cooldowns().await.map_err(|e| e.to_string())
}

/// Get recent logs
#[tauri::command]
async fn get_recent_logs(state: State<'_, AppState>) -> Result<Vec<LogEntry>, String> {
    let client = state.client.read().await;
    Ok(client.recent_logs().await)
}

/// Get recent arbiter decisions
#[tauri::command]
async fn get_recent_decisions(state: State<'_, AppState>) -> Result<Vec<ArbiterDecision>, String> {
    let client = state.client.read().await;
    Ok(client.recent_decisions().await)
}

fn main() {
    let client = Arc::new(RwLock::new(DaemonClient::new()));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            client: client.clone(),
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            let client_clone = client.clone();

            // Start background connection task
            tauri::async_runtime::spawn(async move {
                let mut client = client_clone.write().await;
                if let Err(e) = client.connect("ws://127.0.0.1:7777").await {
                    eprintln!("Failed to connect to daemon: {}", e);
                }

                // Set up message forwarding to frontend
                client.set_event_handler(move |event| {
                    let _ = handle.emit("daemon-event", event);
                });
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_connection_status,
            connect_to_daemon,
            force_speak,
            reset_cooldowns,
            get_recent_logs,
            get_recent_decisions,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
