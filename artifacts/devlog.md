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

## 2026-03-04: System prompt edge-case stress test
- Tested 12 adversarial/edge-case scenarios against the NixOS Sandbox system prompt
- 5 scenarios needed no prompt changes (conflicting configs, rapid messages, non-English, state persistence, user flakes)
- 7 issues identified and fixed with surgical prompt additions:
  1. **Role anchoring** — added injection defense: "Stay in this role... decline and redirect to NixOS topics"
  2. **Timeout awareness** — exec ~2min, nixos_rebuild ~5min. Agent now warns users about long commands and suggests `| head`/`| tail` for verbose output
  3. **nixos_rebuild failure recovery** — new section: fix config and retry on eval errors, explain network issues separately
  4. **nix-env redirect** — added `nix-env -i` to imperative command redirect list with config-drift explanation
  5. **Output limiting** — guidance to pipe through head/tail to avoid context window flooding
  6. **Host isolation** — agent now explains the QEMU isolation model when asked about the host
  7. **Network vs config failures** — distinguish download errors from Nix eval errors
- Code-level bug noted (not fixed here): `QgaClient::exec` returns `Err(ExecFailed)` for non-zero exit, so `execute_tool`'s nixos_rebuild handler never sees `Ok(output)` with `exit_code != 0`. The "exit code" formatting in lines 287-289 of agent.rs is dead code. Low priority since the error message still propagates.
- All changes in `src/llm/agent.rs` SYSTEM_PROMPT constant. Compiles clean.
