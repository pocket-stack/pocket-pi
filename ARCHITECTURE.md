# Pocket Pi architecture

Pocket Pi is one embedded AgentOS runtime with two hosts:

| Run mode | Host | Agent profile |
| --- | --- | --- |
| ESP32 simulator | `hosts/esp32-p4-sim` | embedded `pi-agent-core` System App |
| ESP32-P4 | `firmware/esp32-p4` | embedded `pi-agent-core` System App |

The authoritative detailed design is
[`docs/agentos-architecture.md`](docs/agentos-architecture.md).

## Ownership

- `crates/pocket-pi-agentos` owns App catalog, runtime lifecycle, foreground
  selection, schedules, App Tool routing, and App-scoped FS/SQLite mounts.
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

There is no legacy Rust product UI. Simulator and ESP32 render the same
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
crates/pocket-pi-embedded/  embedded pi-agent-core bridge and host traits
crates/pocket-pi-tools/     native workspace/shell/time/schedule Tools
crates/pocket-pi-protocols/ provider codecs
hosts/esp32-p4-sim/         embedded AgentOS simulator
firmware/esp32-p4/          ESP-IDF hardware composition root
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
evidence tiers. Physical ESP32-P4 remains the final hardware acceptance target.
