# Ephemeral NixOS on Discord

A Discord bot that launches ephemeral NixOS sandbox VMs with LLM agent interaction.

Each sandbox is a QEMU virtual machine managed by [microvm.nix](https://github.com/astro/microvm.nix), controlled via the QEMU Guest Agent protocol. Users chat in a dedicated Discord thread while an LLM agent executes commands, reads/writes files, and applies NixOS config changes inside the VM.

**[Documentation](https://jhhuh.github.io/ephemeral-nixos-on-discord)**

## Features

- **Ephemeral VMs** — fresh NixOS QEMU VMs, auto-destroyed on idle timeout
- **LLM-driven** — pluggable backends (Anthropic, OpenAI, Ollama) with tool-use agent loop
- **Natural language config** — `/create "Python 3.12 and PostgreSQL"` generates NixOS config via LLM
- **Secure isolation** — QEMU hardware boundary, SLIRP networking, per-user rate limiting
- **File transfer** — `/download /path/to/file` retrieves files from the sandbox
- **NixOS module** — deploy with `services.nixos-sandbox.enable = true`

## Quick Start

```bash
git clone https://github.com/jhhuh/ephemeral-nixos-on-discord.git
cd ephemeral-nixos-on-discord

export DISCORD_TOKEN="your-bot-token"
export LLM_API_KEY="your-api-key"
nix develop -c cargo run
```

## Architecture

```
Discord thread  →  Poise bot  →  LLM agent loop  →  QGA client  →  QEMU VM (microvm.nix)
```

## Commands

| Command | Description |
|---------|-------------|
| `/create [description]` | Create a sandbox VM, open a thread |
| `/destroy` | Destroy the sandbox in current thread |
| `/status` | Show VM uptime and idle time |
| `/download <path>` | Download a file from the sandbox |

## License

MIT
