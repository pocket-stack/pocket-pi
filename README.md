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
reference implementation.** The macOS ESP32-P4 simulator is a development and
product-contract testing tool; it is not a desktop Pocket Pi product or a
second hardware target.

> **Project status:** the Waveshare ESP32-P4 target has working end-to-end
> Agent, workspace, schedule, App, display/touch and provider paths. The first
> target remains board-specific and under active development; see
> [Current validation](#current-validation) for the exact evidence and limits.

## One complete device runtime

The current runtime runs upstream `pi-agent-core` in QuickJS and implements Pi
tools as bounded native capabilities. Its architecture preserves the Pi Harness
contracts while adding the device-level responsibilities Pocket Pi owns:

- the model decides when to call a tool;
- tools have explicit schemas and return structured results;
- the Agent loop is separate from model transports and platform APIs;
- workspace and schedules are durable device capabilities;
- Apps combine Agent-facing Tools, local state, autonomous Tasks and a fixed
  human-facing View;
- the native host owns credentials, transport, hardware and lifecycle.

The companion simulator runs the same AgentOS, PocketJS App bundles, UI, Tool
registry and workspace contracts while replacing hardware adapters with macOS
implementations. It is not an ESP32 CPU/peripheral emulator, and simulator
success never replaces physical-device acceptance.

## Quick start

### Prerequisites

- Rust stable for the development simulator;
- Bun for rebuilding the embedded JavaScript guest;
- a logged-in `codex` CLI for the simulator's local Codex backend;
- the esp-rs/ESP-IDF toolchain, `nightly-2026-05-01`, and `espflash` for the
  supported ESP32-P4 target.

The current firmware target is the
**Waveshare ESP32-P4-WIFI6-Touch-LCD-5**.

### Build the device and development simulator

```sh
cargo xtask build pi-agent
cargo xtask build esp32-p4-sim
cargo xtask build esp32-p4
```

Pi Agent is always included in firmware. Ordinary Apps are standalone packages:

```sh
cargo xtask build app exa path/to/exa-credentials.json
```

Packages are written to `target/pocketapps/<id>.pocketapp`. The firmware image
contains the resident Pi Agent, workspace, native tools, schedules and Root View,
but no ordinary App. Apps without declared credentials omit the final argument.

### 1. Pocket Pi on ESP32-P4

Build and flash the release firmware:

```sh
cargo xtask build esp32-p4

DEVICE_PORT=/dev/cu.usbmodem...
espflash flash --baud 921600 --port "$DEVICE_PORT" \
  --partition-table firmware/esp32-p4/partitions.csv \
  firmware/esp32-p4/target/riscv32imafc-esp-espidf/release/pocket-pi-p4
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

Use `espflash monitor` when only serial logs are needed:

```sh
espflash monitor --port "$DEVICE_PORT"
```

To install over the local network, open `http://<device-ip>/` from a computer or
phone and upload the `.pocketapp`. When local peer access is unavailable, upload
the same artifact over USB UART:

```sh
python3 tools/uart-install.py "$DEVICE_PORT" \
  target/pocketapps/exa.pocketapp
```

Both transports stop at the same review screen; installation starts only after
confirmation on Pocket Pi. Every credential declared by the App must be present
in the package. Installer removes those values from the package and stores them in
native NVS; they are not exposed through the App filesystem or Agent workspace.
Neither HTTP nor UART writes App storage, credentials or runtime state directly.
UART upload does not reset the board or change its model configuration.
The UART CLIs leave DTR/RTS inactive before closing the port so the USB serial
bridge does not reset a running Pocket Pi.

To remove an ordinary App, open **Apps**, tap **UNINSTALL APP**, then tap the
`X` on that App's row. Uninstall removes the App release, SQLite/data files,
schedules, credentials, native session state and cached View/Data Action
Runtimes. It does not retain App data or provide rollback. The resident Pi Agent
System App cannot be uninstalled.

### 2. Develop with the ESP32-P4 simulator on macOS

The simulator defaults to the Mac's existing Codex Coding Plan login:

```sh
cargo xtask run esp32-p4-sim \
  --backend codex \
  --workspace target/esp32-workspace
```

Direct API-key backends are also available:

```sh
OPENAI_API_KEY=... \
  cargo xtask run esp32-p4-sim --backend openai --model gpt-5.6

OPENROUTER_API_KEY=... \
  cargo xtask run esp32-p4-sim \
  --backend openrouter --model openai/gpt-5.6

ANTHROPIC_API_KEY=... \
  cargo xtask run esp32-p4-sim \
  --backend anthropic --model claude-sonnet-4-6

DEEPSEEK_API_KEY=... DEEPSEEK_THINKING_LEVEL=xhigh \
  cargo xtask run esp32-p4-sim --backend deepseek
```

The window uses the ESP32-P4 product's 720x1280 coordinate system. Mouse input
is mapped through the same hit-testing code as physical touch input. Generate a
deterministic UI snapshot with:

```sh
cargo xtask snapshot esp32-p4-sim
```

## Model backends

Backends belong to their host composition, not to the Agent core:

| Host | Supported backends |
|---|---|
| ESP32 simulator | local Codex, OpenAI, OpenRouter, Anthropic, DeepSeek V4 |
| Physical ESP32-P4 | standalone wireless OpenAI, OpenRouter, Anthropic or DeepSeek V4 |

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

The simulator and physical firmware register the same portable core tools:

- workspace files: `read`, `write`, `edit`, `find`, `grep`, `ls`;
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

- Chat with provider-dependent incremental replies, recent-turn history and a
  full-message reader;
- Files with workspace metadata, file viewing and scrolling;
- Apps with install discovery and destructive uninstall mode;
- Settings with Wi-Fi scanning, selection and password entry;
- touch keyboard and next-schedule status.

The embedded AgentOS ships Pi Agent as its resident System App. Robinhood and
Exa use the same installable package as every ordinary App. Each ordinary App
owns its Tool catalog, Data Action, SQLite projection and fixed PocketJS View;
hosts provide only scoped credentials, transport and hardware adapters.

## Repository map

```text
crates/pocket-pi-embedded/    bounded Agent Loop bridge used by device runtimes
crates/pocket-pi-tools/       portable workspace, shell, time and schedule tools
crates/pocket-pi-protocols/   model request, response and streaming codecs
crates/pocket-pi-agentos/     App Supervisor, System App lifecycle and App contracts
hosts/esp32-p4-sim/           macOS development simulator for ESP32-P4 contracts
firmware/esp32-p4/            first supported device and reference implementation
tools/uart_io.py              shared raw UART read/write helpers
tools/uart-provision.py       one-time wireless model provisioning
tools/uart-install.py         App package ingress over UART
tools/uart_bridge/            development-only Codex/Claude adapters
tools/uart-model-bridge.py    optional development-only model bridge
```

Dependencies point inward: hosts depend on shared runtimes, tools, UI and
protocols; those shared crates do not depend on a host. Hardware APIs remain in
the firmware, simulator adapters remain in `hosts/esp32-p4-sim`, and optional
external services remain Apps.

See [ARCHITECTURE.md](ARCHITECTURE.md) for ownership and lifecycle boundaries,
and [docs/esp32-p4-port.md](docs/esp32-p4-port.md) for board-specific details.

## Current validation

On **2026-08-14**, the workspace completed 35 Rust tests and 3 App text behavior
tests with no failures, passed workspace Clippy with warnings denied, and built
the ESP32-P4 release firmware. A simulator LAN smoke uploaded the generated Exa
package through `POST /install` and received HTTP 202; the core install test
proves activation, Tool routing, SQLite ownership and restart recovery for a
previously absent App. The uninstall lifecycle test proves complete App-owned
state removal, live Tool removal, restart absence and reinstall. A physical
ESP32-P4 then accepted the same Exa package over UART, stopped at the shared
review screen, installed after touch confirmation, uninstalled it from Apps,
kept it absent across restart, and accepted a clean UART reinstall.
The split UART tools were then verified on the same board: one-time DeepSeek
provisioning received `PPI-CONFIG-STORED`; a bridge-free reset logged
`loaded wireless model configuration from NVS`, restored Wi-Fi/DHCP on
the saved network, and reached an idle Pi Agent; the independent App uploader
transferred Exa to the shared review screen without resetting the board.

Phone upload, fresh provider calls and unattended memory pressure remain
separate evidence tiers. Simulator and cross-build success do not substitute
for fresh physical-board acceptance.

## Development checks

```sh
cargo test --workspace
bun test apps/_shared/text.test.ts
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
