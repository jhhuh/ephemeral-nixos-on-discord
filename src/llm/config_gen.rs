use crate::llm::traits::{LlmBackend, LlmResponse, Message, MessageContent, Role};

const CONFIG_GEN_SYSTEM_PROMPT: &str = r#"You are a NixOS configuration generator. Given a description of what the user wants in their VM sandbox, output ONLY a valid NixOS module expression. No explanation, no markdown, just the Nix expression.

The output must be a NixOS module of the form:
{ pkgs, config, lib, ... }: { ... }

Examples:
- "Python 3.12 with numpy" → { pkgs, ... }: { environment.systemPackages = [ pkgs.python312 pkgs.python312Packages.numpy ]; }
- "Web server with nginx" → { pkgs, ... }: { services.nginx.enable = true; networking.firewall.allowedTCPPorts = [ 80 443 ]; }
- "PostgreSQL database" → { pkgs, ... }: { services.postgresql.enable = true; services.postgresql.package = pkgs.postgresql_16; }

Output ONLY the Nix expression. No backticks, no explanation."#;

/// Generate a NixOS module from a natural language description.
pub async fn generate_nixos_config(
    description: &str,
    backend: &dyn LlmBackend,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let messages = vec![
        Message {
            role: Role::System,
            content: MessageContent::Text(CONFIG_GEN_SYSTEM_PROMPT.into()),
        },
        Message {
            role: Role::User,
            content: MessageContent::Text(description.into()),
        },
    ];

    let response = backend.chat(&messages, &[]).await?;

    match response {
        LlmResponse::Text(text) => {
            // Strip any accidental markdown fencing
            let trimmed = text.trim();
            let config = if trimmed.starts_with("```") {
                trimmed
                    .trim_start_matches("```nix")
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim()
            } else {
                trimmed
            };
            Ok(config.to_string())
        }
        LlmResponse::ToolCalls(_) => {
            Err("LLM returned tool calls instead of config text".into())
        }
    }
}
