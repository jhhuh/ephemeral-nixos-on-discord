# Ephemeral NixOS Sandbox Discord Bot — Design Document

**Date**: 2026-03-04
**Status**: Approved

## Overview

A Discord bot that launches ephemeral NixOS virtual machines (sandboxes) and provides conversational LLM-agent access to them. Users interact with their sandbox through a dedicated Discord thread. Sandboxes are isolated QEMU VMs managed by microvm.nix.

## Requirements

- **Sandbox tech**: QEMU VMs via microvm.nix (strongest isolation)
- **Language**: Rust (Discord bot + orchestration)
- **LLM**: Pluggable backend (Anthropic, OpenAI, Ollama/local)
- **Config input**: Natural language (LLM generates NixOS config) + user-provided .nix override
- **Networking**: Default SLIRP user-mode NAT; bridge and veth+nftables as options. Host unreachable from VM.
- **Nix store**: VM runs own nix-daemon; host serves as binary cache (nix-serve/HTTP)
- **Agent<->VM**: QEMU Guest Agent (QGA) over virtio-serial
- **Lifecycle**: Auto-timeout on idle + explicit destroy command
- **Scale**: 5-10 concurrent VMs per host
- **File download**: Discord command to retrieve files (lower priority)

## Architecture

```
Discord Server
  Thread #1 (VM-abc)  Thread #2 (VM-def)  Thread #3 (VM-ghi)
       |                    |                    |
       v                    v                    v
+-------------------------------------------------------------+
|              Rust Discord Bot (poise)                        |
|                                                              |
|  +-------------+  +--------------+  +--------------------+   |
|  | LLM Gateway |  | VM Manager   |  | Session Tracker    |   |
|  | (pluggable) |  | (microvm.nix)|  | (lifecycle/timeout)|   |
|  +------+------+  +------+-------+  +--------+-----------+   |
|         |                |                    |               |
|         |    +-----------+-----------+        |               |
|         |    | QGA Client (per VM)   |        |               |
|         |    | - exec commands       |        |               |
|         |    | - file read/write     |        |               |
|         |    +-----------------------+        |               |
+---------+------------+-------------------+----+---------------+
          |            |                   |
          v            v                   v
+-------------------------------------------------------------+
|                    Host NixOS System                         |
|  +----------+  +----------+  +----------+                    |
|  | VM-abc   |  | VM-def   |  | VM-ghi   |                   |
|  | (QEMU)   |  | (QEMU)   |  | (QEMU)   |                   |
|  | QGA      |  | QGA      |  | QGA      |  virtio-serial     |
|  | SLIRP    |  | SLIRP    |  | bridge   |  (per VM)          |
|  +----------+  +----------+  +----------+                    |
|                                                              |
|  nix-daemon (host) -- VMs use host as binary cache           |
|  /nix/store (shared read-only via virtiofs)                  |
+--------------------------------------------------------------+
```

### Components

1. **Rust Discord Bot** (poise/serenity): Handles slash commands and thread message routing.
2. **LLM Gateway**: Trait-based pluggable LLM backend. Drives a tool-use agent loop.
3. **VM Manager**: Generates microvm.nix flakes, builds VMs, launches/stops QEMU, manages QGA connections.
4. **Session Tracker**: Maps Discord threads to VMs, handles idle timeouts and cleanup.

## VM Management (microvm.nix)

### Base VM Template

Every sandbox starts from a base NixOS configuration:

```nix
{
  microvm = {
    hypervisor = "qemu";

    # Default: SLIRP user-mode networking
    interfaces = [{
      type = "user";
      id = "vm-usernet";
      mac = "<generated>";
    }];

    # Share host /nix/store read-only for cache
    shares = [{
      proto = "virtiofs";
      source = "/nix/store";
      mountPoint = "/nix/store";
      readOnly = true;
    }];

    # Virtio-serial for QGA communication
    qemu.extraArgs = [
      "-chardev" "socket,path=/run/microvm/<vm-id>/qga.sock,server=on,wait=off,id=qga0"
      "-device" "virtio-serial"
      "-device" "virtserialport,chardev=qga0,name=org.qemu.guest_agent.0"
    ];
  };

  services.qemuGuest.enable = true;

  nix.settings = {
    substituters = [ "http://<host-ip>:<port>" "https://cache.nixos.org" ];
  };

  # User-requested packages/services merged here
}
```

### VM Lifecycle

1. User requests sandbox -> bot generates flake (base template + user config)
2. `nix build .#nixosConfigurations.<vm-id>.config.microvm.runner.qemu` -> run script
3. Bot executes runner, QEMU starts
4. QGA socket appears at `/run/microvm/<vm-id>/qga.sock`
5. Bot connects QGA client -> VM ready
6. On timeout/destroy: QEMU killed, temp directory cleaned

### Networking Modes

| Mode | Config | Host Setup | Host Isolation |
|------|--------|------------|----------------|
| SLIRP (default) | `type = "user"` | None | Built-in (host unreachable) |
| Bridge | `type = "bridge"; bridge = "br-sandbox"` | Host bridge + nftables | nftables rules block host IPs |
| veth + nftables | Custom via `qemu.extraArgs` | veth pairs + nftables | Full nftables control |

## LLM Agent Layer

### Pluggable Backend

```rust
trait LlmBackend {
    async fn chat(&self, messages: Vec<Message>, tools: Vec<Tool>) -> Result<Response>;
}
```

Implementations: `AnthropicBackend`, `OpenAiBackend`, `OllamaBackend`.

### Tools

| Tool | Description | QGA Implementation |
|------|-------------|-------------------|
| `exec` | Run shell command in VM | `guest-exec` + `guest-exec-status` |
| `read_file` | Read file from VM | `guest-file-open` + `guest-file-read` + `guest-file-close` |
| `write_file` | Write file to VM | `guest-file-open` + `guest-file-write` + `guest-file-close` |
| `nixos_rebuild` | Apply NixOS config change | `guest-exec` running `nixos-rebuild switch` |
| `generate_config` | NixOS module from natural language | LLM generates .nix, merged into VM config |

### Agent Loop

```
User message -> Append to conversation history
  -> Send to LLM with tool definitions
  -> LLM returns tool calls
  -> Execute via QGA
  -> Return results to LLM
  -> LLM formulates response
  -> Post to Discord thread
```

### Config Generation

**Natural language**: LLM generates a NixOS module from the user's description, merged with base template.

**File override**: User attaches .nix file. Bot validates syntax (`nix-instantiate --parse`), imports into VM flake.

## Discord Integration

### Commands

| Command | Description |
|---------|-------------|
| `/sandbox create [description]` | Create sandbox. Opens thread. LLM interprets description. |
| `/sandbox create` + attachment | Create from attached .nix config |
| `/sandbox destroy` | Destroy sandbox in current thread |
| `/sandbox status` | Show VM status (uptime, resources, networking) |
| `/sandbox download <path>` | Download file from sandbox (low priority) |
| (message in thread) | Forwarded to LLM agent |

### Thread Model

- Each sandbox gets a dedicated Discord thread: `sandbox-<short-id>`
- All thread messages route to the LLM agent for that VM
- Thread archive/delete triggers VM cleanup

## Security

### Threat Model

Untrusted users run arbitrary code inside VMs. VMs must not access host filesystem (beyond read-only /nix/store), reach host network, escape VM boundary, or DoS host resources.

### Mitigations

| Threat | Mitigation |
|--------|------------|
| VM->Host network | SLIRP: host unreachable by default. Bridge/veth: nftables block host IPs |
| VM escape | QEMU hardware isolation. QEMU runs as unprivileged user. |
| /nix/store write | Mounted read-only via virtiofs. VM uses overlay for writes. |
| Resource abuse | QEMU cgroup limits: CPU quota, memory cap, disk I/O. systemd unit overrides. |
| QGA abuse | Host-side client only issues commands, never accepts from guest. |
| Nix-daemon abuse | VM uses host as HTTP binary cache only (nix-serve). No daemon socket sharing. |
| Discord abuse | Rate limiting per user. Max 1-2 concurrent VMs per user. Creation cooldown. |
| LLM prompt injection | Tools scoped to VM only. Damage contained to ephemeral VM. |

### QEMU Hardening

- `-sandbox on` (seccomp)
- KVM enabled for performance, no device passthrough by default
- QEMU process runs as dedicated unprivileged user

### Network Isolation (Bridge/veth modes)

```nix
networking.nftables.rules = ''
  table inet sandbox-isolation {
    chain forward {
      type filter hook forward priority 0;
      iifname "br-sandbox" oifname "wan" accept
      iifname "br-sandbox" ip daddr <host-ips> drop
      ct state established,related accept
    }
  }
'';
```

## Error Handling

- **VM build failure**: Post build error to Discord thread, LLM suggests fixes
- **VM startup failure**: Retry once, then report with QEMU stderr
- **QGA connection timeout**: Wait up to 60s for VM boot, then report
- **QGA command timeout**: 120s default, configurable. Streamed output for long commands.
- **LLM API failure**: Exponential backoff (3 attempts), then report to user
- **Resource exhaustion**: Refuse new VM creation at capacity, report to user

## Testing

- **Unit tests**: LLM tool parsing, config generation, QGA protocol, session state machine
- **Integration tests**: Full flow with test VM
- **NixOS VM tests**: `nixosTest` for host+guest integration in CI

## Project Structure

```
ephemeral-nixos-on-discord/
├── flake.nix                    # Project flake (Rust build + NixOS modules)
├── flake.lock
├── Procfile                     # overmind: bot process
├── CLAUDE.md
├── Cargo.toml
├── src/
│   ├── main.rs                  # Entry point, Discord bot setup
│   ├── bot/
│   │   ├── mod.rs
│   │   ├── commands.rs          # Slash commands
│   │   └── handler.rs           # Thread message -> LLM routing
│   ├── llm/
│   │   ├── mod.rs
│   │   ├── traits.rs            # LlmBackend trait
│   │   ├── anthropic.rs
│   │   ├── openai.rs
│   │   ├── tools.rs             # Tool definitions
│   │   └── agent.rs             # Agent loop
│   ├── vm/
│   │   ├── mod.rs
│   │   ├── manager.rs           # VM lifecycle
│   │   ├── config.rs            # NixOS config generation
│   │   └── qga.rs               # QGA client
│   └── session/
│       ├── mod.rs
│       └── tracker.rs           # Thread<->VM mapping, timeouts
├── nix/
│   ├── base-vm.nix              # Base VM template
│   ├── networking/
│   │   ├── slirp.nix
│   │   ├── bridge.nix
│   │   └── veth.nix
│   └── host.nix                 # Host NixOS module
├── tests/
│   ├── integration/
│   └── nixos-test.nix
├── docs/
│   └── plans/
└── artifacts/
    ├── skills/
    └── devlog.md
```
