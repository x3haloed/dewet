use std::io::Cursor;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use image::{DynamicImage, ImageFormat, RgbaImage};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use tracing::{debug, info, warn};

use crate::{
    bridge::ChatPacket,
    character::{CharacterSpec, LoadedCharacter},
    config::DirectorConfig,
    llm::{ChatMessage, LlmClients, strip_images_for_logging},
    observation::Observation,
    storage::{Storage, StoredDecision},
};

/// Result of VLA (Vision-Language Analysis)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlaResult {
    /// Whether something significant changed that warrants attention
    pub significant_change: bool,
    /// What changed (or "nothing significant" if no change)
    pub description: String,
}

/// Eligibility status for a companion
#[derive(Debug, Clone)]
pub enum CompanionEligibility {
    /// Companion is allowed to speak
    Allow { reason: String },
    /// Companion should not speak
    Stop { reason: String },
}

impl CompanionEligibility {
    pub fn is_allowed(&self) -> bool {
        matches!(self, CompanionEligibility::Allow { .. })
    }
}

pub struct Director {
    storage: Storage,
    clients: LlmClients,
    config: DirectorConfig,
    characters: Vec<LoadedCharacter>,
    last_decision: Instant,
}

impl Director {
    pub fn new(
        storage: Storage,
        clients: LlmClients,
        director_config: DirectorConfig,
        characters: Vec<LoadedCharacter>,
    ) -> Self {
        Self {
            storage,
            clients,
            config: director_config,
            characters,
            last_decision: Instant::now()
                .checked_sub(Duration::from_secs(3600))
                .unwrap_or_else(Instant::now),
        }
    }

    pub fn characters(&self) -> &[LoadedCharacter] {
        &self.characters
    }

    /// Step 1: VLA (Vision-Language Analysis) - determine if something significant changed
    pub async fn analyze_vla(&self, observation: &Observation) -> Result<(VlaResult, PromptLog)> {
        let composite = observation
            .composite
            .as_ref()
            .ok_or_else(|| anyhow!("No composite image available for VLA"))?;

        // Build image list: composite first, then ARIAOS if available
        let mut images = vec![encode_rgba_to_base64(composite)?];
        let has_ariaos = observation.ariaos.is_some();
        if let Some(ariaos) = &observation.ariaos {
            images.push(encode_rgba_to_base64(ariaos)?);
        }

        let prompt = if has_ariaos {
            r#"You are a CHANGE DETECTOR. Your ONLY job: determine if something MEANINGFULLY DIFFERENT happened.

**IMAGE 1 - COMPOSITE** layout:
- DESKTOP (top-left): Current screen
- PREV 1/2/3: Previous screenshots

**IMAGE 2 - ARIAOS**: Companion's dashboard

## YOUR TASK
Compare DESKTOP directly to the PREV panels. Answer ONE question:
**Is DESKTOP showing something MEANINGFULLY DIFFERENT from PREV?**

### significant_change: TRUE only if:
- User opened a DIFFERENT application (not just the same app with minor changes)
- Completely NEW content appeared (new file, new webpage, new document)
- An error, alert, or notification popped up
- The ARIAOS notes content changed

### significant_change: FALSE if:
- Same application, same general content
- Cursor position changed
- Scroll position changed slightly  
- Chat messages updated (we already see this in chat history)
- Time passed but nothing substantive changed
- Screen looks "basically the same"

**DEFAULT TO FALSE.** Only mark true if you can point to a specific, concrete difference that a human would notice and find noteworthy."#
        } else {
            r#"You are a CHANGE DETECTOR. Your ONLY job: determine if something MEANINGFULLY DIFFERENT happened.

**IMAGE 1 - COMPOSITE** layout:
- DESKTOP (top-left): Current screen
- PREV 1/2/3: Previous screenshots

## YOUR TASK
Compare DESKTOP directly to the PREV panels. Answer ONE question:
**Is DESKTOP showing something MEANINGFULLY DIFFERENT from PREV?**

### significant_change: TRUE only if:
- User opened a DIFFERENT application (not just the same app with minor changes)
- Completely NEW content appeared (new file, new webpage, new document)
- An error, alert, or notification popped up

### significant_change: FALSE if:
- Same application, same general content
- Cursor position changed
- Scroll position changed slightly
- Chat messages updated (we already see this in chat history)
- Time passed but nothing substantive changed
- Screen looks "basically the same"

**DEFAULT TO FALSE.** Only mark true if you can point to a specific, concrete difference that a human would notice and find noteworthy."#
        };

        let schema = json!({
            "type": "object",
            "properties": {
                "significant_change": {
                    "type": "boolean",
                    "description": "true if something meaningful changed, false otherwise"
                },
                "description": {
                    "type": "string",
                    "description": "Brief description of what changed (or 'nothing significant' if no change)"
                }
            },
            "required": ["significant_change", "description"]
        });

        let response = self
            .clients
            .vla
            .complete_vision_json(&self.clients.vla_model, prompt, images, schema)
            .await?;

        let response_str = serde_json::to_string_pretty(&response).unwrap_or_default();
        let prompt_log = PromptLog {
            model_type: "vla".to_string(),
            model_name: self.clients.vla_model.clone(),
            prompt: prompt.to_string(),
            response: response_str,
        };

        let vla: VlaResult = serde_json::from_value(response)?;
        info!(
            significant_change = vla.significant_change,
            description = %vla.description,
            "VLA complete"
        );

        Ok((vla, prompt_log))
    }

    /// Step 2: Determine eligibility for each companion (algorithmic, no LLM)
    fn compute_eligibility(
        &self,
        observation: &Observation,
        vla: &VlaResult,
    ) -> Vec<(String, CompanionEligibility)> {
        let last_speaker = observation.recent_chat.last().map(|p| p.sender.as_str());
        let long_silence_threshold = self.config.cooldown_after_speak();

        self.characters
            .iter()
            .map(|c| {
                let id = c.spec.id.clone();
                let is_last_speaker = last_speaker == Some(id.as_str());

                let eligibility = if is_last_speaker {
                    // This companion spoke last
                    let time_since_spoke = c.state.time_since_last_spoke();
                    let long_time = time_since_spoke
                        .map(|d| d > long_silence_threshold)
                        .unwrap_or(true);

                    if long_time {
                        CompanionEligibility::Allow {
                            reason: format!(
                                "Last speaker, but {}s since speaking (threshold: {}s)",
                                time_since_spoke.map(|d| d.as_secs()).unwrap_or(0),
                                long_silence_threshold.as_secs()
                            ),
                        }
                    } else if vla.significant_change {
                        CompanionEligibility::Allow {
                            reason: format!(
                                "Last speaker, but VLA-YES: {}",
                                vla.description
                            ),
                        }
                    } else {
                        CompanionEligibility::Stop {
                            reason: format!(
                                "Last speaker, only {}s ago, VLA-NO",
                                time_since_spoke.map(|d| d.as_secs()).unwrap_or(0)
                            ),
                        }
                    }
                } else {
                    // Different companion spoke last (or no one spoke yet)
                    CompanionEligibility::Allow {
                        reason: "Not last speaker".to_string(),
                    }
                };

                debug!(
                    companion = %id,
                    eligibility = ?eligibility,
                    "Computed eligibility"
                );

                (id, eligibility)
            })
            .collect()
    }

    pub async fn evaluate(&mut self, observation: &Observation) -> Result<EvaluateResult> {
        let mut prompt_logs = Vec::new();

        // Rate limiting check
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

        // Check if user just spoke (unanswered message)
        let last_speaker = observation.recent_chat.last().map(|p| p.sender.as_str());
        let user_unanswered = last_speaker == Some("user");

        // STEP 1: VLA - Vision-Language Analysis
        let vla = if observation.composite.is_some() {
            match self.analyze_vla(observation).await {
                Ok((result, log)) => {
                    prompt_logs.push(log);
                    result
                }
                Err(err) => {
                    warn!(?err, "VLA failed, assuming no significant change");
                    VlaResult {
                        significant_change: false,
                        description: format!("VLA failed: {}", err),
                    }
                }
            }
        } else {
            VlaResult {
                significant_change: false,
                description: "No composite image available".to_string(),
            }
        };

        // STEP 2: Compute eligibility for each companion
        let eligibilities = self.compute_eligibility(observation, &vla);

        // Filter to only ALLOW companions
        let allowed_companions: Vec<_> = eligibilities
            .iter()
            .filter(|(_, e)| e.is_allowed())
            .map(|(id, e)| (id.clone(), e.clone()))
            .collect();

        // If no companions are allowed, stop here
        if allowed_companions.is_empty() {
            let reasons: Vec<_> = eligibilities
                .iter()
                .filter_map(|(id, e)| match e {
                    CompanionEligibility::Stop { reason } => Some(format!("{}: {}", id, reason)),
                    _ => None,
                })
                .collect();

            info!(
                vla_change = vla.significant_change,
                "No companions eligible to speak"
            );

            return Ok(EvaluateResult {
                decision: Decision::Pass {
                    reasoning: format!("No eligible companions. {}", reasons.join("; ")),
                    urgency: 0.0,
                },
                prompt_logs,
            });
        }

        // HARD GATE: If user has been silent for 5+ minutes AND no VLA change AND no unanswered user message,
        // skip the arbiter entirely - there's clearly no stimulus worth responding to
        let user_silence_threshold_secs = 300; // 5 minutes
        if !user_unanswered 
            && !vla.significant_change 
            && observation.seconds_since_user_message > user_silence_threshold_secs
        {
            info!(
                user_silence_secs = observation.seconds_since_user_message,
                vla_change = vla.significant_change,
                "No stimulus: user silent and no VLA change - skipping arbiter"
            );

            return Ok(EvaluateResult {
                decision: Decision::Pass {
                    reasoning: format!(
                        "No stimulus: user silent for {}s, VLA detected no change",
                        observation.seconds_since_user_message
                    ),
                    urgency: 0.0,
                },
                prompt_logs,
            });
        }

        // STEP 3: Arbiter - given ALLOW companions, who (if anyone) should speak?
        let arbiter_prompt = self.build_arbiter_prompt(observation, &vla, &allowed_companions, user_unanswered);
        let schema = arbiter_schema();
        
        // Arbiter gets vision context too - helps make better decisions about what's on screen
        let response = if let Some(composite) = &observation.composite {
            let mut images = vec![encode_rgba_to_base64(composite)?];
            if let Some(ariaos) = &observation.ariaos {
                images.push(encode_rgba_to_base64(ariaos)?);
            }
            self.clients
                .arbiter
                .complete_vision_json(&self.clients.arbiter_model, &arbiter_prompt, images, schema)
                .await?
        } else {
            self.clients
                .arbiter
                .complete_json(&self.clients.arbiter_model, &arbiter_prompt, schema)
                .await?
        };

        let arbiter_response_str = serde_json::to_string_pretty(&response).unwrap_or_default();
        prompt_logs.push(PromptLog {
            model_type: "arbiter".to_string(),
            model_name: self.clients.arbiter_model.clone(),
            prompt: arbiter_prompt.clone(),
            response: arbiter_response_str,
        });

        let arbiter: ArbiterDecision = serde_json::from_value(response)?;

        info!(
            who_should_talk = ?arbiter.who_should_talk,
            reasoning = %arbiter.reasoning,
            "Arbiter decision"
        );

        // Record the decision
        let should_respond = arbiter.who_should_talk.is_some();
        self.storage
            .record_decision(&StoredDecision::now(
                should_respond,
                arbiter.who_should_talk.clone(),
                arbiter.reasoning.clone(),
                if should_respond { 0.5 } else { 0.0 },
            ))
            .await?;

        // If arbiter says "none", we're done
        let responder_id = match &arbiter.who_should_talk {
            Some(id) if !id.is_empty() && id.to_lowercase() != "none" => id.clone(),
            _ => {
                return Ok(EvaluateResult {
                    decision: Decision::Pass {
                        reasoning: arbiter.reasoning,
                        urgency: 0.0,
                    },
                    prompt_logs,
                });
            }
        };

        // Validate the responder exists and is in the allowed list
        let Some(responder_index) = self
            .characters
            .iter()
            .position(|c| c.spec.id == responder_id)
        else {
            warn!(responder_id = %responder_id, "Arbiter chose unknown companion");
            return Ok(EvaluateResult {
                decision: Decision::Pass {
                    reasoning: format!("{} (unknown companion '{}')", arbiter.reasoning, responder_id),
                    urgency: 0.0,
                },
                prompt_logs,
            });
        };

        if !allowed_companions.iter().any(|(id, _)| id == &responder_id) {
            warn!(responder_id = %responder_id, "Arbiter chose ineligible companion");
            return Ok(EvaluateResult {
                decision: Decision::Pass {
                    reasoning: format!("{} (companion '{}' not eligible)", arbiter.reasoning, responder_id),
                    urgency: 0.0,
                },
                prompt_logs,
            });
        }

        // Check cooldown - BUT bypass if:
        // 1. User has an unanswered message (always respond to direct interaction)
        // 2. VLA detected a significant change (something new happened worth commenting on)
        let bypass_cooldown = user_unanswered || vla.significant_change;
        if !bypass_cooldown
            && self.characters[responder_index]
                .state
                .is_on_cooldown(self.config.cooldown_after_speak())
        {
            info!(responder_id = %responder_id, "Character on cooldown, skipping");
            return Ok(EvaluateResult {
                decision: Decision::Pass {
                    reasoning: format!("{} (on cooldown)", arbiter.reasoning),
                    urgency: 0.0,
                },
                prompt_logs,
            });
        }

        // STEP 4: Generate response using proper chat message structure
        info!(responder_id = %responder_id, "Generating response...");

        // Build images list for the message
        let images = if let Some(composite) = &observation.composite {
            let mut imgs = vec![encode_rgba_to_base64(composite)?];
            if let Some(ariaos) = &observation.ariaos {
                imgs.push(encode_rgba_to_base64(ariaos)?);
            }
            imgs
        } else {
            vec![]
        };

        // Build proper chat messages with turn structure
        let response_messages = Self::build_response_messages(
            &self.characters[responder_index].spec,
            observation,
            images,
        );

        // Serialize messages for logging (strip images to keep logs readable)
        let response_prompt_json = serde_json::to_string_pretty(&strip_images_for_logging(&response_messages))
            .unwrap_or_else(|_| "(failed to serialize)".to_string());

        // Use chat completion for proper turn-taking
        let mut text = self
            .clients
            .response
            .complete_vision_chat(&self.clients.response_model, response_messages)
            .await?;

        prompt_logs.push(PromptLog {
            model_type: "response".to_string(),
            model_name: self.clients.response_model.clone(),
            prompt: response_prompt_json,
            response: text.clone(),
        });

        // Optional audit
        if let Some((audit_client, audit_model)) = &self.clients.audit {
            text = match self
                .run_audit(
                    &self.characters[responder_index].spec,
                    &text,
                    observation,
                    audit_client.as_ref(),
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
                            urgency: 0.0,
                        },
                        prompt_logs,
                    });
                }
            };
        }

        // Update character state
        if let Some(character) = self.characters.get_mut(responder_index) {
            character.state.update_last_spoke();
        }

        Ok(EvaluateResult {
            decision: Decision::Speak {
                character_id: responder_id,
                reasoning: arbiter.reasoning,
                text,
                urgency: 0.5,
                suggested_mood: None,
            },
            prompt_logs,
        })
    }

    async fn run_audit(
        &self,
        spec: &CharacterSpec,
        text: &str,
        observation: &Observation,
        client: &dyn crate::llm::LlmClient,
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
        let result = client.complete_json(model, &prompt, schema).await?;
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

    fn build_arbiter_prompt(
        &self,
        observation: &Observation,
        vla: &VlaResult,
        allowed_companions: &[(String, CompanionEligibility)],
        user_unanswered: bool,
    ) -> String {
        let chat = format_chat(&observation.recent_chat);

        // Build character section ONLY for allowed companions
        let character_section = allowed_companions
            .iter()
            .filter_map(|(id, eligibility)| {
                let character = self.characters.iter().find(|c| &c.spec.id == id)?;
                let reason = match eligibility {
                    CompanionEligibility::Allow { reason } => reason.clone(),
                    _ => return None,
                };
                Some(format!(
                    "### {name} (id: {id})\n\
                    Personality: {personality}\n\
                    Description: {description}\n\
                    Scenario: {scenario}\n\
                    Eligible because: {reason}\n",
                    name = character.spec.name,
                    id = character.spec.id,
                    personality = truncate(&character.spec.personality, 300),
                    description = truncate(&character.spec.description, 200),
                    scenario = truncate(&character.spec.scenario, 200),
                    reason = reason
                ))
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

        let last_speaker = observation.recent_chat.last().map(|p| p.sender.as_str());

        // VLA summary
        let vla_summary = if vla.significant_change {
            format!("**VLA: SIGNIFICANT CHANGE DETECTED**\n{}", vla.description)
        } else {
            format!("**VLA: No significant change**\n{}", vla.description)
        };

        // Image layout explanation (only if we have images)
        let image_context = if observation.composite.is_some() {
            let ariaos_note = if observation.ariaos.is_some() {
                "\n\n**IMAGE 2 - ARIAOS**: The companion's personal dashboard showing their notes, focus tracking, and activity log."
            } else {
                ""
            };
            format!(
                r#"# Visual Context
**IMAGE 1 - COMPOSITE** layout:
- DESKTOP (top-left): The user's current screen
- PREV 1/2/3 (right side): Previous screenshots for temporal context
- MEMORY/CHAT/STATUS panels: Optical memory visualization{ariaos}

Use these images to understand what the user is doing and whether a companion comment would be welcome or intrusive.

"#,
                ariaos = ariaos_note
            )
        } else {
            String::new()
        };

        format!(
            r#"You are the Arbiter for Dewet companions. Your job: decide WHO (if anyone) should speak.

{image_context}# Context Analysis
{vla}

# Timing
{silence}
Last speaker: {last_speaker}

# Recent Chat
{chat}

# Eligible Companions
These companions have passed eligibility checks and MAY speak:
{companions}

# Your Decision

You must choose ONE of:
1. **A specific companion ID** - if that companion has something valuable to say
2. **"none"** - if silence is the better choice

## When to pick a companion:
- User asked a question or made a comment that deserves a response
- VLA detected a significant change that a companion would naturally comment on
- A companion has unique insight relevant to the current context

## When to pick "none":
- The recent chat shows the companion already commented on this topic
- Nothing new has happened worth discussing
- The user appears focused and shouldn't be interrupted
- Any response would feel repetitive or forced

**Default to "none" unless there's a clear reason to speak.**"#,
            image_context = image_context,
            vla = vla_summary,
            silence = silence_note,
            last_speaker = if user_unanswered { 
                "user (UNANSWERED - prioritize responding!)" 
            } else { 
                last_speaker.unwrap_or("none") 
            },
            chat = chat,
            companions = character_section
        )
    }

    /// Build response prompt as proper chat messages with turn structure.
    /// This helps the model distinguish its own voice from the user's.
    fn build_response_messages(
        spec: &CharacterSpec,
        observation: &Observation,
        images_base64: Vec<String>,
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // System message: character's system_prompt plus their card details
        let system_content = format!(
            "{system_prompt}\n\n\
            Character: {name} ({id})\n\
            Description: {description}\n\
            Personality: {personality}\n\
            Scenario: {scenario}",
            system_prompt = spec.system_prompt,
            name = spec.name,
            id = spec.id,
            description = spec.description,
            personality = spec.personality,
            scenario = spec.scenario,
        );
        messages.push(ChatMessage::system(system_content));

        // Convert chat history into proper user/assistant turns
        for packet in &observation.recent_chat {
            let sender_lower = packet.sender.to_lowercase();
            if sender_lower == "user" {
                // User's messages are user turns
                messages.push(ChatMessage::user(&packet.content));
            } else if sender_lower == spec.id.to_lowercase() || sender_lower == spec.name.to_lowercase() {
                // This character's previous messages become assistant turns
                messages.push(ChatMessage::assistant(&packet.content));
            } else {
                // Other characters' messages shown as user turns with speaker prefix
                // so the model sees the full conversation but knows it's not its own voice
                let prefixed = format!("[{}]: {}", packet.sender, packet.content);
                messages.push(ChatMessage::user(prefixed));
            }
        }

        // Final user message with current context (what's on screen)
        let ariaos_note = if observation.ariaos.is_some() {
            "\n\nThe second image shows your personal dashboard - your notes, focus tracking, \
            and activity log. Use this to inform your response, but don't mention it explicitly."
        } else {
            ""
        };

        let context_content = format!(
            "[Current context: {screen}{ariaos}]\n\n\
            Respond conversationally based on what you see.",
            screen = observation.screen_summary.notes,
            ariaos = ariaos_note,
        );

        // If we have images, attach them to the final context message
        if !images_base64.is_empty() {
            messages.push(ChatMessage::user_with_images(context_content, images_base64));
        } else {
            messages.push(ChatMessage::user(context_content));
        }

        messages
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

fn arbiter_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "who_should_talk": { 
                "type": "string", 
                "description": "The companion ID who should speak, or 'none' if no one should" 
            },
            "reasoning": { 
                "type": "string",
                "description": "Brief explanation of why this companion should speak (or why no one should)"
            }
        },
        "required": ["who_should_talk", "reasoning"]
    })
}

#[derive(Debug, Deserialize)]
struct ArbiterDecision {
    #[serde(deserialize_with = "deserialize_optional_string")]
    who_should_talk: Option<String>,
    reasoning: String,
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
