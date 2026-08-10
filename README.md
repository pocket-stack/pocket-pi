# Pocket Pi

Pocket Pi runs the [Pi coding agent](https://github.com/badlogic/pi-mono) in
QuickJS. It provides a full desktop runtime and an embedded profile whose
touch UI is built with PocketJS for the ESP32-P4 product.

This repository contains one Pocket Pi runtime family with two profiles and
exactly three supported run modes:

| Run mode | Agent profile | What it is for |
|---|---|---|
| Native macOS | Full `pi-coding-agent` | Normal desktop Pocket Pi, including sessions and extensions |
| ESP32 simulator on macOS | Embedded `pi-agent-core` | Fast development of the ESP32 product UI, tools and Agent flows |
| Physical ESP32-P4 | Embedded `pi-agent-core` | The real standalone PocketJS/QuickJS device |

They are not three forks. The simulator and firmware compile the same embedded
Agent, device UI, tool contracts and interaction state. Only their platform
adapters differ.

> **Project status:** the macOS host, ESP32 simulator and physical
> Waveshare ESP32-P4 target all have working end-to-end paths. The embedded
> port is still board-specific and under active development; see
> [Current validation](#current-validation) for the exact evidence and limits.

## Why two profiles?

The desktop runtime can afford a broad Node/Web compatibility layer and embeds
the full, unmodified `pi-coding-agent`. An ESP32 cannot carry that entire
desktop platform unchanged, so the embedded profile runs upstream
`pi-agent-core` in QuickJS and implements the Pi tools as small native Rust
capabilities.

The architecture keeps the Pi Harness design in both profiles:

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

- Rust stable for the native macOS host and simulator;
- Bun for rebuilding the embedded JavaScript guest;
- a logged-in `codex` CLI for the simulator's local Codex backend;
- the esp-rs/ESP-IDF toolchain, `nightly-2026-05-01`, and `espflash` for the
  physical ESP32-P4 target.

The current firmware target is the
**Waveshare ESP32-P4-WIFI6-Touch-LCD-5**.

### Build all three modes

```sh
cargo xtask build macos
cargo xtask build esp32-p4-sim
cargo xtask build esp32-p4
```

### 1. Native Pocket Pi on macOS

```sh
cargo xtask run macos 'Who are you?'
```

The host selects a model in this order:

1. `OPENAI_API_KEY` with optional `OPENAI_MODEL`;
2. `ANTHROPIC_API_KEY` with optional `ANTHROPIC_MODEL`;
3. Pi's deterministic Faux Provider when neither key exists.

The Faux Provider is an offline development fallback. It still runs through a
real `createAgentSession`; it is not evidence of a live provider request.

### 2. ESP32 Pocket Pi simulator on macOS

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
```

The window uses the ESP32's 720x1280 coordinate system. Mouse input is mapped
through the same hit-testing code as physical touch input. Generate a
deterministic UI snapshot with:

```sh
cargo xtask snapshot esp32-p4-sim
```

### 3. Physical Pocket Pi on ESP32-P4

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

## Model backends

Backends belong to their host composition, not to the Agent core:

| Host | Supported backends |
|---|---|
| Native macOS | OpenAI, Anthropic, offline Faux Provider |
| ESP32 simulator | local Codex, OpenAI, OpenRouter, Anthropic |
| Physical ESP32-P4 | UART to Mac Codex or Claude Code; wireless OpenAI, OpenRouter or Anthropic |

`UartBackend` and `WirelessBackend` expose the same streaming model boundary to
the embedded Agent. Provider request/streaming codecs live in
`pocket-pi-protocols`; serial framing, desktop CLIs and ESP-IDF HTTPS stay in
their platform layers.

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

- Chat with streamed replies, recent-turn history and a full-message reader;
- Files with workspace metadata, file viewing and scrolling;
- Settings with Wi-Fi scanning, selection and password entry;
- touch keyboard and next-schedule status.

Settings is an embedded-device feature and is not linked into native macOS
Pocket Pi. This repository does not include Robinhood or Exa UI, clients,
credentials or tools. External products add their own plugin/tool and UI
adapters without changing Agent core.

## Desktop profile

`crates/pocket-pi` embeds the full, unmodified `pi-coding-agent` bundle in one
QuickJS realm. It provides enough Node/Web compatibility for Pi sessions,
extensions, native tools and streaming model turns without requiring Node or
Bun on the destination machine.

Desktop extensions use Pi's normal `(pi) => void` factory and may register
tools and lifecycle hooks. Pocket Pi transpiles TypeScript with oxc and injects
the factory through Pi's `extensionFactories` seam; Pi itself is not patched.

The desktop compatibility layer includes real filesystem, path, buffer,
events, stream, process, synchronous subprocess and global streaming `fetch`
support. Socket servers, worker threads and several lower-level Node builtins
remain stubs. These desktop APIs are intentionally **not** part of the embedded
ESP32 contract.

The full Pi bundle is committed and embedded, so normal Rust builds do not need
Node. Rebuild it only after changing the desktop JavaScript guest:

```sh
npm --prefix js ci
npm --prefix js run build
```

## Repository map

```text
crates/pocket-pi/             full desktop pi-coding-agent runtime
crates/pocket-pi-embedded/    embedded pi-agent-core guest and host traits
crates/pocket-pi-tools/       portable workspace, shell, time and schedule tools
crates/pocket-pi-protocols/   model request, response and streaming codecs
crates/pocket-pi-agentos/     App Supervisor, System App lifecycle and App contracts
hosts/macos/                  native desktop composition root
hosts/esp32-p4-sim/           macOS adapters for the embedded product
firmware/esp32-p4/            ESP-IDF hardware composition root and adapters
tools/uart_bridge/            Mac Codex and Claude Code streaming adapters
tools/uart-model-bridge.py    UART framing and provisioning CLI
```

Dependencies point inward: hosts depend on shared runtimes, tools, UI and
protocols; those shared crates do not depend on a host. Hardware APIs remain in
the firmware, macOS APIs remain in hosts/tools, and optional external services
remain plugins.

See [ARCHITECTURE.md](ARCHITECTURE.md) for ownership and lifecycle boundaries,
and [docs/esp32-p4-port.md](docs/esp32-p4-port.md) for board-specific details.

## Current validation

The following three paths were exercised end-to-end on **2026-08-05**:

| Mode | Exercised path | Result |
|---|---|---|
| Native macOS | Full `PiRuntime` and `createAgentSession` with Pi's offline Faux Provider | Passed |
| ESP32 simulator | Embedded Agent + local Codex; `write -> read -> recurring schedule.set -> schedule.list`; shared UI rendering | Passed |
| Physical ESP32-P4 | Release firmware + UART Mac Codex; streamed reply; real LittleFS `write/read`; recurring schedule creation/listing | Passed |

The workspace suite also completed with 46 passing tests and 3 ignored tests,
and workspace clippy passed with warnings denied. These results validate the
recorded revision; they are not a substitute for CI on later changes.

The physical `WirelessBackend` and provider codecs are implemented and compile,
but a live ESP32 Wi-Fi -> OpenAI request was **not** exercised in that session
because the available AP did not provide the required network route. Do not
interpret the UART E2E as evidence for that separate network path.

## Development checks

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

CI runs the workspace build, tests and clippy checks. LCD scanout, touch,
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
