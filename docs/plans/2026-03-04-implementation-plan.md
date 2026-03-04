# Ephemeral NixOS Discord Bot — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a Rust Discord bot that spawns ephemeral NixOS QEMU VMs (via microvm.nix), controlled by a pluggable LLM agent through QEMU Guest Agent, with per-sandbox Discord threads.

**Architecture:** Poise Discord bot → LLM agent loop (pluggable backend) → QGA client → microvm.nix QEMU VMs. Each sandbox gets a Discord thread. VMs use SLIRP networking (default), host /nix/store shared read-only, host as binary cache.

**Tech Stack:** Rust (poise, tokio, serde, reqwest), Nix (microvm.nix, crane), QEMU Guest Agent protocol (JSON over Unix socket)

**References:**
- Design doc: `docs/plans/2026-03-04-ephemeral-nixos-discord-bot-design.md`
- QGA protocol: https://www.qemu.org/docs/master/interop/qemu-ga-ref.html
- microvm.nix options: https://microvm-nix.github.io/microvm.nix/microvm-options.html
- poise docs: https://docs.rs/poise/latest/poise/

---

## Phase 1: Foundation

### Task 1: Project Scaffolding

**Files:**
- Create: `flake.nix`
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/lib.rs`
- Create: `CLAUDE.md`
- Create: `Procfile`
- Create: `.envrc`
- Create: `.gitignore`
- Create: `artifacts/devlog.md`

**Step 1: Create .gitignore**

```gitignore
/target
/result
.direnv
.env
```

**Step 2: Create flake.nix**

```nix
{
  description = "Ephemeral NixOS sandbox Discord bot";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    microvm.url = "github:astro/microvm.nix";
    microvm.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, crane, flake-utils, microvm, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        craneLib = crane.mkLib pkgs;

        src = craneLib.cleanCargoSource ./.;

        commonArgs = {
          inherit src;
          strictDeps = true;
          buildInputs = [ pkgs.openssl ];
          nativeBuildInputs = [ pkgs.pkg-config ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        bot = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
        });
      in
      {
        packages.default = bot;

        devShells.default = craneLib.devShell {
          packages = with pkgs; [
            overmind
            tmux
            rust-analyzer
            cargo-watch
          ];

          RUST_LOG = "debug";
        };
      }
    );
}
```

**Step 3: Create Cargo.toml**

```toml
[package]
name = "ephemeral-nixos-bot"
version = "0.1.0"
edition = "2021"

[dependencies]
poise = "0.6"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", features = ["json"] }
uuid = { version = "1", features = ["v4"] }
base64 = "0.22"
tempfile = "3"
tracing = "0.1"
tracing-subscriber = "0.3"
thiserror = "2"

[dev-dependencies]
tokio-test = "0.4"
```

**Step 4: Create src/lib.rs and src/main.rs stubs**

`src/lib.rs`:
```rust
pub mod qga;
```

`src/main.rs`:
```rust
fn main() {
    println!("ephemeral-nixos-bot");
}
```

**Step 5: Create supporting files**

`CLAUDE.md`:
```markdown
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
```

`.envrc`:
```
use flake
```

`Procfile`:
```
bot: cargo run
```

`artifacts/devlog.md`:
```markdown
# Dev Log

## 2026-03-04: Project initialized
- Scaffolded flake.nix (crane + microvm.nix), Cargo.toml, directory structure
- Design doc approved: QEMU VMs via microvm.nix, QGA control, poise Discord bot
```

**Step 6: Verify build**

Run: `nix develop -c cargo check`
Expected: Compiles with no errors (dependencies download, type-checks pass)

**Step 7: Commit**

```bash
git add -A
git commit -m "scaffold: flake.nix, Cargo.toml, and project structure"
```

---

### Task 2: QGA Client — Protocol Types

**Files:**
- Create: `src/qga/mod.rs`
- Create: `src/qga/protocol.rs`
- Create: `src/qga/client.rs`
- Modify: `src/lib.rs`

**Step 1: Write QGA protocol types with serde**

`src/qga/protocol.rs`:
```rust
use serde::{Deserialize, Serialize};

/// QGA request envelope
#[derive(Debug, Serialize)]
pub struct QgaRequest<T: Serialize> {
    pub execute: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<T>,
}

/// QGA success response
#[derive(Debug, Deserialize)]
pub struct QgaResponse<T> {
    #[serde(rename = "return")]
    pub result: T,
}

/// QGA error response
#[derive(Debug, Deserialize)]
pub struct QgaError {
    pub error: QgaErrorDetail,
}

#[derive(Debug, Deserialize)]
pub struct QgaErrorDetail {
    pub class: String,
    pub desc: String,
}

// -- guest-sync --

#[derive(Debug, Serialize)]
pub struct GuestSyncArgs {
    pub id: u64,
}

// -- guest-exec --

#[derive(Debug, Serialize)]
pub struct GuestExecArgs {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<String>>,
    #[serde(rename = "input-data", skip_serializing_if = "Option::is_none")]
    pub input_data: Option<String>,
    #[serde(rename = "capture-output")]
    pub capture_output: bool,
}

#[derive(Debug, Deserialize)]
pub struct GuestExecResult {
    pub pid: u64,
}

// -- guest-exec-status --

#[derive(Debug, Serialize)]
pub struct GuestExecStatusArgs {
    pub pid: u64,
}

#[derive(Debug, Deserialize)]
pub struct GuestExecStatusResult {
    pub exited: bool,
    #[serde(default)]
    pub exitcode: Option<i32>,
    #[serde(default)]
    pub signal: Option<i32>,
    #[serde(rename = "out-data", default)]
    pub out_data: Option<String>,
    #[serde(rename = "err-data", default)]
    pub err_data: Option<String>,
    #[serde(rename = "out-truncated", default)]
    pub out_truncated: Option<bool>,
    #[serde(rename = "err-truncated", default)]
    pub err_truncated: Option<bool>,
}

// -- guest-file-open --

#[derive(Debug, Serialize)]
pub struct GuestFileOpenArgs {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

// -- guest-file-read --

#[derive(Debug, Serialize)]
pub struct GuestFileReadArgs {
    pub handle: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct GuestFileReadResult {
    pub count: u64,
    #[serde(rename = "buf-b64")]
    pub buf_b64: String,
    pub eof: bool,
}

// -- guest-file-write --

#[derive(Debug, Serialize)]
pub struct GuestFileWriteArgs {
    pub handle: u64,
    #[serde(rename = "buf-b64")]
    pub buf_b64: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct GuestFileWriteResult {
    pub count: u64,
    pub eof: bool,
}

// -- guest-file-close --

#[derive(Debug, Serialize)]
pub struct GuestFileCloseArgs {
    pub handle: u64,
}
```

**Step 2: Write tests for protocol serialization**

Add to bottom of `src/qga/protocol.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_guest_exec() {
        let req = QgaRequest {
            execute: "guest-exec",
            arguments: Some(GuestExecArgs {
                path: "/bin/sh".into(),
                arg: Some(vec!["-c".into(), "echo hello".into()]),
                env: None,
                input_data: None,
                capture_output: true,
            }),
        };
        let json = serde_json::to_string(&req).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["execute"], "guest-exec");
        assert_eq!(v["arguments"]["path"], "/bin/sh");
        assert_eq!(v["arguments"]["capture-output"], true);
        assert!(v["arguments"].get("env").is_none());
    }

    #[test]
    fn deserialize_guest_exec_result() {
        let json = r#"{"return": {"pid": 42}}"#;
        let resp: QgaResponse<GuestExecResult> = serde_json::from_str(json).unwrap();
        assert_eq!(resp.result.pid, 42);
    }

    #[test]
    fn deserialize_exec_status_with_output() {
        let json = r#"{"return": {"exited": true, "exitcode": 0, "out-data": "aGVsbG8K", "err-data": ""}}"#;
        let resp: QgaResponse<GuestExecStatusResult> = serde_json::from_str(json).unwrap();
        assert!(resp.result.exited);
        assert_eq!(resp.result.exitcode, Some(0));
        assert_eq!(resp.result.out_data.as_deref(), Some("aGVsbG8K"));
    }

    #[test]
    fn deserialize_error_response() {
        let json = r#"{"error": {"class": "GenericError", "desc": "command not found"}}"#;
        let err: QgaError = serde_json::from_str(json).unwrap();
        assert_eq!(err.error.class, "GenericError");
    }

    #[test]
    fn serialize_file_open() {
        let req = QgaRequest {
            execute: "guest-file-open",
            arguments: Some(GuestFileOpenArgs {
                path: "/etc/hostname".into(),
                mode: Some("r".into()),
            }),
        };
        let json = serde_json::to_string(&req).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["arguments"]["path"], "/etc/hostname");
        assert_eq!(v["arguments"]["mode"], "r");
    }

    #[test]
    fn deserialize_file_read() {
        let json = r#"{"return": {"count": 5, "buf-b64": "aGVsbG8=", "eof": true}}"#;
        let resp: QgaResponse<GuestFileReadResult> = serde_json::from_str(json).unwrap();
        assert_eq!(resp.result.count, 5);
        assert_eq!(resp.result.buf_b64, "aGVsbG8=");
        assert!(resp.result.eof);
    }
}
```

**Step 3: Run tests to verify protocol types**

Run: `cargo test qga::protocol`
Expected: All 6 tests pass

**Step 4: Commit**

```bash
git add src/qga/protocol.rs
git commit -m "feat: QGA protocol types with serde serialization"
```

---

### Task 3: QGA Client — Socket Communication

**Files:**
- Create: `src/qga/client.rs`
- Create: `src/qga/mod.rs`
- Modify: `src/lib.rs`

**Step 1: Write the QGA client**

`src/qga/mod.rs`:
```rust
pub mod client;
pub mod protocol;

pub use client::QgaClient;
```

`src/qga/client.rs`:
```rust
use std::path::Path;
use base64::prelude::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::qga::protocol::*;

#[derive(Debug, thiserror::Error)]
pub enum QgaError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("QGA error: {class}: {desc}")]
    Qga { class: String, desc: String },
    #[error("Command timed out")]
    Timeout,
    #[error("Command failed with exit code {code}: {stderr}")]
    ExecFailed { code: i32, stderr: String },
}

pub struct QgaClient {
    reader: BufReader<tokio::io::ReadHalf<UnixStream>>,
    writer: tokio::io::WriteHalf<UnixStream>,
}

impl QgaClient {
    pub async fn connect(socket_path: &Path) -> Result<Self, QgaError> {
        let stream = UnixStream::connect(socket_path).await?;
        let (read, write) = tokio::io::split(stream);
        let mut client = Self {
            reader: BufReader::new(read),
            writer: write,
        };
        // Synchronize protocol
        client.sync().await?;
        Ok(client)
    }

    /// For testing: wrap an existing UnixStream
    #[cfg(test)]
    pub fn from_stream(stream: UnixStream) -> Self {
        let (read, write) = tokio::io::split(stream);
        Self {
            reader: BufReader::new(read),
            writer: write,
        }
    }

    async fn send_raw(&mut self, json: &str) -> Result<String, QgaError> {
        self.writer.write_all(json.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;

        let mut line = String::new();
        self.reader.read_line(&mut line).await?;
        Ok(line)
    }

    fn parse_response<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T, QgaError> {
        // Try success first
        if let Ok(resp) = serde_json::from_str::<QgaResponse<T>>(raw) {
            return Ok(resp.result);
        }
        // Try error
        if let Ok(err) = serde_json::from_str::<crate::qga::protocol::QgaError>(raw) {
            return Err(QgaError::Qga {
                class: err.error.class,
                desc: err.error.desc,
            });
        }
        // Fall back to JSON parse error
        let result: QgaResponse<T> = serde_json::from_str(raw)?;
        Ok(result.result)
    }

    async fn sync(&mut self) -> Result<(), QgaError> {
        let id: u64 = rand_id();
        let req = QgaRequest {
            execute: "guest-sync",
            arguments: Some(GuestSyncArgs { id }),
        };
        let json = serde_json::to_string(&req)?;
        let raw = self.send_raw(&json).await?;
        let result: u64 = Self::parse_response(&raw)?;
        assert_eq!(result, id);
        Ok(())
    }

    /// Execute a command in the guest, wait for completion, return stdout/stderr.
    pub async fn exec(
        &mut self,
        command: &str,
        timeout: std::time::Duration,
    ) -> Result<ExecOutput, QgaError> {
        let req = QgaRequest {
            execute: "guest-exec",
            arguments: Some(GuestExecArgs {
                path: "/bin/sh".into(),
                arg: Some(vec!["-c".into(), command.into()]),
                env: None,
                input_data: None,
                capture_output: true,
            }),
        };
        let json = serde_json::to_string(&req)?;
        let raw = self.send_raw(&json).await?;
        let result: GuestExecResult = Self::parse_response(&raw)?;

        // Poll for completion
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if tokio::time::Instant::now() > deadline {
                return Err(QgaError::Timeout);
            }

            let status_req = QgaRequest {
                execute: "guest-exec-status",
                arguments: Some(GuestExecStatusArgs { pid: result.pid }),
            };
            let json = serde_json::to_string(&status_req)?;
            let raw = self.send_raw(&json).await?;
            let status: GuestExecStatusResult = Self::parse_response(&raw)?;

            if status.exited {
                let stdout = status
                    .out_data
                    .map(|d| decode_b64(&d))
                    .transpose()?
                    .unwrap_or_default();
                let stderr = status
                    .err_data
                    .map(|d| decode_b64(&d))
                    .transpose()?
                    .unwrap_or_default();
                let exit_code = status.exitcode.unwrap_or(-1);

                return Ok(ExecOutput {
                    exit_code,
                    stdout,
                    stderr,
                });
            }

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    /// Read an entire file from the guest.
    pub async fn read_file(&mut self, path: &str) -> Result<Vec<u8>, QgaError> {
        // Open
        let req = QgaRequest {
            execute: "guest-file-open",
            arguments: Some(GuestFileOpenArgs {
                path: path.into(),
                mode: Some("r".into()),
            }),
        };
        let json = serde_json::to_string(&req)?;
        let raw = self.send_raw(&json).await?;
        let handle: u64 = Self::parse_response(&raw)?;

        // Read chunks
        let mut data = Vec::new();
        loop {
            let req = QgaRequest {
                execute: "guest-file-read",
                arguments: Some(GuestFileReadArgs {
                    handle,
                    count: Some(65536),
                }),
            };
            let json = serde_json::to_string(&req)?;
            let raw = self.send_raw(&json).await?;
            let chunk: GuestFileReadResult = Self::parse_response(&raw)?;

            if chunk.count > 0 {
                let decoded = BASE64_STANDARD.decode(&chunk.buf_b64)
                    .map_err(|e| QgaError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
                data.extend_from_slice(&decoded);
            }
            if chunk.eof {
                break;
            }
        }

        // Close
        let req = QgaRequest {
            execute: "guest-file-close",
            arguments: Some(GuestFileCloseArgs { handle }),
        };
        let json = serde_json::to_string(&req)?;
        self.send_raw(&json).await?;

        Ok(data)
    }

    /// Write data to a file in the guest.
    pub async fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), QgaError> {
        // Open for writing
        let req = QgaRequest {
            execute: "guest-file-open",
            arguments: Some(GuestFileOpenArgs {
                path: path.into(),
                mode: Some("w".into()),
            }),
        };
        let json = serde_json::to_string(&req)?;
        let raw = self.send_raw(&json).await?;
        let handle: u64 = Self::parse_response(&raw)?;

        // Write in chunks
        for chunk in data.chunks(65536) {
            let encoded = BASE64_STANDARD.encode(chunk);
            let req = QgaRequest {
                execute: "guest-file-write",
                arguments: Some(GuestFileWriteArgs {
                    handle,
                    buf_b64: encoded,
                    count: Some(chunk.len() as u64),
                }),
            };
            let json = serde_json::to_string(&req)?;
            self.send_raw(&json).await?;
        }

        // Close
        let req = QgaRequest {
            execute: "guest-file-close",
            arguments: Some(GuestFileCloseArgs { handle }),
        };
        let json = serde_json::to_string(&req)?;
        self.send_raw(&json).await?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct ExecOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

fn decode_b64(s: &str) -> Result<String, QgaError> {
    let bytes = BASE64_STANDARD
        .decode(s)
        .map_err(|e| QgaError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn rand_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixListener;

    /// Helper: create a mock QGA server that responds to commands
    async fn mock_qga_server(listener: UnixListener, responses: Vec<String>) {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut write) = tokio::io::split(stream);
        let mut reader = BufReader::new(read);

        for response in responses {
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            // Send response
            write.write_all(response.as_bytes()).await.unwrap();
            write.write_all(b"\n").await.unwrap();
            write.flush().await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_exec_command() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("qga.sock");

        let listener = UnixListener::bind(&sock_path).unwrap();

        // Base64 of "hello\n" = "aGVsbG8K"
        let sync_id_holder = std::sync::Arc::new(std::sync::Mutex::new(0u64));
        let holder = sync_id_holder.clone();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = tokio::io::split(stream);
            let mut reader = BufReader::new(read);

            // 1. guest-sync
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let req: serde_json::Value = serde_json::from_str(&line).unwrap();
            let sync_id = req["arguments"]["id"].as_u64().unwrap();
            *holder.lock().unwrap() = sync_id;
            let resp = format!("{{\"return\": {}}}\n", sync_id);
            write.write_all(resp.as_bytes()).await.unwrap();
            write.flush().await.unwrap();

            // 2. guest-exec
            line.clear();
            reader.read_line(&mut line).await.unwrap();
            write.write_all(b"{\"return\": {\"pid\": 99}}\n").await.unwrap();
            write.flush().await.unwrap();

            // 3. guest-exec-status
            line.clear();
            reader.read_line(&mut line).await.unwrap();
            write.write_all(b"{\"return\": {\"exited\": true, \"exitcode\": 0, \"out-data\": \"aGVsbG8K\", \"err-data\": \"\"}}\n").await.unwrap();
            write.flush().await.unwrap();
        });

        let mut client = QgaClient::connect(&sock_path).await.unwrap();
        let output = client
            .exec("echo hello", std::time::Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(output.exit_code, 0);
        assert_eq!(output.stdout, "hello\n");
        assert_eq!(output.stderr, "");

        server.await.unwrap();
    }
}
```

Update `src/lib.rs`:
```rust
pub mod qga;
```

**Step 2: Run tests**

Run: `cargo test qga`
Expected: All tests pass (protocol serialization tests + exec mock test)

**Step 3: Commit**

```bash
git add src/qga/ src/lib.rs
git commit -m "feat: QGA client with exec, read_file, write_file over Unix socket"
```

---

### Task 4: VM Config Generation

**Files:**
- Create: `nix/base-vm.nix`
- Create: `nix/vm-template-flake.nix`
- Create: `src/vm/mod.rs`
- Create: `src/vm/config.rs`
- Modify: `src/lib.rs`

**Step 1: Create the base VM NixOS module**

`nix/base-vm.nix` — the NixOS module every sandbox VM imports:
```nix
# Base NixOS configuration for sandbox VMs.
# Receives vm-specific settings via `_module.args`.
{ config, lib, pkgs, vmId, hostCacheUrl, ... }:

{
  microvm = {
    hypervisor = "qemu";

    interfaces = [{
      type = "user";
      id = "vm-net0";
      mac = "02:00:00:00:00:01";
    }];

    shares = [{
      proto = "virtiofs";
      source = "/nix/store";
      mountPoint = "/nix/store";
      readOnly = true;
      tag = "nix-store";
    }];

    qemu.extraArgs = [
      "-chardev" "socket,path=/tmp/microvm/${vmId}/qga.sock,server=on,wait=off,id=qga0"
      "-device" "virtio-serial"
      "-device" "virtserialport,chardev=qga0,name=org.qemu.guest_agent.0"
    ];

    mem = 1024;  # MB
    vcpu = 2;
  };

  services.qemuGuest.enable = true;

  nix.settings = {
    substituters = lib.mkForce [
      hostCacheUrl
      "https://cache.nixos.org"
    ];
    trusted-public-keys = [
      "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
    ];
  };

  # Minimal system
  networking.hostName = vmId;
  users.users.root.initialPassword = "";
  system.stateVersion = "24.11";
}
```

**Step 2: Create the flake template**

`nix/vm-template-flake.nix` — a Nix expression that generates the per-VM flake.nix content as a string. The Rust code writes this to a temp directory.

This is a reference for the Rust config generator. The actual flake is generated as a string by Rust.

**Step 3: Write the Rust config generator**

`src/vm/mod.rs`:
```rust
pub mod config;
```

`src/vm/config.rs`:
```rust
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::fs;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Nix syntax validation failed: {0}")]
    NixSyntax(String),
}

/// Configuration for a sandbox VM
pub struct VmConfig {
    pub vm_id: String,
    pub host_cache_url: String,
    pub user_config_nix: Option<String>,
    pub mem_mb: u32,
    pub vcpu: u32,
}

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            vm_id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            host_cache_url: "https://cache.nixos.org".into(),
            user_config_nix: None,
            mem_mb: 1024,
            vcpu: 2,
        }
    }
}

/// Generate a VM flake directory. Returns the TempDir (must be kept alive).
pub async fn generate_vm_flake(
    config: &VmConfig,
    project_root: &Path,
) -> Result<(TempDir, PathBuf), ConfigError> {
    let dir = TempDir::new()?;
    let flake_dir = dir.path().to_path_buf();

    let user_module = config.user_config_nix.as_deref().unwrap_or("{ }");

    let flake_content = format!(
        r#"{{
  inputs = {{
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    microvm.url = "github:astro/microvm.nix";
    microvm.inputs.nixpkgs.follows = "nixpkgs";
  }};

  outputs = {{ self, nixpkgs, microvm, ... }}:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${{system}};
    in {{
      nixosConfigurations."{vm_id}" = nixpkgs.lib.nixosSystem {{
        inherit system;
        specialArgs = {{
          vmId = "{vm_id}";
          hostCacheUrl = "{host_cache_url}";
        }};
        modules = [
          microvm.nixosModules.microvm
          {base_vm_path}
          ({user_module})
        ];
      }};
    }};
}}"#,
        vm_id = config.vm_id,
        host_cache_url = config.host_cache_url,
        base_vm_path = project_root.join("nix/base-vm.nix").display(),
        user_module = user_module,
    );

    fs::write(flake_dir.join("flake.nix"), &flake_content).await?;

    Ok((dir, flake_dir))
}

/// Validate nix syntax of user-provided config
pub async fn validate_nix_syntax(nix_expr: &str) -> Result<(), ConfigError> {
    let dir = TempDir::new()?;
    let file_path = dir.path().join("check.nix");
    fs::write(&file_path, nix_expr).await?;

    let output = tokio::process::Command::new("nix-instantiate")
        .args(["--parse", file_path.to_str().unwrap()])
        .output()
        .await?;

    if output.status.success() {
        Ok(())
    } else {
        Err(ConfigError::NixSyntax(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generate_flake_writes_file() {
        let config = VmConfig {
            vm_id: "test-vm".into(),
            host_cache_url: "http://localhost:5000".into(),
            user_config_nix: Some("{ pkgs, ... }: { environment.systemPackages = [ pkgs.hello ]; }".into()),
            ..Default::default()
        };

        // Use a fake project root — we just need the path in the string
        let project_root = Path::new("/tmp/fake-project");
        let (dir, flake_dir) = generate_vm_flake(&config, project_root).await.unwrap();

        let content = fs::read_to_string(flake_dir.join("flake.nix")).await.unwrap();
        assert!(content.contains("test-vm"));
        assert!(content.contains("http://localhost:5000"));
        assert!(content.contains("pkgs.hello"));

        // TempDir kept alive by `dir`
        drop(dir);
    }

    #[tokio::test]
    async fn test_validate_nix_syntax_valid() {
        let result = validate_nix_syntax("{ pkgs, ... }: { environment.systemPackages = [ pkgs.hello ]; }").await;
        // This test requires nix-instantiate in PATH; skip if not available
        match result {
            Ok(()) => {}
            Err(ConfigError::Io(_)) => {} // nix-instantiate not available
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    #[tokio::test]
    async fn test_validate_nix_syntax_invalid() {
        let result = validate_nix_syntax("{ pkgs, ... }: {{{").await;
        match result {
            Err(ConfigError::NixSyntax(_)) => {}
            Err(ConfigError::Io(_)) => {} // nix-instantiate not available
            other => panic!("expected NixSyntax error, got: {other:?}"),
        }
    }
}
```

Update `src/lib.rs`:
```rust
pub mod qga;
pub mod vm;
```

**Step 4: Run tests**

Run: `cargo test vm::config`
Expected: All tests pass

**Step 5: Commit**

```bash
git add nix/ src/vm/ src/lib.rs
git commit -m "feat: VM config generation with base NixOS template and flake builder"
```

---

### Task 5: VM Manager — Build, Launch, Destroy

**Files:**
- Create: `src/vm/manager.rs`
- Modify: `src/vm/mod.rs`

**Step 1: Write the VM manager**

`src/vm/manager.rs`:
```rust
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::qga::QgaClient;
use crate::vm::config::{self, VmConfig};

#[derive(Debug, thiserror::Error)]
pub enum VmError {
    #[error("Config error: {0}")]
    Config(#[from] config::ConfigError),
    #[error("Build failed: {0}")]
    BuildFailed(String),
    #[error("Launch failed: {0}")]
    LaunchFailed(String),
    #[error("QGA connection failed: {0}")]
    QgaFailed(#[from] crate::qga::client::QgaError),
    #[error("VM not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

struct VmInstance {
    process: Child,
    qga_socket: PathBuf,
    _flake_dir: TempDir, // kept alive to prevent cleanup
}

pub struct VmManager {
    project_root: PathBuf,
    state_dir: PathBuf,
    host_cache_url: String,
    instances: Arc<Mutex<HashMap<String, VmInstance>>>,
}

impl VmManager {
    pub fn new(project_root: PathBuf, state_dir: PathBuf, host_cache_url: String) -> Self {
        Self {
            project_root,
            state_dir,
            host_cache_url,
            instances: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Build and launch a VM. Returns the vm_id.
    pub async fn create(
        &self,
        user_config: Option<String>,
    ) -> Result<String, VmError> {
        let vm_config = VmConfig {
            host_cache_url: self.host_cache_url.clone(),
            user_config_nix: user_config,
            ..Default::default()
        };
        let vm_id = vm_config.vm_id.clone();

        // Ensure state dir exists
        let vm_state = self.state_dir.join(&vm_id);
        tokio::fs::create_dir_all(&vm_state).await?;

        // Generate flake
        let (flake_dir, flake_path) =
            config::generate_vm_flake(&vm_config, &self.project_root).await?;

        // Build VM
        let build_attr = format!(
            ".#nixosConfigurations.{}.config.microvm.runner.qemu",
            vm_id
        );
        let output = Command::new("nix")
            .args(["build", &build_attr, "--no-link", "--print-out-paths"])
            .current_dir(&flake_path)
            .output()
            .await?;

        if !output.status.success() {
            return Err(VmError::BuildFailed(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
        }

        let runner_path = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Launch VM
        let qga_socket = vm_state.join("qga.sock");
        let child = Command::new(format!("{}/bin/microvm-run", runner_path))
            .env("MICROVM_STATE_DIR", &vm_state)
            .spawn()
            .map_err(|e| VmError::LaunchFailed(e.to_string()))?;

        let instance = VmInstance {
            process: child,
            qga_socket: qga_socket.clone(),
            _flake_dir: flake_dir,
        };

        self.instances.lock().await.insert(vm_id.clone(), instance);

        Ok(vm_id)
    }

    /// Connect QGA client to a running VM. Waits up to `timeout` for the socket.
    pub async fn connect_qga(
        &self,
        vm_id: &str,
        timeout: std::time::Duration,
    ) -> Result<QgaClient, VmError> {
        let instances = self.instances.lock().await;
        let instance = instances
            .get(vm_id)
            .ok_or_else(|| VmError::NotFound(vm_id.into()))?;
        let socket_path = instance.qga_socket.clone();
        drop(instances);

        // Wait for socket to appear
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if socket_path.exists() {
                match QgaClient::connect(&socket_path).await {
                    Ok(client) => return Ok(client),
                    Err(_) if tokio::time::Instant::now() < deadline => {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    }
                    Err(e) => return Err(VmError::QgaFailed(e)),
                }
            }
            if tokio::time::Instant::now() > deadline {
                return Err(VmError::QgaFailed(crate::qga::client::QgaError::Timeout));
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    /// Destroy a VM
    pub async fn destroy(&self, vm_id: &str) -> Result<(), VmError> {
        let mut instances = self.instances.lock().await;
        let mut instance = instances
            .remove(vm_id)
            .ok_or_else(|| VmError::NotFound(vm_id.into()))?;

        // Kill QEMU process
        let _ = instance.process.kill().await;
        let _ = instance.process.wait().await;

        // Clean up state dir
        let vm_state = self.state_dir.join(vm_id);
        let _ = tokio::fs::remove_dir_all(&vm_state).await;

        Ok(())
    }

    /// List running VM IDs
    pub async fn list(&self) -> Vec<String> {
        self.instances.lock().await.keys().cloned().collect()
    }
}
```

Update `src/vm/mod.rs`:
```rust
pub mod config;
pub mod manager;

pub use manager::VmManager;
```

**Step 2: Run check (no integration test yet — needs real Nix + QEMU)**

Run: `cargo check`
Expected: Compiles with no errors

**Step 3: Commit**

```bash
git add src/vm/
git commit -m "feat: VM manager with create, connect_qga, destroy lifecycle"
```

---

## Phase 2: Intelligence

### Task 6: LLM Trait and Tool Definitions

**Files:**
- Create: `src/llm/mod.rs`
- Create: `src/llm/traits.rs`
- Create: `src/llm/tools.rs`
- Modify: `src/lib.rs`

**Step 1: Define the LLM trait and message types**

`src/llm/traits.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug)]
pub enum LlmResponse {
    /// LLM wants to call tools
    ToolCalls(Vec<ToolCall>),
    /// LLM produced a final text response
    Text(String),
}

#[derive(Debug)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Pluggable LLM backend
#[async_trait::async_trait]
pub trait LlmBackend: Send + Sync {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<LlmResponse, Box<dyn std::error::Error + Send + Sync>>;
}
```

Add `async-trait = "0.1"` to `Cargo.toml` `[dependencies]`.

**Step 2: Define tool schemas**

`src/llm/tools.rs`:
```rust
use crate::llm::traits::ToolDef;
use serde_json::json;

pub fn sandbox_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "exec".into(),
            description: "Execute a shell command in the sandbox VM. Returns stdout, stderr, and exit code.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute"
                    }
                },
                "required": ["command"]
            }),
        },
        ToolDef {
            name: "read_file".into(),
            description: "Read the contents of a file in the sandbox VM.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the file"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDef {
            name: "write_file".into(),
            description: "Write contents to a file in the sandbox VM.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the file"
                    },
                    "content": {
                        "type": "string",
                        "description": "File content to write"
                    }
                },
                "required": ["path", "content"]
            }),
        },
        ToolDef {
            name: "nixos_rebuild".into(),
            description: "Apply NixOS configuration changes by running nixos-rebuild switch.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "config_nix": {
                        "type": "string",
                        "description": "NixOS configuration module to apply"
                    }
                },
                "required": ["config_nix"]
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_serialize_to_valid_json() {
        let tools = sandbox_tools();
        assert_eq!(tools.len(), 4);
        for tool in &tools {
            let json = serde_json::to_string(tool).unwrap();
            let _: serde_json::Value = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn exec_tool_has_command_param() {
        let tools = sandbox_tools();
        let exec = tools.iter().find(|t| t.name == "exec").unwrap();
        let required = exec.parameters["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "command"));
    }
}
```

`src/llm/mod.rs`:
```rust
pub mod tools;
pub mod traits;

pub use traits::{LlmBackend, LlmResponse, Message, ToolCall, ToolDef};
```

Update `src/lib.rs`:
```rust
pub mod llm;
pub mod qga;
pub mod vm;
```

**Step 3: Run tests**

Run: `cargo test llm`
Expected: All tests pass

**Step 4: Commit**

```bash
git add src/llm/ src/lib.rs Cargo.toml
git commit -m "feat: LLM trait, message types, and sandbox tool definitions"
```

---

### Task 7: Anthropic LLM Backend

**Files:**
- Create: `src/llm/anthropic.rs`
- Modify: `src/llm/mod.rs`

**Step 1: Implement the Anthropic backend**

`src/llm/anthropic.rs`:
```rust
use crate::llm::traits::*;
use reqwest::Client;
use serde_json::{json, Value};

pub struct AnthropicBackend {
    client: Client,
    api_key: String,
    model: String,
}

impl AnthropicBackend {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model: model.unwrap_or_else(|| "claude-sonnet-4-6".into()),
        }
    }

    fn convert_messages(messages: &[Message]) -> Vec<Value> {
        messages
            .iter()
            .filter(|m| !matches!(m.role, Role::System))
            .map(|m| {
                let role = match m.role {
                    Role::User | Role::Tool => "user",
                    Role::Assistant => "assistant",
                    Role::System => unreachable!(),
                };
                let content = match &m.content {
                    MessageContent::Text(t) => json!(t),
                    MessageContent::Blocks(blocks) => {
                        let converted: Vec<Value> = blocks
                            .iter()
                            .map(|b| match b {
                                ContentBlock::Text { text } => {
                                    json!({"type": "text", "text": text})
                                }
                                ContentBlock::ToolUse { id, name, input } => {
                                    json!({"type": "tool_use", "id": id, "name": name, "input": input})
                                }
                                ContentBlock::ToolResult {
                                    tool_use_id,
                                    content,
                                } => {
                                    json!({"type": "tool_result", "tool_use_id": tool_use_id, "content": content})
                                }
                            })
                            .collect();
                        json!(converted)
                    }
                };
                json!({"role": role, "content": content})
            })
            .collect()
    }

    fn convert_tools(tools: &[ToolDef]) -> Vec<Value> {
        tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                })
            })
            .collect()
    }

    fn extract_system(messages: &[Message]) -> Option<String> {
        messages.iter().find_map(|m| {
            if matches!(m.role, Role::System) {
                match &m.content {
                    MessageContent::Text(t) => Some(t.clone()),
                    _ => None,
                }
            } else {
                None
            }
        })
    }
}

#[async_trait::async_trait]
impl LlmBackend for AnthropicBackend {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<LlmResponse, Box<dyn std::error::Error + Send + Sync>> {
        let mut body = json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": Self::convert_messages(messages),
        });

        if !tools.is_empty() {
            body["tools"] = json!(Self::convert_tools(tools));
        }

        if let Some(system) = Self::extract_system(messages) {
            body["system"] = json!(system);
        }

        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let resp_body: Value = resp.json().await?;

        if !status.is_success() {
            return Err(format!("Anthropic API error {}: {}", status, resp_body).into());
        }

        // Parse response content blocks
        let content = resp_body["content"]
            .as_array()
            .ok_or("missing content array")?;

        let mut tool_calls = Vec::new();
        let mut text_parts = Vec::new();

        for block in content {
            match block["type"].as_str() {
                Some("tool_use") => {
                    tool_calls.push(ToolCall {
                        id: block["id"].as_str().unwrap_or("").into(),
                        name: block["name"].as_str().unwrap_or("").into(),
                        input: block["input"].clone(),
                    });
                }
                Some("text") => {
                    if let Some(t) = block["text"].as_str() {
                        text_parts.push(t.to_string());
                    }
                }
                _ => {}
            }
        }

        if !tool_calls.is_empty() {
            Ok(LlmResponse::ToolCalls(tool_calls))
        } else {
            Ok(LlmResponse::Text(text_parts.join("\n")))
        }
    }
}
```

Update `src/llm/mod.rs`:
```rust
pub mod anthropic;
pub mod tools;
pub mod traits;

pub use traits::{LlmBackend, LlmResponse, Message, ToolCall, ToolDef};
```

**Step 2: Run check (no unit test — requires API key)**

Run: `cargo check`
Expected: Compiles with no errors

**Step 3: Commit**

```bash
git add src/llm/
git commit -m "feat: Anthropic LLM backend implementation"
```

---

### Task 8: Agent Loop

**Files:**
- Create: `src/llm/agent.rs`
- Modify: `src/llm/mod.rs`

**Step 1: Implement the agent loop that executes tools via QGA**

`src/llm/agent.rs`:
```rust
use crate::llm::traits::*;
use crate::llm::tools::sandbox_tools;
use crate::qga::QgaClient;
use std::time::Duration;

const SYSTEM_PROMPT: &str = r#"You are an assistant that helps users work inside a NixOS virtual machine sandbox. You can execute commands, read and write files, and apply NixOS configuration changes.

When the user asks you to do something in the sandbox, use the available tools. Always show command output to the user. If a command fails, explain what went wrong and suggest fixes.

The sandbox is an ephemeral NixOS VM. It will be destroyed when the session ends."#;

const MAX_TOOL_ROUNDS: usize = 10;
const EXEC_TIMEOUT: Duration = Duration::from_secs(120);

pub struct Agent {
    backend: Box<dyn LlmBackend>,
    conversation: Vec<Message>,
}

impl Agent {
    pub fn new(backend: Box<dyn LlmBackend>) -> Self {
        let conversation = vec![Message {
            role: Role::System,
            content: MessageContent::Text(SYSTEM_PROMPT.into()),
        }];
        Self {
            backend,
            conversation,
        }
    }

    /// Process a user message, execute any tool calls, return the final response text.
    pub async fn handle_message(
        &mut self,
        user_text: &str,
        qga: &mut QgaClient,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // Add user message
        self.conversation.push(Message {
            role: Role::User,
            content: MessageContent::Text(user_text.into()),
        });

        let tools = sandbox_tools();

        for _ in 0..MAX_TOOL_ROUNDS {
            let response = self.backend.chat(&self.conversation, &tools).await?;

            match response {
                LlmResponse::Text(text) => {
                    self.conversation.push(Message {
                        role: Role::Assistant,
                        content: MessageContent::Text(text.clone()),
                    });
                    return Ok(text);
                }
                LlmResponse::ToolCalls(calls) => {
                    // Record assistant message with tool_use blocks
                    let tool_use_blocks: Vec<ContentBlock> = calls
                        .iter()
                        .map(|c| ContentBlock::ToolUse {
                            id: c.id.clone(),
                            name: c.name.clone(),
                            input: c.input.clone(),
                        })
                        .collect();
                    self.conversation.push(Message {
                        role: Role::Assistant,
                        content: MessageContent::Blocks(tool_use_blocks),
                    });

                    // Execute each tool call
                    let mut result_blocks = Vec::new();
                    for call in &calls {
                        let result = execute_tool(call, qga).await;
                        result_blocks.push(ContentBlock::ToolResult {
                            tool_use_id: call.id.clone(),
                            content: result,
                        });
                    }

                    // Add tool results as user message
                    self.conversation.push(Message {
                        role: Role::Tool,
                        content: MessageContent::Blocks(result_blocks),
                    });
                }
            }
        }

        Ok("Reached maximum tool execution rounds. Please try a simpler request.".into())
    }
}

async fn execute_tool(call: &ToolCall, qga: &mut QgaClient) -> String {
    match call.name.as_str() {
        "exec" => {
            let command = call.input["command"].as_str().unwrap_or("");
            match qga.exec(command, EXEC_TIMEOUT).await {
                Ok(output) => {
                    let mut result = String::new();
                    if !output.stdout.is_empty() {
                        result.push_str(&format!("stdout:\n{}\n", output.stdout));
                    }
                    if !output.stderr.is_empty() {
                        result.push_str(&format!("stderr:\n{}\n", output.stderr));
                    }
                    result.push_str(&format!("exit_code: {}", output.exit_code));
                    result
                }
                Err(e) => format!("Error executing command: {e}"),
            }
        }
        "read_file" => {
            let path = call.input["path"].as_str().unwrap_or("");
            match qga.read_file(path).await {
                Ok(data) => String::from_utf8_lossy(&data).into_owned(),
                Err(e) => format!("Error reading file: {e}"),
            }
        }
        "write_file" => {
            let path = call.input["path"].as_str().unwrap_or("");
            let content = call.input["content"].as_str().unwrap_or("");
            match qga.write_file(path, content.as_bytes()).await {
                Ok(()) => format!("Successfully wrote to {path}"),
                Err(e) => format!("Error writing file: {e}"),
            }
        }
        "nixos_rebuild" => {
            let config_nix = call.input["config_nix"].as_str().unwrap_or("");
            // Write config to temp file, then rebuild
            let write_result = qga
                .write_file("/etc/nixos/sandbox-extra.nix", config_nix.as_bytes())
                .await;
            if let Err(e) = write_result {
                return format!("Error writing config: {e}");
            }
            match qga
                .exec("nixos-rebuild switch", Duration::from_secs(300))
                .await
            {
                Ok(output) => {
                    let mut result = String::new();
                    if !output.stdout.is_empty() {
                        result.push_str(&output.stdout);
                    }
                    if !output.stderr.is_empty() {
                        result.push_str(&output.stderr);
                    }
                    if output.exit_code != 0 {
                        result.push_str(&format!("\nnixos-rebuild failed (exit {})", output.exit_code));
                    } else {
                        result.push_str("\nnixos-rebuild completed successfully");
                    }
                    result
                }
                Err(e) => format!("Error running nixos-rebuild: {e}"),
            }
        }
        _ => format!("Unknown tool: {}", call.name),
    }
}
```

Update `src/llm/mod.rs`:
```rust
pub mod agent;
pub mod anthropic;
pub mod tools;
pub mod traits;

pub use traits::{LlmBackend, LlmResponse, Message, ToolCall, ToolDef};
```

**Step 2: Run check**

Run: `cargo check`
Expected: Compiles with no errors

**Step 3: Commit**

```bash
git add src/llm/
git commit -m "feat: agent loop with tool execution via QGA"
```

---

## Phase 3: Integration

### Task 9: Session Tracker

**Files:**
- Create: `src/session/mod.rs`
- Create: `src/session/tracker.rs`
- Modify: `src/lib.rs`

**Step 1: Write the session tracker with timeout logic**

`src/session/tracker.rs`:
```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::llm::agent::Agent;
use crate::qga::QgaClient;

pub struct Session {
    pub vm_id: String,
    pub thread_id: u64,
    pub agent: Agent,
    pub qga: QgaClient,
    pub created_at: Instant,
    pub last_activity: Instant,
}

pub struct SessionTracker {
    /// Discord thread ID -> Session
    sessions: Arc<Mutex<HashMap<u64, Session>>>,
    idle_timeout: Duration,
}

impl SessionTracker {
    pub fn new(idle_timeout: Duration) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            idle_timeout,
        }
    }

    pub async fn add(&self, thread_id: u64, session: Session) {
        self.sessions.lock().await.insert(thread_id, session);
    }

    pub async fn get_mut<F, R>(&self, thread_id: u64, f: F) -> Option<R>
    where
        F: FnOnce(&mut Session) -> R,
    {
        let mut sessions = self.sessions.lock().await;
        sessions.get_mut(&thread_id).map(|s| {
            s.last_activity = Instant::now();
            f(s)
        })
    }

    pub async fn remove(&self, thread_id: u64) -> Option<Session> {
        self.sessions.lock().await.remove(&thread_id)
    }

    pub async fn find_by_vm(&self, vm_id: &str) -> Option<u64> {
        self.sessions
            .lock()
            .await
            .iter()
            .find(|(_, s)| s.vm_id == vm_id)
            .map(|(tid, _)| *tid)
    }

    /// Returns thread IDs of sessions that have exceeded idle timeout
    pub async fn expired_sessions(&self) -> Vec<u64> {
        let now = Instant::now();
        self.sessions
            .lock()
            .await
            .iter()
            .filter(|(_, s)| now.duration_since(s.last_activity) > self.idle_timeout)
            .map(|(tid, _)| *tid)
            .collect()
    }

    pub async fn count(&self) -> usize {
        self.sessions.lock().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full session tests require QgaClient + Agent which need real connections.
    // Test the tracker logic with basic state tracking.

    #[tokio::test]
    async fn test_expired_sessions() {
        let tracker = SessionTracker::new(Duration::from_millis(50));

        // No sessions -> no expired
        assert!(tracker.expired_sessions().await.is_empty());

        // Count starts at 0
        assert_eq!(tracker.count().await, 0);
    }
}
```

`src/session/mod.rs`:
```rust
pub mod tracker;

pub use tracker::{Session, SessionTracker};
```

Update `src/lib.rs`:
```rust
pub mod llm;
pub mod qga;
pub mod session;
pub mod vm;
```

**Step 2: Run tests**

Run: `cargo test session`
Expected: Tests pass

**Step 3: Commit**

```bash
git add src/session/ src/lib.rs
git commit -m "feat: session tracker with idle timeout detection"
```

---

### Task 10: Discord Bot — Commands and Message Handler

**Files:**
- Create: `src/bot/mod.rs`
- Create: `src/bot/commands.rs`
- Create: `src/bot/handler.rs`
- Modify: `src/main.rs`
- Modify: `src/lib.rs`

**Step 1: Write bot data types and commands**

`src/bot/mod.rs`:
```rust
pub mod commands;
pub mod handler;

use std::sync::Arc;

use crate::session::SessionTracker;
use crate::vm::VmManager;

pub struct BotData {
    pub vm_manager: Arc<VmManager>,
    pub sessions: Arc<SessionTracker>,
    pub llm_backend_factory: Arc<dyn LlmBackendFactory>,
}

/// Factory for creating LLM backends (one per session)
pub trait LlmBackendFactory: Send + Sync {
    fn create(&self) -> Box<dyn crate::llm::LlmBackend>;
}
```

`src/bot/commands.rs`:
```rust
use poise::serenity_prelude as serenity;

use crate::bot::BotData;
use crate::llm::agent::Agent;
use crate::session::Session;
use std::time::Instant;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, BotData, Error>;

/// Create a new sandbox VM
#[poise::command(slash_command, prefix_command)]
pub async fn create(
    ctx: Context<'_>,
    #[description = "Description of what you want in the sandbox"] description: Option<String>,
) -> Result<(), Error> {
    let data = ctx.data();

    // Check capacity
    let count = data.sessions.count().await;
    if count >= 10 {
        ctx.say("All sandbox slots are in use. Please try again later.").await?;
        return Ok(());
    }

    ctx.say("Creating sandbox VM... This may take a few minutes.").await?;

    // Create VM
    let user_config = description.as_deref().map(|_| {
        // For now, no LLM config generation — just use base config
        // TODO: Use LLM to generate NixOS config from description
        "{ }".to_string()
    });

    let vm_id = match data.vm_manager.create(user_config).await {
        Ok(id) => id,
        Err(e) => {
            ctx.say(format!("Failed to create sandbox: {e}")).await?;
            return Ok(());
        }
    };

    // Create thread for this sandbox
    let thread = ctx
        .channel_id()
        .create_thread(
            ctx.http(),
            serenity::CreateThread::new(format!("sandbox-{vm_id}"))
                .kind(serenity::ChannelType::PublicThread),
        )
        .await?;

    // Connect QGA
    let qga = match data
        .vm_manager
        .connect_qga(&vm_id, std::time::Duration::from_secs(60))
        .await
    {
        Ok(client) => client,
        Err(e) => {
            ctx.say(format!("VM created but QGA connection failed: {e}")).await?;
            let _ = data.vm_manager.destroy(&vm_id).await;
            return Ok(());
        }
    };

    // Create agent and session
    let backend = data.llm_backend_factory.create();
    let agent = Agent::new(backend);
    let session = Session {
        vm_id: vm_id.clone(),
        thread_id: thread.id.get(),
        agent,
        qga,
        created_at: Instant::now(),
        last_activity: Instant::now(),
    };
    data.sessions.add(thread.id.get(), session).await;

    thread
        .id
        .say(
            ctx.http(),
            format!(
                "Sandbox `{vm_id}` is ready! Send messages here to interact with your VM.\n\
                 Use `/destroy` to tear it down."
            ),
        )
        .await?;

    Ok(())
}

/// Destroy the sandbox in the current thread
#[poise::command(slash_command, prefix_command)]
pub async fn destroy(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let thread_id = ctx.channel_id().get();

    let session = data.sessions.remove(thread_id).await;
    match session {
        Some(session) => {
            let _ = data.vm_manager.destroy(&session.vm_id).await;
            ctx.say(format!("Sandbox `{}` destroyed.", session.vm_id)).await?;
        }
        None => {
            ctx.say("No sandbox found in this thread.").await?;
        }
    }

    Ok(())
}

/// Show sandbox status
#[poise::command(slash_command, prefix_command)]
pub async fn status(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let thread_id = ctx.channel_id().get();

    let info = data
        .sessions
        .get_mut(thread_id, |s| {
            let uptime = s.created_at.elapsed();
            let idle = s.last_activity.elapsed();
            format!(
                "**Sandbox `{}`**\nUptime: {:.0}s\nIdle: {:.0}s",
                s.vm_id,
                uptime.as_secs_f64(),
                idle.as_secs_f64()
            )
        })
        .await;

    match info {
        Some(msg) => ctx.say(msg).await?,
        None => ctx.say("No sandbox in this thread.").await?,
    };

    Ok(())
}
```

**Step 2: Write the message handler for thread conversations**

`src/bot/handler.rs`:
```rust
use poise::serenity_prelude::{self as serenity, Context, Message};
use std::sync::Arc;

use crate::bot::BotData;

/// Handle messages in sandbox threads — forward to LLM agent
pub async fn handle_message(
    ctx: &Context,
    msg: &Message,
    data: &BotData,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Ignore bot messages
    if msg.author.bot {
        return Ok(());
    }

    let thread_id = msg.channel_id.get();

    // Check if this thread has a session
    // We need mutable access to both agent and qga, so use get_mut
    let response = data
        .sessions
        .get_mut(thread_id, |session| {
            let user_text = msg.content.clone();
            // We can't do async inside get_mut, so we return what we need
            (user_text,)
        })
        .await;

    if response.is_none() {
        // Not a sandbox thread
        return Ok(());
    }

    // For the actual async agent call, we need a different approach.
    // The session tracker needs to allow async access.
    // For now, signal typing and do the work:
    let typing = msg.channel_id.start_typing(&ctx.http);

    // TODO: This needs refactoring — the session tracker's Mutex
    // doesn't allow holding across await points easily.
    // For MVP, we'll access the session directly:
    let mut sessions = data.sessions.sessions_mut().await;
    if let Some(session) = sessions.get_mut(&thread_id) {
        session.last_activity = std::time::Instant::now();
        let user_text = &msg.content;

        match session.agent.handle_message(user_text, &mut session.qga).await {
            Ok(response_text) => {
                // Split long messages for Discord's 2000 char limit
                for chunk in split_message(&response_text, 1900) {
                    msg.channel_id.say(&ctx.http, chunk).await?;
                }
            }
            Err(e) => {
                msg.channel_id
                    .say(&ctx.http, format!("Error: {e}"))
                    .await?;
            }
        }
    }
    drop(sessions);

    Ok(())
}

fn split_message(text: &str, max_len: usize) -> Vec<&str> {
    if text.len() <= max_len {
        return vec![text];
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let end = (start + max_len).min(text.len());
        // Try to split at newline
        let split_at = if end < text.len() {
            text[start..end]
                .rfind('\n')
                .map(|i| start + i + 1)
                .unwrap_or(end)
        } else {
            end
        };
        chunks.push(&text[start..split_at]);
        start = split_at;
    }
    chunks
}
```

Note: The session tracker needs a `sessions_mut` method. Update `src/session/tracker.rs` to add:

```rust
    /// Direct access to sessions (for async operations that need mutable session access)
    pub async fn sessions_mut(&self) -> tokio::sync::MutexGuard<'_, HashMap<u64, Session>> {
        self.sessions.lock().await
    }
```

**Step 3: Write main.rs**

`src/main.rs`:
```rust
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use poise::serenity_prelude as serenity;

use ephemeral_nixos_bot::bot::{self, BotData, LlmBackendFactory};
use ephemeral_nixos_bot::llm::anthropic::AnthropicBackend;
use ephemeral_nixos_bot::llm::LlmBackend;
use ephemeral_nixos_bot::session::SessionTracker;
use ephemeral_nixos_bot::vm::VmManager;

struct AnthropicFactory {
    api_key: String,
}

impl LlmBackendFactory for AnthropicFactory {
    fn create(&self) -> Box<dyn LlmBackend> {
        Box::new(AnthropicBackend::new(self.api_key.clone(), None))
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let discord_token =
        std::env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN env var required");
    let llm_api_key =
        std::env::var("LLM_API_KEY").expect("LLM_API_KEY env var required");
    let state_dir = std::env::var("VM_STATE_DIR")
        .unwrap_or_else(|_| "/var/lib/nixos-sandbox".into());
    let host_cache_url = std::env::var("HOST_CACHE_URL")
        .unwrap_or_else(|_| "https://cache.nixos.org".into());
    let project_root = std::env::var("PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap());

    let vm_manager = Arc::new(VmManager::new(
        project_root,
        PathBuf::from(&state_dir),
        host_cache_url,
    ));

    let sessions = Arc::new(SessionTracker::new(Duration::from_secs(1800))); // 30 min idle timeout
    let sessions_clone = sessions.clone();
    let vm_manager_clone = vm_manager.clone();

    // Spawn idle timeout reaper
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let expired = sessions_clone.expired_sessions().await;
            for thread_id in expired {
                if let Some(session) = sessions_clone.remove(thread_id).await {
                    tracing::info!("Destroying idle sandbox {}", session.vm_id);
                    let _ = vm_manager_clone.destroy(&session.vm_id).await;
                }
            }
        }
    });

    let data = BotData {
        vm_manager,
        sessions: sessions.clone(),
        llm_backend_factory: Arc::new(AnthropicFactory {
            api_key: llm_api_key,
        }),
    };

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                bot::commands::create(),
                bot::commands::destroy(),
                bot::commands::status(),
            ],
            event_handler: |ctx, event, _framework, data| {
                Box::pin(async move {
                    if let serenity::FullEvent::Message { new_message } = event {
                        bot::handler::handle_message(ctx, new_message, data).await?;
                    }
                    Ok(())
                })
            },
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                tracing::info!("Bot is ready!");
                Ok(data)
            })
        })
        .build();

    let intents = serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::MESSAGE_CONTENT;

    let client = serenity::ClientBuilder::new(discord_token, intents)
        .framework(framework)
        .await;

    client.unwrap().start().await.unwrap();
}
```

Update `src/lib.rs`:
```rust
pub mod bot;
pub mod llm;
pub mod qga;
pub mod session;
pub mod vm;
```

**Step 4: Run check**

Run: `cargo check`
Expected: Compiles (may need minor adjustments for lifetime/type issues — fix them)

**Step 5: Commit**

```bash
git add src/ Cargo.toml
git commit -m "feat: Discord bot with /create, /destroy, /status and thread message handling"
```

---

### Task 11: Nix Host Module (nix-serve + security)

**Files:**
- Create: `nix/host.nix`
- Modify: `flake.nix` (add NixOS module output)

**Step 1: Write the host NixOS module**

`nix/host.nix`:
```nix
# Host NixOS module for running the sandbox bot.
# Import this on the machine that hosts the VMs.
{ config, lib, pkgs, ... }:

let
  cfg = config.services.nixos-sandbox;
in {
  options.services.nixos-sandbox = {
    enable = lib.mkEnableOption "NixOS sandbox bot";

    stateDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/nixos-sandbox";
      description = "Directory for VM state";
    };

    hostCachePort = lib.mkOption {
      type = lib.types.port;
      default = 5557;
      description = "Port for nix-serve binary cache";
    };
  };

  config = lib.mkIf cfg.enable {
    # Serve host nix store as binary cache
    services.nix-serve = {
      enable = true;
      port = cfg.hostCachePort;
      bindAddress = "127.0.0.1";
    };

    # Create state directory
    systemd.tmpfiles.rules = [
      "d ${cfg.stateDir} 0750 root root -"
    ];

    # QEMU/KVM access
    virtualisation.libvirtd.enable = false;  # We manage QEMU directly
    boot.kernelModules = [ "kvm-intel" "kvm-amd" ];

    # Security: sandbox user for QEMU processes
    users.users.sandbox-runner = {
      isSystemUser = true;
      group = "sandbox-runner";
      home = cfg.stateDir;
    };
    users.groups.sandbox-runner = {};
  };
}
```

**Step 2: Add NixOS module to flake outputs**

Add to `flake.nix` outputs (alongside the `eachDefaultSystem` block):
```nix
nixosModules.default = import ./nix/host.nix;
```

**Step 3: Commit**

```bash
git add nix/host.nix flake.nix
git commit -m "feat: NixOS host module with nix-serve and sandbox-runner user"
```

---

### Task 12: End-to-End Smoke Test

**Files:**
- Create: `tests/nixos-test.nix`
- Modify: `flake.nix` (add check)

**Step 1: Write a NixOS VM integration test**

`tests/nixos-test.nix`:
```nix
# NixOS integration test: verify base VM boots and QGA responds
{ pkgs, microvm, ... }:

pkgs.nixosTest {
  name = "sandbox-smoke-test";

  nodes.host = { config, pkgs, ... }: {
    imports = [ ../nix/host.nix ];
    services.nixos-sandbox.enable = true;

    # Test needs QEMU
    environment.systemPackages = [ pkgs.qemu_kvm ];

    virtualisation.memorySize = 2048;
    virtualisation.cores = 2;
  };

  testScript = ''
    host.start()
    host.wait_for_unit("nix-serve.service")
    host.succeed("curl -s http://127.0.0.1:5557/nix-cache-info")
  '';
}
```

**Step 2: Add to flake checks**

In `flake.nix`, add to the `eachDefaultSystem` outputs:
```nix
checks.smoke-test = import ./tests/nixos-test.nix {
  inherit pkgs microvm;
};
```

**Step 3: Run test (on NixOS host only)**

Run: `nix build .#checks.x86_64-linux.smoke-test`
Expected: Test builds and passes (nix-serve starts, responds to requests)

**Step 4: Commit**

```bash
git add tests/ flake.nix
git commit -m "test: NixOS integration smoke test for host module"
```

---

## Post-MVP Tasks (Phase 2)

These are documented for future work but not part of the initial implementation:

- **OpenAI backend**: Implement `OpenAiBackend` following same pattern as Anthropic
- **Ollama backend**: Implement `OllamaBackend` for local models
- **Natural language config generation**: Use LLM to generate NixOS config from description before VM creation
- **File download command**: `/sandbox download <path>` using QGA file-read + Discord attachment upload
- **Bridge networking module**: `nix/networking/bridge.nix` with nftables host isolation
- **veth + nftables module**: `nix/networking/veth.nix` for full network control
- **Rate limiting**: Per-user VM creation limits, cooldown periods
- **Persistent sessions**: Optional snapshot/resume of VMs across bot restarts
