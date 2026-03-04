use serde_json::{json, Value};

use super::traits::{
    ContentBlock, LlmBackend, LlmResponse, Message, MessageContent, Role, ToolCall, ToolDef,
};

pub struct OpenAiBackend {
    client: reqwest::Client,
    api_key: String,
    model: String,
    api_base: String,
}

impl OpenAiBackend {
    pub fn new(api_key: String, model: Option<String>, api_base: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model: model.unwrap_or_else(|| "gpt-4o".into()),
            api_base: api_base.unwrap_or_else(|| "https://api.openai.com/v1".into()),
        }
    }
}

#[async_trait::async_trait]
impl LlmBackend for OpenAiBackend {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<LlmResponse, Box<dyn std::error::Error + Send + Sync>> {
        let mut body = json!({
            "model": self.model,
            "messages": convert_messages(messages),
        });

        if !tools.is_empty() {
            body["tools"] = json!(convert_tools(tools));
        }

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.api_base))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let data: Value = resp.json().await?;

        let message = &data["choices"][0]["message"];

        if let Some(tool_calls) = message["tool_calls"].as_array() {
            if !tool_calls.is_empty() {
                let calls = tool_calls
                    .iter()
                    .map(|tc| {
                        let function = &tc["function"];
                        let arguments: Value =
                            serde_json::from_str(function["arguments"].as_str().unwrap_or("{}"))
                                .unwrap_or(json!({}));
                        ToolCall {
                            id: tc["id"].as_str().unwrap_or_default().into(),
                            name: function["name"].as_str().unwrap_or_default().into(),
                            input: arguments,
                        }
                    })
                    .collect();
                return Ok(LlmResponse::ToolCalls(calls));
            }
        }

        let text = message["content"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        Ok(LlmResponse::Text(text))
    }
}

/// Convert internal messages to OpenAI chat format.
///
/// Key differences from Anthropic:
/// - System messages stay in the array (role: "system")
/// - Assistant tool_use blocks become `tool_calls` on the assistant message
/// - Tool results become separate messages with role "tool" and `tool_call_id`
fn convert_messages(messages: &[Message]) -> Vec<Value> {
    let mut out = Vec::new();

    for m in messages {
        match m.role {
            Role::System => {
                let text = match &m.content {
                    MessageContent::Text(s) => s.clone(),
                    MessageContent::Blocks(blocks) => blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(""),
                };
                out.push(json!({"role": "system", "content": text}));
            }
            Role::User => {
                let content = match &m.content {
                    MessageContent::Text(s) => s.clone(),
                    MessageContent::Blocks(blocks) => blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(""),
                };
                out.push(json!({"role": "user", "content": content}));
            }
            Role::Assistant => {
                match &m.content {
                    MessageContent::Text(s) => {
                        out.push(json!({"role": "assistant", "content": s}));
                    }
                    MessageContent::Blocks(blocks) => {
                        // Collect text and tool_use blocks separately
                        let text_parts: Vec<&str> = blocks
                            .iter()
                            .filter_map(|b| match b {
                                ContentBlock::Text { text } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect();
                        let tool_calls: Vec<Value> = blocks
                            .iter()
                            .filter_map(|b| match b {
                                ContentBlock::ToolUse { id, name, input } => Some(json!({
                                    "id": id,
                                    "type": "function",
                                    "function": {
                                        "name": name,
                                        "arguments": input.to_string(),
                                    }
                                })),
                                _ => None,
                            })
                            .collect();

                        let mut msg = json!({"role": "assistant"});
                        let content_text = text_parts.join("");
                        if !content_text.is_empty() {
                            msg["content"] = json!(content_text);
                        }
                        if !tool_calls.is_empty() {
                            msg["tool_calls"] = json!(tool_calls);
                        }
                        out.push(msg);
                    }
                }
            }
            Role::Tool => {
                // Tool result messages: extract tool_call_id and content from blocks
                match &m.content {
                    MessageContent::Blocks(blocks) => {
                        for b in blocks {
                            if let ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                            } = b
                            {
                                out.push(json!({
                                    "role": "tool",
                                    "tool_call_id": tool_use_id,
                                    "content": content,
                                }));
                            }
                        }
                    }
                    MessageContent::Text(s) => {
                        // Fallback: plain text tool result without an id
                        out.push(json!({"role": "tool", "content": s}));
                    }
                }
            }
        }
    }

    out
}

fn convert_tools(tools: &[ToolDef]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        })
        .collect()
}
