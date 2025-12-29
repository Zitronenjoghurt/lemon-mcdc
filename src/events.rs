use crate::config::Config;
use crate::log_watcher::watch_logs;
use std::sync::Arc;
use tracing::{error, info};

pub struct EventHandler {
    pub config: Arc<Config>,
}

impl EventHandler {
    pub fn new(config: &Arc<Config>) -> Self {
        Self {
            config: config.clone(),
        }
    }
}

#[serenity::async_trait]
impl serenity::all::EventHandler for EventHandler {
    async fn message(&self, ctx: serenity::all::Context, msg: serenity::all::Message) {
        if msg.author.bot || msg.channel_id != self.config.mc_channel_id {
            return;
        }

        let display_name = msg
            .member
            .as_ref()
            .and_then(|m| m.nick.as_deref())
            .unwrap_or_else(|| msg.author.display_name());

        let name_color = match msg.guild_id {
            Some(guild_id) => match guild_id.member(&ctx.http, msg.author.id).await {
                Ok(member) => member
                    .colour(&ctx.cache)
                    .map(|c| format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b()))
                    .unwrap_or_else(|| "white".to_string()),
                Err(err) => {
                    error!("Failed to fetch member: {err}");
                    "white".to_string()
                }
            },
            None => "white".to_string(),
        };

        let conn =
            rcon::Connection::connect(self.config.rcon_url(), &self.config.rcon_password).await;
        match conn {
            Ok(mut conn) => {
                let cmd = format!(
                    r#"tellraw @a [{{"text":"[{}]","color":"{name_color}"}},{{"text":" {}","color":"white"}}]"#,
                    display_name.replace('\\', "\\\\").replace('"', "\\\""),
                    msg.content.replace('\\', "\\\\").replace('"', "\\\"")
                );
                if let Err(err) = conn.cmd(&cmd).await {
                    error!("Failed to send RCON command: {err}");
                }
            }
            Err(err) => {
                error!("Failed to connect to RCON: {err}");
            }
        }
    }

    async fn ready(&self, ctx: serenity::all::Context, _ready: serenity::all::Ready) {
        info!("Bot online!");

        let http = ctx.http.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            watch_logs(http, config).await;
        });
    }
}
