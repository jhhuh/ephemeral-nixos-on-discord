# Getting Started

## Prerequisites

- [Nix](https://nixos.org/download.html) with flakes enabled
- A Discord bot token ([Discord Developer Portal](https://discord.com/developers/applications))
- An LLM API key (Anthropic, OpenAI, or a local Ollama instance)
- Linux host with KVM for running VMs (for production)

## Development Setup

```bash
git clone https://github.com/jhhuh/ephemeral-nixos-on-discord.git
cd ephemeral-nixos-on-discord
nix develop  # enter dev shell with Rust toolchain, overmind, etc.
```

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DISCORD_TOKEN` | Yes | — | Discord bot token |
| `LLM_BACKEND` | No | `anthropic` | `anthropic`, `openai`, or `ollama` |
| `LLM_API_KEY` | Anthropic/OpenAI | — | API key for the LLM provider |
| `VM_STATE_DIR` | No | `/tmp/ephemeral-vms` | Directory for VM runtime state |
| `HOST_CACHE_URL` | No | `http://localhost:5557` | nix-serve binary cache URL |
| `PROJECT_ROOT` | No | `.` | Path to project root (for `nix/base-vm.nix`) |
| `OPENAI_API_BASE` | No | `https://api.openai.com/v1` | Custom OpenAI-compatible API URL |
| `OLLAMA_BASE_URL` | No | `http://localhost:11434` | Ollama server URL |
| `OLLAMA_MODEL` | No | `llama3.1` | Ollama model name |

### Running Locally

```bash
export DISCORD_TOKEN="..."
export LLM_API_KEY="..."
cargo run
```

Or with overmind:

```bash
nix develop -c overmind start
```

### Running Tests

```bash
cargo test        # unit tests (20 tests)
cargo clippy      # linting
nix flake check   # Nix validation
```

## NixOS Deployment

For production, import the NixOS module into your system configuration:

```nix
{
  inputs.nixos-sandbox.url = "github:jhhuh/ephemeral-nixos-on-discord";

  outputs = { nixos-sandbox, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [
        nixos-sandbox.nixosModules.default
        {
          services.nixos-sandbox = {
            enable = true;
            package = nixos-sandbox.packages.x86_64-linux.default;
            projectRoot = nixos-sandbox;
            discordTokenFile = "/run/secrets/discord-token";
            llmApiKeyFile = "/run/secrets/llm-api-key";
            llmBackend = "anthropic";  # or "openai" or "ollama"
          };
        }
      ];
    };
  };
}
```

The module configures:

- **systemd service** with hardening (ProtectSystem, NoNewPrivileges, etc.)
- **nix-serve** binary cache on localhost:5557
- **sandbox-runner** system user with KVM access
- **State directory** at `/var/lib/nixos-sandbox`

See [NixOS Module Reference](nixos-module.md) for all options.
