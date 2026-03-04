# Ephemeral NixOS on Discord

**Learn NixOS by doing.** A Discord bot that launches ephemeral NixOS sandbox VMs with an LLM agent that teaches you NixOS interactively.

Every command the agent runs streams to your Discord thread in real-time — you watch it work, learn the NixOS way, and experiment freely in a disposable VM.

**[Documentation](https://jhhuh.github.io/ephemeral-nixos-on-discord)**

## How It Works

```
/create "Python and PostgreSQL"
```

> :gear: **Rebuilding NixOS:**
> ```nix
> { pkgs, ... }: {
>   environment.systemPackages = [ pkgs.python3 ];
>   services.postgresql.enable = true;
> }
> ```
> :white_check_mark: nixos-rebuild completed successfully
>
> Your sandbox is ready. The agent used NixOS's declarative config
> instead of `apt install` — and it'll explain why.

## Features

- **Live learning** — every command streams to Discord as it runs
- **NixOS tutor** — explains concepts, prefers declarative config, teaches by showing
- **Break things safely** — VMs are ephemeral, destructive commands welcome
- **Nix language** — interactive learning with `nix eval` and `nix repl`
- **3 LLM backends** — Anthropic, OpenAI, Ollama (local)
- **Natural language config** — describe what you want, LLM generates NixOS modules
- **Secure** — QEMU hardware isolation, SLIRP networking, rate limiting
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
