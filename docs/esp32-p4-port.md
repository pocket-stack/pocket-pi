# ESP32-P4 host

Pocket Pi has two agent profiles and three hosts:

| Target | Agent profile | Runs on |
|---|---|---|
| `macos` | full `pi-coding-agent` | macOS |
| `esp32-p4` | bounded `pi-agent-core` | ESP32-P4 |
| `esp32-p4-sim` | bounded `pi-agent-core` | macOS |

The ESP host and simulator consume the same embedded agent bundle, PocketJS
application bundle, application state protocol, and logical 360x640 viewport.
Only their platform adapters differ.

The physical host boots with a small offline model adapter so the firmware is
self-contained. OpenAI, OpenRouter, Anthropic or UART adapters are host choices;
they are not compiled into the UI or embedded agent core.

```sh
cargo xtask build macos
cargo xtask build esp32-p4
cargo xtask build esp32-p4-sim
cargo xtask run esp32-p4-sim
cargo xtask snapshot esp32-p4-sim
```

### Simulator model backends

The simulator always runs the embedded `pi-agent-core` profile. `--backend`
only selects the host transport used when that Agent asks for a model turn.

```sh
# Deterministic and offline (default)
cargo xtask run esp32-p4-sim --backend scripted

# OpenAI Chat Completions with the same SSE/tool protocol as the board
OPENAI_API_KEY=... cargo xtask run esp32-p4-sim \
  --backend openai --model gpt-5.6

# Local Codex CLI using the Mac's existing Coding Plan login
cargo xtask run esp32-p4-sim --backend codex
```

The physical host uses equivalent logical backends with different transports:
ESP-IDF HTTPS for OpenAI, and UART or Wi-Fi bridge transport for a Codex process
running on another machine. An ESP32 cannot launch the local `codex` executable.

The simulator is a product-level simulator, not a CPU emulator. It recompiles
the embedded Rust profile for macOS and substitutes host filesystem, input and
wgpu display adapters. ESP-IDF, PSRAM, PPA, MIPI-DSI, touch-controller and Wi-Fi
driver behavior still require a physical-board test.

## Shared UI contract

`pocket-pi-app-core` owns the versioned `AppSnapshot` and `AppCommand` wire
types. `apps/agent-shell` receives snapshots and emits commands over the
PocketJS service mailbox. The UI has no filesystem, model, network or secret
access of its own.

The first shared UI contains Chat and Workspace. Provider-specific product
features do not belong in Pocket Pi core.
