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

    # KVM access
    virtualisation.libvirtd.enable = false;
    boot.kernelModules = [ "kvm-intel" "kvm-amd" ];

    # Sandbox runner user
    users.users.sandbox-runner = {
      isSystemUser = true;
      group = "sandbox-runner";
      home = cfg.stateDir;
    };
    users.groups.sandbox-runner = {};
  };
}
