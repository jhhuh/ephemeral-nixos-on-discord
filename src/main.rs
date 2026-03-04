use std::sync::Arc;
use std::time::Duration;

use poise::serenity_prelude as serenity;
use tracing::{error, info};

use ephemeral_nixos_bot::bot::{BotData, LlmBackendFactory};
use ephemeral_nixos_bot::llm::anthropic::AnthropicBackend;
use ephemeral_nixos_bot::llm::LlmBackend;
use ephemeral_nixos_bot::session::SessionTracker;
use ephemeral_nixos_bot::vm::VmManager;

struct AnthropicFactory {
    api_key: String,
}

impl LlmBackendFactory for AnthropicFactory {
    fn create(&self) -> Box<dyn LlmBackend> {
        Box::new(AnthropicBackend::new(self.api_key.clone(), None))
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let discord_token =
        std::env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN env var required");
    let llm_api_key =
        std::env::var("LLM_API_KEY").expect("LLM_API_KEY env var required");
    let vm_state_dir =
        std::env::var("VM_STATE_DIR").unwrap_or_else(|_| "/tmp/ephemeral-vms".into());
    let host_cache_url =
        std::env::var("HOST_CACHE_URL").unwrap_or_else(|_| "http://localhost:5557".into());
    let project_root =
        std::env::var("PROJECT_ROOT").unwrap_or_else(|_| ".".into());

    let vm_manager = Arc::new(VmManager::new(&project_root, &vm_state_dir, &host_cache_url));
    let sessions = Arc::new(SessionTracker::new(Duration::from_secs(30 * 60)));
    let factory: Arc<dyn LlmBackendFactory> = Arc::new(AnthropicFactory {
        api_key: llm_api_key,
    });

    // Spawn idle timeout reaper
    {
        let sessions = Arc::clone(&sessions);
        let vm_manager = Arc::clone(&vm_manager);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let expired = sessions.expired_sessions().await;
                for thread_id in expired {
                    if let Some(session) = sessions.remove(thread_id).await {
                        info!(vm_id = %session.vm_id, thread_id = %thread_id, "reaping idle session");
                        if let Err(e) = vm_manager.destroy(&session.vm_id).await {
                            error!(error = %e, vm_id = %session.vm_id, "failed to destroy idle VM");
                        }
                    }
                }
            }
        });
    }

    let data = BotData {
        vm_manager,
        sessions,
        llm_backend_factory: factory,
    };

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                ephemeral_nixos_bot::bot::commands::create(),
                ephemeral_nixos_bot::bot::commands::destroy(),
                ephemeral_nixos_bot::bot::commands::status(),
            ],
            event_handler: |ctx, event, _framework, data| {
                Box::pin(async move {
                    if let serenity::FullEvent::Message { new_message } = event {
                        if let Err(e) =
                            ephemeral_nixos_bot::bot::handler::handle_message(ctx, new_message, data)
                                .await
                        {
                            error!(error = %e, "message handler error");
                        }
                    }
                    Ok(())
                })
            },
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                info!("bot ready, commands registered globally");
                Ok(data)
            })
        })
        .build();

    let intents =
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT;

    let mut client = serenity::ClientBuilder::new(&discord_token, intents)
        .framework(framework)
        .await
        .expect("failed to create Discord client");

    info!("starting bot");
    if let Err(e) = client.start().await {
        error!(error = %e, "client error");
    }
}
