use std::{env, fs, path::Path, time::Duration};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub bridge: BridgeConfig,
    pub vision: VisionConfig,
    pub observation: ObservationConfig,
    pub storage: StorageConfig,
    pub director: DirectorConfig,
    pub llm: LlmConfig,
    pub tts: TtsConfig,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        if let Ok(path) = env::var("DEWET_CONFIG") {
            return Self::from_path(Path::new(&path));
        }

        let project_root = env::var("DEWET_ROOT").unwrap_or_else(|_| ".".to_string());
        let default_path = Path::new(&project_root).join("config/dewet.toml");
        if default_path.exists() {
            return Self::from_path(&default_path);
        }

        let example_path = Path::new(&project_root).join("config/dewet.example.toml");
        if example_path.exists() {
            tracing::warn!("Using example configuration at {:?}", example_path);
            return Self::from_path(&example_path);
        }

        Ok(Self::default())
    }

    fn from_path(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file {:?}", path))?;
        let config: Self =
            toml::from_str(&contents).with_context(|| format!("invalid config: {:?}", path))?;
        Ok(config)
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bridge: BridgeConfig::default(),
            vision: VisionConfig::default(),
            observation: ObservationConfig::default(),
            storage: StorageConfig::default(),
            director: DirectorConfig::default(),
            llm: LlmConfig::default(),
            tts: TtsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BridgeConfig {
    #[serde(default = "BridgeConfig::default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "BridgeConfig::default_max_clients")]
    pub max_clients: usize,
}

impl BridgeConfig {
    fn default_listen_addr() -> String {
        "127.0.0.1:7777".into()
    }
    fn default_max_clients() -> usize {
        4
    }
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            listen_addr: Self::default_listen_addr(),
            max_clients: Self::default_max_clients(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct VisionConfig {
    #[serde(default = "VisionConfig::default_capture_interval_ms")]
    pub capture_interval_ms: u64,
    #[serde(default = "VisionConfig::default_diff_threshold")]
    pub diff_threshold: f32,
    #[serde(default = "VisionConfig::default_max_history")]
    pub max_history: usize,
}

impl VisionConfig {
    fn default_capture_interval_ms() -> u64 {
        1500
    }
    fn default_diff_threshold() -> f32 {
        0.12
    }
    fn default_max_history() -> usize {
        12
    }

    pub fn capture_interval(&self) -> Duration {
        Duration::from_millis(self.capture_interval_ms)
    }
}

impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            capture_interval_ms: Self::default_capture_interval_ms(),
            diff_threshold: Self::default_diff_threshold(),
            max_history: Self::default_max_history(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ObservationConfig {
    #[serde(default = "ObservationConfig::default_chat_depth")]
    pub chat_depth: usize,
    #[serde(default = "ObservationConfig::default_screen_history")]
    pub screen_history: usize,
}

impl ObservationConfig {
    fn default_chat_depth() -> usize {
        30
    }
    fn default_screen_history() -> usize {
        8
    }
}

impl Default for ObservationConfig {
    fn default() -> Self {
        Self {
            chat_depth: Self::default_chat_depth(),
            screen_history: Self::default_screen_history(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "StorageConfig::default_url")]
    pub url: String,
    #[serde(default = "StorageConfig::default_auth_token_env")]
    pub auth_token_env: String,
}

impl StorageConfig {
    fn default_url() -> String {
        "libsql://dewet.turso.io".into()
    }
    fn default_auth_token_env() -> String {
        "TURSO_AUTH_TOKEN".into()
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            url: Self::default_url(),
            auth_token_env: Self::default_auth_token_env(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DirectorConfig {
    #[serde(default = "DirectorConfig::default_min_decision_interval_ms")]
    pub min_decision_interval_ms: u64,
    #[serde(default = "DirectorConfig::default_cooldown_after_speak_ms")]
    pub cooldown_after_speak_ms: u64,
}

impl DirectorConfig {
    fn default_min_decision_interval_ms() -> u64 {
        2000
    }
    fn default_cooldown_after_speak_ms() -> u64 {
        30_000
    }

    pub fn min_decision_interval(&self) -> Duration {
        Duration::from_millis(self.min_decision_interval_ms)
    }

    pub fn cooldown_after_speak(&self) -> Duration {
        Duration::from_millis(self.cooldown_after_speak_ms)
    }
}

impl Default for DirectorConfig {
    fn default() -> Self {
        Self {
            min_decision_interval_ms: Self::default_min_decision_interval_ms(),
            cooldown_after_speak_ms: Self::default_cooldown_after_speak_ms(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub decision_model: String,
    pub response_model: String,
    #[serde(default)]
    pub audit_model: Option<String>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::LmStudio {
                endpoint: "http://127.0.0.1:1234".into(),
            },
            decision_model: "qwen2.5-vl-7b-instruct".into(),
            response_model: "qwen2.5-7b-instruct".into(),
            audit_model: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum LlmProvider {
    #[serde(rename = "lmstudio")]
    LmStudio { endpoint: String },
    #[serde(rename = "openrouter")]
    OpenRouter {
        /// API key - can be literal or read from env var if api_key_env is set
        #[serde(default)]
        api_key: Option<String>,
        /// Environment variable name containing the API key
        #[serde(default)]
        api_key_env: Option<String>,
        #[serde(default)]
        site_url: Option<String>,
        #[serde(default)]
        site_name: Option<String>,
    },
}

impl LlmProvider {
    /// Get the OpenRouter API key, checking env var if specified
    pub fn openrouter_api_key(&self) -> Option<String> {
        match self {
            LlmProvider::OpenRouter { api_key, api_key_env, .. } => {
                // First try env var
                if let Some(env_name) = api_key_env {
                    if let Ok(key) = std::env::var(env_name) {
                        return Some(key);
                    }
                }
                // Fall back to literal key
                api_key.clone()
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TtsConfig {
    #[serde(default = "TtsConfig::default_provider")]
    pub provider: String,
}

impl TtsConfig {
    fn default_provider() -> String {
        "null".into()
    }
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            provider: Self::default_provider(),
        }
    }
}
