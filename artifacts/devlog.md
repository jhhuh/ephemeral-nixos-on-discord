# Dev Log

## 2026-03-04: Project initialized
- Scaffolded flake.nix (crane + microvm.nix), Cargo.toml, directory structure
- Design doc approved: QEMU VMs via microvm.nix, QGA control, poise Discord bot

## 2026-03-04: Natural language NixOS config generation
- Created `src/llm/config_gen.rs`: `generate_nixos_config()` sends description to LLM with a system prompt that constrains output to a NixOS module expression. Strips accidental markdown fencing from response.
- Wired into `/create` command: if user provides a description, it generates config via LLM, validates syntax with `nix-instantiate --parse` (via `spawn_blocking` since `validate_nix_syntax` is synchronous), and passes to `VmManager::create`. Falls back to base config on LLM error or syntax failure.
- Updated slash command description param from "reserved for future use" to actionable hint.
