use crate::config::Config;
use crate::events::EventHandler;
use serenity::all::GatewayIntents;
use serenity::Client;
use tokio::signal::unix::{signal, SignalKind};
use tracing::{error, info};
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

    let shard_manager = client.shard_manager.clone();
    tokio::spawn(async move {
        wait_for_shutdown_signal().await;
        info!("Shutting down...");
        shard_manager.shutdown_all().await;
    });

    if let Err(err) = client.start().await {
        error!("Client error: {err}");
    }
}

async fn wait_for_shutdown_signal() {
    let mut sigterm = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
    tokio::select! {
        _ = sigterm.recv() => {}
        _ = sigint.recv() => {}
    }
}

fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}
