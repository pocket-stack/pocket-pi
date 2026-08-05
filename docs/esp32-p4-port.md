# ESP32-P4 host

Pocket Pi has two agent profiles and three hosts:

| Target | Agent profile | Runs on |
|---|---|---|
| `macos` | full `pi-coding-agent` | macOS |
| `esp32-p4` | bounded `pi-agent-core` | ESP32-P4 |
| `esp32-p4-sim` | bounded `pi-agent-core` | macOS |

The physical host and simulator use the exact same
`pocket-pi-device-ui` crate: one 720x1280 draw list, one set of fonts, and one
touch hit map. Simulator mouse clicks are converted to physical panel
coordinates and dispatched through the same `ScreenState::handle_tap` method.
Without a portfolio plugin the shared embedded UI displays Chat, Files and
Settings. Settings is not part of the normal macOS host.

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

The simulator always runs the embedded `pi-agent-core` profile. `--backend`
only chooses how its native host fulfills a model request.

```sh
# Deterministic and offline (default)
cargo xtask run esp32-p4-sim --backend scripted

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

The simulator is a product-level simulator, not a CPU emulator. ESP-IDF, PSRAM,
PPA, MIPI-DSI, touch-controller and Wi-Fi driver behavior still require a
physical-board test.

## Physical UART bridge

The bridge provisions the boot-time model choice and serves framed model
decisions. Wi-Fi can be selected later from the on-device Settings page.

```sh
tools/uart-model-bridge.py /dev/cu.usbserial-... --backend uart --provider codex

tools/uart-model-bridge.py /dev/cu.usbserial-... --backend wireless \
  --provider openai --model gpt-5-mini --provision-wifi
```

The Settings page scans visible networks, opens the shared PocketJS keyboard
for secured-network passwords, connects, forgets credentials, and requests a
restart. The ESP host persists only Wi-Fi SSID/password in NVS. Model API keys
arrive through provisioning and remain outside UI projections.

## Shared UI boundary

`pocket-pi-device-ui` owns rendering, interaction state, the portable
workspace browser and data projections. Each host supplies a mounted workspace
root and its model adapter. External plugins own provider clients and
credentials. The Robinhood-shaped screen is retained solely as an optional
projection slot; it is hidden until the host enables that capability. This
repository does not contain Robinhood networking or trading logic.
