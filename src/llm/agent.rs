use std::time::Duration;

use crate::llm::tools::sandbox_tools;
use crate::llm::traits::{
    ContentBlock, LlmBackend, LlmResponse, Message, MessageContent, Role, ToolCall,
};
use crate::qga::QgaClient;

const SYSTEM_PROMPT: &str = "You are an assistant that helps users work inside a NixOS virtual machine sandbox. You can execute commands, read and write files, and apply NixOS configuration changes.\n\nWhen the user asks you to do something in the sandbox, use the available tools. Always show command output to the user. If a command fails, explain what went wrong and suggest fixes.\n\nThe sandbox is an ephemeral NixOS VM. It will be destroyed when the session ends.";

const MAX_TOOL_ROUNDS: usize = 10;
const EXEC_TIMEOUT: Duration = Duration::from_secs(120);

pub struct Agent {
    backend: Box<dyn LlmBackend>,
    conversation: Vec<Message>,
}

impl Agent {
    pub fn new(backend: Box<dyn LlmBackend>) -> Self {
        let conversation = vec![Message {
            role: Role::System,
            content: MessageContent::Text(SYSTEM_PROMPT.into()),
        }];
        Self {
            backend,
            conversation,
        }
    }

    pub async fn handle_message(
        &mut self,
        user_text: &str,
        qga: &mut QgaClient,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.conversation.push(Message {
            role: Role::User,
            content: MessageContent::Text(user_text.into()),
        });

        let tools = sandbox_tools();

        for _ in 0..MAX_TOOL_ROUNDS {
            let response = self.backend.chat(&self.conversation, &tools).await?;

            match response {
                LlmResponse::Text(text) => {
                    self.conversation.push(Message {
                        role: Role::Assistant,
                        content: MessageContent::Text(text.clone()),
                    });
                    return Ok(text);
                }
                LlmResponse::ToolCalls(calls) => {
                    // Append assistant message with ToolUse blocks
                    let tool_use_blocks: Vec<ContentBlock> = calls
                        .iter()
                        .map(|c| ContentBlock::ToolUse {
                            id: c.id.clone(),
                            name: c.name.clone(),
                            input: c.input.clone(),
                        })
                        .collect();
                    self.conversation.push(Message {
                        role: Role::Assistant,
                        content: MessageContent::Blocks(tool_use_blocks),
                    });

                    // Execute each tool call and collect results
                    let mut result_blocks = Vec::with_capacity(calls.len());
                    for call in &calls {
                        let output = execute_tool(call, qga).await;
                        result_blocks.push(ContentBlock::ToolResult {
                            tool_use_id: call.id.clone(),
                            content: output,
                        });
                    }
                    self.conversation.push(Message {
                        role: Role::Tool,
                        content: MessageContent::Blocks(result_blocks),
                    });
                }
            }
        }

        Ok("Reached maximum tool execution rounds".into())
    }
}

async fn execute_tool(call: &ToolCall, qga: &mut QgaClient) -> String {
    match call.name.as_str() {
        "exec" => {
            let command = call.input["command"].as_str().unwrap_or_default();
            match qga.exec(command, EXEC_TIMEOUT).await {
                Ok(output) => {
                    format!(
                        "stdout:\n{}\nstderr:\n{}\nexit_code: {}",
                        output.stdout, output.stderr, output.exit_code
                    )
                }
                Err(e) => format!("Error: {e}"),
            }
        }
        "read_file" => {
            let path = call.input["path"].as_str().unwrap_or_default();
            match qga.read_file(path).await {
                Ok(data) => String::from_utf8_lossy(&data).into_owned(),
                Err(e) => format!("Error: {e}"),
            }
        }
        "write_file" => {
            let path = call.input["path"].as_str().unwrap_or_default();
            let content = call.input["content"].as_str().unwrap_or_default();
            match qga.write_file(path, content.as_bytes()).await {
                Ok(()) => format!("Successfully wrote to {path}"),
                Err(e) => format!("Error: {e}"),
            }
        }
        "nixos_rebuild" => {
            let config_nix = call.input["config_nix"].as_str().unwrap_or_default();
            let write_result = qga
                .write_file("/etc/nixos/sandbox-extra.nix", config_nix.as_bytes())
                .await;
            if let Err(e) = write_result {
                return format!("Error writing config: {e}");
            }
            match qga
                .exec("nixos-rebuild switch", Duration::from_secs(300))
                .await
            {
                Ok(output) => {
                    format!(
                        "stdout:\n{}\nstderr:\n{}\nexit_code: {}",
                        output.stdout, output.stderr, output.exit_code
                    )
                }
                Err(e) => format!("Error: {e}"),
            }
        }
        other => format!("Unknown tool: {other}"),
    }
}
