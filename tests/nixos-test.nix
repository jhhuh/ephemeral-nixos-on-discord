{ pkgs, ... }:

pkgs.testers.nixosTest {
  name = "sandbox-smoke-test";

  nodes.host = { config, pkgs, ... }: {
    imports = [ ../nix/host.nix ];
    services.nixos-sandbox.enable = true;

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
