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

Any message sent in a sandbox thread (that isn't a slash command) is forwarded to the LLM agent. The agent can:

- **Execute commands** in the VM
- **Read and write files**
- **Apply NixOS configuration changes** via `nixos-rebuild switch`
- **Explain** what happened and suggest next steps

The agent maintains conversation history for the duration of the session.
