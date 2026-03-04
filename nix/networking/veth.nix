# Host NixOS module for veth-based per-VM networking.
# Each VM gets a dedicated veth pair with individual nftables rules.
{ config, lib, pkgs, ... }:

let
  cfg = config.services.nixos-sandbox.networking.veth;
in {
  options.services.nixos-sandbox.networking.veth = {
    enable = lib.mkEnableOption "veth-based per-VM networking";

    subnetPrefix = lib.mkOption {
      type = lib.types.str;
      default = "10.100";
      description = "Subnet prefix for VM veth pairs. Each VM gets 10.100.<N>.0/30";
    };

    wanInterface = lib.mkOption {
      type = lib.types.str;
      default = "eth0";
      description = "WAN interface for NAT";
    };

    hostAddresses = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [];
      description = "Host IP addresses to block from VMs";
    };
  };

  config = lib.mkIf cfg.enable {
    # Enable IP forwarding
    boot.kernel.sysctl."net.ipv4.ip_forward" = 1;

    # nftables for isolation and NAT
    networking.nftables.enable = true;
    networking.nftables.tables.sandbox-veth = {
      family = "inet";
      content = ''
        chain forward {
          type filter hook forward priority 0; policy drop;

          # Allow VM -> internet
          iifname "veth-vm-*" oifname "${cfg.wanInterface}" accept

          # Allow return traffic
          ct state established,related accept

          # Block VM -> host addresses
          ${lib.concatMapStringsSep "\n          " (addr:
            "iifname \"veth-vm-*\" ip daddr ${addr} drop"
          ) cfg.hostAddresses}

          # Block VM -> VM (isolation between sandboxes)
          iifname "veth-vm-*" oifname "veth-vm-*" drop
        }

        chain nat_postrouting {
          type nat hook postrouting priority 100;

          # NAT VM traffic going to WAN
          oifname "${cfg.wanInterface}" ip saddr ${cfg.subnetPrefix}.0.0/16 masquerade
        }
      '';
    };

    # Helper script to create/destroy veth pairs for VMs
    environment.systemPackages = [
      (pkgs.writeShellScriptBin "sandbox-veth-setup" ''
        # Usage: sandbox-veth-setup <vm-id> <vm-index>
        # Creates a veth pair for a VM
        VM_ID="$1"
        VM_INDEX="$2"
        HOST_IP="${cfg.subnetPrefix}.''${VM_INDEX}.1"
        VM_IP="${cfg.subnetPrefix}.''${VM_INDEX}.2"

        ip link add "veth-vm-''${VM_ID}" type veth peer name "veth-host-''${VM_ID}"
        ip addr add "''${HOST_IP}/30" dev "veth-host-''${VM_ID}"
        ip link set "veth-host-''${VM_ID}" up
        ip link set "veth-vm-''${VM_ID}" up
        echo "Created veth pair: veth-vm-''${VM_ID} (''${VM_IP}) <-> veth-host-''${VM_ID} (''${HOST_IP})"
      '')
      (pkgs.writeShellScriptBin "sandbox-veth-teardown" ''
        # Usage: sandbox-veth-teardown <vm-id>
        VM_ID="$1"
        ip link del "veth-vm-''${VM_ID}" 2>/dev/null || true
        echo "Removed veth pair for ''${VM_ID}"
      '')
    ];
  };
}
