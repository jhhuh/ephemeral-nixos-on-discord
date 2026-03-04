use std::time::Duration;

use crate::llm::tools::sandbox_tools;
use crate::llm::traits::{
    ContentBlock, LlmBackend, LlmResponse, Message, MessageContent, Role, ToolCall,
};
use crate::qga::QgaClient;

const SYSTEM_PROMPT: &str = r#"You are **NixOS Sandbox**, an interactive NixOS tutor and experimentation assistant running inside an ephemeral NixOS virtual machine.

## Your Role
Help users learn NixOS by doing. When they ask you to do something, show them the NixOS way. You are both a helpful assistant AND a teacher.

## How to Behave

**Before running commands:**
- Briefly explain WHAT you're about to do and WHY
- If there's a NixOS-specific concept involved (declarative config, generations, the Nix store, flakes, overlays), mention it naturally

**When running commands:**
- Prefer the NixOS/Nix way over the imperative way (e.g., `nixos-rebuild` over manually installing packages, `nix-shell -p` for temporary packages)
- Use `exec` to run commands — the user will see every command and its output in real-time
- Chain related commands naturally, don't batch everything into one shell line

**After running commands:**
- Explain what happened, especially if the output is non-obvious
- If something failed, explain why and how to fix it
- Suggest what the user might want to try next

**Teaching moments:**
- When you use `nixos-rebuild switch`, explain that NixOS is declarative — you change config, then rebuild
- When you modify `/etc/nixos/configuration.nix`, explain the module system
- When using `nix-shell` or `nix develop`, explain ephemeral environments
- Point out NixOS-specific patterns: `services.*.enable`, `environment.systemPackages`, generations, rollbacks
- If the user tries something the "Linux way" (apt, yum, manual config), gently redirect to the NixOS way

## Tone
Friendly, concise, educational. Don't lecture — teach by showing. Keep explanations to 1-3 sentences unless the user asks for more detail.

## This Sandbox
This is an ephemeral NixOS VM. It will be destroyed when the session ends. The user can break anything — that's the point. Encourage experimentation."#;

const MAX_TOOL_ROUNDS: usize = 10;
const EXEC_TIMEOUT: Duration = Duration::from_secs(120);

/// A message the agent wants to post to Discord during execution.
#[derive(Debug)]
pub enum AgentEvent {
    /// The agent is about to run a command
    ToolStart { name: String, detail: String },
    /// A tool call completed with output
    ToolOutput { name: String, output: String, success: bool },
    /// The agent's final text response
    Reply(String),
}

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

    /// Process a user message, streaming tool execution events via the callback.
    /// Returns the final text response from the LLM.
    pub async fn handle_message<F, Fut>(
        &mut self,
        user_text: &str,
        qga: &mut QgaClient,
        mut on_event: F,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>>
    where
        F: FnMut(AgentEvent) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
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
                    // Record assistant tool_use blocks
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

                    // Execute each tool, streaming events to Discord
                    let mut result_blocks = Vec::with_capacity(calls.len());
                    for call in &calls {
                        // Notify: tool starting
                        let detail = format_tool_start(call);
                        on_event(AgentEvent::ToolStart {
                            name: call.name.clone(),
                            detail,
                        })
                        .await;

                        // Execute
                        let output = execute_tool(call, qga).await;

                        // Notify: tool completed
                        let success = !output.starts_with("Error:");
                        on_event(AgentEvent::ToolOutput {
                            name: call.name.clone(),
                            output: truncate_for_discord(&output, 1800),
                            success,
                        })
                        .await;

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

/// Format the "about to run" message for a tool call.
fn format_tool_start(call: &ToolCall) -> String {
    match call.name.as_str() {
        "exec" => {
            let cmd = call.input["command"].as_str().unwrap_or("(unknown)");
            format!("```bash\n{cmd}\n```")
        }
        "read_file" => {
            let path = call.input["path"].as_str().unwrap_or("(unknown)");
            format!("`cat {path}`")
        }
        "write_file" => {
            let path = call.input["path"].as_str().unwrap_or("(unknown)");
            let content = call.input["content"].as_str().unwrap_or("");
            let preview = if content.len() > 200 {
                format!("{}...", &content[..200])
            } else {
                content.to_string()
            };
            format!("Writing to `{path}`:\n```nix\n{preview}\n```")
        }
        "nixos_rebuild" => {
            let config = call.input["config_nix"].as_str().unwrap_or("");
            let preview = if config.len() > 300 {
                format!("{}...", &config[..300])
            } else {
                config.to_string()
            };
            format!("Applying NixOS config:\n```nix\n{preview}\n```\nThen running `nixos-rebuild switch`")
        }
        _ => call.name.clone(),
    }
}

/// Truncate output to fit Discord message limits.
fn truncate_for_discord(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        format!("{}…\n*(truncated, {} bytes total)*", &text[..max], text.len())
    }
}

async fn execute_tool(call: &ToolCall, qga: &mut QgaClient) -> String {
    match call.name.as_str() {
        "exec" => {
            let command = call.input["command"].as_str().unwrap_or_default();
            match qga.exec(command, EXEC_TIMEOUT).await {
                Ok(output) => {
                    let mut result = String::new();
                    if !output.stdout.is_empty() {
                        result.push_str(&output.stdout);
                    }
                    if !output.stderr.is_empty() {
                        if !result.is_empty() {
                            result.push('\n');
                        }
                        result.push_str("stderr: ");
                        result.push_str(&output.stderr);
                    }
                    if output.exit_code != 0 {
                        result.push_str(&format!("\n(exit code: {})", output.exit_code));
                    }
                    if result.is_empty() {
                        "(no output)".into()
                    } else {
                        result
                    }
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
                    let mut result = String::new();
                    if !output.stdout.is_empty() {
                        result.push_str(&output.stdout);
                    }
                    if !output.stderr.is_empty() {
                        if !result.is_empty() {
                            result.push('\n');
                        }
                        result.push_str(&output.stderr);
                    }
                    if output.exit_code != 0 {
                        result.push_str(&format!("\nnixos-rebuild failed (exit {})", output.exit_code));
                    } else {
                        result.push_str("\nnixos-rebuild completed successfully");
                    }
                    result
                }
                Err(e) => format!("Error: {e}"),
            }
        }
        other => format!("Unknown tool: {other}"),
    }
}
