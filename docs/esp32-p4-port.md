# ESP32-P4 host

Pocket Pi has two agent profiles and three hosts:

| Target | Agent profile | Runs on |
|---|---|---|
| `macos` | full `pi-coding-agent` | macOS |
| `esp32-p4` | bounded `pi-agent-core` | ESP32-P4 |
| `esp32-p4-sim` | bounded `pi-agent-core` | macOS |

The physical host and simulator use the same PocketJS App bundles and
`pocket-pi-agentos` supervisor at a 720x1280 logical viewport. Simulator mouse
clicks and physical touch coordinates are both dispatched to the selected App
View. The Pi Agent Root View displays Chat, Apps, Files and Settings; there is
no parallel Rust product UI.

The physical host selects `UartBackend` for a Mac Codex/Claude Code bridge or
`WirelessBackend` for direct OpenAI/OpenRouter/Anthropic HTTPS. These remain
host adapters; they do not belong in the UI or embedded Agent core.

```sh
cargo xtask build macos
cargo xtask build esp32-p4
cargo xtask build esp32-p4-sim
cargo xtask run esp32-p4-sim
cargo xtask snapshot esp32-p4-sim
```

## Simulator model backends

The simulator always runs the embedded `pi-agent-core` profile and defaults to
the Mac's local Codex Coding Plan login. `--backend` only chooses how its native
host fulfills a model request.

```sh
# Direct OpenAI request
OPENAI_API_KEY=... cargo xtask run esp32-p4-sim \
  --backend openai --model gpt-5.6

# Local Codex using the Mac's existing Coding Plan login
cargo xtask run esp32-p4-sim --backend codex

# Other direct providers
OPENROUTER_API_KEY=... cargo xtask run esp32-p4-sim \
  --backend openrouter --model openai/gpt-5.6
ANTHROPIC_API_KEY=... cargo xtask run esp32-p4-sim \
  --backend anthropic --model claude-sonnet-4-6

# Real Agent -> native write tool -> simulated LittleFS workspace
cargo xtask run esp32-p4-sim --backend codex \
  --workspace target/esp32-workspace
```

The Mac simulator and physical firmware register the same core tool contracts:
`read/write/edit/find/grep/ls`, bounded `bash`, `device.status`,
`time.now`, `workspace.context`, and the four `schedule.*` operations.
The Pi runtime obtains these definitions directly from the executable tool
registry, so advertised and executable tools cannot drift.

The simulator is a contract-level product simulator, not a CPU or peripheral
emulator. It must exercise the embedded Agent, tools, workspace, schedules and
plugin contracts, but may use simpler macOS adapters. ESP-IDF, PSRAM, PPA,
LittleFS capacity, MIPI-DSI, touch-controller and Wi-Fi/NVS behavior require a
physical-board test.

## Physical UART bridge

The thin bridge CLI provisions the boot-time model choice and routes framed
model decisions. Its `uart_bridge` adapters reuse a logged-in Codex Coding Plan
through the persistent Codex app-server, or Claude Code through `stream-json`.
Both paths forward real text deltas instead of waiting for the whole reply.
Wi-Fi can be selected later from the on-device Settings page.

```sh
tools/uart-model-bridge.py /dev/cu.usbserial-... --backend uart --provider codex

# Submit one boot-time prompt for a repeatable physical E2E test
tools/uart-model-bridge.py /dev/cu.usbserial-... --backend uart \
  --provider codex --prompt 'Use write, read, schedule.set and schedule.list.'

tools/uart-model-bridge.py /dev/cu.usbserial-... --backend wireless \
  --provider openai --model gpt-5-mini --provision-wifi
```

The Settings page scans visible networks, opens the shared PocketJS keyboard
for secured-network passwords, connects, forgets credentials, and requests a
restart. The ESP host persists only Wi-Fi SSID/password in NVS. Model API keys
arrive through provisioning and remain outside UI projections.

UART provisioning also seeds the board clock from the Mac for development.
Standalone operation uses ESP-IDF SNTP after Wi-Fi connects. Both feed the same
native time and persistent schedule implementation.

## App UI boundary

`apps/pi-agent`, `apps/robinhood`, and `apps/exa` own their PocketJS Views.
`pocket-pi-agentos` selects the foreground View and keeps the Pi Agent System
App resident. Each host supplies the mounted workspace, capabilities, model
adapter, and renderer; product UI and domain logic do not live in Rust firmware.
