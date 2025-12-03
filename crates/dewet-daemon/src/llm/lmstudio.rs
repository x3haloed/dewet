use anyhow::{Result, anyhow};
use reqwest::Client;
use serde_json::Value;
use serde_json::json;
use tracing;

use super::LlmClient;

pub struct LmStudioClient {
    http: Client,
    endpoint: String,
}

impl LmStudioClient {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            endpoint: endpoint.into(),
        }
    }

    fn url(&self) -> String {
        format!(
            "{}/v1/chat/completions",
            self.endpoint.trim_end_matches('/')
        )
    }

    async fn send(&self, payload: Value) -> Result<Value> {
        let resp = self.http.post(self.url()).json(&payload).send().await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_else(|_| "no body".to_string());
            tracing::error!(%status, %body, "LM Studio request failed");
            return Err(anyhow!("LM Studio error {}: {}", status, body));
        }

        let json: Value = resp.json().await?;
        Ok(json)
    }
}

#[async_trait::async_trait]
impl LlmClient for LmStudioClient {
    async fn complete_text(&self, model: &str, prompt: &str) -> Result<String> {
        let body = json!({
            "model": model,
            "messages": [{
                "role": "user",
                "content": [{"type": "text", "text": prompt}]
            }],
            "stream": false
        });
        let resp = self.send(body).await?;
        extract_text(&resp)
    }

    async fn complete_json(&self, model: &str, prompt: &str, schema: Value) -> Result<Value> {
        let body = json!({
            "model": model,
            "messages": [{
                "role": "user",
                "content": [{"type": "text", "text": prompt}]
            }],
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "response",
                    "strict": true,
                    "schema": schema
                }
            },
            "stream": false
        });
        let resp = self.send(body).await?;
        let text = extract_text(&resp)?;
        Ok(serde_json::from_str(&text)?)
    }

    async fn complete_vision_json(
        &self,
        model: &str,
        prompt: &str,
        images_base64: Vec<String>,
        schema: Value,
    ) -> Result<Value> {
        let mut content: Vec<Value> = images_base64
            .into_iter()
            .map(|img| {
                json!({
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:image/png;base64,{}", img)
                    }
                })
            })
            .collect();
        content.push(json!({"type": "text", "text": prompt}));

        let body = json!({
            "model": model,
            "messages": [{
                "role": "user",
                "content": content
            }],
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "response",
                    "strict": true,
                    "schema": schema
                }
            },
            "stream": false
        });

        let resp = self.send(body).await?;
        let text = extract_text(&resp)?;
        Ok(serde_json::from_str(&text)?)
    }
}

fn extract_text(resp: &Value) -> Result<String> {
    let choice = resp
        .get("choices")
        .and_then(|c| c.get(0))
        .ok_or_else(|| anyhow!("choices missing"))?;
    let message = choice
        .get("message")
        .ok_or_else(|| anyhow!("message missing"))?;

    if let Some(content) = message.get("content") {
        if let Some(text) = content.as_str() {
            return Ok(text.to_string());
        }
        if let Some(items) = content.as_array() {
            let mut combined = String::new();
            for item in items {
                if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(chunk) = item.get("text").and_then(|t| t.as_str()) {
                        combined.push_str(chunk);
                    }
                }
            }
            if !combined.is_empty() {
                return Ok(combined);
            }
        }
    }

    Err(anyhow!("Unable to extract text from LLM response"))
}
