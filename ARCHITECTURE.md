# Pocket Pi architecture

Pocket Pi is one runtime family with two agent profiles and three hosts.

```text
 full desktop profile                     embedded product profile
    crates/pocket-pi                       crates/pocket-pi-embedded
           │                                         │
     hosts/macos                  AppSnapshot/AppCommand + agent-shell
    (CLI host today)                              │
                                      ┌───────────┴───────────┐
                                      │                       │
                              hosts/esp32-p4       hosts/esp32-p4-sim
```

## Ownership

- `crates/pocket-pi` runs the full, unmodified `pi-coding-agent` with its
  desktop Node/Web compatibility layer.
- `crates/pocket-pi-embedded` runs the small upstream `pi-agent-core` loop. It
  receives model and tool capabilities from native host traits.
- `crates/pocket-pi-app-core` owns only versioned product state and commands.
- `crates/pocket-pi-protocols` owns provider-neutral wire types.
- `apps/agent-shell` owns the PocketJS UI. It cannot access files, network,
  models or secrets directly.
- A host is the composition root. It selects one agent profile and the concrete
  storage, network, input and optional display adapters.

Dependencies point inward: hosts depend on runtimes and contracts; contracts
never depend on hosts. Provider or product plugins stay outside the core.

## Shared embedded product

The ESP32-P4 firmware and macOS simulator share:

- the embedded Pi JavaScript bundle;
- the PocketJS UI JavaScript and pak;
- the logical 360x640 UI viewport at 2x raster density;
- `AppSnapshot` and `AppCommand`;
- Agent and tool semantics.

The simulator substitutes macOS filesystem, pointer and wgpu adapters. It does
not execute the ESP32 RISC-V ELF and does not emulate board peripherals.

## Runtime separation

Agent and UI use separate QuickJS realms and exchange bounded native state;
neither calls into the other. The simulator already runs the Agent realm on a
worker thread so its UI stays responsive during model streaming. The physical
host currently uses its self-contained offline adapter on the main loop; a
blocking wireless adapter must run the Agent realm on a worker as well.

## Build entry point

`cargo xtask` is the only orchestration layer:

```sh
cargo xtask build macos
cargo xtask build esp32-p4
cargo xtask build esp32-p4-sim
cargo xtask run esp32-p4-sim
cargo xtask snapshot esp32-p4-sim
```
