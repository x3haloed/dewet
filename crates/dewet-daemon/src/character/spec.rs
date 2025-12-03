use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterSpec {
    pub id: String,
    pub name: String,
    pub description: String,
    pub personality: String,
    pub scenario: String,
    pub system_prompt: String,
    pub mes_example: String,
    #[serde(default)]
    pub character_book: Vec<LoreEntry>,
    #[serde(default)]
    pub extensions: HashMap<String, Value>,
}

impl CharacterSpec {
    pub fn from_file(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("Failed to read character card {:?}", path))?;
        let spec: Self = if path
            .extension()
            .map(|ext| ext == "json" || ext == "ccv2")
            .unwrap_or(false)
        {
            serde_json::from_str(&raw)?
        } else {
            toml::from_str(&raw)?
        };
        Ok(spec)
    }

    pub fn load_dir(path: &Path) -> Result<Vec<Self>> {
        let mut specs = Vec::new();
        if !path.exists() {
            return Ok(Self::demo());
        }
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                match Self::from_file(&entry.path()) {
                    Ok(spec) => specs.push(spec),
                    Err(err) => tracing::warn!(?err, "Skipping character {:?}", entry.path()),
                }
            }
        }
        if specs.is_empty() {
            Ok(Self::demo())
        } else {
            Ok(specs)
        }
    }

    pub fn demo() -> Vec<Self> {
        vec![
            Self {
                id: "lyra".into(),
                name: "Lyra".into(),
                description: "A curious synth librarian who loves metaphors.".into(),
                personality: "Warm, analytical, occasionally teasing.".into(),
                scenario: "Lyra sits on the user's monitor bezel offering commentary.".into(),
                system_prompt: "Stay playful yet insightful. Reference memories when relevant."
                    .into(),
                mes_example: "Lyra: Sooo... copy-pasting docstrings again? Need a cheerleader?"
                    .into(),
                character_book: vec![LoreEntry {
                    content:
                        "Lyra has an archive of user successes and failures she gently recalls."
                            .into(),
                    is_public: true,
                }],
                extensions: HashMap::from([
                    ("interests".into(), Value::from(vec!["rust", "pixel art"])),
                    ("speech_style".into(), Value::from("playful, emoji-light")),
                ]),
            },
            Self {
                id: "orion".into(),
                name: "Orion".into(),
                description: "A focused engineer fascinated by low-level systems.".into(),
                personality: "Dry humor, pragmatic, protective of the user's focus.".into(),
                scenario: "Orion peers over logs and metrics, chiming in when something matters."
                    .into(),
                system_prompt: "Keep remarks concise, reference concrete evidence before advising."
                    .into(),
                mes_example: "Orion: Tests red, coffee empty. Want triage help or caffeine first?"
                    .into(),
                character_book: vec![],
                extensions: HashMap::new(),
            },
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoreEntry {
    pub content: String,
    #[serde(default)]
    pub is_public: bool,
}
