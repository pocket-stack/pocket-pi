# Pocket Pi architecture

Pocket Pi is one runtime family with two agent profiles and three hosts.

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
  reader, system status and optional product projections. The host supplies the
  mounted workspace root.
- `crates/pocket-pi-protocols` owns model/provider transport protocols.
- Each host is a composition root. It connects the embedded Agent, shared UI,
  filesystem, input, display and model adapter.

Dependencies point inward: hosts depend on the runtime, UI and protocols. The
runtime and UI do not depend on a host. External products can populate a UI
projection or register a native tool without putting provider clients in core.

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

The Robinhood-shaped tab in the shared UI is only a projection slot retained to
match the current device UI. Pocket Pi contains no Robinhood client, credentials
or trading tools.

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
