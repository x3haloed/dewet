use anyhow::{Result, anyhow};
use reqwest::{Client, header::HeaderMap};
use serde_json::{Value, json};

use super::{ChatCompletionWithTools, ChatMessage, FunctionCall, LlmClient, ToolCall, ToolDefinition};

pub struct OpenRouterClient {
    http: Client,
    headers: HeaderMap,
}

impl OpenRouterClient {
    pub fn new(api_key: &str, site_url: Option<String>, site_name: Option<String>) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Authorization",
            format!("Bearer {}", api_key).parse().unwrap(),
        );
        headers.insert("Content-Type", "application/json".parse().unwrap());
        if let Some(url) = site_url {
            headers.insert("HTTP-Referer", url.parse().unwrap());
        }
        if let Some(name) = site_name {
            headers.insert("X-Title", name.parse().unwrap());
        }

        Self {
            http: Client::new(),
            headers,
        }
    }

    fn url(&self) -> &str {
        "https://openrouter.ai/api/v1/chat/completions"
    }

    async fn send(&self, payload: Value) -> Result<Value> {
        let resp = self
            .http
            .post(self.url())
            .headers(self.headers.clone())
            .json(&payload)
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        Ok(resp)
    }
}

#[async_trait::async_trait]
impl LlmClient for OpenRouterClient {
    async fn complete_text(&self, model: &str, prompt: &str) -> Result<String> {
        let body = json!({
            "model": model,
            "messages": [{
                "role": "user",
                "content": prompt
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
                "content": prompt
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

    async fn complete_vision_text(
        &self,
        model: &str,
        prompt: &str,
        images_base64: Vec<String>,
    ) -> Result<String> {
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
            "stream": false
        });

        let resp = self.send(body).await?;
        extract_text(&resp)
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

    async fn complete_chat(&self, model: &str, messages: Vec<ChatMessage>) -> Result<String> {
        let messages_json: Vec<Value> = messages
            .into_iter()
            .map(|msg| serde_json::to_value(msg).unwrap())
            .collect();

        let body = json!({
            "model": model,
            "messages": messages_json,
            "stream": false
        });

        let resp = self.send(body).await?;
        extract_text(&resp)
    }

    async fn complete_vision_chat(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
    ) -> Result<String> {
        // Vision chat uses the same format - images are embedded in ChatContent::Multimodal
        let messages_json: Vec<Value> = messages
            .into_iter()
            .map(|msg| serde_json::to_value(msg).unwrap())
            .collect();

        let body = json!({
            "model": model,
            "messages": messages_json,
            "stream": false
        });

        let resp = self.send(body).await?;
        extract_text(&resp)
    }

    async fn complete_with_tools(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
    ) -> Result<ChatCompletionWithTools> {
        let messages_json: Vec<Value> = messages
            .into_iter()
            .map(|msg| serde_json::to_value(msg).unwrap())
            .collect();

        let tools_json: Vec<Value> = tools
            .into_iter()
            .map(|t| serde_json::to_value(t).unwrap())
            .collect();

        let body = json!({
            "model": model,
            "messages": messages_json,
            "tools": tools_json,
            "stream": false
        });

        let resp = self.send(body).await?;
        extract_with_tools(&resp)
    }

    async fn complete_vision_with_tools(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
    ) -> Result<ChatCompletionWithTools> {
        // Vision with tools uses the same format - images embedded in ChatContent::Multimodal
        let messages_json: Vec<Value> = messages
            .into_iter()
            .map(|msg| serde_json::to_value(msg).unwrap())
            .collect();

        let tools_json: Vec<Value> = tools
            .into_iter()
            .map(|t| serde_json::to_value(t).unwrap())
            .collect();

        let body = json!({
            "model": model,
            "messages": messages_json,
            "tools": tools_json,
            "stream": false
        });

        let resp = self.send(body).await?;
        extract_with_tools(&resp)
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
    if let Some(text) = message.get("content").and_then(|v| v.as_str()) {
        return Ok(text.to_string());
    }
    if let Some(items) = message.get("content").and_then(|v| v.as_array()) {
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
    Err(anyhow!("Unable to extract text from OpenRouter response"))
}

fn extract_with_tools(resp: &Value) -> Result<ChatCompletionWithTools> {
    let choice = resp
        .get("choices")
        .and_then(|c| c.get(0))
        .ok_or_else(|| anyhow!("choices missing"))?;
    let message = choice
        .get("message")
        .ok_or_else(|| anyhow!("message missing"))?;

    // Extract text content (may be null if only tool calls)
    let content = if let Some(text) = message.get("content") {
        if text.is_null() {
            None
        } else if let Some(s) = text.as_str() {
            if s.is_empty() { None } else { Some(s.to_string()) }
        } else if let Some(items) = text.as_array() {
            let mut combined = String::new();
            for item in items {
                if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(chunk) = item.get("text").and_then(|t| t.as_str()) {
                        combined.push_str(chunk);
                    }
                }
            }
            if combined.is_empty() { None } else { Some(combined) }
        } else {
            None
        }
    } else {
        None
    };

    // Extract tool calls
    let tool_calls = if let Some(calls) = message.get("tool_calls").and_then(|v| v.as_array()) {
        calls
            .iter()
            .filter_map(|call| {
                let id = call.get("id")?.as_str()?.to_string();
                let call_type = call.get("type")?.as_str()?.to_string();
                let function = call.get("function")?;
                let name = function.get("name")?.as_str()?.to_string();
                let arguments = function.get("arguments")?.as_str()?.to_string();

                Some(ToolCall {
                    id,
                    call_type,
                    function: FunctionCall { name, arguments },
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    Ok(ChatCompletionWithTools { content, tool_calls })
}
