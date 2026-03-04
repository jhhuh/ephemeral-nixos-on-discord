# NixOS Module Reference

Import `nixosModules.default` from the flake to deploy the bot on a NixOS host.

## `services.nixos-sandbox`

### Core Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enable` | bool | `false` | Enable the sandbox bot service |
| `package` | package | — | The bot package (from flake `packages.default`) |
| `projectRoot` | path | — | Path to project source (needed for `nix/base-vm.nix`) |
| `stateDir` | path | `/var/lib/nixos-sandbox` | Directory for VM runtime state |
| `hostCachePort` | port | `5557` | Port for nix-serve binary cache |

### Secret Options

| Option | Type | Description |
|--------|------|-------------|
| `discordTokenFile` | path | File containing the Discord bot token |
| `llmApiKeyFile` | path | File containing the LLM API key |

!!! tip "Use agenix or sops-nix"
    Store secrets encrypted and reference the decrypted paths:
    ```nix
    discordTokenFile = config.age.secrets.discord-token.path;
    ```

### LLM Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `llmBackend` | enum | `"anthropic"` | `"anthropic"`, `"openai"`, or `"ollama"` |

## What the Module Configures

### systemd Service

`nixos-sandbox-bot.service` runs the bot as `sandbox-runner` with:

- `Restart=on-failure` with 5s delay
- Secrets loaded from files at runtime (not in environment)
- Hardening: `ProtectSystem=strict`, `ProtectHome=true`, `NoNewPrivileges=true`
- `/dev/kvm` device access for QEMU

### nix-serve

Binary cache on `127.0.0.1:<hostCachePort>`. VMs use this to download packages from the host store instead of fetching from the internet.

### sandbox-runner User

System user with:

- KVM group membership (for QEMU hardware acceleration)
- Home directory at `stateDir`
- Trusted nix user (can build derivations)

### KVM

Loads `kvm-intel` and `kvm-amd` kernel modules.

## Example Configuration

```nix
{ inputs, ... }:

{
  imports = [
    inputs.nixos-sandbox.nixosModules.default

    # Optional: bridge networking
    "${inputs.nixos-sandbox}/nix/networking/bridge.nix"
  ];

  services.nixos-sandbox = {
    enable = true;
    package = inputs.nixos-sandbox.packages.x86_64-linux.default;
    projectRoot = inputs.nixos-sandbox;
    discordTokenFile = "/run/secrets/discord-token";
    llmApiKeyFile = "/run/secrets/llm-api-key";
    llmBackend = "anthropic";
  };

  # Optional: enable bridge networking
  services.nixos-sandbox.networking.bridge = {
    enable = true;
    wanInterface = "enp1s0";
  };
}
```

## Bridge Networking Options

See [Networking](networking.md#bridge) for `services.nixos-sandbox.networking.bridge` options.

## veth Networking Options

See [Networking](networking.md#veth-nftables) for `services.nixos-sandbox.networking.veth` options.
