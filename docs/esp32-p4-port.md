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

The physical host currently boots with a small offline model adapter so the
firmware remains self-contained. OpenAI, OpenRouter, Anthropic and UART are host
adapter choices; they do not belong in the UI or embedded Agent core.

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
```

The simulator is a product-level simulator, not a CPU emulator. ESP-IDF, PSRAM,
PPA, MIPI-DSI, touch-controller and Wi-Fi driver behavior still require a
physical-board test.

## Shared UI boundary

`pocket-pi-device-ui` owns rendering, interaction state, the portable
workspace browser and data projections. Each host supplies a mounted workspace
root and its model adapter. External plugins own provider clients and
credentials. The Robinhood-shaped screen is retained solely as an optional
projection slot so the shared UI matches the current ESP32 device; this
repository does not contain Robinhood networking or trading logic.
