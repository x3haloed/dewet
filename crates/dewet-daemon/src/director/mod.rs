use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use serde::Deserialize;
use serde_json::{Value, json};

use tracing::warn;

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

    pub async fn evaluate(&mut self, observation: &Observation) -> Result<Decision> {
        if self.last_decision.elapsed() < self.config.min_decision_interval() {
            return Ok(Decision::Pass);
        }
        self.last_decision = Instant::now();

        let schema = arbiter_schema();
        let prompt = self.build_arbiter_prompt(observation);
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
            return Ok(Decision::Pass);
        }

        let responder_id = match &decision.responder_id {
            Some(id) => id.clone(),
            None => return Ok(Decision::Pass),
        };

        let Some(responder_index) = self
            .characters
            .iter()
            .position(|c| c.spec.id == responder_id)
        else {
            return Ok(Decision::Pass);
        };

        {
            let character = &self.characters[responder_index];
            if character
                .state
                .is_on_cooldown(self.config.cooldown_after_speak())
            {
                return Ok(Decision::Pass);
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
                    return Ok(Decision::Pass);
                }
            };
        }

        if let Some(character) = self.characters.get_mut(responder_index) {
            character.state.update_last_spoke();
        }

        Ok(Decision::Speak {
            character_id: responder_id,
            text,
            urgency: decision.urgency,
            suggested_mood: decision.suggested_mood.clone(),
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

        format!(
            "You orchestrate Dewet companions. Decide whether anyone should speak.\n\n\
            # Screen\n{screen}\n\n\
            # Recent Chat\n{chat}\n\n\
            # Companions\n{companions}\n\n\
            Consider social appropriateness, silence preference, and user focus.",
            screen = observation.screen_summary.notes,
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
            "responder_id": { "type": ["string", "null"] },
            "reasoning": { "type": "string" },
            "suggested_mood": { "type": ["string", "null"] },
            "urgency": { "type": "number", "minimum": 0.0, "maximum": 1.0 }
        },
        "required": ["should_respond", "reasoning", "urgency"]
    })
}

#[derive(Debug, Deserialize)]
struct ArbiterDecision {
    should_respond: bool,
    responder_id: Option<String>,
    reasoning: String,
    #[serde(default)]
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
