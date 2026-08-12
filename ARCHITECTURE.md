# Pocket Pi architecture

Pocket Pi is a complete Agent-native runtime for embedded and dedicated
devices. The Agent is a resident system actor with a persistent workspace,
native capabilities, schedules and Agent-native Apps—not a desktop application
or a generic Agent SDK.

The current implementation has one supported hardware target and one companion
development tool:

| Role | Composition | Status |
| --- | --- | --- |
| Reference hardware | `firmware/esp32-p4` | ESP32-P4 is the first fully supported Pocket Pi target |
| Development simulator | `hosts/esp32-p4-sim` | Runs the ESP32-P4 product contracts on macOS; not a desktop product or hardware target |

Both compositions use the same resident `pi-agent-core` System App and PocketJS
App bundles. The simulator substitutes development adapters; only the physical
composition proves hardware behavior.

The authoritative detailed design is
[`docs/agentos-architecture.md`](docs/agentos-architecture.md).

## Ownership

- `crates/pocket-pi-agentos` owns App catalog, runtime lifecycle, foreground
  selection, schedules, App Tool routing, and App-scoped FS/SQLite mounts.
- `crates/pocket-pi-app-pack` composes the build-selected App artifacts without
  moving their product logic into the AgentOS runtime or host adapters.
- `crates/pocket-pi-embedded` provides the bounded JavaScript Agent Loop bridge.
  In AgentOS hosts, the loop is loaded from the Pi Agent System App release into
  the same PocketJS Guest as its Root View.
- `apps/pi-agent` owns the Root View and Agent Loop release artifacts.
- `apps/robinhood` and `apps/exa` own their Tools, Tasks, SQLite schemas, and
  PocketJS Views.
- `crates/pocket-pi-tools` owns portable native workspace, bounded shell, time,
  device, and Agent schedule Tools.
- `crates/pocket-pi-protocols` owns model/provider transport protocols.
- Hosts own hardware, transport, credentials, and rendering adapters.

There is no legacy Rust product UI or general-purpose desktop runtime. The
simulator and ESP32-P4 render the same
PocketJS App bundles at a 720x1280 logical viewport. Rust firmware supplies the
display/touch driver and renders the selected App's DrawList.

## Runtime lifecycle

The Pi Agent is a first-class, always-resident System App. Its Agent Loop,
context, Tool Registry, and Root View share one Guest and one App lifecycle.
Opening Robinhood or Exa changes only the foreground View; it does not restart
or replace the Agent. Model and native Tool transport complete asynchronously
and return events to that persistent Guest.

Ordinary Apps receive capability-scoped data roots. Pi Agent alone owns the
top-level `/workspace` and cross-App Tool Registry.

## Repository map

```text
apps/                       PocketJS System/ordinary App sources and bundles
crates/pocket-pi-agentos/   App Supervisor and AgentOS contracts
crates/pocket-pi-app-pack/  build-selected embedded App composition
crates/pocket-pi-embedded/  embedded pi-agent-core bridge and host traits
crates/pocket-pi-tools/     native workspace/shell/time/schedule Tools
crates/pocket-pi-protocols/ provider codecs
hosts/esp32-p4-sim/         macOS development simulator for ESP32-P4 contracts
firmware/esp32-p4/          first supported target and reference implementation
tools/uart_bridge/          Mac Codex/Claude streaming adapters
tools/uart-model-bridge.py  UART framing and provisioning CLI
```

## Build entry points

```sh
cargo xtask build agentos-apps
cargo xtask build esp32-p4
cargo xtask build esp32-p4-sim
cargo xtask run esp32-p4-sim
cargo xtask snapshot esp32-p4-sim
```

Simulator proof, firmware compilation, and physical-board proof are separate
evidence tiers. ESP32-P4 is the current reference hardware and physical-board
proof remains its final acceptance tier. Future device targets must provide
their own native composition and physical validation without moving product
logic into firmware.
