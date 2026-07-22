# Pocket Pi

**Run the unmodified [pi](https://github.com/badlogic/pi-mono) coding-agent — and its extensions — inside QuickJS. No Node, no bun.**

Pocket Pi is a small Rust runtime that embeds QuickJS and gives JavaScript exactly
enough of a Node/Web platform — a module system, the `node:` builtins, `fetch`,
and a native TypeScript loader — that the **whole, unmodified `pi-coding-agent`
runs on it**, drives real LLM turns with tools, loads real extensions, and
persists sessions. It is the substrate the
[`cat`](https://github.com/paperboytm/cat-poc) desktop-assistant harness runs on,
and the sibling of [PocketJS](https://github.com/pocket-stack/pocketjs): where
PocketJS proved a *UI* runtime can live outside the browser under a tiny budget,
Pocket Pi does the same for an *agent* runtime.

```
┌────────────────────────── PiRuntime (one QuickJS realm, one thread) ──────────────────────────┐
│  prelude.js        timers · AbortController · fetch/Response/ReadableStream · Blob/FormData     │
│  node: builtins    fs · path · child_process · process · buffer · events · stream · … (JS)      │
│  pi-full.bundle    the UNMODIFIED pi-coding-agent, esbuild-bundled to one ES module             │
│  your extension    a .ts factory, transpiled by oxc at load — NOT jiti                          │
│        │  __node (resolve/fs/spawn)   host.http   host.transpile   host.tool   host.emit        │
│  ──────┼─────────────────────────────────────────────────────────────────────────────────────  │
│ native │  NodeResolver/NodeLoader  ·  oxc TS→JS  ·  fs & subprocess ops  ·  HTTP hub (TLS+SSE)  │
│ pump() │  timers → deliver fetch/LLM chunks → drain job queue → flush events → your callback    │
└────────┴───────────────────────────────────────────────────────────────────────────────────────┘
```

## What works today

- **Unmodified `pi-coding-agent` runs end-to-end** on QuickJS — a real gpt-5.6
  turn streams tokens and completes, tools and all. No source edits to pi; it's a
  real npm dependency synced with `npm update`.
- **Extensions load through Pocket Pi's own loader** (oxc TypeScript transpile,
  **no jiti**). A normal pi extension registers tools and lifecycle hooks, and the
  agent can call those tools in a live turn.
- **Sessions persist and resume** — pi's `SessionManager` reads/writes `.jsonl`
  through the runtime's `fs` builtin.
- **A near-complete Node/Web platform:** a Node resolver/loader (relative,
  `node_modules`, `exports`/`imports`, `.ts` on the fly), CommonJS interop, ~30
  `node:` builtins, and WHATWG `fetch`/`Response`/`ReadableStream`/`Headers`/`URL`
  backed by a native, proxy-aware HTTP hub.
- **A coalesced frame scheduler** — an agent spends almost all its time waiting on
  the model, so the host drives work in `pump()` frames and can run as slow as
  **2 Hz** while a turn streams, at near-zero idle CPU. See [ARCHITECTURE.md](ARCHITECTURE.md).
- **CI**: clippy (`-D warnings`) + build + test on every push and PR.

---

## Writing an extension

An extension for Pocket Pi is **the same file an unmodified pi extension is**: a
module whose default export is a factory `(pi) => void` (or async). Pocket Pi
loads it through its own module system, so the file is written in TypeScript and
may `import` `node:` builtins, relative modules, and npm packages.

```ts
// my-extension.ts
export default (pi) => {
  pi.registerTool({
    name: "echo",
    description: "Echo the given text back.",
    parameters: {
      type: "object",
      properties: { text: { type: "string" } },
      required: ["text"],
    },
    // pi calls execute(toolCallId, input, signal, onUpdate, ctx).
    // Return a result whose `content` is an array of content blocks.
    execute: async (_id, input) => ({
      content: [{ type: "text", text: String(input.text) }],
    }),
  });

  pi.on("agent_start", () => {
    /* lifecycle hook — fires when a turn begins */
  });

  // also available: pi.registerCommand / registerFlag / registerShortcut /
  // registerMessageRenderer / sendMessage / exec / getActiveTools / …
};
```

Pocket Pi loads it through pi's own `extensionFactories` seam — no jiti, no Node.
The host imports the file (which routes through the oxc loader) and hands the
factory to `createAgentSession`; the session's extension runner then exposes the
tool and hook. The runnable reference is [`js/pi-full/driver.js`](js/pi-full/driver.js):

```js
// the essence — see driver.js for the full session setup
const factory = (await import("/abs/path/my-extension.ts")).default; // ← oxc transpiles the .ts
const resourceLoader = new DefaultResourceLoader({
  cwd, agentDir, settingsManager,
  noExtensions: true,           // skip on-disk (jiti) discovery
  extensionFactories: [factory] // inject ours — loaded via loadExtensionFromFactory, no jiti
});
await resourceLoader.reload();
const { session } = await createAgentSession({ model, resourceLoader, tools: ["echo"], /* … */ });
```

### What an extension can call

Everything **import-resolves** (a bundled extension never crashes at load), but
the surface splits into real, partial, and stub:

| Module | Status |
|---|---|
| `fs`, `fs/promises` | **Real** (native) — read/write/append/readdir/mkdir/stat/exists/realpath/unlink, sync + callback + promise, plus `openSync`/`readSync` |
| `path`, `buffer`, `events`, `util`, `stream`, `string_decoder`, `os`, `url`, `querystring`, `assert`, `timers`, `readline`, `module`, `process` | **Real** JS implementations |
| `child_process` | **Real** `spawnSync` / `execSync` / `execFileSync` (native subprocess) |
| `crypto` | **Partial** — `randomUUID` (real host entropy), `randomBytes`/`getRandomValues` (Math.random), `createHash`/`createHmac` use **FNV** (fast, **non-cryptographic** — fine for cache keys/ids, not for security) |
| `http`, `https`, `net`, `tls`, `dns`, `zlib`, `vm`, `v8`, `worker_threads`, `async_hooks`, `perf_hooks`, `tty` | **Stub** — imports resolve; classic socket/server/client calls throw or no-op |

For **networking**, use the global `fetch()` — real, streaming, and proxy-aware
(routes through the native HTTP hub); `http.request`/`net.Socket` are stubbed
toward it. Also available as globals: `Response`, `Headers`, `URL`,
`URLSearchParams`, `ReadableStream`, `TextEncoder`/`TextDecoder`, `Blob`/`File`/
`FormData`, `AbortController`, `structuredClone`, `Buffer`, `setTimeout`/`setImmediate`.

**Bottom line:** an extension that reads/writes files, shells out, or calls an
HTTP API runs unmodified. Socket servers, native compression, and worker threads
do not yet — those builtins exist only so imports resolve. Adding a real one is
one row in `crates/pocket-pi/src/node/builtins.rs` (the single source of truth for
builtins) plus its JS shim.

---

## Embedding Pocket Pi (Rust API)

If you're building a host on top of Pocket Pi — like `cat` — this is your surface.
`PiRuntime` owns one QuickJS realm and is driven from a single thread; the host
owns the `pump()` cadence.

```rust
use pocket_pi::{PiRuntime, ToolResult};

let mut rt = PiRuntime::new()?;

// Native tools are Rust closures the agent calls by name.
rt.register_tool("current_time", |_args| ToolResult::text(now_unix().to_string()));
rt.on_event(|ev| println!("{}: {}", ev.kind, ev.raw));   // start / text / tool_* / end

rt.boot(r#"{"model":"gpt-5.6","apiKey":"…","systemPrompt":"Be terse."}"#)?;
rt.prompt("What time is it?")?;

// Pump at whatever cadence suits the host — 2 Hz is plenty while streaming.
while !rt.is_idle() {
    rt.pump()?;
    std::thread::sleep(std::time::Duration::from_millis(500));
}
```

There are two ways to run pi on this API:

- **The trimmed embeddable core** (default). `PiRuntime::new()` boots
  `agent.bundle.js` — pi's `Agent` loop with the provider layer replaced by native
  Rust `streamFn`s (Anthropic + OpenAI SSE). Drive it directly with
  `boot` / `prompt` / `register_tool` / `pump`. Smallest footprint; best when you
  want a lightweight agent and native tools, not pi's full CLI feature set.
- **The full, unmodified `pi-coding-agent`**. Load the bundle with
  `rt.run_module(".../pi-full.bundle.js")`, then drive `createAgentSession` — this
  is what unlocks extensions, session persistence, and pi's own tool suite.
  [`js/pi-full/driver.js`](js/pi-full/driver.js) is the reference harness.

Other `PiRuntime` methods: `run_module`, `eval_script`, `get_global_json`,
`load_plugin_ts` (native oxc transpile of an agent-authored TS plugin), `abort`,
`is_idle`.

---

## How pi runs unmodified

QuickJS's module linker **null-derefs** (`js_inner_module_linking`,
`quickjs.c:30806`) when you import `createAgentSession` and pull pi-coding-agent's
~500-module circular graph — an engine-level bug on graphs that large. The fix is
to hand QuickJS **one** module instead of five hundred: esbuild bundles the
*unmodified* pi source into a single ES module (only `node:*` left external), and
Pocket Pi's Node/Web layer satisfies it at runtime. Bundling isn't forking —
every dependency is the real, unmodified upstream package.

```sh
cd js && npm install
node build-pi-full.mjs     # → crates/pocket-pi/js/pi-full.bundle.js (~13 MB, git-ignored)
```

**Staying in sync with upstream pi** is `npm update` + rebuild — no patches to
carry. The bundle is git-ignored (a generated artifact), so the integration tests
that use it are `#[ignore]` and run locally; the crate itself builds with only
Rust.

---

## Footprint

The runtime is small and self-contained — QuickJS + pi's trimmed `Agent` core +
LLM streaming + the oxc TypeScript loader, with **nothing to `npm install` at the
destination**. A stripped release build of the example binary is **~7 MB**
(macOS arm64), dominated by:

- **oxc** (~1.6 MB) — the pure-Rust TypeScript transpiler, linked because
  extensions are authored in TypeScript and loaded at runtime.
- **rustls + ring** (~0.6 MB) — TLS for streaming HTTPS to the model.
- **regex** (~0.5 MB), QuickJS via `rquickjs` (~0.5 MB), and Rust `std`.

| Shipping pi as… | Size |
|---|---|
| **Pocket Pi** — single self-contained binary | **~7 MB** |
| `bun build --compile` (providers external; embeds JavaScriptCore) | ~61 MB |
| node runtime + `node_modules` (pi-agent-core + pi-ai + deps) | ~114 MB + ~131 MB |

Running the *full* unmodified pi currently adds the JS bundle, loaded at runtime
(a generated artifact, not a `node_modules` tree).

---

## Build & test

```sh
cargo test                     # unit + module-system suite (23 tests); no bundle needed
cargo clippy --workspace --all-targets -- -D warnings

# The bundle-backed integration tests are #[ignore] — build the bundle first:
cd js && npm install && node build-pi-full.mjs
cargo test -p pocket-pi loads_bundled_pi_coding_agent   -- --ignored   # bundle evaluates
cargo test -p pocket-pi binds_extension_into_session    -- --ignored   # extension binds to a session (offline)
cargo test -p pocket-pi persists_and_resumes_session    -- --ignored   # session round-trips to disk (offline)

# The turn tests need an API key + (here) a proxy:
https_proxy=http://127.0.0.1:7897 OPENAI_API_KEY=… \
  cargo test -p pocket-pi runs_bundled_pi_turn          -- --ignored --nocapture
https_proxy=http://127.0.0.1:7897 OPENAI_API_KEY=… \
  cargo test -p pocket-pi runs_pi_turn_with_extension_tool -- --ignored --nocapture
```

MIT.
