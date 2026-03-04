# Ephemeral NixOS on Discord

A Discord bot that launches **ephemeral NixOS virtual machines** and lets you interact with them through conversation with an LLM agent. **Learn NixOS by doing** — watch the agent work, see every command, and experiment freely in a disposable sandbox.

Each sandbox is a QEMU VM managed by [microvm.nix](https://github.com/astro/microvm.nix), controlled via the QEMU Guest Agent protocol. The LLM agent executes commands, reads/writes files, and applies NixOS configuration changes inside the VM — streaming every action to your Discord thread in real-time.

## How It Works

```
Discord thread  →  Poise bot  →  LLM agent loop  →  QGA client  →  QEMU VM
```

1. User runs `/create "Python 3.12 with PostgreSQL"`
2. Bot generates a NixOS config from the description via LLM
3. microvm.nix builds and launches a QEMU VM
4. A dedicated Discord thread opens for the sandbox
5. User chats in the thread — the LLM executes tools inside the VM
6. **Every command and its output streams to the thread in real-time**
7. VM auto-destroys after 30 minutes of inactivity

## Example Interaction

> **You:** install htop and show me system info
>
> **Bot:** I'll install htop using the NixOS way — adding it to the system configuration rather than installing imperatively.
>
> 🔧 **Running:**
> ```bash
> nix-env -iA nixpkgs.htop
> ```
> ✅ **Output:**
> ```
> installing 'htop-3.3.0'
> ```
>
> 🔧 **Running:**
> ```bash
> htop --version && uname -a
> ```
> ✅ **Output:**
> ```
> htop 3.3.0
> Linux sandbox-a1b2c3d4 6.6.x #1 SMP NixOS x86_64 GNU/Linux
> ```
>
> htop is installed! I used `nix-env -iA` for a quick imperative install. On NixOS, the **declarative way** would be adding `pkgs.htop` to `environment.systemPackages` in your configuration and running `nixos-rebuild switch`. Want me to show you how?

## Features

- **Live learning** — every command the agent runs streams to Discord with rich formatting, so you can watch and learn NixOS
- **NixOS tutor** — the agent explains what it does, teaches NixOS concepts, and prefers the "NixOS way"
- **Ephemeral VMs** — fresh NixOS QEMU VMs, destroyed on timeout. Break anything — that's the point.
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
