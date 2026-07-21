# Pocket Pi

**Run the [pi](https://github.com/badlogic/pi-mono) coding-agent core inside QuickJS — no Node, no bun — on a PocketJS-style coalesced frame scheduler.**

Pocket Pi is a small Rust runtime that embeds QuickJS, evaluates pi's embeddable
`Agent` core (`@mariozechner/pi-agent-core`), and gives it exactly the native
capabilities an agent needs and nothing more: streaming HTTPS to an LLM, a tool
bridge back to native code, and a heartbeat. It is the substrate the
[`cat`](https://github.com/paperboytm/cat-poc) desktop-assistant harness runs on,
and it is the sibling of [PocketJS](https://github.com/pocket-stack/pocketjs):
where PocketJS proved a *UI* runtime can live outside the browser under an 8 MB
budget, Pocket Pi does the same for an *agent* runtime.

```
┌──────────────────────────── PiRuntime (one thread) ────────────────────────────┐
│  QuickJS realm                                                                  │
│    prelude.js         timers · AbortController · structuredClone · crypto       │
│    agent.bundle.js    pi Agent + loop  (44 KB — pi-ai's 5 MB stripped away)     │
│    anthropic-stream.js  Anthropic SSE → pi events, off the HTTP mailbox         │
│         │  host.http.start/drain/cancel   host.tool   host.emit                 │
│  ───────┼──────────────────────────────────────────────────────────────────    │
│  native │  HttpHub  ──►  background thread per turn: TLS POST + SSE read        │
│         │  tool registry (Rust closures)     event flush → your callback        │
│  pump() │  timers → deliver LLM events → drain job queue → flush events         │
└─────────┴───────────────────────────────────────────────────────────────────────┘
```

## Why

pi is a clean, hang-resistant agent loop, but it ships as a Node/bun program
with a heavy dependency graph (provider SDKs, a 553 KB model table, terminal UI,
`node:child_process`, …). To put an agent inside a low-resource native product —
a desktop widget, an embedded device, anywhere without a JS server — none of that
belongs. The recon behind this runtime found that pi's **agent loop is only
~43 KB of pure JS** and touches almost no globals; everything heavy is the CLI,
the TUI, and the provider layer. Pocket Pi keeps the loop and replaces the rest:

- **No Node, no bun at runtime.** Just QuickJS (via `rquickjs`) and Rust.
- **One native op that matters:** streaming HTTPS. Everything else pi needs
  (`TextDecoder`, `fetch`, `ReadableStream`) is sidestepped by parsing SSE in
  Rust and handing pi complete events.
- **Tools are Rust.** The guest realm has no filesystem and no network except the
  vetted `host.http` op; a tool is a Rust closure the agent calls by name. Heavy
  Node tool-deps become native ops instead of npm packages.
- **The scheduler is the point** — see [ARCHITECTURE.md](ARCHITECTURE.md).

## The frame scheduler

An agent spends almost all of its wall-clock waiting on the model. Pocket Pi
borrows PocketJS's **demand-render** governor: work happens in discrete `pump()`
frames, and each frame only does something if there is something to do (an LLM
chunk arrived, a timer is due, a promise can resolve). Because LLM latency
(seconds) dwarfs a frame period, the host can pump as slowly as **2 Hz** while a
turn is in flight and lose nothing — the runtime coalesces to near-zero CPU
between token bursts. A headless agent doesn't need a hot loop; it needs a
heartbeat. The verdict, with measurements, is in [ARCHITECTURE.md](ARCHITECTURE.md).

## Use it

```rust
use pocket_pi::{PiRuntime, ToolResult};

let mut rt = PiRuntime::new()?;
rt.register_tool("current_time", |_args| {
    ToolResult::text(format!("{}", now_unix()))
});
rt.on_event(|ev| println!("{}: {}", ev.kind, ev.raw));   // start/text/tool_*/end
rt.boot(r#"{"model":"claude-opus-4-8","apiKey":"…","systemPrompt":"Be terse."}"#)?;
rt.prompt("What time is it?")?;

// Pump at whatever cadence suits the host — 2 Hz is plenty while streaming.
while !rt.is_idle() {
    rt.pump()?;
    std::thread::sleep(std::time::Duration::from_millis(500));
}
```

Run the demo (offline scripted answer with no key, real Anthropic stream with one):

```sh
cargo test                                   # scripted turn + native tool round-trip
ANTHROPIC_API_KEY=… cargo test -- live       # exercises the real streaming path
cargo run -p pocket-pi --example chat -- "Say hi in three words."
```

## Building the guest bundle

The JS guest is pre-bundled into `crates/pocket-pi/js/agent.bundle.js` and
embedded with `include_str!`, so **building the crate needs only Rust** — no npm,
no Node at build time either. To regenerate the bundle after editing `js/src/`:

```sh
node js/build.mjs      # shells esbuild via npx; aliases pi-ai to a 4-symbol stub
```

The bundle is pi's `Agent` + our Anthropic `streamFn` + an offline scripted
assistant. pi-ai (its ~5 MB of provider SDKs, TypeBox, and a 553 KB model table)
is replaced at bundle time by `js/src/pi-ai-stub.js`, which reimplements the four
symbols the core actually imports.

## Status

- ✅ pi `Agent` core boots and drives multi-turn tool loops inside QuickJS.
- ✅ Anthropic streaming `streamFn` (raw SSE, `input_json_delta` tool streaming,
  image tool-results) — wired and unit-tested against the scripted path; the live
  path compiles and is covered by an env-gated test.
- ✅ Coalesced 2 Hz+ frame scheduler; tools round-trip through native Rust.
- ⛔ Not hardened: single provider (Anthropic), permissive tool-arg validation,
  no session persistence/compaction. This is a PoC substrate, not pi's full CLI.

MIT.
