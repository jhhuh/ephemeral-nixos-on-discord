# Dev Log

## 2026-03-04: Project initialized
- Scaffolded flake.nix (crane + microvm.nix), Cargo.toml, directory structure
- Design doc approved: QEMU VMs via microvm.nix, QGA control, poise Discord bot

## 2026-03-04: Natural language NixOS config generation
- Created `src/llm/config_gen.rs`: `generate_nixos_config()` sends description to LLM with a system prompt that constrains output to a NixOS module expression. Strips accidental markdown fencing from response.
- Wired into `/create` command: if user provides a description, it generates config via LLM, validates syntax with `nix-instantiate --parse` (via `spawn_blocking` since `validate_nix_syntax` is synchronous), and passes to `VmManager::create`. Falls back to base config on LLM error or syntax failure.
- Updated slash command description param from "reserved for future use" to actionable hint.

## 2026-03-04: Full implementation complete and merged
- **Phase 1 (Foundation):** QGA protocol types, socket client with mock tests, VM config generator, VM manager
- **Phase 2 (Intelligence):** LlmBackend trait, Anthropic/OpenAI/Ollama backends, agent tool-use loop
- **Phase 3 (Integration):** Session tracker, Discord bot (poise), host NixOS module, nixosTest
- **Post-MVP:** Natural language config gen, /download command, bridge + veth networking, rate limiting
- 22 commits, 20 tests, 6128 lines across 37 files
- Merged to master, pushed to github.com/jhhuh/ephemeral-nixos-on-discord

### Known gaps for real deployment
- No actual end-to-end test with a running Discord bot + real VM
- LLM config generation untested with real API (needs LLM_API_KEY)
- nixosTest requires Linux host with KVM
- `nix build` of a microvm flake not tested (needs microvm.nix eval)
- No systemd service unit for the bot itself
- No TLS/auth on nix-serve (currently localhost-only, which is fine for SLIRP but not bridge)
- No persistent storage for session state across bot restarts
