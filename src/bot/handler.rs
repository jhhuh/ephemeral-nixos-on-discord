use poise::serenity_prelude as serenity;
use tracing::error;

use super::BotData;

type Error = Box<dyn std::error::Error + Send + Sync>;

pub async fn handle_message(
    ctx: &serenity::Context,
    msg: &serenity::Message,
    data: &BotData,
) -> Result<(), Error> {
    // Ignore bot messages
    if msg.author.bot {
        return Ok(());
    }

    let thread_id = msg.channel_id.get();
    let user_text = &msg.content;

    // Lock sessions and process if this thread has a session
    let mut sessions = data.sessions.sessions_mut().await;
    let session = match sessions.get_mut(&thread_id) {
        Some(s) => s,
        None => return Ok(()), // Not a sandbox thread
    };

    session.last_activity = std::time::Instant::now();

    let response = match session.agent.handle_message(user_text, &mut session.qga).await {
        Ok(text) => text,
        Err(e) => {
            error!(error = %e, thread_id = %thread_id, "agent error");
            // Drop the lock before sending the error message
            drop(sessions);
            msg.channel_id
                .say(&ctx.http, format!("Error: {e}"))
                .await?;
            return Ok(());
        }
    };

    // Drop the lock before sending messages
    drop(sessions);

    for chunk in split_message(&response, 2000) {
        msg.channel_id.say(&ctx.http, chunk).await?;
    }

    Ok(())
}

/// Split text into chunks that fit within Discord's character limit.
/// Prefers splitting at newlines when possible.
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

        // Try to find a newline to split at
        let search_range = &remaining[..max_len];
        let split_at = match search_range.rfind('\n') {
            Some(pos) if pos > 0 => pos + 1, // Include the newline in the current chunk
            _ => max_len,                     // Hard split at max_len
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
}
