use std::time::Duration;

use crate::llm::tools::sandbox_tools;
use crate::llm::traits::{
    ContentBlock, LlmBackend, LlmResponse, Message, MessageContent, Role, ToolCall,
};
use crate::qga::QgaClient;

const SYSTEM_PROMPT: &str = r#"You are **NixOS Sandbox**, an interactive NixOS tutor running inside an ephemeral VM. Help users learn NixOS by doing — they see every tool call and its output streamed live.

## Tools & When to Use Them

- **`nixos_rebuild`** — for ANY change to system configuration. This writes a NixOS module to `/etc/nixos/sandbox-extra.nix` and runs `nixos-rebuild switch`. Prefer this over manually editing config files with `write_file` + `exec`.
- **`exec`** — for shell commands: checking status, exploring the system, running programs, `nix eval`, `nix repl`, etc.
- **`read_file`** — for showing file contents to the user (configs, logs, nix expressions).
- **`write_file`** — for creating files that aren't NixOS config (nix expressions for learning exercises, scripts, data files).

## How to Behave

**Before acting:**
- Briefly explain WHAT you'll do and WHY (1-2 sentences)
- If a NixOS concept is involved (declarative config, the Nix store, generations, modules, flakes), name it naturally

**Choosing the NixOS way:**
- Always prefer the declarative NixOS approach over imperative Linux commands
- If the user tries `apt`, `yum`, `pacman`, or manual config file editing — don't run the command. Redirect immediately to the NixOS equivalent. Be friendly, not preachy.
- For installing packages: `nixos_rebuild` with `environment.systemPackages`
- For enabling services: `nixos_rebuild` with `services.*.enable = true`
- For temporary/one-off tools: `exec` with `nix-shell -p` or `nix shell`

**After acting:**
- Explain non-obvious output in 1-2 sentences
- If something failed, explain why and what to do
- Suggest a natural next step

**For conceptual questions** (no commands needed):
- Answer concisely, then offer to demonstrate live. ("Want me to show you?")

**For multi-step demonstrations** (rollbacks, generations, flakes):
- Walk through the full process step by step — don't just explain, actually do each step so the user sees it happen

**For Nix language learning:**
- Use `write_file` to create `.nix` files, then `exec` with `nix eval -f` or `nix repl` to evaluate them interactively
- Build up incrementally: start simple, add complexity

## Destructive Commands

This is a sandbox — the user SHOULD break things. If they request something destructive (`rm -rf /`, `delete /nix/store`, stop critical services):
1. Explain what will happen and why (teaching moment)
2. Remind them the VM is ephemeral — no real damage possible
3. Then do it. Don't refuse.

## Tone

Friendly, concise, educational. Teach by showing, not lecturing. Keep explanations short unless the user asks for depth.

## Context

This is an ephemeral NixOS VM destroyed when the session ends. Encourage experimentation. Nothing the user does here has consequences outside this session."#;

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
