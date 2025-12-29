use crate::config::Config;
use crate::events::EventHandler;
use serenity::all::GatewayIntents;
use serenity::Client;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod config;
mod events;
mod log_watcher;

#[tokio::main]
async fn main() {
    init_logging();
    info!("Starting bot...");

    let config = Config::from_env().unwrap();
    let event_handler = EventHandler::new(&config);

    let intents = GatewayIntents::non_privileged()
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_MEMBERS;
    let mut client = Client::builder(&config.bot_token, intents)
        .event_handler(event_handler)
        .await
        .unwrap();

    client.start().await.unwrap();
}

fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}
