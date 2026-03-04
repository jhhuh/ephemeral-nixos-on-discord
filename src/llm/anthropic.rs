use serde_json::{json, Value};

use super::traits::{
    ContentBlock, LlmBackend, LlmResponse, Message, MessageContent, Role, ToolCall, ToolDef,
};

pub struct AnthropicBackend {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl AnthropicBackend {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model: model.unwrap_or_else(|| "claude-sonnet-4-6".into()),
        }
    }
}

#[async_trait::async_trait]
impl LlmBackend for AnthropicBackend {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<LlmResponse, Box<dyn std::error::Error + Send + Sync>> {
        let mut body = json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": convert_messages(messages),
        });

        if let Some(system) = extract_system(messages) {
            body["system"] = json!(system);
        }

        if !tools.is_empty() {
            body["tools"] = json!(convert_tools(tools));
        }

        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let data: Value = resp.json().await?;

        let content = data["content"]
            .as_array()
            .ok_or("missing content array in response")?;

        let tool_calls: Vec<ToolCall> = content
            .iter()
            .filter(|b| b["type"] == "tool_use")
            .map(|b| ToolCall {
                id: b["id"].as_str().unwrap_or_default().into(),
                name: b["name"].as_str().unwrap_or_default().into(),
                input: b["input"].clone(),
            })
            .collect();

        if !tool_calls.is_empty() {
            return Ok(LlmResponse::ToolCalls(tool_calls));
        }

        let text: String = content
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("");

        Ok(LlmResponse::Text(text))
    }
}

fn convert_messages(messages: &[Message]) -> Vec<Value> {
    messages
        .iter()
        .filter(|m| !matches!(m.role, Role::System))
        .map(|m| {
            let role = match m.role {
                Role::User | Role::Tool => "user",
                Role::Assistant => "assistant",
                Role::System => unreachable!(),
            };
            let content = match &m.content {
                MessageContent::Text(s) => json!(s),
                MessageContent::Blocks(blocks) => json!(blocks
                    .iter()
                    .map(|b| match b {
                        ContentBlock::Text { text } => json!({"type": "text", "text": text}),
                        ContentBlock::ToolUse { id, name, input } => {
                            json!({"type": "tool_use", "id": id, "name": name, "input": input})
                        }
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                        } => {
                            json!({"type": "tool_result", "tool_use_id": tool_use_id, "content": content})
                        }
                    })
                    .collect::<Vec<_>>()),
            };
            json!({"role": role, "content": content})
        })
        .collect()
}

fn convert_tools(tools: &[ToolDef]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters,
            })
        })
        .collect()
}

fn extract_system(messages: &[Message]) -> Option<String> {
    messages.iter().find_map(|m| {
        if matches!(m.role, Role::System) {
            match &m.content {
                MessageContent::Text(s) => Some(s.clone()),
                MessageContent::Blocks(blocks) => blocks.iter().find_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.clone()),
                    _ => None,
                }),
            }
        } else {
            None
        }
    })
}
