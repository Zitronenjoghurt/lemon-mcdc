use serenity::all::ChannelId;
use std::env;
use std::sync::Arc;

pub struct Config {
    pub bot_token: String,
    pub mc_channel_id: ChannelId,
    pub webhook_url: String,
    pub rcon_host: String,
    pub rcon_port: String,
    pub rcon_password: String,
    pub mc_log_path: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Arc<Self>> {
        Ok(Arc::new(Self {
            bot_token: env::var("BOT_TOKEN")?,
            mc_channel_id: env::var("MC_CHANNEL_ID")?.parse()?,
            webhook_url: env::var("WEBHOOK_URL")?,
            rcon_host: env::var("RCON_HOST")?,
            rcon_port: env::var("RCON_PORT")?,
            rcon_password: env::var("RCON_PASSWORD")?,
            mc_log_path: env::var("MC_LOG_PATH")?,
        }))
    }

    pub fn rcon_url(&self) -> String {
        format!("{}:{}", self.rcon_host, self.rcon_port)
    }
}
