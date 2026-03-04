# Ephemeral NixOS Discord Bot

## Build
- `nix build` — build the bot binary
- `nix develop` — enter dev shell
- `cargo test` — run tests
- `cargo clippy` — lint

## Architecture
See `docs/plans/2026-03-04-ephemeral-nixos-discord-bot-design.md`

## Environment Variables
- `DISCORD_TOKEN` — Discord bot token (required)
- `LLM_BACKEND` — "anthropic" (default), "openai", or "ollama"
- `LLM_API_KEY` — API key (required for anthropic/openai, not needed for ollama)
- `VM_STATE_DIR` — directory for VM state (default: /tmp/ephemeral-vms)
- `HOST_CACHE_URL` — nix-serve URL (default: http://localhost:5557)
- `PROJECT_ROOT` — path to project root for nix/base-vm.nix (default: .)
- `OPENAI_API_BASE` — custom OpenAI-compatible API base URL (optional)
- `OLLAMA_BASE_URL` — Ollama server URL (default: http://localhost:11434)
- `OLLAMA_MODEL` — Ollama model name (default: llama3.1)

## NixOS Deployment
Import `nixosModules.default` from the flake. See `nix/host.nix` for options.
Secrets go in files referenced by `discordTokenFile` and `llmApiKeyFile`.
