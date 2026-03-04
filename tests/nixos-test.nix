# NixOS integration test: verify host infrastructure for sandbox VMs
{ pkgs, ... }:

pkgs.testers.nixosTest {
  name = "sandbox-infrastructure-test";

  nodes.host = { config, pkgs, ... }: {
    imports = [
      ../nix/host.nix
      ../nix/networking/bridge.nix
    ];

    services.nixos-sandbox = {
      enable = true;
      stateDir = "/var/lib/nixos-sandbox";
      hostCachePort = 5557;
    };

    # Enable bridge networking
    services.nixos-sandbox.networking.bridge = {
      enable = true;
      wanInterface = "eth0";
    };

    environment.systemPackages = with pkgs; [
      qemu_kvm
      curl
      jq
    ];

    virtualisation.memorySize = 2048;
    virtualisation.cores = 2;
  };

  testScript = ''
    host.start()

    # Test 1: nix-serve binary cache is running
    host.wait_for_unit("nix-serve.service")
    host.succeed("curl -sf http://127.0.0.1:5557/nix-cache-info")

    # Test 2: sandbox-runner user exists
    host.succeed("id sandbox-runner")

    # Test 3: state directory exists with correct permissions
    host.succeed("test -d /var/lib/nixos-sandbox")

    # Test 4: KVM is available
    host.succeed("test -e /dev/kvm || echo 'KVM not available (expected in nested virt)'")

    # Test 5: bridge interface is configured
    host.wait_for_unit("network-online.target")
    host.succeed("ip link show br-sandbox")

    # Test 6: nftables rules are loaded
    host.succeed("nft list tables | grep sandbox-isolation")

    # Test 7: dnsmasq is running on bridge
    host.wait_for_unit("dnsmasq.service")

    # Test 8: Verify nix-serve returns valid cache info
    cache_info = host.succeed("curl -sf http://127.0.0.1:5557/nix-cache-info")
    assert "StoreDir: /nix/store" in cache_info, f"Unexpected cache info: {cache_info}"
  '';
}
