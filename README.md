# Pocket Pi

Pocket Pi is a complete Agent-native runtime for embedded and dedicated
devices. It makes the Agent a resident system actor instead of a user-launched
application on a general-purpose desktop or mobile OS. One device runtime
combines a persistent Pi Agent, local `/workspace`, native tools, schedules,
Agent-native Apps, local SQLite state and a PocketJS UI.

> **Development warning:** Pocket Pi is evolving rapidly. Breaking changes land
> frequently while the runtime, App contract and hardware integration are being
> built; do not assume compatibility between revisions yet.

Pocket Pi builds on:

- [`pi-agent-core`](https://github.com/badlogic/pi-mono) for the Agent Loop and
  Tool Call harness;
- [PocketJS](https://github.com/pocket-stack/pocketjs) and QuickJS for embedded
  Apps and device UI;
- native target hosts for storage, credentials, networking, rendering, device
  capabilities and lifecycle.

**ESP32-P4 is the first fully supported hardware target and the current
reference implementation.** ESP32-S3 is also supported through the Waveshare
ESP32-S3-Touch-LCD-4.3 firmware host. The macOS ESP32 simulator is a development
and product-contract testing tool; it is not a desktop Pocket Pi product or a
hardware emulator.

> **Project status:** the Waveshare ESP32-P4 reference target and Waveshare
> ESP32-S3-Touch-LCD-4.3 target both run the shared AgentOS/App stack. See
> [Current validation](#current-validation) for build and physical evidence.

## One complete device runtime

The current runtime runs upstream `pi-agent-core` in QuickJS and implements Pi
tools as bounded native capabilities. Its architecture preserves the Pi Harness
contracts while adding the device-level responsibilities Pocket Pi owns:

- the model decides when to call a tool;
- tools have explicit schemas and return structured results;
- the Agent loop is separate from model transports and platform APIs;
- workspace and schedules are durable device capabilities;
- Apps are Data + actor-neutral Actions + a fixed human-facing View; Agent
  Tools, UI events and Schedules route to the same Actions;
- the native host owns credentials, transport, hardware and lifecycle.

The companion simulator runs the same AgentOS, App source, UI, Tool
registry and workspace contracts while replacing hardware adapters with macOS
implementations. It is not an ESP32 CPU/peripheral emulator, and simulator
success never replaces physical-device acceptance.

## Quick start

### Prerequisites

- Rust stable for the development simulator;
- Bun for rebuilding the embedded JavaScript guest;
- a logged-in `codex` CLI for the simulator's local Codex backend;
- the esp-rs/ESP-IDF toolchains and `espflash` for physical targets; ESP32-P4
  uses `nightly-2026-05-01`, while ESP32-S3 uses the `esp` toolchain declared
  by its firmware directory.

The current firmware targets are **Waveshare ESP32-P4-WIFI6-Touch-LCD-5** and
**Waveshare ESP32-S3-Touch-LCD-4.3**.

### Build the device and development simulator

```sh
cargo xtask build esp32-sim
cargo xtask build esp32-p4
cargo xtask build esp32-s3
```

These commands use the generated Pi Agent bundle and View SDK resources
committed to this repository. A separate PocketJS checkout is needed only when
regenerating View SDK resources:

```sh
cargo xtask build pi-agent

git clone https://github.com/pocket-stack/pocketjs.git ../pocketjs
git -C ../pocketjs checkout e12cf12f82cc60b636368119d49a06eb9ed2a3d5
POCKETJS_ROOT=../pocketjs cargo xtask build view-sdk
```

`POCKETJS_ROOT` is an optional developer override. Normal simulator and
firmware builds never inspect or modify a neighboring PocketJS checkout.

Firmware embeds and seeds the Pi Agent System App release so a blank device can
boot. Its View and Agent loop run in one resident Guest; its Actions use the
shared Action LRU. Ordinary Apps use the installable container:

```sh
cargo xtask package app exa path/to/exa-credentials.json
cargo xtask package app robinhood path/to/robinhood-credentials.json
```

Packages are written to `target/pocketapps/<id>.pocketapp`. The firmware image
contains Pi Agent and native device mechanisms, but no ordinary App. Ordinary
Apps can be installed without rebuilding or flashing Firmware. Apps without
declared credentials omit the final argument. To update an installed App,
package it without credentials; Pocket Pi preserves the native values already
stored for that App:

```sh
cargo xtask package app exa
```

### 1. Pocket Pi on physical ESP32 hardware

Build and flash ESP32-P4:

```sh
cargo xtask build esp32-p4

DEVICE_PORT=/dev/cu.usbmodem...
espflash flash --baud 921600 --port "$DEVICE_PORT" \
  --partition-table firmware/esp32-p4/partitions.csv \
  firmware/esp32-p4/target/riscv32imafc-esp-espidf/release/pocket-pi-p4
```

Build and flash ESP32-S3:

```sh
cargo xtask build esp32-s3

DEVICE_PORT=/dev/cu.usbmodem...
espflash flash --baud 921600 --port "$DEVICE_PORT" \
  --partition-table firmware/esp32-s3/partitions.csv \
  firmware/esp32-s3/target/xtensa-esp32s3-espidf/release/pocket-pi-s3
```

Provision the board once with a direct wireless model backend. DeepSeek is the
default provider and uses `deepseek-v4-flash` unless `--model` is supplied:

```sh
python3 tools/uart-provision.py "$DEVICE_PORT" \
  --provider deepseek --provision-wifi
```

The command reads the DeepSeek key from macOS Keychain when available, otherwise
it prompts without echoing the value. It stores model provider, model, thinking
level and API key in native NVS. Wi-Fi credentials use the existing Wi-Fi NVS
store and can later be changed from Settings. Subsequent boots load both stores
directly and do not wait for a Mac or UART bridge.

Other direct providers are selected explicitly:

```sh
python3 tools/uart-provision.py "$DEVICE_PORT" \
  --provider openai --model gpt-5-mini
```

For bring-up on an unprovisioned development board only, the optional model
bridge can route requests to a logged-in Mac Codex or Claude Code. It is not
part of standalone startup and is never stored as the device backend:

```sh
python3 tools/uart-model-bridge.py "$DEVICE_PORT" --provider codex \
  --prompt 'Use write, read, schedule.set and schedule.list.'
```

Use `espflash monitor` only for boot diagnostics. It controls DTR/RTS on the
board's WCH USB bridge, so it must not run between an App upload and the
on-device confirmation. Restore a normal boot after exiting:

```sh
espflash monitor --port "$DEVICE_PORT"
espflash reset --port "$DEVICE_PORT" --non-interactive
```

To install over the local network, open `http://<device-ip>/` from a computer or
phone and upload the `.pocketapp`. When local peer access is unavailable, upload
the same artifact over USB UART:

```sh
python3 tools/uart-install.py "$DEVICE_PORT" \
  target/pocketapps/exa.pocketapp
```

Both transports stop at the same review screen; installation or update starts only after
confirmation on Pocket Pi. Confirm before opening `espflash monitor`; otherwise
its control-line sequence can reset the board and discard the pending review.
The first install must carry each credential declared by the App. An update
must omit credentials and keep the same native permissions; installed values
remain in native NVS and are not exposed through the App filesystem or Agent
workspace. App `version` is release metadata. SQLite `schemaVersion` advances
separately, using conventional `migrations/<target>.sql` files such as
`migrations/6.sql` for schema 5 to 6. Downgrades and missing steps are rejected.
Neither HTTP nor UART writes App storage, credentials or runtime state directly.
UART upload does not reset the board or change its model configuration.
The UART CLIs leave DTR/RTS inactive before closing the port so the USB serial
bridge does not reset a running Pocket Pi.

The resident Pi Agent can update an already installed ordinary App without a
new transport or package format. It calls `app.checkout`, edits the returned
`apps/<id>/checkout` directory with its normal file Tools, advances the App
version, and calls `app.submit`. Checkout also returns
`.system/app-events/<id>.json`, which contains the latest install and update
outcomes so a retry can see the previous failure. Submit moves that source into
the same installer staging used above and opens the same review screen; nothing
changes until the user confirms on Pocket Pi. Checkout does not copy App data,
temporary files or credentials, and code-only edits do not change
`schemaVersion`.

To remove an ordinary App, open **Apps**, tap **UNINSTALL APP**, then tap the
`X` on that App's row. Uninstall removes the App release, SQLite/data files,
schedules, credentials, native session state and cached View/Action
Runtimes. It does not retain App data or provide rollback. The resident Pi Agent
System App cannot be uninstalled. Uninstall also deletes that App's recent event
file.

### 2. Develop with the ESP32 simulator on macOS

The simulator defaults to the Mac's existing Codex Coding Plan login:

```sh
cargo xtask run esp32-sim \
  --backend codex \
  --workspace target/esp32-workspace
```

Direct API-key backends are also available:

```sh
OPENAI_API_KEY=... \
  cargo xtask run esp32-sim --backend openai --model gpt-5.6

OPENROUTER_API_KEY=... \
  cargo xtask run esp32-sim \
  --backend openrouter --model openai/gpt-5.6

ANTHROPIC_API_KEY=... \
  cargo xtask run esp32-sim \
  --backend anthropic --model claude-sonnet-4-6

DEEPSEEK_API_KEY=... DEEPSEEK_THINKING_LEVEL=xhigh \
  cargo xtask run esp32-sim --backend deepseek
```

The window defaults to the 720x1280 reference viewport. Pass a different
viewport to exercise the same App source on another screen shape;
mouse input is mapped through the same hit-testing code as physical touch
input:

```sh
cargo xtask run esp32-sim --viewport 800x480
cargo xtask run esp32-sim --viewport 480x800
```

Generate a deterministic UI snapshot with:

```sh
cargo xtask snapshot esp32-sim
```

## Model backends

Backends belong to their host composition, not to the Agent core:

| Host | Supported backends |
|---|---|
| ESP32 simulator | local Codex, OpenAI, OpenRouter, Anthropic, DeepSeek V4 |
| Physical ESP32-P4 and ESP32-S3 | standalone wireless OpenAI, OpenRouter, Anthropic or DeepSeek V4 |

The optional development-only `UartBackend` and the standalone
`WirelessBackend` implement the same model-completion contract. Wireless
providers may emit progress events internally; the development bridge coalesces
provider chunks into one final framed result before it reaches an unprovisioned
device. Provider request/streaming codecs live in
`pocket-pi-protocols`; serial framing, desktop CLIs and ESP-IDF HTTPS stay in
their platform layers.

DeepSeek defaults to `deepseek-v4-flash` with thinking level `high`. Set
`DEEPSEEK_THINKING_LEVEL=xhigh` in the simulator, or pass
`--thinking-level xhigh` while provisioning the physical board, to request the
provider's `max` reasoning effort.

## Embedded Agent capabilities

The simulator and physical firmware register the same tools:

- workspace files: `read`, `write`, `edit`, `find`, `grep`, `ls`;
- `workspace.delete`, routed through the Pi Agent System App's `deleteFile`
  Action;
- bounded shell commands through `bash`;
- `device.status` and `time.now`;
- `workspace.context` for durable Agent-managed memory;
- `schedule.set`, `schedule.list`, `schedule.cancel` and `schedule.clear` for
  one-off or recurring wake prompts.

The embedded `bash` tool is an allowlisted command dispatcher, not a POSIX
shell. It provides useful device and workspace operations without pretending
that an ESP32 has processes, pipes, package management or a Unix filesystem.

On physical hardware, workspace files and schedules persist in LittleFS. Wi-Fi
configuration persists in NVS. The Agent can use its workspace to organize
memory and can create or revise its own recurring schedules.

The shared [PocketJS](https://github.com/pocket-stack/pocketjs) device UI
includes:

- Chat with provider replies, recent-turn history and a full-message reader;
- Files with workspace metadata, file viewing and scrolling;
- Apps with install discovery and destructive uninstall mode;
- Settings with Wi-Fi scanning, selection and password entry;
- touch keyboard and next-schedule status.

The embedded AgentOS ships Pi Agent as its resident System App. Robinhood and
Exa use the same installable package as every ordinary App. Each ordinary App
owns its Tool catalog, Actions, SQLite Data and fixed PocketJS View;
hosts provide only scoped credentials, transport and hardware adapters.

## Repository map

```text
crates/pocket-pi-embedded/    bounded Agent Loop bridge used by device runtimes
crates/pocket-pi-tools/       portable workspace, shell, time and schedule tools
crates/pocket-pi-protocols/   model request, response and streaming codecs
crates/pocket-pi-agentos/     App Supervisor, System App lifecycle and App contracts
hosts/esp32-sim/              macOS development simulator for shared ESP32 contracts
firmware/esp32-common/        shared ESP-IDF AgentOS host loop and services
firmware/esp32-p4/            first supported device and reference implementation
firmware/esp32-s3/            Waveshare ESP32-S3-Touch-LCD-4.3 board host
tools/uart_io.py              shared raw UART read/write helpers
tools/uart-provision.py       one-time wireless model provisioning
tools/uart-install.py         App package ingress over UART
tools/uart_bridge/            development-only Codex/Claude adapters
tools/uart-model-bridge.py    optional development-only model bridge
```

Dependencies point inward: hosts depend on shared runtimes, tools, UI and
protocols; those shared crates do not depend on a host. Hardware APIs remain in
the firmware, simulator adapters remain in `hosts/esp32-sim`, and optional
external services remain Apps.

See [ARCHITECTURE.md](ARCHITECTURE.md) for ownership and lifecycle boundaries,
[docs/agentos-architecture.md](docs/agentos-architecture.md) for the viewport
contract, and [docs/esp32-p4-port.md](docs/esp32-p4-port.md) for the reference
target and board-port responsibilities.

## Current validation

On **2026-08-24**, the workspace completed 57 Rust tests, passed workspace
Clippy with warnings denied, and built the simulator plus both ESP32-P4 and
ESP32-S3 release firmware. The core suite covers viewport propagation, geometry
scaling, minimum touch targets, App lifecycle, SQLite ownership and model/tool
contracts.

ESP32-P4 remains the reference target with physical coverage for Agent,
workspace, schedules, App install/update/uninstall, display/touch, Wi-Fi and
direct model-provider operation. Physical ESP32-S3 validation covers boot,
480x800 logical scanout, GT911 touch, integrated Wi-Fi, workspace Tool Calls,
ordinary App installation and an Exa request. Long-running latency, display
stability and memory-pressure testing remain separate acceptance work.

Simulator and cross-build success do not substitute for physical-board
acceptance on either target.

## Development checks

```sh
cargo test --workspace
bun test apps/pi-agent/text.test.js
cargo clippy --workspace --all-targets -- -D warnings
```

CI runs the workspace build, Rust/App behavior tests and clippy checks. LCD scanout, touch,
LittleFS, Wi-Fi/NVS, memory pressure and real UART/wireless behavior still
require physical-board acceptance.

## Non-goals

- providing a macOS/Windows/Linux desktop Pocket Pi product;
- providing a generic `pi-agent-core` SDK or standalone harness;
- Emulating the ESP32 CPU or peripherals on macOS;
- exposing arbitrary desktop shell or Node APIs on the microcontroller;
- coupling model providers, UI, trading services or research services into Pi
  Agent Loop core;
- claiming that an API codec compile proves a live provider connection.

## License

MIT.
