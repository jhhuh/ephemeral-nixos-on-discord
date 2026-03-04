# Dev Log

## 2026-03-04: Project initialized
- Scaffolded flake.nix (crane + microvm.nix), Cargo.toml, directory structure
- Design doc approved: QEMU VMs via microvm.nix, QGA control, poise Discord bot

## 2026-03-04: Natural language NixOS config generation
- Created `src/llm/config_gen.rs`: `generate_nixos_config()` sends description to LLM with a system prompt that constrains output to a NixOS module expression. Strips accidental markdown fencing from response.
- Wired into `/create` command: if user provides a description, it generates config via LLM, validates syntax with `nix-instantiate --parse` (via `spawn_blocking` since `validate_nix_syntax` is synchronous), and passes to `VmManager::create`. Falls back to base config on LLM error or syntax failure.
- Updated slash command description param from "reserved for future use" to actionable hint.

## 2026-03-04: Comprehensive NixOS infrastructure integration test
- Expanded `tests/nixos-test.nix` from a minimal nix-serve smoke test to a full host infrastructure test.
- Now validates: nix-serve cache, sandbox-runner user, state directory, KVM device, bridge interface, nftables rules, dnsmasq, and cache-info content.
- Imports `nix/networking/bridge.nix` alongside `nix/host.nix` so bridge/nftables/dnsmasq are exercised.
- Full microVM boot inside the test was deferred (nested virt complexity); test focuses on host-side infrastructure instead.
