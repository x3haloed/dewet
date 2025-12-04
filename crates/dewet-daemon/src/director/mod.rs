use std::io::Cursor;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use image::{DynamicImage, ImageFormat, RgbaImage};
use serde::Deserialize;
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

    pub async fn evaluate(&mut self, observation: &Observation) -> Result<EvaluateResult> {
        let mut prompt_logs = Vec::new();

        if self.last_decision.elapsed() < self.config.min_decision_interval() {
            return Ok(EvaluateResult {
                decision: Decision::Pass {
                    reasoning: "Rate limited".to_string(),
                    urgency: 0.0,
                },
                prompt_logs,
            });
        }
        self.last_decision = Instant::now();

        // Build arbiter prompt with raw data only - no pre-baked VLM analysis
        let schema = arbiter_schema();
        let arbiter_prompt = self.build_arbiter_prompt(observation);
        let response = self
            .llm
            .complete_json(&self.models.decision, &arbiter_prompt, schema)
            .await?;
        
        // Log the arbiter prompt/response
        let arbiter_response_str = serde_json::to_string_pretty(&response).unwrap_or_default();
        prompt_logs.push(PromptLog {
            model_type: "arbiter".to_string(),
            model_name: self.models.decision.clone(),
            prompt: arbiter_prompt.clone(),
            response: arbiter_response_str,
        });

        let arbiter: ArbiterDecision = serde_json::from_value(response)?;

        info!(
            should_respond = arbiter.should_respond,
            responder = ?arbiter.responder_id,
            urgency = arbiter.urgency,
            reasoning = %arbiter.reasoning,
            "Arbiter decision"
        );

        self.storage
            .record_decision(&StoredDecision::now(
                arbiter.should_respond,
                arbiter.responder_id.clone(),
                arbiter.reasoning.clone(),
                arbiter.urgency,
            ))
            .await?;

        if !arbiter.should_respond {
            return Ok(EvaluateResult {
                decision: Decision::Pass {
                    reasoning: arbiter.reasoning,
                    urgency: arbiter.urgency,
                },
                prompt_logs,
            });
        }

        let responder_id = match &arbiter.responder_id {
            Some(id) if !id.is_empty() => id.clone(),
            _ => {
                info!("Arbiter said respond but no responder_id given");
                return Ok(EvaluateResult {
                    decision: Decision::Pass {
                        reasoning: format!("{} (no responder_id)", arbiter.reasoning),
                        urgency: arbiter.urgency,
                    },
                    prompt_logs,
                });
            }
        };

        let available_ids: Vec<_> = self.characters.iter().map(|c| c.spec.id.as_str()).collect();
        let Some(responder_index) = self
            .characters
            .iter()
            .position(|c| c.spec.id == responder_id)
        else {
            warn!(responder_id = %responder_id, available = ?available_ids, "Responder not found in character list");
            return Ok(EvaluateResult {
                decision: Decision::Pass {
                    reasoning: format!("{} (responder '{}' not found)", arbiter.reasoning, responder_id),
                    urgency: arbiter.urgency,
                },
                prompt_logs,
            });
        };

        {
            let character = &self.characters[responder_index];
            if character
                .state
                .is_on_cooldown(self.config.cooldown_after_speak())
            {
                info!(responder_id = %responder_id, "Character on cooldown, skipping");
                return Ok(EvaluateResult {
                    decision: Decision::Pass {
                        reasoning: format!("{} (on cooldown)", arbiter.reasoning),
                        urgency: arbiter.urgency,
                    },
                    prompt_logs,
                });
            }
        }

        info!(responder_id = %responder_id, "Generating response...");

        let response_prompt =
            Self::build_response_prompt(&self.characters[responder_index].spec, observation);
        
        // Use vision model if composite image is available
        let mut text = if let Some(composite) = &observation.composite {
            // Build list of images: composite first, then ARIAOS if available
            let mut images = vec![encode_rgba_to_base64(composite)?];
            if let Some(ariaos) = &observation.ariaos {
                images.push(encode_rgba_to_base64(ariaos)?);
            }
            self.llm
                .complete_vision_text(&self.models.response, &response_prompt, images)
                .await?
        } else {
            self.llm
                .complete_text(&self.models.response, &response_prompt)
                .await?
        };

        // Log the response prompt/response
        prompt_logs.push(PromptLog {
            model_type: "response".to_string(),
            model_name: self.models.response.clone(),
            prompt: response_prompt,
            response: text.clone(),
        });

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
                    return Ok(EvaluateResult {
                        decision: Decision::Pass {
                            reasoning: format!("{} (audit rejected: {})", arbiter.reasoning, err),
                            urgency: arbiter.urgency,
                        },
                        prompt_logs,
                    });
                }
            };
        }

        if let Some(character) = self.characters.get_mut(responder_index) {
            character.state.update_last_spoke();
        }

        Ok(EvaluateResult {
            decision: Decision::Speak {
                character_id: responder_id,
                reasoning: arbiter.reasoning,
                text,
                urgency: arbiter.urgency,
                suggested_mood: arbiter.suggested_mood.clone(),
            },
            prompt_logs,
        })
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

    fn build_arbiter_prompt(&self, observation: &Observation) -> String {
        let chat = format_chat(&observation.recent_chat);
        let character_section = self
            .characters
            .iter()
            .map(|c| {
                let cooldown: String = if c.state.is_on_cooldown(self.config.cooldown_after_speak())
                {
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

        // Format time since user message
        let silence_note = if observation.seconds_since_user_message == u64::MAX {
            "User has not spoken yet.".to_string()
        } else if observation.seconds_since_user_message < 5 {
            "User just spoke.".to_string()
        } else {
            format!("{}s since user last spoke.", observation.seconds_since_user_message)
        };

        // Check if last message in chat is from user (unanswered)
        let last_speaker = observation.recent_chat.last().map(|p| p.sender.as_str());
        let unanswered_user = last_speaker == Some("user");

        format!(
            "You orchestrate Dewet companions. Decide whether anyone should speak.\n\n\
            # Understanding the Composite Image\n\
            The image shows the user's context in multiple panels:\n\
            - **DESKTOP**: Current screen capture\n\
            - **PREV 1/2/3** (if present): Historical screenshots from when companions last responded - compare these to DESKTOP to see what changed\n\
            - **RECENT CHAT**: Conversation history\n\
            - **MEMORY/STATUS**: Context visualization\n\n\
            # Screen Data\n{screen}\n\
            **Timing**: {silence}\n\
            **Last speaker**: {last_speaker}\n\n\
            # Recent Chat\n{chat}\n\n\
            # Companions\n{companions}\n\n\
            ## When to Respond (ONLY if one of these is true)\n\
            1. **Unanswered user message**: The user said something that appears to be addressing a companion (question, greeting, request) and no companion has responded yet.\n\
            2. **Significant context change**: Comparing DESKTOP to the PREV panels shows something meaningfully different happened (new app, new content, completed task, error appeared) that a companion would naturally comment on.\n\n\
            ## When NOT to Respond\n\
            - Nothing significant changed since the last PREV screenshot\n\
            - The last chat message was from the same companion (don't pile on)\n\
            **Default to should_respond: false** unless criterion 1 or 2 clearly applies.",
            screen = observation.screen_summary.notes,
            silence = silence_note,
            last_speaker = if unanswered_user { "user (UNANSWERED)" } else { last_speaker.unwrap_or("none") },
            chat = chat,
            companions = character_section
        )
    }

    fn build_response_prompt(spec: &CharacterSpec, observation: &Observation) -> String {
        let ariaos_note = if observation.ariaos.is_some() {
            "\n\n# Your Dashboard (ARIAOS)\n\
            The second image shows your personal ARIAOS display - your notes, focus tracking, \
            and activity log. Use this context to inform your response, but don't explicitly \
            mention ARIAOS to the user."
        } else {
            ""
        };
        
        format!(
            "You are {name} ({id}). {description}\n\n\
            Personality: {personality}\nScenario: {scenario}\n\
            Stay in voice and respond naturally.\n\n\
            # What's Happening\n{screen}{ariaos}\n\n\
            # Recent Chat\n{chat}\n\n\
            Respond conversationally based on what you see and the conversation so far.",
            name = spec.name,
            id = spec.id,
            description = spec.description,
            personality = spec.personality,
            scenario = spec.scenario,
            screen = observation.screen_summary.notes,
            ariaos = ariaos_note,
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
    Pass {
        reasoning: String,
        urgency: f32,
    },
    Speak {
        character_id: String,
        text: String,
        urgency: f32,
        reasoning: String,
        suggested_mood: Option<String>,
    },
}

/// Log of a prompt/response exchange with a model
#[derive(Debug, Clone)]
pub struct PromptLog {
    /// "arbiter" or "response"
    pub model_type: String,
    /// The model name used
    pub model_name: String,
    /// The full prompt text (images stripped)
    pub prompt: String,
    /// The model's response
    pub response: String,
}

/// Result of evaluate() including prompt logs for debugging
pub struct EvaluateResult {
    pub decision: Decision,
    pub prompt_logs: Vec<PromptLog>,
}

fn deserialize_optional_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = serde::Deserialize::deserialize(deserializer)?;
    if s.is_empty() { Ok(None) } else { Ok(Some(s)) }
}
