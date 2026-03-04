use std::time::Duration;

use poise::serenity_prelude as serenity;
use serenity::all::{ChannelType, CreateThread};
use tracing::{error, info};

use super::BotData;
use crate::llm::agent::Agent;
use crate::session::Session;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, BotData, Error>;

const MAX_SESSIONS: usize = 10;

/// Create a new sandbox VM and attach it to a Discord thread.
#[poise::command(slash_command)]
pub async fn create(
    ctx: Context<'_>,
    #[description = "Describe what you want installed (e.g. 'Python 3.12 and PostgreSQL')"]
    description: Option<String>,
) -> Result<(), Error> {
    let data = ctx.data();

    // Check capacity
    if data.sessions.count().await >= MAX_SESSIONS {
        ctx.say("At capacity (max 10 sandboxes). Try again later.")
            .await?;
        return Ok(());
    }

    ctx.say("Creating sandbox VM...").await?;

    // Generate NixOS config from description if provided
    let user_config = if let Some(ref desc) = description {
        ctx.say(format!("Generating NixOS config from: *{}*", desc))
            .await?;
        let backend = data.llm_backend_factory.create();
        match crate::llm::config_gen::generate_nixos_config(desc, backend.as_ref()).await {
            Ok(config) => {
                let config_clone = config.clone();
                let syntax_check = tokio::task::spawn_blocking(move || {
                    crate::vm::config::validate_nix_syntax(&config_clone)
                })
                .await
                .map_err(|e| -> Error { format!("spawn_blocking: {e}").into() })?;
                match syntax_check {
                    Ok(()) => Some(config),
                    Err(e) => {
                        ctx.say(format!(
                            "Generated config has syntax errors: {e}\nFalling back to base config."
                        ))
                        .await?;
                        None
                    }
                }
            }
            Err(e) => {
                ctx.say(format!(
                    "Config generation failed: {e}\nFalling back to base config."
                ))
                .await?;
                None
            }
        }
    } else {
        None
    };

    let vm_id = match data.vm_manager.create(user_config).await {
        Ok(id) => id,
        Err(e) => {
            error!(error = %e, "failed to create VM");
            ctx.say(format!("Failed to create sandbox: {e}")).await?;
            return Ok(());
        }
    };

    // Create a Discord thread
    let thread_name = format!("sandbox-{}", vm_id);
    let thread = match ctx
        .channel_id()
        .create_thread(
            ctx.http(),
            CreateThread::new(&thread_name).kind(ChannelType::PublicThread),
        )
        .await
    {
        Ok(t) => t,
        Err(e) => {
            error!(error = %e, vm_id = %vm_id, "failed to create thread, destroying VM");
            let _ = data.vm_manager.destroy(&vm_id).await;
            ctx.say(format!("Failed to create thread: {e}")).await?;
            return Ok(());
        }
    };

    // Connect QGA
    let qga = match data.vm_manager.connect_qga(&vm_id, Duration::from_secs(60)).await {
        Ok(q) => q,
        Err(e) => {
            error!(error = %e, vm_id = %vm_id, "QGA connection failed, destroying VM");
            let _ = data.vm_manager.destroy(&vm_id).await;
            ctx.say(format!("VM created but QGA connection failed: {e}"))
                .await?;
            return Ok(());
        }
    };

    // Create agent with LLM backend
    let backend = data.llm_backend_factory.create();
    let agent = Agent::new(backend);

    let thread_id = thread.id.get();
    let session = Session {
        vm_id: vm_id.clone(),
        thread_id,
        agent,
        qga,
        created_at: std::time::Instant::now(),
        last_activity: std::time::Instant::now(),
    };

    data.sessions.add(thread_id, session).await;

    info!(vm_id = %vm_id, thread_id = %thread_id, "sandbox ready");

    thread
        .id
        .say(
            ctx.http(),
            format!(
                "Sandbox **{}** is ready! Send messages here to interact with the VM.",
                vm_id
            ),
        )
        .await?;

    Ok(())
}

/// Destroy the sandbox in the current thread.
#[poise::command(slash_command)]
pub async fn destroy(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let thread_id = ctx.channel_id().get();

    let session = data.sessions.remove(thread_id).await;
    match session {
        Some(s) => {
            let vm_id = s.vm_id.clone();
            if let Err(e) = data.vm_manager.destroy(&vm_id).await {
                error!(error = %e, vm_id = %vm_id, "failed to destroy VM");
                ctx.say(format!("Error destroying VM: {e}")).await?;
                return Ok(());
            }
            info!(vm_id = %vm_id, "sandbox destroyed");
            ctx.say(format!("Sandbox **{}** destroyed.", vm_id)).await?;
        }
        None => {
            ctx.say("No sandbox found in this thread.").await?;
        }
    }
    Ok(())
}

/// Download a file from the sandbox VM.
#[poise::command(slash_command)]
pub async fn download(
    ctx: Context<'_>,
    #[description = "Path to the file in the sandbox"] path: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let thread_id = ctx.channel_id().get();

    // Get file data from QGA
    let file_data = {
        let mut sessions = data.sessions.sessions_mut().await;
        let session = match sessions.get_mut(&thread_id) {
            Some(s) => s,
            None => {
                ctx.say("No sandbox in this thread.").await?;
                return Ok(());
            }
        };
        session.last_activity = std::time::Instant::now();

        match session.qga.read_file(&path).await {
            Ok(data) => data,
            Err(e) => {
                ctx.say(format!("Failed to read file: {e}")).await?;
                return Ok(());
            }
        }
    };

    // Check file size (Discord limit: 25MB for free, 50MB for boosted)
    if file_data.len() > 25 * 1024 * 1024 {
        ctx.say("File is too large for Discord (>25MB).").await?;
        return Ok(());
    }

    // Extract filename from path
    let filename = path.rsplit('/').next().unwrap_or("file");

    // Upload as attachment
    let attachment = serenity::CreateAttachment::bytes(file_data, filename);
    let reply = poise::CreateReply::default()
        .attachment(attachment)
        .content(format!("Downloaded `{path}`"));

    ctx.send(reply).await?;
    Ok(())
}

/// Show status of the sandbox in the current thread.
#[poise::command(slash_command)]
pub async fn status(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let thread_id = ctx.channel_id().get();

    let info = data
        .sessions
        .get_mut(thread_id, |s| {
            let uptime = s.created_at.elapsed();
            let idle = s.last_activity.elapsed();
            (s.vm_id.clone(), uptime, idle)
        })
        .await;

    match info {
        Some((vm_id, uptime, idle)) => {
            ctx.say(format!(
                "**Sandbox:** {}\n**Uptime:** {}s\n**Idle:** {}s",
                vm_id,
                uptime.as_secs(),
                idle.as_secs(),
            ))
            .await?;
        }
        None => {
            ctx.say("No sandbox in this thread.").await?;
        }
    }
    Ok(())
}
