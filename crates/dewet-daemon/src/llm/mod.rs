mod lmstudio;
mod openrouter;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

pub use lmstudio::LmStudioClient;
pub use openrouter::OpenRouterClient;

use crate::config::{LlmConfig, LlmProvider};

pub type SharedLlm = Arc<dyn LlmClient>;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete_text(&self, model: &str, prompt: &str) -> Result<String>;

    async fn complete_json(&self, model: &str, prompt: &str, schema: Value) -> Result<Value>;

    async fn complete_vision_json(
        &self,
        model: &str,
        prompt: &str,
        images_base64: Vec<String>,
        schema: Value,
    ) -> Result<Value>;
}

pub fn create_client(config: &LlmConfig) -> SharedLlm {
    match &config.provider {
        LlmProvider::LmStudio { endpoint } => Arc::new(LmStudioClient::new(endpoint)),
        LlmProvider::OpenRouter {
            api_key,
            site_url,
            site_name,
        } => Arc::new(OpenRouterClient::new(
            api_key,
            site_url.clone(),
            site_name.clone(),
        )),
    }
}
