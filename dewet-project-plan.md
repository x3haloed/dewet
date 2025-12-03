# Dewet Architecture

> *Dewet* — a phonetic respelling of "duet"

## Overview

**Dewet** is a desktop buddy app that watches what you're doing, maintains persistent character-driven conversations, and offers contextually-aware commentary through one or more AI personalities.

refs:
substrate: /Users/chad/Repos/substrate
vtube-bot: /Users/chad/Repos/vtube-bot
Vampire: /Users/chad/Repos/vampire

---

## Core Design Principles

Drawing from your existing projects:

| Substrate | vtube-bot | Vampire | Dewet |
|-----------|-----------|---------|-------------------|
| OpticalMemory (image-based context) | Planner (speak/wait decisions) | Event batching + vector search | All of the above, unified |
| EditorEngine (self-auditing) | Character Card v2 + lorebook | MCP tool integration | Multi-character with shared memory |
| Turn-based action queue | Puppet animations + TTS | Streaming action dispatch | Real-time observation loop |

---

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              Operating System                                    │
├────────────────────────┬────────────────────────┬───────────────────────────────┤
│                        │                        │                               │
│   ┌────────────────┐   │   ┌────────────────┐   │   ┌─────────────────────────┐ │
│   │  Debug Window  │   │   │  Puppet Window │   │   │      Dewet Daemon       │ │
│   │    (Tauri)     │   │   │    (Godot)     │   │   │        (Rust)           │ │
│   │                │   │   │                │   │   │                         │ │
│   │ • Dev console  │   │   │ • Avatar/Live2D│   │   │ • Screen capture        │ │
│   │ • Memory view  │   │   │ • Chat bubbles │   │   │ • LLM orchestration     │ │
│   │ • Decision log │   │   │ • Optical mem  │   │   │ • Director/Arbiter      │ │
│   │ • Reset buttons│   │   │   rendering    │   │   │ • Turso storage         │ │
│   │ • Log stream   │   │   │ • TTS playback │   │   │ • TTS generation        │ │
│   │                │   │   │                │   │   │                         │ │
│   └───────┬────────┘   │   └───────┬────────┘   │   └───────────┬─────────────┘ │
│           │            │           │            │               │               │
│           └────────────┴───────────┴────────────┴───────────────┘               │
│                                    │                                            │
│                        ┌───────────┴───────────┐                                │
│                        │   WebSocket Bridge    │                                │
│                        │   (localhost:7777)    │                                │
│                        └───────────────────────┘                                │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 1. Vision Pipeline (Continuous Screen Observation)

**Inspired by**: OpticalMemory rendering + screen OCR systems

```rust
// Core capture loop (Rust)
pub struct VisionPipeline {
    capture_interval: Duration,      // 1-5 seconds
    ocr_engine: OcrEngine,           // Tesseract / PaddleOCR / Vision LLM
    diff_detector: ScreenDiffDetector,
    context_buffer: RingBuffer<ScreenFrame>,
}

pub struct ScreenFrame {
    timestamp: Instant,
    image: Image,                    // Raw screenshot
    ocr_text: String,                // Extracted text
    active_window: WindowInfo,       // App name, title
    regions_of_interest: Vec<Region>,// Detected UI elements, text blocks
    embedding: Vec<f32>,             // CLIP/SigLIP embedding for similarity
}
```

**Key features:**
- **Differential capture**: Only process when screen content changes significantly (embedding cosine distance > threshold)
- **Region-of-interest detection**: Focus on active window, text areas, cursor position
- **Multi-modal encoding**: Store both raw image (for VLM) and OCR text (for efficiency)

---

## 2. Observation Buffer (Rolling Context Window)

**Inspired by**: vtube-bot's `Backlog` + Substrate's `WorldDB.history`

```rust
pub struct ObservationBuffer {
    screen_history: VecDeque<ScreenFrame>,    // Last N screen captures
    chat_history: VecDeque<ChatMessage>,       // User/character messages
    event_log: VecDeque<SystemEvent>,          // App switches, notifications
    
    // Computed context windows
    short_term: ContextWindow,    // Last 30 seconds (working memory)
    medium_term: ContextWindow,   // Last 5 minutes (session context)
    
    // Optical memory renders (cached)
    optical_cache: HashMap<String, RenderedPage>,
}

pub struct ContextWindow {
    screen_summary: String,        // Condensed OCR text
    activity_description: String,  // "User is writing code in VS Code"
    notable_events: Vec<Event>,
    embedding: Vec<f32>,           // For similarity search
}
```

---

## The Three Windows

### 1. **Dewet Daemon** (Rust, headless)

The always-running background service. No UI of its own.

```rust
// src/main.rs
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize storage
    let db = TursoDb::connect("libsql://dewet.turso.io").await?;
    
    // Start subsystems
    let vision = VisionPipeline::new();
    let director = Director::new(db.clone());
    let bridge = WebSocketBridge::bind("127.0.0.1:7777").await?;
    
    // Main perception loop (0.5-2 Hz)
    let perception_loop = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            interval.tick().await;
            
            // 1. Capture screen
            let frame = vision.capture().await;
            
            // 2. Request optical memory render from Godot
            bridge.send(Request::RenderOpticalMemory { 
                chat_history: director.recent_chat(50),
                memory_nodes: director.spatial_memory_nodes(),
            }).await;
            
            // 3. Wait for rendered images
            let optical_pages = bridge.recv_images().await;
            
            // 4. Compose 4-quadrant input
            let composite = compose_quadrant_image(
                frame.screenshot,      // Top-left: Desktop
                optical_pages.memory,  // Top-right: Spatial memory
                optical_pages.chat,    // Bottom-left: Chat transcript
                optical_pages.status,  // Bottom-right: Character HUDs
            );
            
            // 5. Director decides
            let decision = director.evaluate(composite).await;
            
            // 6. Execute decision
            match decision {
                Decision::Pass => { /* silence */ },
                Decision::Speak { character, text, puppet_directives } => {
                    let audio = tts.synthesize(&text).await;
                    bridge.send(Response::Speak {
                        character_id: character,
                        text,
                        audio_base64: encode_audio(&audio),
                        puppet: puppet_directives,
                    }).await;
                },
                Decision::React { character, expression } => {
                    bridge.send(Response::React { character_id: character, expression }).await;
                },
            }
        }
    });
    
    // Chat handler (instant response to user input)
    let chat_loop = tokio::spawn(async move {
        while let Some(msg) = bridge.recv_chat().await {
            // User sent a message - immediate processing
            director.handle_user_message(msg).await;
        }
    });
    
    tokio::select! {
        _ = perception_loop => {},
        _ = chat_loop => {},
    }
    
    Ok(())
}
```

### 2. **Puppet Window** (Godot 4.5)

The user-facing Dewet interface. Transparent overlay or windowed mode.

```
res://
├── main/
│   └── Dewet.tscn              # Main scene
├── puppet/
│   ├── PuppetController2D.gd   # From vtube-bot
│   ├── PuppetController3D.gd   # Optional Live2D/3D
│   └── CharacterSlot.tscn      # Per-character display
├── chat/
│   ├── ChatBubble.tscn
│   └── ChatWindow.gd
├── optical/
│   ├── OpticalMemory.gd        # Ported from Substrate
│   ├── TranscriptRenderer.gd
│   ├── SpatialMemoryMap.gd
│   └── CharacterHUD.gd
├── bridge/
│   └── DaemonBridge.gd         # WebSocket to Rust
└── data/
    └── types/                  # CCv2 resources from Substrate
```

**Key Godot script: DaemonBridge.gd**

```gdscript
extends Node
class_name DaemonBridge

signal connected
signal disconnected
signal speak_requested(character_id: String, text: String, audio: PackedByteArray, puppet: Dictionary)
signal react_requested(character_id: String, expression: String)
signal render_optical_memory_requested(chat_history: Array, memory_nodes: Array)

var _socket: WebSocketPeer = WebSocketPeer.new()
var _url: String = "ws://127.0.0.1:7777"

func _ready() -> void:
    _connect()

func _process(_delta: float) -> void:
    _socket.poll()
    var state = _socket.get_ready_state()
    
    if state == WebSocketPeer.STATE_OPEN:
        while _socket.get_available_packet_count() > 0:
            var packet = _socket.get_packet()
            _handle_message(packet.get_string_from_utf8())
    elif state == WebSocketPeer.STATE_CLOSED:
        call_deferred("_reconnect")

func _handle_message(json_str: String) -> void:
    var msg = JSON.parse_string(json_str)
    if msg == null:
        return
    
    match msg.get("type", ""):
        "speak":
            var audio = Marshalls.base64_to_raw(msg.get("audio_base64", ""))
            speak_requested.emit(msg.character_id, msg.text, audio, msg.get("puppet", {}))
        "react":
            react_requested.emit(msg.character_id, msg.expression)
        "render_optical_memory":
            render_optical_memory_requested.emit(msg.chat_history, msg.memory_nodes)
        _:
            push_warning("Unknown message type: " + msg.get("type", ""))

func send_user_message(text: String) -> void:
    _send({"type": "user_chat", "text": text})

func send_rendered_images(memory_png: PackedByteArray, chat_png: PackedByteArray, status_png: PackedByteArray) -> void:
    _send({
        "type": "optical_render_result",
        "memory": Marshalls.raw_to_base64(memory_png),
        "chat": Marshalls.raw_to_base64(chat_png),
        "status": Marshalls.raw_to_base64(status_png),
    })

func _send(data: Dictionary) -> void:
    if _socket.get_ready_state() == WebSocketPeer.STATE_OPEN:
        _socket.send_text(JSON.stringify(data))
```

### 3. **Debug Window** (Tauri, separate process)

A developer-only window for introspection and control.

```
dewet-debug/
├── src-tauri/
│   ├── Cargo.toml
│   └── src/
│       └── main.rs           # Connects to daemon via separate channel
└── src/
    ├── App.svelte            # or Vue/React
    ├── components/
    │   ├── DecisionLog.svelte
    │   ├── MemoryBrowser.svelte
    │   ├── LogStream.svelte
    │   ├── CharacterInspector.svelte
    │   └── ControlPanel.svelte
    └── lib/
        └── daemon.ts         # WebSocket to Rust debug endpoint
```

**Debug Window Features:**

| Panel | Purpose |
|-------|---------|
| **Decision Log** | Real-time stream of arbiter decisions with reasoning |
| **Memory Browser** | SQLite viewer for Turso tables (episodes, context, etc.) |
| **Log Stream** | Live daemon logs with filtering |
| **Character Inspector** | View/edit loaded CCv2 profiles at runtime |
| **Control Panel** | Reset buttons, force-speak, adjust cooldowns |
| **Screen Preview** | See what the VLM sees (4-quadrant composite) |

---

## Storage: Turso (libSQL)

All data in one place, with optional vector search when needed.

```sql
-- Schema

-- Episode memory (the "what happened" log)
CREATE TABLE episodes (
    id TEXT PRIMARY KEY,
    timestamp INTEGER NOT NULL,
    event_type TEXT NOT NULL,        -- 'user_chat', 'companion_spoke', 'screen_change', etc.
    actor TEXT,                      -- character_id or 'user'
    content TEXT NOT NULL,           -- The actual text/description
    emotional_valence REAL DEFAULT 0.0,  -- -1.0 to 1.0
    importance REAL DEFAULT 0.5,     -- For forgetting (decays over time)
    screen_context TEXT,             -- JSON: active window, app name
    embedding F32_BLOB(384)          -- Optional: for semantic search
);

-- Spatial memory (associating memories with contexts)
CREATE TABLE spatial_contexts (
    id TEXT PRIMARY KEY,
    context_type TEXT NOT NULL,      -- 'app', 'window_title', 'project', 'time_of_day'
    context_value TEXT NOT NULL,     -- 'VS Code', 'substrate/yzh', 'evening'
    last_seen INTEGER,
    visit_count INTEGER DEFAULT 1
);

CREATE TABLE memory_spatial_links (
    episode_id TEXT REFERENCES episodes(id),
    context_id TEXT REFERENCES spatial_contexts(id),
    strength REAL DEFAULT 1.0,       -- Decays if not reinforced
    PRIMARY KEY (episode_id, context_id)
);

-- Character state (runtime, not the CCv2 definition)
CREATE TABLE character_states (
    character_id TEXT PRIMARY KEY,
    current_mood TEXT DEFAULT 'neutral',
    last_spoke_at INTEGER,
    relationship_score REAL DEFAULT 0.5
);

-- Chat history (fast access for recent context)
CREATE TABLE chat_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    sender TEXT NOT NULL,            -- 'user' or character_id
    content TEXT NOT NULL,
    in_response_to INTEGER REFERENCES chat_messages(id)
);

-- Arbiter decision log (for debugging and analysis)
CREATE TABLE arbiter_decisions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    should_respond INTEGER NOT NULL,  -- 0 or 1
    responder_id TEXT,                -- NULL if no response
    reasoning TEXT NOT NULL,          -- LLM's explanation
    urgency REAL,
    context_summary TEXT              -- JSON snapshot of what arbiter saw
);

-- Create vector index for semantic search (Turso supports this)
CREATE INDEX episodes_embedding_idx ON episodes(
    libsql_vector_idx(embedding, 'metric=cosine')
);
```

**Rust Turso client:**

```rust
// src/storage/mod.rs
use libsql::{Builder, Connection, Database};

pub struct TursoDb {
    conn: Connection,
}

impl TursoDb {
    pub async fn connect(url: &str) -> Result<Self> {
        let db = Builder::new_remote(url, std::env::var("TURSO_AUTH_TOKEN")?)
            .build()
            .await?;
        let conn = db.connect()?;
        Ok(Self { conn })
    }
    
    pub async fn add_episode(&self, episode: &Episode) -> Result<()> {
        self.conn.execute(
            "INSERT INTO episodes (id, timestamp, event_type, actor, content, emotional_valence, importance, screen_context, embedding)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, vector(?9))",
            libsql::params![
                episode.id,
                episode.timestamp,
                episode.event_type,
                episode.actor,
                episode.content,
                episode.emotional_valence,
                episode.importance,
                serde_json::to_string(&episode.screen_context)?,
                episode.embedding.as_ref().map(|e| e.as_slice()),
            ],
        ).await?;
        Ok(())
    }
    
    pub async fn search_similar_episodes(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<Episode>> {
        let rows = self.conn.query(
            "SELECT id, timestamp, event_type, actor, content, emotional_valence, importance
             FROM episodes
             WHERE embedding IS NOT NULL
             ORDER BY vector_distance_cos(embedding, vector(?1))
             LIMIT ?2",
            libsql::params![query_embedding, limit as i64],
        ).await?;
        
        // ... parse rows
    }
    
    pub async fn decay_importance(&self, decay_factor: f32, min_age_hours: i64) -> Result<u64> {
        let cutoff = chrono::Utc::now().timestamp() - (min_age_hours * 3600);
        let result = self.conn.execute(
            "UPDATE episodes 
             SET importance = importance * ?1 
             WHERE timestamp < ?2 AND importance > 0.01",
            libsql::params![decay_factor, cutoff],
        ).await?;
        Ok(result)
    }
    
    pub async fn prune_forgotten(&self, threshold: f32) -> Result<u64> {
        let result = self.conn.execute(
            "DELETE FROM episodes WHERE importance < ?1",
            libsql::params![threshold],
        ).await?;
        Ok(result)
    }
}
```

---

## The Director: LLM-Native Decision Engine

No heuristics. No keyword matching. No trigger tables. Just context → LLM → decision.

The core insight: keyword matching is fragile, uncontextual garbage. The whole point of this project is to let LLMs do what they're good at—understanding context and making nuanced decisions.

```rust
// src/director/mod.rs

use crate::character::CharacterSpec;
use crate::llm::LlmClient;
use crate::storage::TursoDb;

pub struct Director {
    db: TursoDb,
    llm: LlmClient,
    characters: Vec<LoadedCharacter>,
    config: DirectorConfig,
    last_decision_at: Instant,
}

pub struct LoadedCharacter {
    pub spec: CharacterSpec,
    pub last_spoke_at: Option<Instant>,
}

pub struct DirectorConfig {
    pub min_decision_interval: Duration,     // Don't spam the LLM
    pub cooldown_after_speak: Duration,      // Per-character cooldown
    pub llm: LlmConfig,                       // Provider + model config (see LLM Provider Integration)
}

/// The LLM's decision about what to do
#[derive(Debug, Deserialize)]
pub struct ArbiterDecision {
    /// Should anyone respond at all?
    pub should_respond: bool,
    
    /// If responding, which character? (null if no one)
    pub responder_id: Option<String>,
    
    /// Brief reasoning (for debug window)
    pub reasoning: String,
    
    /// Suggested mood/expression for the responder
    pub suggested_mood: Option<String>,
    
    /// How urgent is this response? (affects animation intensity)
    pub urgency: f32,
}

impl Director {
    pub async fn evaluate(&mut self, observation: &Observation) -> Decision {
        // Rate limit: don't spam the arbiter
        if self.last_decision_at.elapsed() < self.config.min_decision_interval {
            return Decision::Pass;
        }
        self.last_decision_at = Instant::now();
        
        // 1. Gather ALL context
        let context = self.assemble_full_context(observation).await;
        
        // 2. Ask the LLM: should anyone respond?
        let arbiter_decision = self.query_arbiter(&context).await?;
        
        // 3. If no response warranted, we're done
        if !arbiter_decision.should_respond || arbiter_decision.responder_id.is_none() {
            tracing::debug!("Arbiter: pass - {}", arbiter_decision.reasoning);
            return Decision::Pass;
        }
        
        let responder_id = arbiter_decision.responder_id.unwrap();
        
        // 4. Check cooldown (the ONE rule we enforce mechanically)
        let character = self.characters.iter()
            .find(|c| c.spec.id == responder_id);
        
        if let Some(char) = character {
            if let Some(last) = char.last_spoke_at {
                if last.elapsed() < self.config.cooldown_after_speak {
                    tracing::debug!("Arbiter wanted {} but on cooldown", responder_id);
                    return Decision::Pass;
                }
            }
        }
        
        // 5. Generate the actual response
        let response = self.generate_response(&responder_id, &context, &arbiter_decision).await?;
        
        // 6. Self-audit (optional second pass)
        let audited = self.self_audit(&responder_id, &response, &context).await?;
        
        match audited {
            AuditResult::Approved(text) => {
                // Update last spoke time
                if let Some(char) = self.characters.iter_mut().find(|c| c.spec.id == responder_id) {
                    char.last_spoke_at = Some(Instant::now());
                }
                
                Decision::Speak {
                    character_id: responder_id,
                    text,
                    mood: arbiter_decision.suggested_mood,
                    urgency: arbiter_decision.urgency,
                }
            },
            AuditResult::Revised(text) => {
                if let Some(char) = self.characters.iter_mut().find(|c| c.spec.id == responder_id) {
                    char.last_spoke_at = Some(Instant::now());
                }
                
                Decision::Speak {
                    character_id: responder_id,
                    text,
                    mood: arbiter_decision.suggested_mood,
                    urgency: arbiter_decision.urgency,
                }
            },
            AuditResult::Blocked(reason) => {
                tracing::debug!("Self-audit blocked: {}", reason);
                Decision::Pass
            },
        }
    }
    
    /// Assemble everything the LLM needs to make a decision
    async fn assemble_full_context(&self, observation: &Observation) -> FullContext {
        // Recent chat (last N messages)
        let recent_chat = self.db.get_recent_chat(30).await.unwrap_or_default();
        
        // Recent memories that might be relevant
        let recent_episodes = self.db.get_recent_episodes(20).await.unwrap_or_default();
        
        // Character summaries (who they are, what they care about)
        let character_summaries: Vec<CharacterSummary> = self.characters.iter()
            .map(|c| CharacterSummary {
                id: c.spec.id.clone(),
                name: c.spec.name.clone(),
                personality: c.spec.personality.clone(),
                description: c.spec.description.clone(),
                interests: c.spec.extensions.get("interests")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(String::from).collect())
                    .unwrap_or_default(),
                currently_on_cooldown: c.last_spoke_at
                    .map(|t| t.elapsed() < self.config.cooldown_after_speak)
                    .unwrap_or(false),
                seconds_since_last_spoke: c.last_spoke_at
                    .map(|t| t.elapsed().as_secs())
                    .unwrap_or(u64::MAX),
            })
            .collect();
        
        FullContext {
            // What's on screen right now
            screen_ocr: observation.ocr_text.clone(),
            active_window: observation.active_window.clone(),
            active_app: observation.active_app.clone(),
            
            // What's been said
            recent_chat,
            
            // Who the companions are
            characters: character_summaries,
            
            // Background context
            recent_episodes,
            
            // Timing context
            seconds_since_any_response: self.seconds_since_any_response(),
            seconds_since_user_message: observation.seconds_since_user_message,
        }
    }
    
    /// Query the arbiter LLM: who (if anyone) should respond?
    async fn query_arbiter(&self, context: &FullContext) -> Result<ArbiterDecision> {
        let prompt = self.build_arbiter_prompt(context);
        
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "should_respond": {
                    "type": "boolean",
                    "description": "Should any companion respond right now?"
                },
                "responder_id": {
                    "type": ["string", "null"],
                    "description": "The ID of the character who should respond, or null if no one should"
                },
                "reasoning": {
                    "type": "string",
                    "description": "Brief explanation of why this decision was made"
                },
                "suggested_mood": {
                    "type": ["string", "null"],
                    "description": "Suggested emotional tone for the response"
                },
                "urgency": {
                    "type": "number",
                    "minimum": 0.0,
                    "maximum": 1.0,
                    "description": "How urgent/excited is this response (0=calm, 1=very urgent)"
                }
            },
            "required": ["should_respond", "reasoning", "urgency"]
        });
        
        let response = self.llm.complete_json(
            &self.config.decision_model,
            &prompt,
            schema,
        ).await?;
        
        Ok(serde_json::from_value(response)?)
    }
    
    fn build_arbiter_prompt(&self, ctx: &FullContext) -> String {
        format!(r#"
You are the Arbiter for Dewet, a desktop AI companion app. Your job is to decide if any companion should speak right now, and if so, which one.

## Current Screen Context
- **Active App**: {active_app}
- **Window Title**: {active_window}
- **Screen Content (OCR)**:
```
{screen_ocr}
```

## Recent Conversation
{recent_chat}

## Available Companions
{characters}

## Recent Memories
{episodes}

## Timing
- Seconds since any companion spoke: {since_response}
- Seconds since user's last message: {since_user}

## Decision Criteria

Answer these questions in your reasoning:

1. **Is there something worth responding to?**
   - Did the user say something? Ask a question? Share something?
   - Did something interesting happen on screen that a companion would notice?
   - Has it been awkwardly silent for a while?

2. **Would responding add value?**
   - Would a response feel natural and welcome, or intrusive and annoying?
   - Is this a moment where companionship matters, or should the user be left alone?

3. **If responding, who is the best fit?**
   - Which companion's personality/interests align with the current context?
   - Who hasn't spoken in a while and might have something to contribute?
   - Would multiple companions want to respond? Pick the ONE best fit.

4. **What should the tone be?**
   - Casual observation? Enthusiastic comment? Gentle question?
   - Match urgency to the situation (coding focus = low urgency, user asked a question = higher)

## Rules

- **Silence is golden.** Most of the time, the answer is "don't respond."
- **Never be annoying.** Companions should feel like friends, not pop-up ads.
- **Respect focus.** If the user is clearly in deep work, stay quiet unless addressed.
- **Be contextual.** Generic comments are worse than silence.
- **One voice at a time.** Only one companion speaks per decision.

Return your decision as JSON.
"#,
            active_app = ctx.active_app,
            active_window = ctx.active_window,
            screen_ocr = truncate(&ctx.screen_ocr, 2000),
            recent_chat = self.format_chat(&ctx.recent_chat),
            characters = self.format_characters(&ctx.characters),
            episodes = self.format_episodes(&ctx.recent_episodes),
            since_response = ctx.seconds_since_any_response,
            since_user = ctx.seconds_since_user_message,
        )
    }
    
    fn format_characters(&self, chars: &[CharacterSummary]) -> String {
        chars.iter().map(|c| {
            format!(
                "### {name} (id: `{id}`)
**Personality**: {personality}
**Interests**: {interests}
**Status**: {status}
",
                name = c.name,
                id = c.id,
                personality = truncate(&c.personality, 200),
                interests = c.interests.join(", "),
                status = if c.currently_on_cooldown {
                    "On cooldown (just spoke)".to_string()
                } else {
                    format!("Available (last spoke {}s ago)", c.seconds_since_last_spoke)
                },
            )
        }).collect::<Vec<_>>().join("\n")
    }
    
    fn format_chat(&self, messages: &[ChatMessage]) -> String {
        if messages.is_empty() {
            return "(No recent messages)".to_string();
        }
        
        messages.iter().map(|m| {
            format!("**{}**: {}", m.sender, m.content)
        }).collect::<Vec<_>>().join("\n")
    }
    
    fn format_episodes(&self, episodes: &[Episode]) -> String {
        if episodes.is_empty() {
            return "(No recent memories)".to_string();
        }
        
        episodes.iter().take(10).map(|e| {
            format!("- [{}] {}", e.event_type, truncate(&e.content, 100))
        }).collect::<Vec<_>>().join("\n")
    }
    
    fn seconds_since_any_response(&self) -> u64 {
        self.characters.iter()
            .filter_map(|c| c.last_spoke_at)
            .max()
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(u64::MAX)
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}
```

### What We Eliminated

| Before (Heuristic Slop) | After (LLM-Native) |
|-------------------------|---------------------|
| `character_book` keyword triggers | LLM reads full personality |
| `desktop_triggers` dictionary | LLM sees active app + window |
| `tension` score accumulation | LLM judges "is this worth responding to?" |
| `entry.priority` scoring | LLM picks best responder holistically |
| Regex pattern matching | Natural language understanding |
| Silence timers with thresholds | LLM considers "has it been too quiet?" |

### The Only Mechanical Rules

We keep exactly **two** hard rules that aren't LLM-decided:

1. **Rate limiting**: Don't query the arbiter more than once per N seconds (cost/performance)
2. **Cooldown**: A character can't speak again for M seconds after speaking (prevents rapid-fire)

Everything else—who speaks, why, when, what mood—is LLM-decided.

```rust
pub struct DirectorConfig {
    /// Don't spam the arbiter LLM
    pub min_decision_interval: Duration,  // e.g., 2 seconds
    
    /// Per-character cooldown after speaking
    pub cooldown_after_speak: Duration,   // e.g., 30 seconds
    
    // Everything else is up to the LLM
}
```

---

## Character Profiles: Identity, Not Triggers

The CCv2 profile becomes purely descriptive. No `trigger_type`, no `pattern`, no `priority`:

```rust
pub struct CharacterSpec {
    pub id: String,
    pub name: String,
    pub description: String,
    pub personality: String,
    pub scenario: String,
    pub system_prompt: String,
    pub mes_example: String,
    
    // Lorebook entries become just "things this character knows"
    // NOT "keywords that trigger responses"
    pub character_book: Vec<LoreEntry>,
    
    pub extensions: HashMap<String, Value>,
}

pub struct LoreEntry {
    pub content: String,        // The knowledge itself
    pub is_public: bool,        // Can other characters know this?
    // No more: keys, secondary_keys, priority, trigger patterns
}
```

The `extensions` can include things like:

```json
{
  "interests": ["programming", "art", "music"],
  "dislikes": ["being ignored", "rudeness"],
  "speech_style": "casual, uses emoji occasionally",
  "expertise": ["rust", "game development"]
}
```

These are **descriptive**, not **prescriptive**. The LLM reads them and makes its own judgment.

---

## The 4-Quadrant Composite Image

The key insight from the other advice: composing multiple context sources into a single image for the VLM.

```rust
// src/vision/composite.rs

pub struct CompositeRenderer {
    output_size: (u32, u32),  // e.g., (1536, 1536)
}

impl CompositeRenderer {
    /// Compose the 4-quadrant context image
    pub fn render(&self, parts: CompositeParts) -> Image {
        let (w, h) = self.output_size;
        let half_w = w / 2;
        let half_h = h / 2;
        
        let mut canvas = Image::new(w, h);
        
        // Top-left: User's desktop (downscaled)
        let desktop_scaled = parts.desktop_screenshot.resize(half_w, half_h);
        canvas.blit(0, 0, &desktop_scaled);
        
        // Top-right: Spatial memory visualization (from Godot)
        canvas.blit(half_w, 0, &parts.memory_visualization);
        
        // Bottom-left: Chat transcript (rendered text as image)
        canvas.blit(0, half_h, &parts.chat_transcript);
        
        // Bottom-right: Character status HUDs
        canvas.blit(half_w, half_h, &parts.character_status);
        
        // Add quadrant labels for VLM clarity
        canvas.draw_text(10, 10, "DESKTOP", Color::WHITE);
        canvas.draw_text(half_w + 10, 10, "MEMORY MAP", Color::WHITE);
        canvas.draw_text(10, half_h + 10, "RECENT CHAT", Color::WHITE);
        canvas.draw_text(half_w + 10, half_h + 10, "COMPANIONS", Color::WHITE);
        
        canvas
    }
}

pub struct CompositeParts {
    pub desktop_screenshot: Image,    // From screen capture
    pub memory_visualization: Image,  // From Godot OpticalMemory
    pub chat_transcript: Image,       // From Godot OpticalMemory
    pub character_status: Image,      // From Godot (mood, energy bars, portraits)
}
```

**VLM Prompt for composite analysis:**

```rust
const PERCEPTION_PROMPT: &str = r#"
You are observing Dewet's context through 4 quadrants:

**TOP-LEFT (DESKTOP)**: The user's current screen.
**TOP-RIGHT (MEMORY MAP)**: Spatial visualization of recent topics and their relationships.
**BOTTOM-LEFT (RECENT CHAT)**: The conversation history.
**BOTTOM-RIGHT (COMPANIONS)**: Active AI companions with their current moods.

Analyze this context and answer:
1. What is the user currently doing? (Be specific: app, task, content)
2. Does this relate to any nodes in the MEMORY MAP? Which ones?
3. For each companion in BOTTOM-RIGHT, rate their likely interest (0-10) in the current activity.
4. Has the user said anything that warrants a response?

Return JSON:
{
  "activity": "string",
  "related_memories": ["node_id", ...],
  "companion_interest": {"char_id": score, ...},
  "warrants_response": bool,
  "response_trigger": "string or null"
}
"#;
```

---

## Optical Memory Integration with Arbiter

The 4-quadrant composite feeds directly into the arbiter. Instead of (or in addition to) OCR text, we can send the composite image to a vision-capable model:

```rust
async fn query_arbiter_with_vision(
    &self,
    context: &FullContext,
    composite: &Image
) -> Result<ArbiterDecision> {
    let prompt = self.build_arbiter_prompt(context);
    
    // Vision-capable arbiter sees the actual screen context
    let response = self.llm.complete_vision_json(
        &self.config.decision_model,
        &prompt,
        vec![composite.to_base64()],
        self.arbiter_schema(),
    ).await?;
    
    Ok(serde_json::from_value(response)?)
}
```

The VLM sees everything at once:
- The user's desktop (what they're doing)
- The memory map (what's been happening)
- The chat transcript (what's been said)
- The character status (who's available)

And makes one holistic decision: **should anyone speak, and who?**

This approach trusts the LLM to do what it's good at—understanding context, nuance, and social dynamics—instead of building a brittle heuristic system that we'd constantly need to tune.

---

## Inner Monologue / Self-Audit

**Inspired by**: Substrate's `EditorEngine`

A secondary LLM pass that reviews outputs before delivery:

```rust
pub struct InnerMonologue {
    audit_model: LlmClient,  // Can be same or different from response model
}

pub enum AuditDecision {
    Approve,
    Revise { suggestion: String },
    Block { reason: String },
}

impl InnerMonologue {
    pub async fn review(&self, response: &CharacterResponse, context: &Context) -> AuditDecision {
        let prompt = format!(r#"
You are the Editor: a self-consistency auditor for {character_name}.
Review this response for:
1. Character voice consistency (personality, speech patterns)
2. Factual consistency with prior statements
3. Appropriateness for current context
4. Avoiding repetition of recent responses

Response to review:
{response_text}

Recent context:
{context_summary}

Return JSON: {{"status": "approve|revise|block", "reason": "...", "suggestion": "..."}}
"#, character_name=response.character_name, response_text=response.text, context_summary=context.summary());

        // Parse and return decision
    }
}
```

---

## Project Structure

```
dewet/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── dewet-daemon/             # The Rust brain
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── director/
│   │       │   ├── mod.rs
│   │       │   ├── arbiter.rs
│   │       │   └── inner_monologue.rs
│   │       ├── vision/
│   │       │   ├── mod.rs
│   │       │   ├── capture.rs
│   │       │   ├── ocr.rs
│   │       │   └── composite.rs
│   │       ├── storage/
│   │       │   ├── mod.rs
│   │       │   ├── turso.rs
│   │       │   └── forgetting.rs
│   │       ├── llm/
│   │       │   ├── mod.rs
│   │       │   ├── provider.rs     # LlmProvider trait
│   │       │   ├── lmstudio.rs     # LM Studio (local)
│   │       │   ├── openrouter.rs   # OpenRouter (cloud)
│   │       │   └── prompts.rs
│   │       ├── tts/
│   │       │   ├── mod.rs
│   │       │   └── neutts.rs
│   │       ├── bridge/
│   │       │   ├── mod.rs
│   │       │   └── websocket.rs
│   │       └── character/
│   │           ├── mod.rs
│   │           ├── ccv2.rs       # Character Card v2 parser
│   │           └── state.rs
│   │
│   └── dewet-debug/              # Tauri debug window
│       ├── Cargo.toml
│       ├── tauri.conf.json
│       ├── src-tauri/
│       │   └── src/
│       │       └── main.rs
│       └── src/
│           ├── App.svelte
│           └── components/
│
├── godot/                        # Godot 4.5 project (the body)
│   ├── project.godot
│   ├── main/
│   │   └── Dewet.tscn
│   ├── puppet/
│   │   ├── PuppetController2D.gd
│   │   └── assets/
│   ├── chat/
│   │   ├── ChatWindow.gd
│   │   └── ChatBubble.tscn
│   ├── optical/
│   │   ├── OpticalMemory.gd      # Ported from Substrate
│   │   ├── TranscriptRenderer.gd
│   │   ├── SpatialMemoryMap.gd
│   │   └── CharacterHUD.gd
│   ├── bridge/
│   │   └── DaemonBridge.gd
│   └── data/
│       └── types/                # Shared with Substrate
│           ├── CharacterProfile.gd
│           ├── CharacterBookEntry.gd
│           └── ...
│
├── shared/                       # Shared data types (optional)
│   └── schemas/
│       ├── character_v2.json     # CCv2 JSON Schema
│       └── bridge_protocol.json  # WebSocket message schema
│
└── docs/
    ├── ARCHITECTURE.md
    └── PROTOCOLS.md
```

---

## Dependencies

**Cargo.toml (dewet-daemon):**

```toml
[package]
name = "dewet-daemon"
version = "0.1.0"
edition = "2024"

[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Storage (Turso/libSQL)
libsql = "0.6"

# LLM (via LM Studio or OpenRouter)
reqwest = { version = "0.12", features = ["json", "multipart"] }
async-trait = "0.1"

# Screen capture
xcap = "0.0.11"
image = "0.25"
base64 = "0.22"

# OCR (optional, can use VLM instead)
# rusty-tesseract = "1.1"

# WebSocket server
tokio-tungstenite = "0.24"

# TTS
rodio = "0.19"

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"

# Character card parsing
toml = "0.8"  # For .tres parsing (GDScript resources)

# Embedding generation (optional, for semantic search)
fastembed = "4"

[features]
default = []
local-llm = ["llama-cpp-2"]
neutts = []
```

---

## LLM Provider Integration

The daemon connects to **external LLM services** rather than embedding inference. This keeps the daemon lightweight and lets you choose between local (LM Studio) or cloud (OpenRouter) providers.

### Supported Providers

| Provider | Use Case | Endpoint | Vision Models |
|----------|----------|----------|---------------|
| **LM Studio** | Local inference, privacy | `http://127.0.0.1:1234` | Qwen2.5-VL, LLaVA, Pixtral |
| **OpenRouter** | Cloud inference, model variety | `https://openrouter.ai/api` | GPT-4o, Claude 3.5, Gemini |

### Configuration

```rust
// src/llm/mod.rs

#[derive(Debug, Clone, Deserialize)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub decision_model: String,    // For arbiter (vision-capable)
    pub response_model: String,    // For response generation
    pub audit_model: Option<String>, // For self-audit (defaults to response_model)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum LlmProvider {
    #[serde(rename = "lmstudio")]
    LmStudio {
        endpoint: String,  // "http://127.0.0.1:1234"
    },
    #[serde(rename = "openrouter")]
    OpenRouter {
        api_key: String,   // From env: OPENROUTER_API_KEY
        site_url: Option<String>,
        site_name: Option<String>,
    },
}
```

**Example configs:**

```toml
# config.toml - Local with LM Studio
[llm]
provider = { type = "lmstudio", endpoint = "http://127.0.0.1:1234" }
decision_model = "qwen2.5-vl-7b-instruct"
response_model = "qwen2.5-7b-instruct"

# config.toml - Cloud with OpenRouter
[llm]
provider = { type = "openrouter", api_key = "${OPENROUTER_API_KEY}" }
decision_model = "google/gemini-flash-1.5"
response_model = "anthropic/claude-3.5-sonnet"
```

### Provider Trait

Both providers implement a common trait for text and vision completions:

```rust
// src/llm/provider.rs

use async_trait::async_trait;

#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Text completion with JSON schema enforcement
    async fn complete_json(
        &self,
        model: &str,
        prompt: &str,
        schema: serde_json::Value,
    ) -> Result<serde_json::Value>;
    
    /// Vision completion with images + JSON schema
    async fn complete_vision_json(
        &self,
        model: &str,
        prompt: &str,
        images_base64: Vec<String>,
        schema: serde_json::Value,
    ) -> Result<serde_json::Value>;
}

/// Create client from config
pub fn create_client(config: &LlmConfig) -> Box<dyn LlmClient> {
    match &config.provider {
        LlmProvider::LmStudio { endpoint } => {
            Box::new(LmStudioClient::new(endpoint))
        }
        LlmProvider::OpenRouter { api_key, site_url, site_name } => {
            Box::new(OpenRouterClient::new(api_key, site_url.clone(), site_name.clone()))
        }
    }
}
```

### LM Studio Client

Connects to LM Studio's OpenAI-compatible API on localhost:

```rust
// src/llm/lmstudio.rs

use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct LmStudioClient {
    client: Client,
    endpoint: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    stream: bool,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: MessageContent,
}

#[derive(Serialize)]
#[serde(untagged)]
enum MessageContent {
    Text(String),
    MultiModal(Vec<ContentPart>),
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
}

#[derive(Serialize)]
struct ImageUrl {
    url: String,  // "data:image/png;base64,..."
}

impl LmStudioClient {
    pub fn new(endpoint: &str) -> Self {
        Self {
            client: Client::new(),
            endpoint: endpoint.to_string(),
        }
    }
}

#[async_trait]
impl LlmClient for LmStudioClient {
    async fn complete_json(
        &self,
        model: &str,
        prompt: &str,
        schema: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let request = ChatRequest {
            model: model.to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: MessageContent::Text(prompt.to_string()),
            }],
            response_format: Some(ResponseFormat {
                r#type: "json_schema".to_string(),
                json_schema: schema,
            }),
            stream: false,
        };
        
        let response = self.client
            .post(format!("{}/v1/chat/completions", self.endpoint))
            .json(&request)
            .send()
            .await?;
        
        let result: ChatResponse = response.json().await?;
        let content = &result.choices[0].message.content;
        Ok(serde_json::from_str(content)?)
    }
    
    async fn complete_vision_json(
        &self,
        model: &str,
        prompt: &str,
        images_base64: Vec<String>,
        schema: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let mut content_parts: Vec<ContentPart> = images_base64
            .into_iter()
            .map(|b64| ContentPart::ImageUrl {
                image_url: ImageUrl {
                    url: format!("data:image/png;base64,{}", b64),
                },
            })
            .collect();
        
        content_parts.push(ContentPart::Text { text: prompt.to_string() });
        
        let request = ChatRequest {
            model: model.to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: MessageContent::MultiModal(content_parts),
            }],
            response_format: Some(ResponseFormat {
                r#type: "json_schema".to_string(),
                json_schema: schema,
            }),
            stream: false,
        };
        
        let response = self.client
            .post(format!("{}/v1/chat/completions", self.endpoint))
            .json(&request)
            .send()
            .await?;
        
        let result: ChatResponse = response.json().await?;
        let content = &result.choices[0].message.content;
        Ok(serde_json::from_str(content)?)
    }
}
```

### OpenRouter Client

Connects to OpenRouter's API for cloud models:

```rust
// src/llm/openrouter.rs

pub struct OpenRouterClient {
    client: Client,
    api_key: String,
    site_url: Option<String>,
    site_name: Option<String>,
}

impl OpenRouterClient {
    pub fn new(api_key: &str, site_url: Option<String>, site_name: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
            site_url,
            site_name,
        }
    }
}

#[async_trait]
impl LlmClient for OpenRouterClient {
    async fn complete_json(
        &self,
        model: &str,
        prompt: &str,
        schema: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Authorization", format!("Bearer {}", self.api_key).parse()?);
        headers.insert("Content-Type", "application/json".parse()?);
        
        if let Some(url) = &self.site_url {
            headers.insert("HTTP-Referer", url.parse()?);
        }
        if let Some(name) = &self.site_name {
            headers.insert("X-Title", name.parse()?);
        }
        
        let request = serde_json::json!({
            "model": model,
            "messages": [{
                "role": "user",
                "content": prompt
            }],
            "response_format": {
                "type": "json_schema",
                "json_schema": schema
            }
        });
        
        let response = self.client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .headers(headers)
            .json(&request)
            .send()
            .await?;
        
        let result: ChatResponse = response.json().await?;
        let content = &result.choices[0].message.content;
        Ok(serde_json::from_str(content)?)
    }
    
    async fn complete_vision_json(
        &self,
        model: &str,
        prompt: &str,
        images_base64: Vec<String>,
        schema: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Authorization", format!("Bearer {}", self.api_key).parse()?);
        headers.insert("Content-Type", "application/json".parse()?);
        
        if let Some(url) = &self.site_url {
            headers.insert("HTTP-Referer", url.parse()?);
        }
        if let Some(name) = &self.site_name {
            headers.insert("X-Title", name.parse()?);
        }
        
        let mut content: Vec<serde_json::Value> = images_base64
            .into_iter()
            .map(|b64| serde_json::json!({
                "type": "image_url",
                "image_url": {
                    "url": format!("data:image/png;base64,{}", b64)
                }
            }))
            .collect();
        
        content.push(serde_json::json!({
            "type": "text",
            "text": prompt
        }));
        
        let request = serde_json::json!({
            "model": model,
            "messages": [{
                "role": "user",
                "content": content
            }],
            "response_format": {
                "type": "json_schema",
                "json_schema": schema
            }
        });
        
        let response = self.client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .headers(headers)
            .json(&request)
            .send()
            .await?;
        
        let result: ChatResponse = response.json().await?;
        let content = &result.choices[0].message.content;
        Ok(serde_json::from_str(content)?)
    }
}
```

### Recommended Models

| Task | LM Studio (Local) | OpenRouter (Cloud) |
|------|-------------------|-------------------|
| **Arbiter (vision)** | `qwen2.5-vl-7b-instruct`, `llava-v1.6-mistral-7b` | `google/gemini-flash-1.5`, `openai/gpt-4o-mini` |
| **Response generation** | `qwen2.5-7b-instruct`, `llama-3.1-8b-instruct` | `anthropic/claude-3.5-sonnet`, `openai/gpt-4o` |
| **Self-audit** | Same as response (or smaller for speed) | `anthropic/claude-3-haiku`, `openai/gpt-4o-mini` |

**Cost considerations:**
- **LM Studio**: Free (local compute), but requires capable GPU (8GB+ VRAM for 7B vision models)
- **OpenRouter**: Pay-per-token, but no local hardware needed. Gemini Flash is extremely cheap for vision.

---

## Implementation Roadmap

### Phase 1: The Bridge (Week 1)
- [x] Scaffold Rust daemon with WebSocket server
- [x] Create minimal Godot project that connects to daemon
- [x] Bi-directional message passing (ping/pong, then chat)
- [x] Scaffold Tauri debug window with connection status

### Phase 2: The Eye (Week 2)
- [x] Screen capture in Rust (xcap)
- [x] Send screenshot to Godot for display in debug
- [x] LLM provider abstraction (LM Studio + OpenRouter)
- [x] Vision analysis via provider (Qwen2.5-VL / Gemini Flash)
- [x] Display VLM description in debug window
- [x] Implement observation buffer for frames, chat, and system events

### Phase 3: The Memory (Week 3)
- [x] Port `OpticalMemory.gd` to Godot Dewet project
- [x] Render chat transcript as image
- [x] Turso schema and basic CRUD
- [ ] Episode storage and retrieval
- [ ] Handle `optical_render_result` responses in daemon and persist rendered quadrants

### Phase 4: The Brain (Week 4)
- [ ] Director arbiter LLM integration
- [ ] Arbiter prompt engineering and JSON schema
- [ ] Inner monologue / self-audit
- [x] 4-quadrant composite rendering
- [ ] Feed observation buffer + Turso context into director prompts

### Phase 5: The Soul (Week 5)
- [ ] Import CCv2 parser from vtube-bot
- [ ] Load character profiles from `.tres` files
- [ ] Multi-character ensemble support
- [ ] Character-specific response generation

### Phase 6: The Body (Week 6)
- [ ] Puppet rendering in Godot (2D sprites first)
- [ ] TTS integration (NeuTTS or cloud)
- [ ] Expression/mood animations

### Phase 7: Polish (Week 7+)
- [ ] Forgetting scheduler (memory decay)
- [ ] Spatial memory visualization
- [x] Debug window: decision log
- [ ] Debug window: memory browser
- [ ] Transparent window mode

---