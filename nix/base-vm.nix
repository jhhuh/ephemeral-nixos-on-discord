{ config, lib, pkgs, vmId, hostCacheUrl, ... }:

{
  microvm = {
    hypervisor = "qemu";

    interfaces = [{
      type = "user";
      id = "vm-net0";
      mac = "02:00:00:00:00:01";
    }];

    shares = [{
      proto = "virtiofs";
      source = "/nix/store";
      mountPoint = "/nix/store";
      readOnly = true;
      tag = "nix-store";
    }];

    qemu.extraArgs = [
      "-chardev" "socket,path=/tmp/microvm/${vmId}/qga.sock,server=on,wait=off,id=qga0"
      "-device" "virtio-serial"
      "-device" "virtserialport,chardev=qga0,name=org.qemu.guest_agent.0"
    ];

    mem = 1024;
    vcpu = 2;
  };

  services.qemuGuest.enable = true;

  nix.settings = {
    substituters = lib.mkForce [
      hostCacheUrl
      "https://cache.nixos.org"
    ];
    trusted-public-keys = [
      "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
    ];
  };

  networking.hostName = vmId;
  users.users.root.initialPassword = "";
  system.stateVersion = "24.11";
}
