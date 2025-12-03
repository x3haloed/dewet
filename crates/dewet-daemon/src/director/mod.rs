use std::io::Cursor;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use image::{DynamicImage, ImageFormat, RgbaImage};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use tracing::{info, warn};

use crate::{
    bridge::ChatPacket,
    character::{CharacterSpec, LoadedCharacter},
    config::{DirectorConfig, LlmConfig},
    llm::SharedLlm,
    observation::Observation,
    storage::{Storage, StoredDecision},
};

pub struct Director {
    storage: Storage,
    llm: SharedLlm,
    config: DirectorConfig,
    models: ModelCatalog,
    characters: Vec<LoadedCharacter>,
    last_decision: Instant,
}

impl Director {
    pub fn new(
        storage: Storage,
        llm: SharedLlm,
        director_config: DirectorConfig,
        llm_config: LlmConfig,
        characters: Vec<LoadedCharacter>,
    ) -> Self {
        Self {
            storage,
            llm,
            config: director_config,
            models: ModelCatalog::from(llm_config),
            characters,
            last_decision: Instant::now()
                .checked_sub(Duration::from_secs(3600))
                .unwrap_or_else(Instant::now),
        }
    }

    pub fn characters(&self) -> &[LoadedCharacter] {
        &self.characters
    }

    /// Analyze the composite image using VLM to understand current context
    pub async fn analyze_vision(&self, observation: &Observation) -> Result<VisionAnalysis> {
        let composite = observation.composite.as_ref()
            .ok_or_else(|| anyhow!("No composite image available"))?;
        
        let image_b64 = encode_rgba_to_base64(composite)?;
        
        let prompt = r#"You are observing Dewet's context through 4 quadrants:

**TOP-LEFT (DESKTOP)**: The user's current screen.
**TOP-RIGHT (MEMORY MAP)**: Spatial visualization of recent topics and their relationships.
**BOTTOM-LEFT (RECENT CHAT)**: The conversation history.
**BOTTOM-RIGHT (COMPANIONS)**: Active AI companions with their current moods.

Analyze this context and provide:
1. What is the user currently doing? (Be specific: app, task, content)
2. Does this warrant any companion response?
3. If so, what triggered this (user question, interesting activity, long silence)?
4. Rate each companion's likely interest in the current activity (0.0-1.0).

Be concise but specific."#;

        let schema = json!({
            "type": "object",
            "properties": {
                "activity": {
                    "type": "string",
                    "description": "What the user is currently doing"
                },
                "warrants_response": {
                    "type": "boolean",
                    "description": "Whether a companion should respond"
                },
                "response_trigger": {
                    "type": "string",
                    "description": "What triggered the potential response, or empty string if none"
                },
                "companion_interest": {
                    "type": "object",
                    "description": "Interest level per companion (0.0-1.0)",
                    "additionalProperties": { "type": "number" }
                }
            },
            "required": ["activity", "warrants_response", "response_trigger", "companion_interest"]
        });

        let response = self.llm
            .complete_vision_json(
                &self.models.decision,
                prompt,
                vec![image_b64],
                schema,
            )
            .await?;

        let analysis: VisionAnalysis = serde_json::from_value(response)?;
        info!(activity = %analysis.activity, warrants = analysis.warrants_response, "VLM analysis complete");
        
        Ok(analysis)
    }

    pub async fn evaluate(&mut self, observation: &Observation) -> Result<(Decision, Option<VisionAnalysis>)> {
        if self.last_decision.elapsed() < self.config.min_decision_interval() {
            return Ok((Decision::Pass, None));
        }
        self.last_decision = Instant::now();

        // Try vision analysis if composite is available
        let vision_analysis = if observation.composite.is_some() {
            match self.analyze_vision(observation).await {
                Ok(analysis) => Some(analysis),
                Err(err) => {
                    warn!(?err, "Vision analysis failed, falling back to text-only");
                    None
                }
            }
        } else {
            None
        };

        // Build arbiter prompt, enriched with vision analysis if available
        let schema = arbiter_schema();
        let prompt = self.build_arbiter_prompt_with_vision(observation, vision_analysis.as_ref());
        let response = self
            .llm
            .complete_json(&self.models.decision, &prompt, schema)
            .await?;
        let decision: ArbiterDecision = serde_json::from_value(response)?;

        self.storage
            .record_decision(&StoredDecision::now(
                decision.should_respond,
                decision.responder_id.clone(),
                decision.reasoning.clone(),
                decision.urgency,
            ))
            .await?;

        if !decision.should_respond {
            return Ok((Decision::Pass, vision_analysis));
        }

        let responder_id = match &decision.responder_id {
            Some(id) => id.clone(),
            None => return Ok((Decision::Pass, vision_analysis)),
        };

        let Some(responder_index) = self
            .characters
            .iter()
            .position(|c| c.spec.id == responder_id)
        else {
            return Ok((Decision::Pass, vision_analysis));
        };

        {
            let character = &self.characters[responder_index];
            if character
                .state
                .is_on_cooldown(self.config.cooldown_after_speak())
            {
                return Ok((Decision::Pass, vision_analysis));
            }
        }

        let response_prompt =
            Self::build_response_prompt(&self.characters[responder_index].spec, observation);
        let mut text = self
            .llm
            .complete_text(&self.models.response, &response_prompt)
            .await?;

        if let Some(audit_model) = &self.models.audit {
            text = match self
                .run_audit(
                    &self.characters[responder_index].spec,
                    &text,
                    observation,
                    audit_model,
                )
                .await
            {
                Ok(validated) => validated,
                Err(err) => {
                    warn!(?err, "Audit rejected response");
                    return Ok((Decision::Pass, vision_analysis));
                }
            };
        }

        if let Some(character) = self.characters.get_mut(responder_index) {
            character.state.update_last_spoke();
        }

        Ok((Decision::Speak {
            character_id: responder_id,
            text,
            urgency: decision.urgency,
            suggested_mood: decision.suggested_mood.clone(),
        }, vision_analysis))
    }

    async fn run_audit(
        &self,
        spec: &CharacterSpec,
        text: &str,
        observation: &Observation,
        model: &str,
    ) -> Result<String> {
        let schema = json!({
            "type": "object",
            "properties": {
                "status": { "type": "string", "enum": ["approve", "revise", "block"] },
                "text": { "type": "string" },
                "reason": { "type": "string" }
            },
            "required": ["status"]
        });
        let prompt = format!(
            "You are the self-audit system for {name}. Review the drafted reply and ensure it \
            matches tone, avoids repetition, and fits this context.\n\n\
            # Draft Reply\n{text}\n\n\
            # Screen Summary\n{summary}\n\n\
            # Recent Chat\n{chat}\n\n\
            Respond with status approve/revise/block. Provide revised text if needed.",
            name = spec.name,
            summary = observation.screen_summary.notes,
            chat = format_chat(&observation.recent_chat)
        );
        let result = self.llm.complete_json(model, &prompt, schema).await?;
        let audit: AuditResult = serde_json::from_value(result)?;

        match audit.status.as_str() {
            "approve" => Ok(text.to_string()),
            "revise" => Ok(audit.text.unwrap_or_else(|| text.to_string())),
            _ => Err(anyhow!(
                "Audit blocked response: {}",
                audit.reason.unwrap_or_default()
            )),
        }
    }

    fn build_arbiter_prompt_with_vision(&self, observation: &Observation, vision: Option<&VisionAnalysis>) -> String {
        let chat = format_chat(&observation.recent_chat);
        let character_section = self
            .characters
            .iter()
            .map(|c| {
                let cooldown: String =
                    if c.state.is_on_cooldown(self.config.cooldown_after_speak()) {
                        "on cooldown".to_string()
                    } else {
                        "available".to_string()
                    };
                format!(
                    "### {name} (id: {id})\nPersonality: {personality}\nStatus: {cooldown}\n",
                    name = c.spec.name,
                    id = c.spec.id,
                    personality = truncate(&c.spec.personality, 240),
                    cooldown = cooldown
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let screen_context = if let Some(v) = vision {
            format!(
                "## VLM Analysis\n**Activity**: {}\n**Warrants Response**: {}\n**Trigger**: {}\n**Companion Interest**: {}\n\n## Raw Screen Data\n{}",
                v.activity,
                v.warrants_response,
                v.response_trigger.as_deref().unwrap_or("none"),
                serde_json::to_string_pretty(&v.companion_interest).unwrap_or_default(),
                observation.screen_summary.notes
            )
        } else {
            format!("## Screen\n{}", observation.screen_summary.notes)
        };

        format!(
            "You orchestrate Dewet companions. Decide whether anyone should speak.\n\n\
            {screen_context}\n\n\
            # Recent Chat\n{chat}\n\n\
            # Companions\n{companions}\n\n\
            Consider social appropriateness, silence preference, and user focus.\n\n\
            ## Decision Criteria\n\
            - **Silence is golden.** Most of the time, don't respond.\n\
            - **Never be annoying.** Companions should feel like friends, not pop-up ads.\n\
            - **Respect focus.** If the user is in deep work, stay quiet unless addressed.\n\
            - **Be contextual.** Generic comments are worse than silence.",
            screen_context = screen_context,
            chat = chat,
            companions = character_section
        )
    }

    fn build_response_prompt(spec: &CharacterSpec, observation: &Observation) -> String {
        format!(
            "You are {name} ({id}). {description}\n\n\
            Personality: {personality}\nScenario: {scenario}\n\
            Stay in voice and respond naturally.\n\n\
            # Screen Summary\n{screen}\n\n\
            # Recent Chat\n{chat}\n\n\
            Respond conversationally.",
            name = spec.name,
            id = spec.id,
            description = spec.description,
            personality = spec.personality,
            scenario = spec.scenario,
            screen = observation.screen_summary.notes,
            chat = format_chat(&observation.recent_chat),
        )
    }
}

fn format_chat(packets: &[ChatPacket]) -> String {
    if packets.is_empty() {
        return "(no recent chat)".into();
    }
    packets
        .iter()
        .map(|p| format!("{}: {}", p.sender, p.content))
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate(input: &str, max: usize) -> String {
    if input.len() <= max {
        input.to_string()
    } else {
        format!("{}...", &input[..max])
    }
}

fn encode_rgba_to_base64(image: &RgbaImage) -> Result<String> {
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer);
    DynamicImage::ImageRgba8(image.clone()).write_to(&mut cursor, ImageFormat::Png)?;
    Ok(BASE64.encode(buffer))
}

#[derive(Debug)]
struct ModelCatalog {
    decision: String,
    response: String,
    audit: Option<String>,
}

impl From<LlmConfig> for ModelCatalog {
    fn from(config: LlmConfig) -> Self {
        Self {
            decision: config.decision_model,
            response: config.response_model,
            audit: config.audit_model,
        }
    }
}

fn arbiter_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "should_respond": { "type": "boolean" },
            "responder_id": { "type": "string", "description": "Character ID who should respond, or empty string if no one" },
            "reasoning": { "type": "string" },
            "suggested_mood": { "type": "string", "description": "Suggested mood, or empty string for neutral" },
            "urgency": { "type": "number", "minimum": 0.0, "maximum": 1.0 }
        },
        "required": ["should_respond", "responder_id", "reasoning", "suggested_mood", "urgency"]
    })
}

#[derive(Debug, Deserialize)]
struct ArbiterDecision {
    should_respond: bool,
    #[serde(deserialize_with = "deserialize_optional_string")]
    responder_id: Option<String>,
    reasoning: String,
    #[serde(deserialize_with = "deserialize_optional_string")]
    suggested_mood: Option<String>,
    urgency: f32,
}

#[derive(Debug, Deserialize)]
struct AuditResult {
    status: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

pub enum Decision {
    Pass,
    Speak {
        character_id: String,
        text: String,
        urgency: f32,
        suggested_mood: Option<String>,
    },
}

/// Analysis result from VLM about the current screen context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionAnalysis {
    /// What the user is currently doing
    pub activity: String,
    /// Whether the current context warrants a response
    pub warrants_response: bool,
    /// What triggered the potential response (empty string if none)
    #[serde(deserialize_with = "deserialize_optional_string")]
    pub response_trigger: Option<String>,
    /// Interest level per companion (character_id -> 0.0-1.0)
    pub companion_interest: Value,
}

fn deserialize_optional_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = serde::Deserialize::deserialize(deserializer)?;
    if s.is_empty() {
        Ok(None)
    } else {
        Ok(Some(s))
    }
}
