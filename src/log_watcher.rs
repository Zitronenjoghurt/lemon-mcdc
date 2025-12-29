use crate::config::Config;
use regex::Regex;
use serenity::all::{ExecuteWebhook, Http, Webhook};
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader, SeekFrom};
use tracing::{error, info, warn};

pub async fn watch_logs(http: Arc<Http>, config: Arc<Config>) {
    let log_path = config.mc_log_path.as_str();

    loop {
        if tokio::fs::metadata(log_path).await.is_ok() {
            break;
        }
        info!("Waiting for log file: {log_path}");
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    let webhook = match Webhook::from_url(&http, &config.webhook_url).await {
        Ok(wh) => wh,
        Err(err) => {
            error!("Failed to get webhook: {err}");
            return;
        }
    };

    loop {
        if let Err(err) = watch_loop(log_path, &webhook, &http).await {
            warn!("Log watcher error: {err}, restarting...");
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
}

async fn watch_loop(log_path: &str, webhook: &Webhook, http: &Http) -> anyhow::Result<()> {
    let ansi_re = Regex::new(r"\x1b\[[0-9;]*m")?;
    let chat_re = Regex::new(r"INFO\]: <(\w+)> (.+)$")?;
    let join_re = Regex::new(r"INFO\]: (\w+) joined the game")?;
    let leave_re = Regex::new(r"INFO\]: (\w+) left the game")?;
    let death_re = Regex::new(
        r"INFO\]: (\w+) (was|died|fell|drowned|burned|tried|hit|walked|went|experienced|blew|starved|withered|discovered|froze|suffocated)",
    )?;
    let advancement_re = Regex::new(
        r"INFO\]: (\w+) has (made the advancement|completed the challenge|reached the goal) \[(.+)\]$",
    )?;

    let file = File::open(log_path).await?;
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::End(0)).await?;

    let mut last_pos = reader.stream_position().await?;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            let metadata = tokio::fs::metadata(log_path).await?;
            if metadata.len() < last_pos {
                info!("Log file rotated, reopening...");
                return Ok(());
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        }

        last_pos = reader.stream_position().await?;

        let line = line.trim();
        let line = &*ansi_re.replace_all(line, "");

        if let Some(caps) = chat_re.captures(line) {
            let player = &caps[1];
            let message = &caps[2];
            send_as_player(webhook, http, player, message).await;
            continue;
        }

        if let Some(caps) = join_re.captures(line) {
            let player = &caps[1];
            send_as_player(webhook, http, player, "🟢 **`Joined the game`**").await;
            continue;
        }

        if let Some(caps) = leave_re.captures(line) {
            let player = &caps[1];
            send_as_player(webhook, http, player, "🔴 **`Left the game`**").await;
            continue;
        }

        if let Some(caps) = advancement_re.captures(line) {
            let player = &caps[1];
            let advancement = &caps[3];
            send_as_player(webhook, http, player, &format!("🏆 **`{advancement}`**")).await;
            continue;
        }

        if let Some(caps) = death_re.captures(line) {
            let player = &caps[1];
            if let Some(death_start) = line.find("INFO]: ") {
                let death_msg = &line[death_start + 7..];
                send_as_player(webhook, http, player, &format!("☠️ **`{death_msg}`**")).await;
            }
            continue;
        }
    }
}

async fn send_as_player(webhook: &Webhook, http: &Http, player: &str, message: &str) {
    let avatar_url = format!("https://mc-heads.net/avatar/{player}/128");

    let builder = ExecuteWebhook::new()
        .username(player)
        .avatar_url(&avatar_url)
        .content(message);

    if let Err(err) = webhook.execute(http, false, builder).await {
        error!("Failed to send webhook: {err}");
    }
}
