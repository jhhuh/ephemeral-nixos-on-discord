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
> **Bot:** I'll add htop to the system configuration the NixOS way — declaratively through `environment.systemPackages`, then rebuild.
>
> :gear: **Rebuilding NixOS:**
> ```nix
> { pkgs, ... }: { environment.systemPackages = [ pkgs.htop ]; }
> ```
> Then running `nixos-rebuild switch`
>
> :white_check_mark: **Output:**
> ```
> building the system configuration...
> activating the configuration...
> nixos-rebuild completed successfully
> ```
>
> :wrench: **Running:**
> ```bash
> htop --version && uname -a
> ```
> :white_check_mark: **Output:**
> ```
> htop 3.3.0
> Linux sandbox-a1b2c3d4 6.6.x #1 SMP NixOS x86_64 GNU/Linux
> ```
>
> htop is now installed! Notice we didn't use `apt install` or `nix-env` — we declared the package in the NixOS configuration and rebuilt. This is the **declarative approach**: your system state is defined by config, not by a sequence of install commands. Want to try adding a service like PostgreSQL next?

## What You Can Learn

- **Declarative configuration** — add packages and services through NixOS modules, not imperative commands
- **Generations and rollbacks** — watch the agent demonstrate how NixOS tracks system versions
- **The Nix store** — see how `/nix/store` works, why paths are hashed, how closure works
- **Nix language** — write and evaluate Nix expressions interactively with `nix eval` and `nix repl`
- **Service management** — enable services like nginx, PostgreSQL, Redis through `services.*.enable`
- **Break things safely** — delete `/nix/store`, `rm -rf /`, stop systemd — the VM is disposable

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
