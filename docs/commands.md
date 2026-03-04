# Discord Commands

## `/create [description]`

Create a new sandbox VM and open a dedicated Discord thread.

**Parameters:**

| Parameter | Required | Description |
|-----------|----------|-------------|
| `description` | No | Natural language description of what to install (e.g., "Python 3.12 with PostgreSQL") |

**Behavior:**

1. Checks rate limits (max 2 VMs per user, 30s cooldown)
2. If `description` is provided, uses the LLM to generate a NixOS configuration
3. Builds and launches a QEMU VM via microvm.nix
4. Creates a Discord thread named `sandbox-<vm-id>`
5. Connects the QGA client
6. Posts a ready message in the thread

**Examples:**

```
/create
/create Python 3.12 and PostgreSQL
/create Web development environment with Node.js and nginx
```

After creation, send messages in the thread to interact with the VM through the LLM agent.

## `/destroy`

Destroy the sandbox attached to the current thread.

Must be run inside a sandbox thread. Kills the QEMU process, cleans up state, and removes the session.

## `/status`

Show the status of the sandbox in the current thread.

Displays: VM ID, uptime, idle time.

## `/download <path>`

Download a file from the sandbox VM.

**Parameters:**

| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | Yes | Absolute path to the file inside the VM |

**Example:**

```
/download /home/user/output.txt
/download /etc/nixos/configuration.nix
```

The file is uploaded as a Discord attachment. Maximum file size: 25 MB.

## Thread Messages

Any message sent in a sandbox thread (that isn't a slash command) is forwarded to the LLM agent.

### Real-Time Streaming

When the agent runs tools, you see every action **live** in the thread:

| Icon | Meaning |
|------|---------|
| :wrench: **Running:** | A shell command is about to execute |
| :pencil2: **Writing:** | A file is being written (with content preview) |
| :gear: **Rebuilding NixOS:** | A config change is being applied via `nixos-rebuild switch` |
| :white_check_mark: **Output:** | Command succeeded — output shown in code block |
| :x: **Output:** | Command failed — error shown in code block |

### What the Agent Can Do

- **Execute commands** — any shell command inside the VM
- **Read and write files** — view or modify any file
- **Apply NixOS configuration** — write a NixOS module and run `nixos-rebuild switch`
- **Teach NixOS** — explains concepts, prefers the declarative "NixOS way", suggests next steps

### NixOS Tutor Mode

The agent acts as a NixOS tutor:

- **Before running commands** — briefly explains what it's about to do and why
- **NixOS concepts** — naturally introduces declarative config, generations, the Nix store, flakes
- **NixOS way first** — prefers `nixos-rebuild` over imperative installs, explains the difference
- **Encourages experimentation** — the VM is ephemeral, so breaking things is fine

### Example

> **You:** set up a web server
>
> **Bot:** I'll enable nginx through NixOS's declarative configuration. Instead of installing nginx manually, we'll add it as a service in the NixOS config — this is the NixOS way.
>
> :gear: **Rebuilding NixOS:**
> ```nix
> { pkgs, ... }: {
>   services.nginx.enable = true;
>   networking.firewall.allowedTCPPorts = [ 80 ];
> }
> ```
> Then running `nixos-rebuild switch`
>
> :white_check_mark: **Output:**
> ```
> building the system configuration...
> activating the configuration...
> nixos-rebuild completed successfully
> ```
>
> :wrench: **Running:**
> ```bash
> curl -s localhost
> ```
> :white_check_mark: **Output:**
> ```html
> <html><body><h1>Welcome to nginx!</h1></body></html>
> ```
>
> nginx is running! Notice how we didn't install it with a package manager — we declared `services.nginx.enable = true` in the NixOS configuration. NixOS then handles installation, configuration, and the systemd service automatically. Want to customize the nginx config or add a virtual host?

The agent maintains conversation history for the duration of the session.
