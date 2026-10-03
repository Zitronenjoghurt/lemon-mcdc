use crate::config::Config;
use regex::Regex;
use serenity::all::{CreateAllowedMentions, ExecuteWebhook, Http, Webhook};
use std::collections::HashSet;
use std::os::unix::fs::MetadataExt;
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

    let parser = LineParser::new();
    let mut online = online_players(&config).await;
    let mut from_start = false;

    loop {
        match watch_loop(log_path, from_start, &parser, &mut online, &webhook, &http).await {
            Ok(()) => {
                online.clear();
                from_start = true;
            }
            Err(err) => {
                warn!("Log watcher error: {err}, restarting...");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

async fn watch_loop(
    log_path: &str,
    from_start: bool,
    parser: &LineParser,
    online: &mut HashSet<String>,
    webhook: &Webhook,
    http: &Http,
) -> anyhow::Result<()> {
    let file = File::open(log_path).await?;
    let inode = file.metadata().await?.ino();
    let mut reader = BufReader::new(file);
    if !from_start {
        reader.seek(SeekFrom::End(0)).await?;
    }

    let mut last_pos = reader.stream_position().await?;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            let metadata = tokio::fs::metadata(log_path).await?;
            if metadata.ino() != inode || metadata.len() < last_pos {
                info!("Log file rotated, reopening...");
                return Ok(());
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        }

        if !line.ends_with('\n') {
            reader.seek(SeekFrom::Start(last_pos)).await?;
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        }

        last_pos = reader.stream_position().await?;

        let Some(event) = parser.parse(line.trim_end(), online) else {
            continue;
        };
        match &event {
            Event::Join { player } => {
                online.insert(player.clone());
            }
            Event::Leave { player } => {
                online.remove(player);
            }
            _ => {}
        }
        let (username, avatar, content) = event.into_message();
        send(webhook, http, &username, avatar.as_deref(), &content).await;
    }
}

const VILLAGER_AVATAR: &str = "https://minotar.net/helm/MHF_Villager/128";
const SERVER_NAME: &str = "Server";

#[derive(Debug, PartialEq)]
enum Event {
    Chat {
        player: String,
        message: String,
    },
    Emote {
        player: String,
        action: String,
    },
    Announcement {
        sender: String,
        message: String,
    },
    Join {
        player: String,
    },
    Leave {
        player: String,
    },
    Advancement {
        player: String,
        advancement: String,
    },
    Death {
        player: String,
        message: String,
    },
    VillagerDeath {
        name: String,
        message: String,
        pos: (i64, i64, i64),
    },
    NamedEntityDeath {
        kind: String,
        name: String,
        message: String,
    },
    ServerStarted,
    ServerStopping,
}

impl Event {
    fn into_message(self) -> (String, Option<String>, String) {
        match self {
            Event::Chat { player, message } => {
                let avatar = player_avatar(&player);
                (player, Some(avatar), message)
            }
            Event::Emote { player, action } => {
                let avatar = player_avatar(&player);
                (player, Some(avatar), format!("*{action}*"))
            }
            Event::Announcement { sender, message } => {
                let avatar = (sender != SERVER_NAME).then(|| player_avatar(&sender));
                (sender, avatar, format!("📢 **{message}**"))
            }
            Event::Join { player } => {
                let avatar = player_avatar(&player);
                (player, Some(avatar), "🟢 **`Joined the game`**".to_string())
            }
            Event::Leave { player } => {
                let avatar = player_avatar(&player);
                (player, Some(avatar), "🔴 **`Left the game`**".to_string())
            }
            Event::Advancement {
                player,
                advancement,
            } => {
                let avatar = player_avatar(&player);
                (player, Some(avatar), format!("🏆 **`{advancement}`**"))
            }
            Event::Death { player, message } => {
                let avatar = player_avatar(&player);
                (player, Some(avatar), format!("☠️ **`{message}`**"))
            }
            Event::VillagerDeath {
                name,
                message,
                pos: (x, y, z),
            } => (
                name,
                Some(VILLAGER_AVATAR.to_string()),
                format!("⚰️ **`{message}`** at `{x} {y} {z}`"),
            ),
            Event::NamedEntityDeath {
                kind,
                name,
                message,
            } => {
                let avatar = (kind == "WanderingTrader").then(|| VILLAGER_AVATAR.to_string());
                (name, avatar, format!("⚰️ **`{message}`**"))
            }
            Event::ServerStarted => (
                SERVER_NAME.to_string(),
                None,
                "✅ **`Server is online`**".to_string(),
            ),
            Event::ServerStopping => (
                SERVER_NAME.to_string(),
                None,
                "🛑 **`Server is stopping`**".to_string(),
            ),
        }
    }
}

fn player_avatar(player: &str) -> String {
    format!("https://minotar.net/helm/{player}/128")
}

struct LineParser {
    ansi: Regex,
    body: Regex,
    chat: Regex,
    emote: Regex,
    announcement: Regex,
    join: Regex,
    leave: Regex,
    advancement: Regex,
    death: Regex,
    lava_death: Regex,
    villager_death: Regex,
    named_entity_death: Regex,
    server_started: Regex,
}

const ENTITY: &str = r"(\w+)\['(.*)'/\d+, l='[^']*', x=(-?\d+)[.,]\d+, y=(-?\d+)[.,]\d+, z=(-?\d+)[.,]\d+(?:, removed=\w+)?\]";

impl LineParser {
    fn new() -> Self {
        Self {
            ansi: Regex::new(r"\x1b\[[0-9;]*m").unwrap(),
           body: Regex::new(r"/INFO\]: (?:System chat: )?(.+)$").unwrap(),
            chat: Regex::new(r"^(?:\[Not Secure\] )?<(\w{1,16})> (.+)$").unwrap(),
            emote: Regex::new(r"^(?:\[Not Secure\] )?\* (\w{1,16}) (.+)$").unwrap(),
            announcement: Regex::new(r"^(?:\[Not Secure\] )?\[(\w{1,16})\] (.+)$").unwrap(),
            join: Regex::new(r"^(\w{1,16})(?: \(formerly known as \w+\))? joined the game$")
                .unwrap(),
            leave: Regex::new(r"^(\w{1,16}) left the game$").unwrap(),
            advancement: Regex::new(
                r"^(\w{1,16}) has (?:made the advancement|completed the challenge|reached the goal) \[(.+)\]$",
            )
            .unwrap(),
             death: Regex::new(
                r"^(\w{1,16}) (?:was|died|fell|drowned|burned|tried|hit|walked|went|experienced|blew|starved|withered|discovered|froze|suffocated|didn't|left the confines)\b",
            )
            .unwrap(),
            lava_death: Regex::new(r"^\w{1,16} showed (\w{1,16}) that not just the floor is lava")
                .unwrap(),
            villager_death: Regex::new(&format!(r"^Villager {ENTITY} died, message: '(.+)'$"))
                .unwrap(),
            named_entity_death: Regex::new(&format!(r"^Named entity {ENTITY} died: (.+)$")).unwrap(),
            server_started: Regex::new(r#"^Done \([\d.,]+s\)! For help, type "help"$"#).unwrap(),
        }
    }

    fn parse(&self, line: &str, online: &HashSet<String>) -> Option<Event> {
        let line = self.ansi.replace_all(line, "");
        let body = self.body.captures(&line)?.get(1)?.as_str();

        if let Some(caps) = self.chat.captures(body) {
            return Some(Event::Chat {
                player: caps[1].to_string(),
                message: caps[2].to_string(),
            });
        }
        if let Some(caps) = self.join.captures(body) {
            return Some(Event::Join {
                player: caps[1].to_string(),
            });
        }
        if let Some(caps) = self.leave.captures(body) {
            return Some(Event::Leave {
                player: caps[1].to_string(),
            });
        }
        if let Some(caps) = self.advancement.captures(body) {
            return Some(Event::Advancement {
                player: caps[1].to_string(),
                advancement: caps[2].to_string(),
            });
        }
        if let Some(caps) = self.villager_death.captures(body) {
            return Some(Event::VillagerDeath {
                name: caps[2].to_string(),
                message: caps[6].to_string(),
                pos: (
                    caps[3].parse().ok()?,
                    caps[4].parse().ok()?,
                    caps[5].parse().ok()?,
                ),
            });
        }
        if let Some(caps) = self.named_entity_death.captures(body)
            && &caps[1] != "Villager"
        {
            return Some(Event::NamedEntityDeath {
                kind: caps[1].to_string(),
                name: caps[2].to_string(),
                message: caps[6].to_string(),
            });
        }
        if body == "Stopping server" {
            return Some(Event::ServerStopping);
        }
        if self.server_started.is_match(body) {
            return Some(Event::ServerStarted);
        }

        let is_online = |name: &str| online.contains(name);
        if let Some(caps) = self.emote.captures(body)
            && is_online(&caps[1])
        {
            return Some(Event::Emote {
                player: caps[1].to_string(),
                action: caps[2].to_string(),
            });
        }
        if let Some(caps) = self.announcement.captures(body)
            && (&caps[1] == SERVER_NAME || is_online(&caps[1]))
        {
            return Some(Event::Announcement {
                sender: caps[1].to_string(),
                message: caps[2].to_string(),
            });
        }
        let victim = self
            .death
            .captures(body)
            .or_else(|| self.lava_death.captures(body))
            .map(|caps| caps[1].to_string());
        if let Some(victim) = victim
            && is_online(&victim)
        {
            return Some(Event::Death {
                player: victim,
                message: body.to_string(),
            });
        }
        None
    }
}

async fn online_players(config: &Config) -> HashSet<String> {
    let response = async {
        let mut conn = rcon::Connection::connect(config.rcon_url(), &config.rcon_password).await?;
        conn.cmd("list").await
    }
    .await;

    match response {
        Ok(response) => response
            .split_once(": ")
            .map(|(_, names)| {
                names
                    .split(", ")
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        Err(err) => {
            warn!("Could not list online players: {err}");
            HashSet::new()
        }
    }
}

async fn send(webhook: &Webhook, http: &Http, username: &str, avatar: Option<&str>, content: &str) {
    let mut builder = ExecuteWebhook::new()
        .username(username)
        .content(content)
        .allowed_mentions(CreateAllowedMentions::new());
    if let Some(avatar) = avatar {
        builder = builder.avatar_url(avatar);
    }

    if let Err(err) = webhook.execute(http, false, builder).await {
        error!("Failed to send webhook: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn online(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn parse(line: &str, players: &[&str]) -> Option<Event> {
        LineParser::new().parse(line, &online(players))
    }

    #[test]
    fn system_chat_prefix_of_26_3() {
        assert_eq!(
            parse(
                "[15:00:34] [Server thread/INFO]: System chat: Steve joined the game",
                &[]
            ),
            Some(Event::Join {
                player: "Steve".into()
            })
        );
        assert_eq!(
            parse(
                "[15:10:12] [Server thread/INFO]: System chat: Steve left the game",
                &[]
            ),
            Some(Event::Leave {
                player: "Steve".into()
            })
        );
        assert_eq!(
            parse(
                "[15:10:12] [Server thread/INFO]: System chat: <Steve> hi there",
                &[]
            ),
            Some(Event::Chat {
                player: "Steve".into(),
                message: "hi there".into()
            })
        );
    }

    #[test]
    fn plain_lines_of_older_versions() {
        assert_eq!(
            parse("[18:17:42] [Server thread/INFO]: <Steve> stahp", &[]),
            Some(Event::Chat {
                player: "Steve".into(),
                message: "stahp".into()
            })
        );
        assert_eq!(
            parse(
                "[18:17:30] [Server thread/INFO]: Alex has made the advancement [Monster Hunter]",
                &[]
            ),
            Some(Event::Advancement {
                player: "Alex".into(),
                advancement: "Monster Hunter".into()
            })
        );
        assert_eq!(
            parse(
                "[18:17:53] [Server thread/INFO]: Steve was slain by Alex",
                &["Steve"]
            ),
            Some(Event::Death {
                player: "Steve".into(),
                message: "Steve was slain by Alex".into()
            })
        );
    }

    #[test]
    fn not_secure_chat() {
        assert_eq!(
            parse(
                "[12:00:00] [Server thread/INFO]: [Not Secure] <Steve> hello",
                &[]
            ),
            Some(Event::Chat {
                player: "Steve".into(),
                message: "hello".into()
            })
        );
    }

    #[test]
    fn deaths_only_for_online_players() {
        let line = "[12:00:00] [Server thread/INFO]: System chat: Eric died by zombie at 1, 64, 2";
        assert_eq!(parse(line, &["Steve"]), None);
        assert!(matches!(parse(line, &["Eric"]), Some(Event::Death { .. })));
    }

    #[test]
    fn ignores_other_system_chat() {
        for line in [
            "[14:59:31] [Server thread/INFO]: System chat: [Rcon: Saved the game]",
            "[15:00:50] [Server thread/INFO]: System chat: [Steve: Set own game mode to Creative Mode]",
            "[15:02:27] [spark-worker-pool-1-thread-3/INFO]: System chat: [⚡] Health Report:",
            "[12:00:00] [Server thread/INFO]: System chat: Steve is now AFK",
        ] {
            assert_eq!(parse(line, &["Steve"]), None, "{line}");
        }
    }

    #[test]
    fn villager_deaths_from_the_vanilla_log_line() {
        let line = "[12:00:00] [Server thread/INFO]: Villager Villager['Eric'/412, l='ServerLevel[world]', x=101.50, y=64.00, z=-23.30] died, message: 'Eric was slain by Zombie'";
        assert_eq!(
            parse(line, &[]),
            Some(Event::VillagerDeath {
                name: "Eric".into(),
                message: "Eric was slain by Zombie".into(),
                pos: (101, 64, -23),
            })
        );
    }

    #[test]
    fn named_villagers_are_not_reported_twice() {
        let line = "[12:00:00] [Server thread/INFO]: Named entity Villager['Eric'/412, l='ServerLevel[world]', x=101.50, y=64.00, z=-23.30] died: Eric was slain by Zombie";
        assert_eq!(parse(line, &[]), None);
    }

    #[test]
    fn named_pet_deaths() {
        let line = "[12:00:00] [Server thread/INFO]: Named entity Wolf['Rex'/77, l='ServerLevel[world]', x=5.00, y=70.00, z=5.00] died: Rex was blown up by Creeper";
        assert_eq!(
            parse(line, &[]),
            Some(Event::NamedEntityDeath {
                kind: "Wolf".into(),
                name: "Rex".into(),
                message: "Rex was blown up by Creeper".into(),
            })
        );
    }

    #[test]
    fn say_and_me() {
        assert_eq!(
            parse(
                "[12:00:00] [Server thread/INFO]: [Steve] hello all",
                &["Steve"]
            ),
            Some(Event::Announcement {
                sender: "Steve".into(),
                message: "hello all".into()
            })
        );
        assert_eq!(
            parse(
                "[12:00:00] [Server thread/INFO]: [Server] restart in 5",
                &[]
            ),
            Some(Event::Announcement {
                sender: "Server".into(),
                message: "restart in 5".into()
            })
        );
        assert_eq!(
            parse("[12:00:00] [Server thread/INFO]: * Steve waves", &["Steve"]),
            Some(Event::Emote {
                player: "Steve".into(),
                action: "waves".into()
            })
        );
        assert_eq!(
            parse(
                "[12:00:00] [Server thread/INFO]: System chat: [Chunky] Task started",
                &["Steve"]
            ),
            None
        );
    }

    #[test]
    fn server_start_and_stop() {
        assert_eq!(
            parse(
                r#"[14:59:27] [Server thread/INFO]: Done (41.354s)! For help, type "help""#,
                &[]
            ),
            Some(Event::ServerStarted)
        );
        assert_eq!(
            parse("[15:13:35] [Server thread/INFO]: Stopping server", &[]),
            Some(Event::ServerStopping)
        );
    }

    #[test]
    fn floor_is_lava_names_the_attacker_first() {
        let line = "[12:00:00] [Server thread/INFO]: System chat: Alex showed Steve that not just the floor is lava";
        assert!(matches!(
            parse(line, &["Steve"]),
            Some(Event::Death { player, .. }) if player == "Steve"
        ));
    }
}
