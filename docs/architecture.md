# Architecture

## Overview

```
Discord Server
  Thread #1 (VM-abc)    Thread #2 (VM-def)
       |                      |
       v                      v
+--------------------------------------------------+
|           Rust Discord Bot (poise)               |
|                                                  |
|  +-----------+  +----------+  +---------------+  |
|  | LLM       |  | VM       |  | Session       |  |
|  | Gateway   |  | Manager  |  | Tracker       |  |
|  | (pluggable|  | (microvm)|  | (lifecycle)   |  |
|  +-----------+  +----------+  +---------------+  |
|       |              |              |             |
|       |    +---------+--------+     |             |
|       |    | QGA Client       |     |             |
|       |    | (virtio-serial)  |     |             |
|       |    +------------------+     |             |
+-------+----------+-----------+------+-------------+
        |          |           |
        v          v           v
+--------------------------------------------------+
|              Host NixOS System                   |
|  +--------+  +--------+  +--------+             |
|  | VM-abc |  | VM-def |  | VM-ghi |  (QEMU)     |
|  +--------+  +--------+  +--------+             |
|                                                  |
|  nix-serve (binary cache for VMs)               |
+--------------------------------------------------+
```

## Components

### QGA Client (`src/qga/`)

Communicates with VMs via the QEMU Guest Agent protocol — a line-delimited JSON protocol over a Unix domain socket (virtio-serial).

**Operations:** `exec` (run shell commands), `read_file`, `write_file`.

No SSH needed. The QGA socket is a host-side Unix socket created by QEMU, so there's no network path from guest to host.

### VM Manager (`src/vm/`)

**Config generation:** Each VM gets a dynamically generated Nix flake that imports `microvm.nixosModules.microvm` and the base VM template (`nix/base-vm.nix`). User-provided NixOS config is merged in.

**Lifecycle:** `nix build` produces a QEMU runner script → execute it → QEMU starts → QGA socket appears → connect client → ready.

### LLM Agent (`src/llm/`)

A tool-use agent loop:

1. Receive user message from Discord thread
2. Send to LLM with tool definitions (exec, read_file, write_file, nixos_rebuild)
3. LLM returns tool calls → execute via QGA → return results
4. Repeat until LLM produces a text response
5. Post response to Discord

**Pluggable backends:** `LlmBackend` trait with Anthropic, OpenAI, and Ollama implementations.

### Session Tracker (`src/session/`)

Maps Discord thread IDs to `(Agent, QgaClient, VmId)` tuples. Tracks last activity per session for idle timeout (30 min default). Includes per-user rate limiting.

### Discord Bot (`src/bot/`)

Built with [poise](https://docs.rs/poise/) (on top of serenity). Slash commands for sandbox lifecycle, message handler that routes thread messages to the LLM agent.

## Security Model

| Threat | Mitigation |
|--------|------------|
| VM → Host network | SLIRP: host unreachable. Bridge/veth: nftables block host IPs |
| VM escape | QEMU hardware virtualization boundary |
| /nix/store write | Mounted read-only via virtiofs |
| Resource abuse | Cgroup limits on QEMU processes |
| QGA abuse | Host-side client only; never accepts commands from guest |
| Discord abuse | Per-user rate limiting (2 VMs max, 30s cooldown) |
| LLM prompt injection | Tools scoped to ephemeral VM only |
