# Ephemeral NixOS on Discord

A Discord bot that launches **ephemeral NixOS virtual machines** and lets you interact with them through conversation with an LLM agent.

Each sandbox is a QEMU VM managed by [microvm.nix](https://github.com/astro/microvm.nix), controlled via the QEMU Guest Agent protocol. The LLM can execute commands, read/write files, and apply NixOS configuration changes inside the VM — all from a Discord thread.

## How It Works

```
Discord thread  →  Poise bot  →  LLM agent loop  →  QGA client  →  QEMU VM
```

1. User runs `/create "Python 3.12 with PostgreSQL"`
2. Bot generates a NixOS config from the description via LLM
3. microvm.nix builds and launches a QEMU VM
4. A dedicated Discord thread opens for the sandbox
5. User chats in the thread — the LLM executes tools inside the VM
6. VM auto-destroys after 30 minutes of inactivity

## Features

- **Ephemeral VMs** — each sandbox is a fresh NixOS QEMU VM, destroyed on timeout or command
- **LLM-driven** — pluggable backends (Anthropic, OpenAI, Ollama) with tool-use agent loop
- **Natural language config** — describe what you want, the LLM generates NixOS config
- **Secure isolation** — QEMU hardware boundary, SLIRP networking (host unreachable), cgroup limits
- **File transfer** — `/download` command to retrieve files from the sandbox
- **Per-user rate limiting** — max 2 concurrent VMs per user, 30s cooldown
- **Multiple networking modes** — SLIRP (default), bridge + nftables, veth + nftables

## Quick Start

```bash
export DISCORD_TOKEN="your-bot-token"
export LLM_API_KEY="your-api-key"
nix develop -c cargo run
```

See [Getting Started](getting-started.md) for full setup instructions.
