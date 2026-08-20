# ESP32-P4 reference target

ESP32-P4 is Pocket Pi's first fully supported hardware target and current
reference implementation. It demonstrates the complete device runtime:
resident Pi Agent, `/workspace`, native tools, schedules, Agent-native Apps,
local state, PocketJS UI and native device lifecycle.

The repository also provides one companion development composition:

| Role | Implementation | Runs on |
|---|---|---|
| Supported hardware target | `firmware/esp32-p4` | ESP32-P4 |
| Product-contract simulator | `hosts/esp32-p4-sim` | macOS development host |

The physical host and simulator use the same ordinary App source and
`pocket-pi-agentos` supervisor. The ESP32-P4 host supplies its 720x1280 logical
viewport; the simulator defaults to that value and accepts `--viewport
WIDTHxHEIGHT` for another screen shape. Simulator mouse clicks and physical
touch coordinates are both dispatched in the selected View's logical viewport.
The Pi Agent Root View displays Chat, Apps, Files and Settings; there is no
parallel Rust product UI. The simulator is not a desktop Pocket Pi product, a
generic Agent harness or a second supported target.

The physical host selects `UartBackend` for a Mac Codex/Claude Code bridge or
`WirelessBackend` for direct OpenAI/OpenRouter/Anthropic/DeepSeek HTTPS. These remain
host adapters; they do not belong in the UI or embedded Agent core.

The ESP host exposes HTTP and UART ingress for ordinary App packages. Both
adapters only receive a complete `.pocketapp` and hand it to the shared Installer
in `pocket-pi-agentos`; neither writes App storage, credentials or runtime state
itself.

```sh
cargo xtask build esp32-p4
cargo xtask build esp32-p4-sim
cargo xtask run esp32-p4-sim
cargo xtask snapshot esp32-p4-sim
```

Use the simulator to exercise another logical display without forking an App:

```sh
cargo xtask run esp32-p4-sim --viewport 800x480
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
DEEPSEEK_API_KEY=... DEEPSEEK_THINKING_LEVEL=xhigh \
  cargo xtask run esp32-p4-sim --backend deepseek

# Real Agent -> native write tool -> simulated LittleFS workspace
cargo xtask run esp32-p4-sim --backend codex \
  --workspace target/esp32-workspace
```

The development simulator and physical firmware register the same core tool contracts:
`read/write/edit/find/grep/ls`, bounded `bash`, `device.status`,
`time.now`, `workspace.context`, and the four `schedule.*` operations.
The Pi runtime obtains these definitions directly from the executable tool
registry, so advertised and executable tools cannot drift.

The simulator is a contract-level development tool, not a CPU or peripheral
emulator. It must exercise the embedded Agent, tools, workspace, schedules and
App contracts, but may use simpler macOS adapters. ESP-IDF, PSRAM, PPA,
LittleFS capacity, MIPI-DSI, touch-controller and Wi-Fi/NVS behavior require a
physical-board test.

## Physical provisioning, install and diagnostics

The physical board is provisioned once with a standalone wireless model backend.
Model provider, model, thinking level and API key are stored in native NVS;
Wi-Fi SSID/password use the existing Wi-Fi NVS store. Normal boots load those
values directly and do not wait for UART.

```sh
tools/uart-provision.py /dev/cu.usbserial-... \
  --provider deepseek --provision-wifi

tools/uart-install.py /dev/cu.usbserial-... \
  target/pocketapps/exa.pocketapp
```

The App uploader does not reset the board or configure a model. It only transfers
one complete package to the same Installer used by HTTP. Confirm installation
on the device before opening `espflash monitor`: on this board's WCH USB bridge,
the monitor's DTR/RTS sequence can reset the board and discard a pending review.
For boot-only diagnostics, restore a normal boot after exiting the monitor:

```sh
espflash monitor --port /dev/cu.usbserial-...
espflash reset --port /dev/cu.usbserial-... --non-interactive
```

For unprovisioned development-board bring-up, the optional model bridge can
still route requests
to a logged-in Mac Codex or Claude Code without persisting that backend:

All UART CLIs leave DTR/RTS inactive before closing the port so the WCH USB
serial bridge does not reset a running board.

```sh
tools/uart-model-bridge.py /dev/cu.usbserial-... --provider codex \
  --prompt 'Use write, read, schedule.set and schedule.list.'
```

The Settings page scans visible networks, opens the shared PocketJS keyboard
for secured-network passwords, connects, forgets credentials, and requests a
restart. Model and Wi-Fi credentials remain outside Agent workspace and UI
projections.

UART provisioning also seeds the board clock from the Mac for development.
Standalone operation uses ESP-IDF SNTP after Wi-Fi connects. Both feed the same
native time and persistent schedule implementation.

## App UI boundary

`apps/pi-agent`, `apps/robinhood`, and `apps/exa` own their PocketJS Views.
`pocket-pi-agentos` selects the foreground View and keeps the Pi Agent System
App resident. Each host supplies the mounted workspace, capabilities, model
adapter, and renderer; product UI and domain logic do not live in Rust firmware.

## Porting another display or board

A new board port has three responsibilities:

1. implement its display scanout, touch input and other board adapters in a new
   firmware host;
2. pass the same logical `Viewport` to `AppSupervisor`, the renderer/framebuffer
   and touch-coordinate mapping;
3. validate the existing App sources in the simulator at that viewport, then
   repeat boot, scanout, touch and memory checks on the physical board.

The runtime reports that viewport to every View Guest, and the shared View SDK
turns it into `View.viewport`. The current Apps use orientation to select a
portrait stack or landscape columns and use viewport-derived visible row
counts. Shared components own reusable geometry such as headers, navigation,
keyboard and chart drawing. A board port must not add a board-specific App
fork or a second Rust UI.

The exact ESP32-S3 firmware work still depends on the selected module, panel
bus/controller, touch controller, PSRAM and flash layout. Those are host adapter
inputs; they do not change the Viewport or App contract above.
