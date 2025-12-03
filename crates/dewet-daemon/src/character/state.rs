use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct CharacterState {
    pub current_mood: String,
    pub last_spoke_at: Option<Instant>,
    pub relationship_score: f32,
}

impl CharacterState {
    pub fn new() -> Self {
        Self {
            current_mood: "neutral".into(),
            last_spoke_at: None,
            relationship_score: 0.5,
        }
    }

    pub fn update_last_spoke(&mut self) {
        self.last_spoke_at = Some(Instant::now());
    }

    pub fn is_on_cooldown(&self, cooldown: Duration) -> bool {
        self.last_spoke_at
            .map(|ts| ts.elapsed() < cooldown)
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone)]
pub struct LoadedCharacter {
    pub spec: crate::character::spec::CharacterSpec,
    pub state: CharacterState,
}

impl LoadedCharacter {
    pub fn new(spec: crate::character::spec::CharacterSpec) -> Self {
        Self {
            spec,
            state: CharacterState::new(),
        }
    }
}
