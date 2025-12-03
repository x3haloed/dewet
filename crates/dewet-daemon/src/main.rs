use std::{path::Path, sync::Arc};

use anyhow::Result;
use std::io::Cursor;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::Utc;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba, RgbaImage};
use serde_json::json;
use tokio::sync::Mutex;
use tracing::{error, info};

use dewet_daemon::{
    bridge::{Bridge, BridgeHandle, ChatPacket, ClientMessage, DaemonMessage, MemoryNode},
    character::{CharacterSpec, LoadedCharacter},
    config::AppConfig,
    director::{Decision, Director},
    llm,
    observation::ObservationBuffer,
    storage::Storage,
    tts,
    vision::{CompositeParts, CompositeRenderer, VisionPipeline},
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let config = AppConfig::load()?;
    info!("Starting Dewet daemon");

    let storage = Storage::connect(&config.storage).await?;
    let llm_client = llm::create_client(&config.llm);
    let synth = tts::create_synthesizer(&config.tts);

    let character_specs =
        CharacterSpec::load_dir(Path::new("characters")).unwrap_or_else(|_| CharacterSpec::demo());
    let characters = character_specs
        .into_iter()
        .map(LoadedCharacter::new)
        .collect::<Vec<_>>();

    let mut director = Director::new(
        storage.clone(),
        llm_client.clone(),
        config.director.clone(),
        config.llm.clone(),
        characters,
    );

    let mut bridge = Bridge::bind(config.bridge.clone()).await?;
    let bridge_handle = bridge.handle();

    let mut vision = VisionPipeline::new(config.vision.clone());
    let mut observation_buffer = ObservationBuffer::new(config.observation.clone());
    
    // Hydrate observation buffer with recent chat from database
    let recent_chat = storage.recent_chat(config.observation.chat_depth).await?;
    for packet in recent_chat {
        observation_buffer.record_chat(packet);
    }
    info!("Loaded {} chat messages from database", observation_buffer.chat_count());
    
    let composite_renderer = CompositeRenderer::default();

    let optical_assets = Arc::new(Mutex::new(OpticalAssets::default()));
    let capture_delay = vision.capture_interval();
    
    // Use a sleep that resets after each tick completes, rather than a fixed interval
    // This prevents backpressure when LLM calls take longer than the interval
    let mut next_tick = tokio::time::Instant::now();

    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(next_tick) => {
                let tick_start = std::time::Instant::now();
                if let Err(err) = perception_tick(
                    &mut vision,
                    &mut observation_buffer,
                    &mut director,
                    &bridge_handle,
                    &synth,
                    &storage,
                    &composite_renderer,
                    &optical_assets
                ).await {
                    error!(?err, "Perception tick failed");
                }
                let elapsed = tick_start.elapsed();
                info!("Perception tick completed in {:?}", elapsed);
                // Schedule next tick AFTER this one completes
                next_tick = tokio::time::Instant::now() + capture_delay;
            }
            next = bridge.next_message() => {
                if let Some(msg) = next {
                    if let Err(err) = handle_client_message(
                        msg,
                        &storage,
                        &mut observation_buffer,
                        &optical_assets,
                        &bridge_handle
                    ).await {
                        error!(?err, "Failed to handle client event");
                    }
                } else {
                    break;
                }
            }
        }
    }

    Ok(())
}

async fn perception_tick(
    vision: &mut VisionPipeline,
    buffer: &mut ObservationBuffer,
    director: &mut Director,
    bridge: &BridgeHandle,
    synth: &tts::SharedSynth,
    storage: &Storage,
    composite_renderer: &CompositeRenderer,
    optical_assets: &Arc<Mutex<OpticalAssets>>,
) -> Result<()> {
    let frame = vision.capture_frame()?;

    let optical = optical_assets.lock().await.clone();
    let composite_image = composite_renderer.render(&CompositeParts {
        desktop: frame.rgba(),
        memory_visualization: optical.memory,
        chat_transcript: optical.chat,
        character_status: optical.status,
    });

    // Ingest screen with composite for vision analysis
    let observation = buffer.ingest_screen(frame, Some(composite_image.clone()));

    bridge.broadcast(DaemonMessage::ObservationSnapshot {
        active_app: "unknown".into(),
        active_window: "unknown".into(),
        screen_summary: observation.screen_summary.notes.clone(),
        timestamp: Utc::now().timestamp(),
    })?;

    let (decision, vision_analysis) = director.evaluate(&observation).await?;

    // Broadcast vision analysis if available
    if let Some(ref analysis) = vision_analysis {
        bridge.broadcast(DaemonMessage::VisionAnalysis {
            activity: analysis.activity.clone(),
            warrants_response: analysis.warrants_response,
            response_trigger: analysis.response_trigger.clone(),
            companion_interest: analysis.companion_interest.clone(),
            timestamp: Utc::now().timestamp(),
        })?;

        log_event(
            bridge,
            "debug",
            format!(
                "VLM: {} (warrants_response={})",
                analysis.activity, analysis.warrants_response
            ),
        );
    }

    match decision {
        Decision::Pass => {}
        Decision::Speak {
            character_id,
            text,
            urgency,
            suggested_mood,
        } => {
            bridge.broadcast(DaemonMessage::DecisionUpdate {
                decision: json!({
                    "should_respond": true,
                    "responder_id": character_id,
                    "reasoning": "LLM approved",
                    "urgency": urgency,
                    "suggested_mood": suggested_mood
                }),
                observation: json!({
                    "screen_summary": observation.screen_summary.notes
                }),
            })?;

            // Record the assistant's response in chat history so future prompts see it
            let assistant_packet = ChatPacket {
                sender: character_id.clone(),
                content: text.clone(),
                timestamp: Utc::now().timestamp(),
            };
            storage.record_chat(&assistant_packet).await?;
            buffer.record_chat(assistant_packet);

            let audio = synth.synthesize(&text)?;
            let audio_b64 = BASE64.encode(audio);
            bridge.broadcast(DaemonMessage::Speak {
                character_id,
                text,
                audio_base64: Some(audio_b64),
                puppet: serde_json::json!({
                    "mood": suggested_mood.unwrap_or_else(|| "neutral".into()),
                    "urgency": urgency
                }),
            })?;

            log_event(
                bridge,
                "info",
                format!("Arbiter response queued (urgency {urgency:.2})"),
            );
        }
    }

    bridge.broadcast(DaemonMessage::RenderOpticalMemory {
        chat_history: storage.recent_chat(10).await?,
        memory_nodes: vec![MemoryNode {
            id: "focus".into(),
            label: "Recent activity".into(),
            weight: 0.8,
            metadata: serde_json::json!({
                "summary": observation.screen_summary.notes
            }),
        }],
    })?;

    // Persist composite snapshot for the debug window
    let composite_b64 = encode_image_base64(&composite_image)?;
    bridge.broadcast(DaemonMessage::DecisionUpdate {
        decision: serde_json::json!({"composite": composite_b64}),
        observation: serde_json::json!({ "kind": "composite" }),
    })?;

    Ok(())
}

async fn handle_client_message(
    message: ClientMessage,
    storage: &Storage,
    buffer: &mut ObservationBuffer,
    optical_assets: &Arc<Mutex<OpticalAssets>>,
    bridge: &BridgeHandle,
) -> Result<()> {
    match message {
        ClientMessage::Ping { nonce } => {
            bridge.broadcast(DaemonMessage::DecisionUpdate {
                decision: serde_json::json!({ "ping": nonce }),
                observation: serde_json::json!({ "type": "ping" }),
            })?;

            log_event(bridge, "debug", "Ping received from client");
        }
        ClientMessage::UserChat { text } => {
            let packet = ChatPacket {
                sender: "user".into(),
                content: text,
                timestamp: Utc::now().timestamp(),
            };
            storage.record_chat(&packet).await?;
            buffer.record_chat(packet.clone());
            bridge.broadcast(DaemonMessage::DecisionUpdate {
                decision: serde_json::to_value(&packet)?,
                observation: serde_json::json!({ "type": "user_chat" }),
            })?;

            log_event(
                bridge,
                "info",
                format!("User message stored: {}", packet.content),
            );
        }
        ClientMessage::OpticalRenderResult {
            memory,
            chat,
            status,
        } => {
            let mut assets = optical_assets.lock().await;
            if let Some(img) = decode_png(&memory) {
                assets.memory = img;
            }
            if let Some(img) = decode_png(&chat) {
                assets.chat = img;
            }
            if let Some(img) = decode_png(&status) {
                assets.status = img;
            }
        }
        ClientMessage::DebugCommand { command, payload } => {
            bridge.broadcast(DaemonMessage::DecisionUpdate {
                decision: serde_json::json!({ "debug_command": command, "payload": payload }),
                observation: serde_json::json!({ "type": "debug_command" }),
            })?;
        }
    }
    Ok(())
}

fn decode_png(b64: &str) -> Option<image::RgbaImage> {
    let bytes = BASE64.decode(b64).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    Some(img.to_rgba8())
}

fn log_event(bridge: &BridgeHandle, level: &str, message: impl Into<String>) {
    let _ = bridge.broadcast(DaemonMessage::Log {
        level: level.to_string(),
        message: message.into(),
        timestamp: Utc::now().timestamp(),
    });
}

fn encode_image_base64(image: &RgbaImage) -> Result<String> {
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer);
    DynamicImage::ImageRgba8(image.clone()).write_to(&mut cursor, ImageFormat::Png)?;
    Ok(BASE64.encode(buffer))
}

#[derive(Clone)]
struct OpticalAssets {
    memory: image::RgbaImage,
    chat: image::RgbaImage,
    status: image::RgbaImage,
}

impl Default for OpticalAssets {
    fn default() -> Self {
        let blank = ImageBuffer::from_pixel(512, 512, Rgba([0, 0, 0, 255]));
        Self {
            memory: blank.clone(),
            chat: blank.clone(),
            status: blank,
        }
    }
}
