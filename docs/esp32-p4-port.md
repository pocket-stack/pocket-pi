# ESP32-P4 port

This branch treats the P4 device as a durable appliance rather than a tiny
desktop. The existing desktop `PiRuntime` remains intact while an embedded
profile is built from the parts that fit and are safe on-device.

## Why the existing runtime cannot simply be cross-compiled

The current runtime loads the full unmodified coding-agent bundle into QuickJS.
On a desktop diagnostic build, a freshly booted runtime used about 46.6 MB of
QuickJS-managed memory and about 55.7 MB of allocator memory before doing a
model turn. It also relies on desktop facilities including subprocesses, a
filesystem-backed Node resolver, `ureq`/Rustls, and on-device TypeScript
transpilation. Those are incompatible with a reliable 24/7 P4 appliance even on
a board fitted with PSRAM.

The embedded profile therefore keeps the agent state machine, streaming model
transport, bounded tool calls, durable session projection, and UI events. It
does not ship the desktop TUI, Bash tool, arbitrary Node module loading, or the
on-device TypeScript compiler.

## Hardware profile

The attached board was identified from its original firmware image and checked
against the vendor examples:

- Waveshare ESP32-P4-WIFI6-Touch-LCD-5;
- ESP32-P4 revision 1.3, 32 MB flash and 32 MB external PSRAM;
- 720 x 1280 HX8394 panel over two-lane MIPI-DSI, targeting RGB565;
- GT911 capacitive touch controller; and
- ESP32-C6 Wi-Fi coprocessor over SDIO using `esp_wifi_remote` and
  `esp_hosted`.

The first board-specific firmware milestone enables the 200 MHz PSRAM and
probes its total/free heap before enabling either high-bandwidth peripheral.
The vendor BSP is the source of truth for panel timing and pins.

## Hardware discovery gate

ESP32-P4 does not contain a Wi-Fi radio. A P4 development board advertised with
Wi-Fi uses a companion radio, commonly an ESP32-C6, connected over SDIO, SPI or
UART. The exact board profile must define:

- flash and PSRAM size/mode;
- Wi-Fi companion model, transport, reset pin and handshake pins;
- display controller, resolution, pixel bus and backlight pin;
- touch controller and I2C pins, if present;
- native USB or USB-to-UART programming path.

Until that profile is selected, the first firmware deliberately builds only a
serial boot/memory/PSRAM probe. It must not guess pins.

## Frontend runtime

PocketJS already provides a dedicated ESP32-P4 RGB565 renderer at
`engine/backends/esp32p4-ppa` plus a reusable ESP-IDF PPA component. The
firmware pins PocketJS commit `4c5dc9e` and cross-compiles its retained UI core
and portable P4 renderer now. The boot probe constructs the real renderer, not
a local mock.

The hardware adapter is deliberately still disabled. PocketJS leaves display
initialization, DMA framebuffer allocation, panel rotation and presentation to
the product BSP, and its PPA C adapter is supported on ESP-IDF 6.0 or newer.
After board identification, the firmware will move to that IDF baseline, link
the adapter, and select the panel BSP in the same change.

## Security and trading rollout

1. Boot probe and display status with networking disabled.
2. Wi-Fi companion connectivity and verified HTTPS.
3. Codex auth experiment. Show `Coding Plan` and `API key` as distinct modes.
4. Robinhood Agentic MCP OAuth with read-only portfolio synchronization.
5. Paper trading or confirmation-gated order intents.
6. Live orders only after hard limits, idempotency, an on-device kill switch,
   and recovery tests are verified.

Secrets are provisioned after flashing and stored in encrypted NVS. They are
never committed, embedded with `env!`, or printed in serial logs.

The Coding Plan experiment follows Pocket Pi's existing Pi dependency: Pi
0.81.1 exposes an `openai-codex` headless device-code login for ChatGPT Plus/Pro.
The P4 port will reimplement only its device authorization, PKCE, refresh, and
SSE request path in Rust using the hardware RNG and ESP-IDF TLS. It will not
bring the Node-only OAuth server or the full provider SDK onto the device. API
key mode targets the public OpenAI API and remains an explicit fallback.

Robinhood uses Streamable HTTP MCP at
`https://agent.robinhood.com/mcp/trading`. Its published authorization metadata
advertises Authorization Code with PKCE S256, refresh tokens, and dynamic client
registration. Initial Agentic account onboarding must be completed in a desktop
browser. The firmware starts by exposing only `get_accounts` and `get_portfolio`;
order tools stay behind the device-enforced `TradingPolicy`, whose default is
read-only.

## Build

The firmware is an independent ESP-IDF workspace so desktop CI remains
unchanged:

```sh
cd firmware/esp32-p4
cargo build --release
```

Once the board enumerates over USB:

```sh
cargo run --release
```

The configured runner invokes `espflash flash --monitor`.

The firmware pins its Rust nightly. Update that pin only together with a clean
P4 build; upstream nightly `std` and ESP-IDF's libc do not always move in lockstep.
