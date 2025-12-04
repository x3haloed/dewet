mod lmstudio;
mod openrouter;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

pub use lmstudio::LmStudioClient;
pub use openrouter::OpenRouterClient;

use crate::config::{LlmConfig, LlmProvider, ModelConfig};

pub type SharedLlm = Arc<dyn LlmClient>;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete_text(&self, model: &str, prompt: &str) -> Result<String>;

    async fn complete_json(&self, model: &str, prompt: &str, schema: Value) -> Result<Value>;

    async fn complete_vision_text(
        &self,
        model: &str,
        prompt: &str,
        images_base64: Vec<String>,
    ) -> Result<String>;

    async fn complete_vision_json(
        &self,
        model: &str,
        prompt: &str,
        images_base64: Vec<String>,
        schema: Value,
    ) -> Result<Value>;
}

/// Collection of LLM clients for different roles
#[derive(Clone)]
pub struct LlmClients {
    /// Client for VLA (Vision-Language Analysis) - change detection
    pub vla: SharedLlm,
    pub vla_model: String,
    /// Client for Arbiter - decision making
    pub arbiter: SharedLlm,
    pub arbiter_model: String,
    /// Client for Response generation - character dialogue
    pub response: SharedLlm,
    pub response_model: String,
    /// Optional client for Audit - response review
    pub audit: Option<(SharedLlm, String)>,
}

impl LlmClients {
    pub fn from_config(config: &LlmConfig) -> Self {
        Self {
            vla: create_client_from_provider(&config.vla.provider),
            vla_model: config.vla.model.clone(),
            arbiter: create_client_from_provider(&config.arbiter.provider),
            arbiter_model: config.arbiter.model.clone(),
            response: create_client_from_provider(&config.response.provider),
            response_model: config.response.model.clone(),
            audit: config.audit.as_ref().map(|a| {
                (create_client_from_provider(&a.provider), a.model.clone())
            }),
        }
    }
}

/// Create a client from a provider configuration
pub fn create_client_from_provider(provider: &LlmProvider) -> SharedLlm {
    match provider {
        LlmProvider::LmStudio { endpoint } => Arc::new(LmStudioClient::new(endpoint)),
        LlmProvider::OpenRouter {
            site_url,
            site_name,
            ..
        } => {
            let api_key = provider.openrouter_api_key()
                .expect("OpenRouter requires api_key or api_key_env to be set");
            Arc::new(OpenRouterClient::new(
                &api_key,
                site_url.clone(),
                site_name.clone(),
            ))
        }
    }
}

/// Create a client from a model configuration (convenience wrapper)
pub fn create_client(config: &ModelConfig) -> SharedLlm {
    create_client_from_provider(&config.provider)
}
