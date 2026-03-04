# Host NixOS module for bridge networking mode.
# Creates a bridge interface and nftables rules to isolate VMs from the host.
{ config, lib, pkgs, ... }:

let
  cfg = config.services.nixos-sandbox.networking.bridge;
in {
  options.services.nixos-sandbox.networking.bridge = {
    enable = lib.mkEnableOption "bridge networking for sandbox VMs";

    bridgeName = lib.mkOption {
      type = lib.types.str;
      default = "br-sandbox";
      description = "Name of the bridge interface for sandbox VMs";
    };

    subnet = lib.mkOption {
      type = lib.types.str;
      default = "10.99.0.0/24";
      description = "Subnet for VM bridge network";
    };

    hostAddress = lib.mkOption {
      type = lib.types.str;
      default = "10.99.0.1";
      description = "Host IP address on the bridge";
    };

    wanInterface = lib.mkOption {
      type = lib.types.str;
      default = "eth0";
      description = "WAN interface for NAT";
    };
  };

  config = lib.mkIf cfg.enable {
    # Create bridge interface
    networking.bridges.${cfg.bridgeName} = {
      interfaces = [];  # VMs tap interfaces attach dynamically
    };

    networking.interfaces.${cfg.bridgeName} = {
      ipv4.addresses = [{
        address = cfg.hostAddress;
        prefixLength = 24;
      }];
    };

    # Enable IP forwarding
    boot.kernel.sysctl."net.ipv4.ip_forward" = 1;

    # nftables rules for VM isolation and NAT
    networking.nftables.enable = true;
    networking.nftables.tables.sandbox-isolation = {
      family = "inet";
      content = ''
        chain forward {
          type filter hook forward priority 0; policy drop;

          # Allow VM -> internet (via WAN)
          iifname "${cfg.bridgeName}" oifname "${cfg.wanInterface}" accept

          # Allow return traffic
          ct state established,related accept

          # Block VM -> host (any host IP)
          iifname "${cfg.bridgeName}" ip daddr ${cfg.hostAddress} drop

          # Allow VM <-> VM
          iifname "${cfg.bridgeName}" oifname "${cfg.bridgeName}" accept
        }

        chain nat_postrouting {
          type nat hook postrouting priority 100;

          # NAT VM traffic going to WAN
          oifname "${cfg.wanInterface}" ip saddr ${cfg.subnet} masquerade
        }
      '';
    };

    # DHCP server for VMs on the bridge
    services.dnsmasq = {
      enable = true;
      settings = {
        interface = cfg.bridgeName;
        bind-interfaces = true;
        dhcp-range = "10.99.0.100,10.99.0.200,1h";
        dhcp-option = [ "3,${cfg.hostAddress}" "6,8.8.8.8,8.8.4.4" ];
      };
    };
  };
}
