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

/// Character Card v2 wrapper format
#[derive(Debug, Deserialize)]
struct CharacterCardV2 {
    #[allow(dead_code)]
    spec: String,
    #[allow(dead_code)]
    spec_version: String,
    data: CharacterCardV2Data,
}

#[derive(Debug, Deserialize)]
struct CharacterCardV2Data {
    name: String,
    description: String,
    personality: String,
    scenario: String,
    system_prompt: String,
    mes_example: String,
    #[serde(default)]
    character_book: Option<CharacterBookV2>,
    #[serde(default)]
    extensions: HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
struct CharacterBookV2 {
    #[serde(default)]
    entries: Vec<CharacterBookEntryV2>,
}

#[derive(Debug, Deserialize)]
struct CharacterBookEntryV2 {
    content: String,
    #[serde(default)]
    selective: bool,
    #[serde(default)]
    comment: Option<String>,
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
            // Try CCv2 format first, then flat format
            if let Ok(ccv2) = serde_json::from_str::<CharacterCardV2>(&raw) {
                Self::from_ccv2(ccv2)?
            } else {
                serde_json::from_str(&raw)?
            }
        } else {
            toml::from_str(&raw)?
        };
        Ok(spec)
    }

    /// Convert CCv2 format to our internal format
    fn from_ccv2(ccv2: CharacterCardV2) -> Result<Self> {
        let data = ccv2.data;

        // Extract ID from extensions or generate from name
        let id = data
            .extensions
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| data.name.to_lowercase().replace(' ', "_"));

        // Convert character book entries
        let character_book = data
            .character_book
            .map(|book| {
                book.entries
                    .into_iter()
                    .map(|entry| LoreEntry {
                        content: entry.content,
                        is_public: !entry.selective,
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            id,
            name: data.name,
            description: data.description,
            personality: data.personality,
            scenario: data.scenario,
            system_prompt: data.system_prompt,
            mes_example: data.mes_example,
            character_book,
            extensions: data.extensions,
        })
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
