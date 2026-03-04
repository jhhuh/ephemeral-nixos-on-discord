use poise::serenity_prelude as serenity;
use tracing::error;

use crate::llm::agent::AgentEvent;

use super::BotData;

type Error = Box<dyn std::error::Error + Send + Sync>;

pub async fn handle_message(
    ctx: &serenity::Context,
    msg: &serenity::Message,
    data: &BotData,
) -> Result<(), Error> {
    if msg.author.bot {
        return Ok(());
    }

    let thread_id = msg.channel_id.get();
    let channel_id = msg.channel_id;
    let http = ctx.http.clone();

    let mut sessions = data.sessions.sessions_mut().await;
    let session = match sessions.get_mut(&thread_id) {
        Some(s) => s,
        None => return Ok(()),
    };

    session.last_activity = std::time::Instant::now();

    // Create callback that streams tool execution to Discord
    let response = session
        .agent
        .handle_message(&msg.content, &mut session.qga, |event| {
            let http = http.clone();
            async move {
                let text = format_event(event);
                // Send each event as a separate Discord message
                for chunk in split_message(&text, 2000) {
                    if let Err(e) = channel_id.say(&http, chunk).await {
                        error!(error = %e, "failed to send tool event");
                    }
                }
            }
        })
        .await;

    // Release lock before sending final response
    drop(sessions);

    match response {
        Ok(text) if !text.is_empty() => {
            for chunk in split_message(&text, 2000) {
                channel_id.say(&ctx.http, chunk).await?;
            }
        }
        Ok(_) => {} // empty response, events already posted
        Err(e) => {
            error!(error = %e, thread_id = %thread_id, "agent error");
            channel_id.say(&ctx.http, format!("Error: {e}")).await?;
        }
    }

    Ok(())
}

/// Format an agent event into a Discord message with rich formatting.
fn format_event(event: AgentEvent) -> String {
    match event {
        AgentEvent::ToolStart { name, detail } => match name.as_str() {
            "exec" => format!("\u{1f527} **Running:**\n{detail}"),
            "read_file" => format!("\u{1f4c4} **Reading:** {detail}"),
            "write_file" => format!("\u{270f}\u{fe0f} **Writing:**\n{detail}"),
            "nixos_rebuild" => format!("\u{2699}\u{fe0f} **Rebuilding NixOS:**\n{detail}"),
            _ => format!("\u{1f6e0}\u{fe0f} **{name}:**\n{detail}"),
        },
        AgentEvent::ToolOutput { name: _, output, success } => {
            let icon = if success { "\u{2705}" } else { "\u{274c}" };
            if output.len() > 1900 {
                // Already truncated by agent, just wrap in code block
                format!("{icon} **Output:**\n```\n{output}\n```")
            } else {
                format!("{icon} **Output:**\n```\n{output}\n```")
            }
        }
        AgentEvent::Reply(text) => text,
    }
}

/// Split text into chunks that fit within Discord's character limit.
fn split_message(text: &str, max_len: usize) -> Vec<&str> {
    if text.len() <= max_len {
        return vec![text];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining);
            break;
        }

        let search_range = &remaining[..max_len];
        let split_at = match search_range.rfind('\n') {
            Some(pos) if pos > 0 => pos + 1,
            _ => max_len,
        };

        chunks.push(&remaining[..split_at]);
        remaining = &remaining[split_at..];
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_message_not_split() {
        let chunks = split_message("hello", 2000);
        assert_eq!(chunks, vec!["hello"]);
    }

    #[test]
    fn splits_at_newline() {
        let text = "line1\nline2\nline3";
        let chunks = split_message(text, 10);
        assert_eq!(chunks, vec!["line1\n", "line2\n", "line3"]);
    }

    #[test]
    fn hard_splits_without_newline() {
        let text = "abcdefghij";
        let chunks = split_message(text, 4);
        assert_eq!(chunks, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn empty_message() {
        let chunks = split_message("", 2000);
        assert_eq!(chunks, vec![""]);
    }

    #[test]
    fn format_exec_event() {
        let event = AgentEvent::ToolStart {
            name: "exec".into(),
            detail: "```bash\nls -la\n```".into(),
        };
        let formatted = format_event(event);
        assert!(formatted.contains("Running:"));
        assert!(formatted.contains("ls -la"));
    }

    #[test]
    fn format_output_success() {
        let event = AgentEvent::ToolOutput {
            name: "exec".into(),
            output: "hello world".into(),
            success: true,
        };
        let formatted = format_event(event);
        assert!(formatted.contains("\u{2705}"));
        assert!(formatted.contains("hello world"));
    }

    #[test]
    fn format_output_failure() {
        let event = AgentEvent::ToolOutput {
            name: "exec".into(),
            output: "Error: command not found".into(),
            success: false,
        };
        let formatted = format_event(event);
        assert!(formatted.contains("\u{274c}"));
    }
}
