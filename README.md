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

The bundle is pi's `Agent` + our Anthropic/OpenAI `streamFn`s + an offline
scripted assistant. pi-ai (its ~5 MB of provider SDKs, TypeBox, and a 553 KB
model table) is replaced at bundle time by `js/src/pi-ai-stub.js`, which
reimplements the four symbols the core actually imports.

## Staying in sync with upstream pi

The concern this design takes seriously: **don't fork pi.** `pi-agent-core` — the
loop that receives upstream feature updates — is a **real, unmodified npm
dependency** (`js/package.json`), byte-identical to what `npm install` fetches.
Syncing a new pi release is:

```sh
cd js && npm update @mariozechner/pi-agent-core && node build.mjs
```

The only deliberate substitution is the *provider layer* (`pi-ai`), replaced by a
four-symbol stub plus native-Rust `streamFn`s — that's the "heavy Node dep →
native op" rule, not a fork of pi's logic. (Upstream has since moved to the
`@earendil-works` scope; following it is a one-line dependency bump.)

## Plugins: agent-authored TypeScript, loaded at runtime

Real pi loads extensions — **TypeScript files** — at runtime via `jiti` (a Node
toolchain). Pocket Pi has no Node, so it does the same thing with a **native
Rust transpiler**: [`oxc`](https://oxc.rs) strips the types and QuickJS evaluates
the result. The agent can write and install its own tools mid-session:

```rust
rt.load_plugin_ts("adder", r#"
    export default (api) => api.addTool({
        name: "add", description: "add two numbers",
        parameters: { type:"object", properties:{ a:{type:"number"}, b:{type:"number"} }, required:["a","b"] },
        execute: (args: { a: number; b: number }): string => String(args.a + args.b),
    });
"#)?;
```

Boot with `"selfExtend": true` and the agent gets a `define_plugin` tool, so it
can author a `.ts` plugin itself; the tools it registers are callable on its next
turn (tools are read at run start, matching pi's extension model). A plugin is a
single file that `export default`s a factory `(api) => void`; `api.addTool`,
`api.onEvent`, `api.callHostTool`, and `api.log` are the current surface (a lean
subset of pi's `ExtensionAPI` — the "Option A" scope).

## Footprint vs shipping pi on bun / node

A standalone Pocket Pi binary **includes** pi's core, LLM streaming, **and** the
TypeScript transpiler for plugins — where the bun/node path gets runtime TS from
`jiti` + `typescript` inside a heavy `node_modules`:

| Shipping pi as… | Size |
|---|---|
| **Pocket Pi** — single self-contained binary (QuickJS + pi core + streaming + oxc TS) | **5.9 MB** |
| `bun build --compile` (providers external; embeds JavaScriptCore) | 61 MB |
| node runtime + `node_modules` (pi-agent-core + pi-ai + deps, 106 pkgs) | 114 MB + 131 MB |

~**10× smaller than a bun-compiled pi, ~40× smaller than node + node_modules** —
and nothing to `npm install` at the destination.

## Status

- ✅ pi `Agent` core boots and drives multi-turn tool loops inside QuickJS.
- ✅ Anthropic streaming `streamFn` (raw SSE, `input_json_delta` tool streaming,
  image tool-results) — wired and unit-tested against the scripted path; the live
  path compiles and is covered by an env-gated test.
- ✅ OpenAI provider too (Chat Completions streaming, gpt-5.x); system proxy.
- ✅ Coalesced 2 Hz+ frame scheduler; tools round-trip through native Rust.
- ✅ Agent-authored **TypeScript plugins** loaded at runtime (oxc transpiler),
  including agent self-extension via `define_plugin`.
- 🚧 **Node-compat runtime (in progress)** — the path to running *unmodified*
  `pi-coding-agent` and its real extensions.
  - **M1 — module system:** rquickjs resolver/loader; relative + `node_modules`
    (exports/module/main) + `node:` builtins; on-the-fly `.ts` transpile;
    builtins `path`/`os`/`fs`/`events`/`util`/`buffer`/`process` over native ops.
  - **M2 — Web globals + real pi code:** WHATWG `fetch`→`Response`→`ReadableStream`
    (native raw-HTTP backing, driven by the frame pump), `Headers`, `URL`,
    `TextEncoder`/`TextDecoder`, `atob`/`btoa`, `crypto.getRandomValues` — proven
    against a live endpoint. And **real, unmodified pi-ai code loads and runs**
    from `node_modules` through the loader.
  - **M3 — CommonJS interop + full builtin surface.** The transitive tree has
    ~90 CJS-only packages (chalk's `ansi-styles`/`color-convert`, `debug`, `yaml`,
    `cli-highlight`, …), so CJS *is* required: a synchronous `require`, a
    cjs-module-lexer for named exports, and an ESM bridge (CJS wrapped as ESM).
    Plus `child_process`/`crypto`/`url`/`module`/`stream`/`string_decoder`/
    `readline`/`perf_hooks`/`tty`/`fs/promises`/`stream/promises` and a broad `fs`
    surface; subpath (`exports["./x"]`) and internal (`#x`) resolution. The loader
    now clears the *entire* Node + CJS dependency surface of pi-coding-agent.
  - **M4 — ESM cycle rewrite:** indirect re-exports (`export { x } from "./y"`)
    are rewritten to `import { x as _local } from "./y"; export { _local as x }`,
    which sidesteps QuickJS's "circular reference" on pi-coding-agent's cyclic
    graph. This lets it link *hundreds* of real pi modules.
  - **Now blocked on:** importing `createAgentSession` pulls pi-coding-agent's
    entire ~500-module graph (including the interactive TUI), and QuickJS's module
    linker **null-derefs** (`js_inner_module_linking`, `quickjs.c:30806`) on a
    circular structure that large — an engine-level bug. Two paths under
    evaluation: patch/upgrade the bundled QuickJS, or **build-time bundle** the
    unmodified pi source into one module (sidesteps the multi-module linker,
    reuses this whole Node-compat runtime), with extensions still runtime-loaded.
- ⛔ Not hardened: two providers, permissive tool-arg validation, no session
  persistence/compaction, and the plugin API is a lean subset of pi's
  `ExtensionAPI` (single-file plugins, no value imports). A PoC substrate, not
  pi's full CLI.

MIT.
