use crate::config::Config;
use crate::log_watcher::watch_logs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{error, info};

// Minecraft drops RCON commands longer than this
const MAX_RCON_COMMAND_BYTES: usize = 1413;

pub struct EventHandler {
    pub config: Arc<Config>,
    log_watcher_started: AtomicBool,
}

impl EventHandler {
    pub fn new(config: &Arc<Config>) -> Self {
        Self {
            config: config.clone(),
            log_watcher_started: AtomicBool::new(false),
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

        let mut content = msg.content.split_whitespace().collect::<Vec<_>>().join(" ");
        if !msg.attachments.is_empty() {
            if !content.is_empty() {
                content.push(' ');
            }
            content.push_str("[attachment]");
        }
        if content.is_empty() {
            return;
        }

        let Some(cmd) = tellraw_command(display_name, &name_color, &content) else {
            error!("Discord message too long to relay, even truncated");
            return;
        };

        let conn =
            rcon::Connection::connect(self.config.rcon_url(), &self.config.rcon_password).await;
        match conn {
            Ok(mut conn) => {
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

        if self.log_watcher_started.swap(true, Ordering::SeqCst) {
            return;
        }

        let http = ctx.http.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            watch_logs(http, config).await;
        });
    }
}

fn tellraw_command(display_name: &str, name_color: &str, content: &str) -> Option<String> {
    let build = |text: &str| {
        format!(
            r#"tellraw @a [{{"text":"[{}]","color":"{name_color}"}},{{"text":" {}","color":"white"}}]"#,
            escape(display_name),
            escape(text)
        )
    };

    let cmd = build(content);
    if cmd.len() <= MAX_RCON_COMMAND_BYTES {
        return Some(cmd);
    }

    let chars: Vec<char> = content.chars().collect();
    let mut keep = chars.len();
    while keep > 0 {
        keep = keep.saturating_sub(50);
        let truncated: String = chars[..keep].iter().collect::<String>() + "…";
        let cmd = build(&truncated);
        if cmd.len() <= MAX_RCON_COMMAND_BYTES {
            return Some(cmd);
        }
    }
    None
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}
