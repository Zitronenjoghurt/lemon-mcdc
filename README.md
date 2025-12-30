# lemon-mcdc

A minecraft-discord bridge.

# Usage

You can drop this bot into your docker-compose setup.

Example using itzg's minecraft server image:

```yaml
services:
  mc:
    image: itzg/minecraft-server
    tty: true
    stdin_open: true
    ports:
      - "25565:25565"
    environment:
      EULA: "TRUE"
      TYPE: "PAPER"
      VERSION: "1.21.11"
  
      MAX_PLAYERS: "10"
      MOTD: "Test Server"
      DIFFICULTY: "normal"
      MODE: "survival"
      PVP: "false"
      SPAWN_PROTECTION: "1"
  
      MEMORY: "3G"
      USE_AIKAR_FLAGS: "TRUE"
  
      ENABLE_RCON: "TRUE"
      RCON_PASSWORD: "YOUR_RCON_PASSWORD"
    volumes:
      - ./data:/data
    restart: unless-stopped
  
  backup:
    image: itzg/mc-backup
    depends_on:
      mc:
        condition: service_healthy
    environment:
      BACKUP_INTERVAL: "2h"
      RCON_HOST: mc
      RCON_PASSWORD: "YOUR_RCON_PASSWORD"
      INITIAL_DELAY: "0"
      PRUNE_BACKUPS_DAYS: "3"
      BACKUP_METHOD: "tar"
      TZ: "Europe/Berlin"
    volumes:
      - ./data:/data:ro
      - ./backups:/backups
    restart: unless-stopped

  restore-backup:
    image: itzg/mc-backup
    restart: "no"
    entrypoint: restore-tar-backup
    volumes:
      - ./data:/data
      - ./backups:/backups:ro
  
  bot:
    image: zitronenjoghurt/mcdc-bot:latest
    environment:
      BOT_TOKEN: "YOUR DISCORD BOT TOKEN"
      MC_CHANNEL_ID: "THE CHANNEL ID WHERE THE BOT WILL LISTEN TO MESSAGES"
      WEBHOOK_URL: "A WEBHOOK URL INTO THE CHANNEL WHERE THE BOT WILL SEND MESSAGES"
      RCON_HOST: "mc"
      RCON_PORT: "25575"
      RCON_PASSWORD: "YOUR_RCON_PASSWORD"
      MC_LOG_PATH: "/data/logs/latest.log"
      RUST_LOG: "debug,h2=info,hyper=info,rustls=info"
    volumes:
      - ./data:/data:ro
    depends_on:
      mc:
        condition: service_healthy
    restart: unless-stopped
```
