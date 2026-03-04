{ config, lib, pkgs, ... }:

let
  cfg = config.services.nixos-sandbox;
in {
  options.services.nixos-sandbox = {
    enable = lib.mkEnableOption "NixOS sandbox bot";

    package = lib.mkOption {
      type = lib.types.package;
      description = "The bot package to run";
    };

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

    discordTokenFile = lib.mkOption {
      type = lib.types.path;
      description = "Path to file containing Discord bot token";
    };

    llmApiKeyFile = lib.mkOption {
      type = lib.types.path;
      description = "Path to file containing LLM API key";
    };

    llmBackend = lib.mkOption {
      type = lib.types.enum [ "anthropic" "openai" "ollama" ];
      default = "anthropic";
      description = "Which LLM backend to use";
    };

    projectRoot = lib.mkOption {
      type = lib.types.path;
      description = "Path to the project source (for nix/base-vm.nix)";
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
      "d ${cfg.stateDir} 0750 sandbox-runner sandbox-runner -"
    ];

    # Bot systemd service
    systemd.services.nixos-sandbox-bot = {
      description = "Ephemeral NixOS Sandbox Discord Bot";
      wantedBy = [ "multi-user.target" ];
      after = [ "network-online.target" "nix-serve.service" ];
      wants = [ "network-online.target" ];

      serviceConfig = {
        ExecStart = "${cfg.package}/bin/ephemeral-nixos-bot";
        User = "sandbox-runner";
        Group = "sandbox-runner";
        WorkingDirectory = cfg.stateDir;
        Restart = "on-failure";
        RestartSec = 5;

        # Hardening
        ProtectSystem = "strict";
        ProtectHome = true;
        ReadWritePaths = [ cfg.stateDir "/tmp" ];
        PrivateTmp = true;
        NoNewPrivileges = true;
        CapabilityBoundingSet = "";
        # Need /dev/kvm for QEMU
        DeviceAllow = [ "/dev/kvm rw" ];
      };

      environment = {
        VM_STATE_DIR = cfg.stateDir;
        HOST_CACHE_URL = "http://127.0.0.1:${toString cfg.hostCachePort}";
        PROJECT_ROOT = toString cfg.projectRoot;
        RUST_LOG = "info";
        LLM_BACKEND = cfg.llmBackend;
      };

      script = ''
        export DISCORD_TOKEN="$(cat ${cfg.discordTokenFile})"
        export LLM_API_KEY="$(cat ${cfg.llmApiKeyFile})"
        exec ${cfg.package}/bin/ephemeral-nixos-bot
      '';
    };

    # KVM access
    boot.kernelModules = [ "kvm-intel" "kvm-amd" ];

    # Give sandbox-runner access to KVM
    users.users.sandbox-runner = {
      isSystemUser = true;
      group = "sandbox-runner";
      home = cfg.stateDir;
      extraGroups = [ "kvm" ];
    };
    users.groups.sandbox-runner = {};

    # Allow sandbox-runner to build nix derivations
    nix.settings.trusted-users = [ "sandbox-runner" ];
  };
}
