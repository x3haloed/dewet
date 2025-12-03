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
    ariaos::{self, AriaosCommand, NotesAction},
    bridge::{Bridge, BridgeHandle, ChatPacket, ClientMessage, DaemonMessage, MemoryNode, MemoryTier},
    character::{CharacterSpec, LoadedCharacter},
    config::AppConfig,
    director::{Decision, Director},
    llm,
    observation::ObservationBuffer,
    storage::{AriaosNotesState, Storage},
    tts,
    vision::{CompositeParts, CompositeRenderer, VisionPipeline},
};

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if present (before reading config)
    dotenvy::dotenv().ok();
    
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
    let ariaos_assets = Arc::new(Mutex::new(AriaosAssets::default()));
    
    // Load ARIAOS notes state from database
    let initial_notes = storage.load_ariaos_notes().await?.unwrap_or_default();
    info!("Loaded ARIAOS notes ({} chars)", initial_notes.content.len());
    let notes_state = Arc::new(Mutex::new(initial_notes));
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
                    &optical_assets,
                    &ariaos_assets,
                    &notes_state,
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
                        &ariaos_assets,
                        &notes_state,
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
    ariaos_assets: &Arc<Mutex<AriaosAssets>>,
    notes_state: &Arc<Mutex<AriaosNotesState>>,
) -> Result<()> {
    // Flush any pending user messages into chat history before processing
    let pending_messages = buffer.flush_pending_messages();
    if !pending_messages.is_empty() {
        log_event(
            bridge,
            "info",
            format!("Flushed {} pending user message(s) into chat history", pending_messages.len()),
        );
    }
    
    // Apply relevance decay based on time elapsed (assume ~capture_interval between ticks)
    let minutes_elapsed = vision.capture_interval().as_secs_f32() / 60.0;
    buffer.apply_relevance_decay(minutes_elapsed);
    
    // Log tier distribution occasionally
    let (hot, warm, cold) = buffer.tier_stats();
    if hot + warm + cold > 0 {
        log_event(
            bridge,
            "debug",
            format!("Memory tiers: {} hot, {} warm, {} cold", hot, warm, cold),
        );
    }
    
    let frame = vision.capture_frame()?;

    let optical = optical_assets.lock().await.clone();
    
    // Get historical approved screenshots for context
    let composite_image = {
        let approved = buffer.approved_screenshots();
        let history: Vec<&image::RgbaImage> = approved
            .iter()
            .map(|s| &s.image)
            .collect();
        
        // Render composite with history if available
        composite_renderer.render_with_history(
            &CompositeParts {
                desktop: frame.rgba(),
                memory_visualization: optical.memory,
                chat_transcript: optical.chat,
                character_status: optical.status,
            },
            &history,
        )
    };

    // Get ARIAOS composite (with history) for VLM
    let ariaos_image = {
        let assets = ariaos_assets.lock().await;
        Some(assets.render_composite())
    };

    // Ingest screen with composite and ARIAOS for vision analysis
    let observation = buffer.ingest_screen(frame, Some(composite_image.clone()), ariaos_image);

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
            // Parse ARIAOS DSL commands from the response
            log_event(
                bridge,
                "debug",
                format!("Checking response for DSL commands: {}", &text[..text.floor_char_boundary(200)]),
            );
            let dsl_commands = ariaos::parse_commands(&text);
            let clean_text = if dsl_commands.is_empty() {
                log_event(bridge, "debug", "No DSL commands found in response");
                text.clone()
            } else {
                log_event(
                    bridge,
                    "info",
                    format!("Parsed {} ARIAOS DSL command(s): {:?}", dsl_commands.len(), dsl_commands),
                );
                
                // Update local notes state and persist
                {
                    let mut notes = notes_state.lock().await;
                    apply_notes_commands(&dsl_commands, &mut notes);
                    storage.save_ariaos_notes(&notes).await?;
                }
                
                // Send DSL commands to Godot for execution
                bridge.broadcast(DaemonMessage::AriaosCommand {
                    commands: serde_json::to_value(&dsl_commands)?,
                })?;
                // Strip DSL from text for TTS/display
                ariaos::strip_commands(&text)
            };
            
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
                content: clean_text.clone(),
                timestamp: Utc::now().timestamp(),
                relevance: 1.0,
                tier: MemoryTier::Hot,
            };
            storage.record_chat(&assistant_packet).await?;
            buffer.record_chat(assistant_packet);
            
            // Record this screenshot as an approved one for visual history
            buffer.record_approved_screenshot(composite_image.clone());
            
            // Record ARIAOS snapshot for history
            ariaos_assets.lock().await.record_approved();

            let audio = synth.synthesize(&clean_text)?;
            let audio_b64 = BASE64.encode(audio);
            bridge.broadcast(DaemonMessage::Speak {
                character_id,
                text: clean_text,
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

    // Send chat with tier info to Godot for visual rendering (fade cold messages)
    bridge.broadcast(DaemonMessage::RenderOpticalMemory {
        chat_history: observation.all_chat.clone(),
        memory_nodes: vec![MemoryNode {
            id: "focus".into(),
            label: "Recent activity".into(),
            weight: 0.8,
            metadata: serde_json::json!({
                "summary": observation.screen_summary.notes
            }),
        }],
    })?;
    
    // Request ARIAOS render from Godot
    bridge.broadcast(DaemonMessage::RenderAriaos {
        ariaos_state: serde_json::json!({
            "activity": observation.screen_summary.notes,
            "timestamp": Utc::now().timestamp()
        }),
    })?;

    
    // Persist composite snapshot for the debug window
    let composite_b64 = encode_image_base64(&composite_image)?;
    bridge.broadcast(DaemonMessage::DecisionUpdate {
        decision: serde_json::json!({"composite": composite_b64}),
        observation: serde_json::json!({ "kind": "composite" }),
    })?;
    
    // Send ARIAOS composite (with history) to debug window
    {
        let assets = ariaos_assets.lock().await;
        let ariaos_composite = assets.render_composite();
        let ariaos_b64 = encode_image_base64(&ariaos_composite)?;
        bridge.broadcast(DaemonMessage::DecisionUpdate {
            decision: serde_json::json!({"ariaos": ariaos_b64}),
            observation: serde_json::json!({ "kind": "ariaos" }),
        })?;
    }

    Ok(())
}

async fn handle_client_message(
    message: ClientMessage,
    storage: &Storage,
    buffer: &mut ObservationBuffer,
    optical_assets: &Arc<Mutex<OpticalAssets>>,
    ariaos_assets: &Arc<Mutex<AriaosAssets>>,
    notes_state: &Arc<Mutex<AriaosNotesState>>,
    bridge: &BridgeHandle,
) -> Result<()> {
    match message {
        ClientMessage::Ping { nonce } => {
            // Send ARIAOS init state to newly connected client
            let notes = notes_state.lock().await;
            bridge.broadcast(DaemonMessage::AriaosInit {
                notes_content: notes.content.clone(),
                notes_scroll: notes.scroll_offset,
            })?;
            
            bridge.broadcast(DaemonMessage::DecisionUpdate {
                decision: serde_json::json!({ "ping": nonce }),
                observation: serde_json::json!({ "type": "ping" }),
            })?;

            log_event(bridge, "debug", "Ping received, sent ARIAOS init state");
        }
        ClientMessage::UserChat { text } => {
            let packet = ChatPacket {
                sender: "user".into(),
                content: text,
                timestamp: Utc::now().timestamp(),
                relevance: 1.0,
                tier: MemoryTier::Hot,
            };
            // Store in DB immediately for persistence
            storage.record_chat(&packet).await?;
            // Queue for batching - will be added to chat history at next perception tick
            buffer.queue_user_message(packet.clone());
            bridge.broadcast(DaemonMessage::DecisionUpdate {
                decision: serde_json::to_value(&packet)?,
                observation: serde_json::json!({ "type": "user_chat" }),
            })?;

            log_event(
                bridge,
                "info",
                format!("User message queued (pending: {}): {}", buffer.pending_message_count(), packet.content),
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
        ClientMessage::AriaosRenderResult { image } => {
            if let Some(img) = decode_png(&image) {
                let mut assets = ariaos_assets.lock().await;
                assets.current = img;
                log_event(bridge, "debug", "ARIAOS render received");
            }
        }
        ClientMessage::DebugCommand { command, payload } => {
            match command.as_str() {
                "exec_dsl" => {
                    // Execute DSL commands directly for testing
                    // payload should be { "text": "ariaos.apps.notes.set_content(\"test\")" }
                    if let Some(text) = payload.get("text").and_then(|v| v.as_str()) {
                        let dsl_commands = ariaos::parse_commands(text);
                        if dsl_commands.is_empty() {
                            log_event(bridge, "warn", format!("No DSL commands found in: {}", text));
                        } else {
                            log_event(
                                bridge,
                                "info",
                                format!("Debug exec: {} DSL command(s)", dsl_commands.len()),
                            );
                            
                            // Update local notes state and persist
                            {
                                let mut notes = notes_state.lock().await;
                                apply_notes_commands(&dsl_commands, &mut notes);
                                storage.save_ariaos_notes(&notes).await?;
                            }
                            
                            bridge.broadcast(DaemonMessage::AriaosCommand {
                                commands: serde_json::to_value(&dsl_commands)?,
                            })?;
                        }
                    }
                }
                _ => {
                    bridge.broadcast(DaemonMessage::DecisionUpdate {
                        decision: serde_json::json!({ "debug_command": command, "payload": payload }),
                        observation: serde_json::json!({ "type": "debug_command" }),
                    })?;
                }
            }
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

/// Apply ARIAOS DSL commands to notes state (for persistence)
fn apply_notes_commands(commands: &[AriaosCommand], notes: &mut AriaosNotesState) {
    for cmd in commands {
        match cmd {
            AriaosCommand::Notes(action) => match action {
                NotesAction::SetContent(content) => {
                    notes.content = content.clone();
                    notes.scroll_offset = 0.0;
                }
                NotesAction::Append(content) => {
                    if notes.content.is_empty() {
                        notes.content = content.clone();
                    } else {
                        notes.content.push('\n');
                        notes.content.push_str(content);
                    }
                }
                NotesAction::Clear => {
                    notes.content.clear();
                    notes.scroll_offset = 0.0;
                }
                NotesAction::ScrollUp => {
                    notes.scroll_offset = (notes.scroll_offset - 100.0).max(0.0);
                }
                NotesAction::ScrollDown => {
                    notes.scroll_offset += 100.0;
                }
                NotesAction::ScrollToTop => {
                    notes.scroll_offset = 0.0;
                }
                NotesAction::ScrollToBottom => {
                    notes.scroll_offset = f32::MAX; // Will be clamped by Godot
                }
            },
        }
    }
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

/// ARIAOS assets - the companion's self-managed display
#[derive(Clone)]
struct AriaosAssets {
    /// Current rendered ARIAOS image (1024x768)
    current: image::RgbaImage,
    /// Historical approved snapshots (captured when Aria responds)
    approved_history: Vec<image::RgbaImage>,
    /// Max history to keep
    max_history: usize,
}

impl Default for AriaosAssets {
    fn default() -> Self {
        let blank = ImageBuffer::from_pixel(1024, 768, Rgba([15, 20, 30, 255]));
        Self {
            current: blank,
            approved_history: Vec::new(),
            max_history: 4,
        }
    }
}

impl AriaosAssets {
    /// Record current ARIAOS as an approved snapshot (call when Aria responds)
    fn record_approved(&mut self) {
        self.approved_history.insert(0, self.current.clone());
        if self.approved_history.len() > self.max_history {
            self.approved_history.pop();
        }
    }
    
    /// Render composite with current ARIAOS + history filmstrip
    /// Layout: [CURRENT (large)] [PREV 1]
    ///                           [PREV 2]
    ///                           [PREV 3]
    fn render_composite(&self) -> RgbaImage {
        use image::imageops::{resize, FilterType};
        
        if self.approved_history.is_empty() {
            // No history, just return current
            return self.current.clone();
        }
        
        // Layout: current takes 75%, history filmstrip takes 25%
        let total_width = 1536u32;  // Wider to accommodate history
        let total_height = 768u32;
        let current_width = (total_width * 3) / 4;  // 75%
        let history_width = total_width - current_width;  // 25%
        
        let mut canvas = ImageBuffer::from_pixel(total_width, total_height, Rgba([15, 20, 30, 255]));
        
        // Draw current ARIAOS (scaled to fit left portion)
        let current_scaled = resize(&self.current, current_width, total_height, FilterType::CatmullRom);
        for (x, y, pixel) in current_scaled.enumerate_pixels() {
            if x < canvas.width() && y < canvas.height() {
                canvas.put_pixel(x, y, *pixel);
            }
        }
        
        // Draw history filmstrip on the right
        let hist_count = self.approved_history.len().min(3);
        let hist_panel_height = total_height / 3;
        
        for (i, hist_img) in self.approved_history.iter().take(3).enumerate() {
            let y_offset = (i as u32) * hist_panel_height;
            let hist_scaled = resize(hist_img, history_width, hist_panel_height, FilterType::CatmullRom);
            
            for (x, y, pixel) in hist_scaled.enumerate_pixels() {
                let tx = current_width + x;
                let ty = y_offset + y;
                if tx < canvas.width() && ty < canvas.height() {
                    canvas.put_pixel(tx, ty, *pixel);
                }
            }
            
            // Draw label
            Self::draw_label(&mut canvas, current_width + 4, y_offset + 12, &format!("PREV {}", i + 1));
        }
        
        // Fill remaining slots with placeholder
        for i in hist_count..3 {
            let y_offset = (i as u32) * hist_panel_height;
            Self::draw_label(&mut canvas, current_width + 4, y_offset + 12, "NO HIST");
        }
        
        // Draw "ARIAOS" label on current
        Self::draw_label(&mut canvas, 8, 12, "ARIAOS");
        
        canvas
    }
    
    fn draw_label(canvas: &mut RgbaImage, x: u32, y: u32, text: &str) {
        // Simple text rendering (reuse the same approach as composite.rs)
        let mut cursor = x;
        for ch in text.chars() {
            if let Some(pattern) = Self::glyph_pattern(ch) {
                for (row, bits) in pattern.iter().enumerate() {
                    for col in 0..5 {
                        if (bits >> (4 - col)) & 1 == 1 {
                            let px = cursor + col as u32;
                            let py = y + row as u32;
                            if px < canvas.width() && py < canvas.height() {
                                canvas.put_pixel(px, py, Rgba([255, 255, 255, 255]));
                            }
                        }
                    }
                }
            }
            cursor += 6;
        }
    }
    
    fn glyph_pattern(ch: char) -> Option<&'static [u8; 7]> {
        match ch {
            'A' => Some(&[0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
            'I' => Some(&[0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111]),
            'O' => Some(&[0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
            'R' => Some(&[0b11110, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001, 0b10001]),
            'S' => Some(&[0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110]),
            'P' => Some(&[0b11110, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000, 0b10000]),
            'E' => Some(&[0b11111, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000, 0b11111]),
            'V' => Some(&[0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100]),
            'N' => Some(&[0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001]),
            'H' => Some(&[0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001, 0b10001]),
            'T' => Some(&[0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100]),
            '1' => Some(&[0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111]),
            '2' => Some(&[0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111]),
            '3' => Some(&[0b01110, 0b10001, 0b00001, 0b00110, 0b00001, 0b10001, 0b01110]),
            ' ' => Some(&[0, 0, 0, 0, 0, 0, 0]),
            _ => None,
        }
    }
}
