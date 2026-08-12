# Pocket Pi

Pocket Pi runs the [Pi coding agent](https://github.com/badlogic/pi-mono) in
QuickJS. Its touch UI is built with PocketJS for the ESP32-P4 product.

This repository contains one embedded Agent profile with two supported run modes:

| Run mode | Agent profile | What it is for |
|---|---|---|
| ESP32 simulator on macOS | Embedded `pi-agent-core` | Fast development of the ESP32 product UI, tools and Agent flows |
| Physical ESP32-P4 | Embedded `pi-agent-core` | The real standalone PocketJS/QuickJS device |

The simulator and firmware compile the same embedded Agent, device UI, tool
contracts and interaction state. Only their platform adapters differ.

> **Project status:** the ESP32 simulator and physical Waveshare ESP32-P4 target
> have working end-to-end paths. The embedded port is still board-specific and
> under active development; see
> [Current validation](#current-validation) for the exact evidence and limits.

## One product profile, two hosts

The embedded profile runs upstream `pi-agent-core` in QuickJS and implements Pi
tools as small native Rust capabilities. The simulator is a product-contract
host for fast development; the physical ESP32-P4 is the final product host.

The architecture keeps the Pi Harness design in both hosts:

- the model decides when to call a tool;
- tools have explicit schemas and return structured results;
- the Agent loop is separate from model transports and platform APIs;
- workspaces and schedules are capabilities, not UI or prompt special cases.

The macOS simulator is a product-contract simulator, not an ESP32 CPU or
peripheral emulator. It gives the embedded Agent the same UI, tool registry and
workspace rules while replacing LCD, touch, storage and networking with macOS
adapters. Physical hardware remains the final acceptance target.

## Quick start

### Prerequisites

- Rust stable for the simulator;
- Bun for rebuilding the embedded JavaScript guest;
- a logged-in `codex` CLI for the simulator's local Codex backend;
- the esp-rs/ESP-IDF toolchain, `nightly-2026-05-01`, and `espflash` for the
  physical ESP32-P4 target.

The current firmware target is the
**Waveshare ESP32-P4-WIFI6-Touch-LCD-5**.

### Build both modes

```sh
cargo xtask build agentos-apps
cargo xtask build esp32-p4-sim
cargo xtask build esp32-p4
```

Pi Agent is always included. Select ordinary Apps at build time with one flag;
omitting it keeps the default Robinhood + Exa image:

```sh
cargo xtask build esp32-p4 --apps robinhood,exa
cargo xtask build esp32-p4 --apps robinhood
cargo xtask build esp32-p4 --apps exa
cargo xtask build esp32-p4 --apps none
```

### 1. ESP32 Pocket Pi simulator on macOS

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

The window uses the ESP32's 720x1280 coordinate system. Mouse input is mapped
through the same hit-testing code as physical touch input. Generate a
deterministic UI snapshot with:

```sh
cargo xtask snapshot esp32-p4-sim
```

### 2. Physical Pocket Pi on ESP32-P4

Build and flash the release firmware:

```sh
cargo xtask build esp32-p4

DEVICE_PORT=/dev/cu.usbmodem...
espflash flash --baud 921600 --port "$DEVICE_PORT" \
  --partition-table firmware/esp32-p4/partitions.csv \
  firmware/esp32-p4/target/riscv32imafc-esp-espidf/release/pocket-pi-p4
```

For development, the simplest model path is UART to a logged-in Mac Codex:

```sh
python3 tools/uart-model-bridge.py "$DEVICE_PORT" \
  --backend uart --provider codex
```

Claude Code can be used instead:

```sh
python3 tools/uart-model-bridge.py "$DEVICE_PORT" \
  --backend uart --provider claude-code
```

The bridge can also send a repeatable boot prompt:

```sh
python3 tools/uart-model-bridge.py "$DEVICE_PORT" \
  --backend uart --provider codex \
  --prompt 'Use write, read, schedule.set and schedule.list.'
```

For standalone use, provision Wi-Fi and a direct model provider over UART:

```sh
python3 tools/uart-model-bridge.py "$DEVICE_PORT" \
  --backend wireless --provider openai --model gpt-5-mini --provision-wifi
```

The provisioning command asks for the Wi-Fi credentials and API key
interactively. Wi-Fi can subsequently be changed from the device Settings UI
without reflashing. Credentials are kept out of Agent workspace files and
PocketJS UI state.

The UART bridge automatically reuses an existing authorized Robinhood session
from Keychain and injects its access token into the board's RAM-only boot
configuration. `--provision-robinhood` is only the interactive fallback when no
saved authorization exists.

## Model backends

Backends belong to their host composition, not to the Agent core:

| Host | Supported backends |
|---|---|
| ESP32 simulator | local Codex, OpenAI, OpenRouter, Anthropic, DeepSeek V4 |
| Physical ESP32-P4 | UART to Mac Codex or Claude Code; wireless OpenAI, OpenRouter, Anthropic or DeepSeek V4 |

`UartBackend` and `WirelessBackend` implement the same model-completion
contract. Wireless providers may emit progress events internally; the current
UART bridge coalesces provider chunks into one final framed result before it
reaches the device. Provider request/streaming codecs live in
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

The shared PocketJS device UI includes:

- Chat with provider-dependent incremental replies, recent-turn history and a
  full-message reader;
- Files with workspace metadata, file viewing and scrolling;
- Settings with Wi-Fi scanning, selection and password entry;
- touch keyboard and next-schedule status.

The embedded AgentOS ships Pi Agent as its resident System App and can include
Robinhood and Exa as build-selected ordinary Apps. Each ordinary App owns its Tool catalog, Data Action,
SQLite projection and fixed PocketJS View; hosts provide only scoped
credentials, transport and hardware adapters.

## Repository map

```text
crates/pocket-pi-embedded/    embedded pi-agent-core guest and host traits
crates/pocket-pi-tools/       portable workspace, shell, time and schedule tools
crates/pocket-pi-protocols/   model request, response and streaming codecs
crates/pocket-pi-agentos/     App Supervisor, System App lifecycle and App contracts
crates/pocket-pi-app-pack/    build-selected embedded App composition
hosts/esp32-p4-sim/           macOS adapters for the embedded product
firmware/esp32-p4/            ESP-IDF hardware composition root and adapters
tools/uart_bridge/            Mac Codex and Claude Code streaming adapters
tools/uart-model-bridge.py    UART framing and provisioning CLI
```

Dependencies point inward: hosts depend on shared runtimes, tools, UI and
protocols; those shared crates do not depend on a host. Hardware APIs remain in
the firmware, simulator adapters remain in `hosts/esp32-p4-sim`, and optional
external services remain Apps.

See [ARCHITECTURE.md](ARCHITECTURE.md) for ownership and lifecycle boundaries,
and [docs/esp32-p4-port.md](docs/esp32-p4-port.md) for board-specific details.

## Current validation

On **2026-08-12**, the embedded-only workspace completed 30 Rust tests and 3
App text behavior tests with no failures, passed workspace Clippy with warnings
denied, and built the ESP32-P4 simulator with none, Exa-only, Robinhood-only and
combined App catalogs.

Physical firmware, boot, Wi-Fi/DHCP, provider calls and unattended memory
pressure remain separate evidence tiers. A successful simulator build is not a
substitute for fresh physical-board acceptance.

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

- Emulating the ESP32 CPU or peripherals on macOS;
- exposing arbitrary desktop shell or Node APIs on the microcontroller;
- coupling model providers, UI, trading services or research services into Pi
  Harness core;
- claiming that an API codec compile proves a live provider connection.

## License

MIT.
