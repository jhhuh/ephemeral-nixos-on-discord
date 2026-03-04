use serde_json::{json, Value};

use super::traits::{
    ContentBlock, LlmBackend, LlmResponse, Message, MessageContent, Role, ToolCall, ToolDef,
};

pub struct OllamaBackend {
    client: reqwest::Client,
    model: String,
    base_url: String,
}

impl OllamaBackend {
    pub fn new(model: Option<String>, base_url: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            model: model.unwrap_or_else(|| "llama3.1".into()),
            base_url: base_url.unwrap_or_else(|| "http://localhost:11434".into()),
        }
    }
}

#[async_trait::async_trait]
impl LlmBackend for OllamaBackend {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<LlmResponse, Box<dyn std::error::Error + Send + Sync>> {
        let mut body = json!({
            "model": self.model,
            "messages": convert_messages(messages),
            "stream": false,
        });

        if !tools.is_empty() {
            body["tools"] = json!(convert_tools(tools));
        }

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let data: Value = resp.json().await?;
        let message = &data["message"];

        // Check for tool calls first
        if let Some(tool_calls) = message["tool_calls"].as_array() {
            if !tool_calls.is_empty() {
                let calls: Vec<ToolCall> = tool_calls
                    .iter()
                    .enumerate()
                    .map(|(i, tc)| {
                        let func = &tc["function"];
                        ToolCall {
                            id: format!("ollama-{i}"),
                            name: func["name"].as_str().unwrap_or_default().into(),
                            input: func["arguments"].clone(),
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

fn convert_messages(messages: &[Message]) -> Vec<Value> {
    messages
        .iter()
        .map(|m| {
            let role = match m.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::Tool => "tool",
            };

            let mut msg = json!({"role": role});

            match &m.content {
                MessageContent::Text(s) => {
                    msg["content"] = json!(s);
                }
                MessageContent::Blocks(blocks) => {
                    // Collect text content
                    let text: String = blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            ContentBlock::ToolResult { content, .. } => Some(content.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("");

                    msg["content"] = json!(text);

                    // Collect tool_calls for assistant messages
                    if matches!(m.role, Role::Assistant) {
                        let tool_calls: Vec<Value> = blocks
                            .iter()
                            .filter_map(|b| match b {
                                ContentBlock::ToolUse { name, input, .. } => Some(json!({
                                    "function": {
                                        "name": name,
                                        "arguments": input,
                                    }
                                })),
                                _ => None,
                            })
                            .collect();
                        if !tool_calls.is_empty() {
                            msg["tool_calls"] = json!(tool_calls);
                        }
                    }
                }
            }

            msg
        })
        .collect()
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
