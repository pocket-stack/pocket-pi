# Pocket Pi architecture

Pocket Pi is one runtime family with two agent profiles and three hosts.

| Run mode | Host | Pocket Pi profile | Purpose |
|---|---|---|---|
| Native Mac | `hosts/macos` | full `pi-coding-agent` | Normal desktop Pocket Pi |
| ESP32 simulator on Mac | `hosts/esp32-p4-sim` | embedded `pi-agent-core` + device UI/tools | Fast development of embedded contracts and product flows |
| Physical ESP32-P4 | `firmware/esp32-p4` | embedded `pi-agent-core` + device UI/tools | Real PocketJS/QuickJS Agent on the board |

These are not three products. They are three compositions of Pocket Pi. The
simulator and physical firmware share the embedded runtime, UI, tool contracts
and interaction semantics. The simulator may use simpler host implementations;
physical hardware remains the final acceptance target.

```text
 full desktop profile                 embedded product profile
 crates/pocket-pi                     crates/pocket-pi-embedded
        │                                      │
 hosts/macos                      crates/pocket-pi-device-ui
                                                   │
                                      ┌────────────┴────────────┐
                                      │                         │
                              firmware/esp32-p4       hosts/esp32-p4-sim
```

## Ownership

- `crates/pocket-pi` runs the full, unmodified `pi-coding-agent` with its
  desktop Node/Web compatibility layer.
- `crates/pocket-pi-embedded` runs the bounded upstream `pi-agent-core` loop.
  Native host traits provide model and tool capabilities.
- `crates/pocket-pi-device-ui` is the single source for the 720x1280 PocketJS
  draw list, fonts, touch hit map, Chat, Workspace browser, keyboard, message
  reader, device Settings and system status. The host supplies the mounted
  workspace root.
- `crates/pocket-pi-protocols` owns model/provider transport protocols.
- `crates/pocket-pi-tools` owns the portable native ESP tool registry:
  filesystem tools, bounded bash, workspace context, time and schedules.
- Each host is a composition root. It connects the embedded Agent, shared UI,
  filesystem, input, display and model adapter.

Dependencies point inward: hosts depend on the runtime, UI and protocols. The
runtime and UI do not depend on a host. External products can populate a UI
projection or register a native tool without putting provider clients in core.

## Repository map

```text
crates/pocket-pi/             full desktop pi-coding-agent runtime
crates/pocket-pi-embedded/    bounded pi-agent-core guest + native host traits
crates/pocket-pi-tools/       portable native workspace, shell, time and schedule tools
crates/pocket-pi-protocols/   provider request/response and streaming codecs
crates/pocket-pi-device-ui/   shared PocketJS embedded UI and interaction state
hosts/macos/                  desktop composition root
hosts/esp32-p4-sim/           macOS implementation of the embedded host adapters
firmware/esp32-p4/            ESP-IDF hardware composition root and adapters
tools/uart_bridge/            Mac Codex/Claude streaming adapters
tools/uart-model-bridge.py    thin UART framing and provisioning CLI
```

The split follows ownership, not product features. A native tool is implemented
once in `pocket-pi-tools`; a provider codec belongs in `pocket-pi-protocols`;
hardware access stays in a host. Optional applications such as Exa or
Robinhood should be separate tool/plugin adapters and must not become
dependencies of the embedded runtime or shared UI.

## ESP32 and simulator parity

The physical ESP32-P4 firmware and macOS simulator compile the same:

- `pocket-pi-embedded` Agent runtime;
- `pocket-pi-device-ui` Rust source and exact font atlases;
- `ScreenState::handle_tap` coordinate hit map;
- 720x1280 PocketJS draw-list viewport.

The simulator maps a mouse pointer into the same 720x1280 coordinates used by
the touch controller and calls the same `handle_tap` method. It substitutes
macOS filesystem, wgpu display and model adapters; it does not emulate the
ESP32 CPU or peripherals.

Both embedded hosts construct the same `CoreToolHost`. The simulator executes
filesystem and schedule operations against its Mac workspace directory using
the exact ESP constraints. Only `device.status`, `wifi status` and
`reboot` cross a small `PlatformTools` adapter.

Parity is contract-level, not peripheral emulation. The simulator must support
the real embedded Pi Agent, core tool registry, workspace flows and schedules.
It may use macOS storage, networking, deterministic fixtures and simplified
telemetry to do so. CPU load, LittleFS capacity, Wi-Fi/NVS behavior, touch, LCD
scanout and other ESP-IDF details are implemented and accepted only on physical
hardware.

The full macOS host does not link the device UI. Embedded products enable Chat,
Files and Settings. External applications such as Exa or Robinhood belong in
separate plugin/tool and UI adapter crates; Pocket Pi core contains none of
their domain models, clients, credentials or tools.

Settings follows the same host boundary as the rest of the device UI. PocketJS
emits `SettingsCommand` values and renders `SettingsProjection`; only the ESP
host calls ESP-IDF Wi-Fi/NVS/restart APIs. The simulator handles the same
commands with deterministic hardware projections. Password input is transient,
masked, cleared after submit, and never enters the Agent workspace or context.

The physical model boundary has two implementations:

- `UartBackend` sends framed model decisions to the Mac bridge, which can use a
  logged-in Codex or Claude Code CLI.
- `WirelessBackend` sends direct HTTPS requests over board Wi-Fi to OpenAI,
  OpenRouter, or Anthropic.

Provider JSON and streaming decoders live in `pocket-pi-protocols`; ESP-IDF and
desktop HTTP transports stay in their hosts.

The UART development path is deliberately layered. The ESP firmware owns only
line framing and `UartBackend`; `tools/uart-model-bridge.py` routes those frames;
`tools/uart_bridge` adapts a logged-in Codex app-server or Claude Code stream.
Only decoded top-level `text` is forwarded as UI deltas, while tool-decision
JSON remains private to the Pi Harness. Because ESP logs share UART0, the
firmware disables ESP-IDF logging after the Pi Harness ready handshake so logs
cannot corrupt model frames.

Device time follows the same dual-adapter rule. A standalone Wi-Fi device uses
SNTP. The UART development bridge may seed Unix time at boot, after which the
same persistent `schedule.*` tools and wake loop run unchanged.

## Runtime separation

Agent work runs on a worker thread. UI and touch remain responsive while model
deltas are projected into `ChatProjection`. The UI never owns network access,
model credentials or broker credentials.

## Build entry point

`cargo xtask` is the orchestration layer:

```sh
cargo xtask build macos
cargo xtask build esp32-p4
cargo xtask build esp32-p4-sim
cargo xtask run esp32-p4-sim
cargo xtask snapshot esp32-p4-sim
```
