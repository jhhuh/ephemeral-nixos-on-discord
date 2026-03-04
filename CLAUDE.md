# Ephemeral NixOS Discord Bot

## Build
- `nix build` — build the bot binary
- `nix develop` — enter dev shell
- `cargo test` — run tests
- `cargo clippy` — lint

## Architecture
See `docs/plans/2026-03-04-ephemeral-nixos-discord-bot-design.md`

## Environment Variables
- `DISCORD_TOKEN` — Discord bot token
- `LLM_BACKEND` — "anthropic", "openai", or "ollama"
- `LLM_API_KEY` — API key for the LLM backend
- `VM_STATE_DIR` — directory for VM state (default: /var/lib/nixos-sandbox)
