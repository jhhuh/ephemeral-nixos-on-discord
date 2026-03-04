# Ephemeral NixOS on Discord

**Learn NixOS by doing.** A Discord bot that launches ephemeral NixOS sandbox VMs with an LLM agent that teaches you NixOS interactively.

Every command the agent runs streams to your Discord thread in real-time — you watch it work, learn the NixOS way, and experiment freely in a disposable VM.

**[Documentation](https://jhhuh.github.io/ephemeral-nixos-on-discord)**

## How It Works

```
/create "Python and PostgreSQL"
```

> 🔧 **Running:** `nixos-rebuild switch`
> ✅ **Output:** `activating the configuration...`
>
> Your sandbox is ready with Python and PostgreSQL installed via NixOS's
> declarative configuration. The agent explains what it did and why.

## Features

- **Live learning** — every command streams to Discord with rich formatting
- **NixOS tutor** — agent explains concepts, prefers the declarative "NixOS way"
- **Ephemeral VMs** — fresh NixOS QEMU VMs via [microvm.nix](https://github.com/astro/microvm.nix), destroyed on timeout
- **LLM-driven** — pluggable backends (Anthropic, OpenAI, Ollama)
- **Natural language config** — describe what you want, LLM generates NixOS config
- **Secure** — QEMU hardware isolation, SLIRP networking, per-user rate limiting
- **NixOS module** — deploy with `services.nixos-sandbox.enable = true`

## Quick Start

```bash
git clone https://github.com/jhhuh/ephemeral-nixos-on-discord.git
cd ephemeral-nixos-on-discord

export DISCORD_TOKEN="your-bot-token"
export LLM_API_KEY="your-api-key"
nix develop -c cargo run
```

## Commands

| Command | Description |
|---------|-------------|
| `/create [description]` | Create a sandbox VM, open a thread |
| `/destroy` | Destroy the sandbox in current thread |
| `/status` | Show VM uptime and idle time |
| `/download <path>` | Download a file from the sandbox |

## Architecture

```
Discord thread  →  Poise bot  →  LLM agent loop  →  QGA client  →  QEMU VM (microvm.nix)
                                     ↓
                              streams every command
                              and output to Discord
```

## License

MIT
