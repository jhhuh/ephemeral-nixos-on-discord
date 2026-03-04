# Networking Modes

VMs need internet access (for `nix build`, package downloads, user tasks) but must **not** be able to reach the host's network services.

Three networking modes are available, configured via NixOS modules on the host.

## SLIRP (default)

User-mode networking built into QEMU. No host configuration required.

```nix
# In nix/base-vm.nix — already configured by default
microvm.interfaces = [{
  type = "user";
  id = "vm-net0";
  mac = "02:00:00:00:00:01";
}];
```

**Pros:** Zero setup, host is unreachable by design, no root needed.

**Cons:** Slower than bridge/veth, no inbound connections to VM.

**Host isolation:** Built-in. SLIRP NAT does not expose the host to the guest.

## Bridge

All VMs share a bridge interface with nftables-based host isolation.

```nix
# In your NixOS host configuration
imports = [ ./nix/networking/bridge.nix ];

services.nixos-sandbox.networking.bridge = {
  enable = true;
  bridgeName = "br-sandbox";      # default
  subnet = "10.99.0.0/24";        # default
  hostAddress = "10.99.0.1";      # default
  wanInterface = "eth0";           # your WAN interface
};
```

**What it configures:**

- Bridge interface (`br-sandbox`) with host IP
- nftables rules: VM→internet allowed, VM→host blocked, VM↔VM allowed
- NAT/masquerade for outbound traffic
- dnsmasq DHCP server (10.99.0.100–200 range)

**Pros:** Better performance than SLIRP, VM-to-VM communication.

**Cons:** Requires host-side configuration, needs nftables for isolation.

## veth + nftables

Per-VM veth pairs with full nftables control. Strictest isolation.

```nix
# In your NixOS host configuration
imports = [ ./nix/networking/veth.nix ];

services.nixos-sandbox.networking.veth = {
  enable = true;
  subnetPrefix = "10.100";         # Each VM gets 10.100.<N>.0/30
  wanInterface = "eth0";
  hostAddresses = [                # IPs to block from VMs
    "192.168.1.1"
    "10.0.0.1"
  ];
};
```

**What it configures:**

- nftables rules: VM→internet allowed, VM→host blocked, **VM→VM blocked**
- NAT/masquerade for outbound traffic
- Helper scripts: `sandbox-veth-setup <vm-id> <index>`, `sandbox-veth-teardown <vm-id>`

**Pros:** Per-VM isolation (VMs can't see each other), full nftables control.

**Cons:** Most complex setup, requires manual veth pair management.

## Comparison

| Feature | SLIRP | Bridge | veth |
|---------|-------|--------|------|
| Setup complexity | None | Medium | High |
| Performance | Low | High | High |
| Host isolation | Built-in | nftables | nftables |
| VM-to-VM | No | Yes | No (blocked) |
| Inbound to VM | No | Yes (via bridge) | Yes (via veth) |
| Root required | No | Yes | Yes |
