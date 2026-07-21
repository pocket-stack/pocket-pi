use super::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Pump until the runtime goes idle or a frame budget is hit. Returns the
/// collected host events. `hz` is the pump cadence — we deliberately run slow to
/// prove the coalesced scheduler carries a real turn to completion.
fn run(rt: &mut PiRuntime, sink: Rc<RefCell<Vec<HostEvent>>>, hz: f64, max_secs: f64) {
    let period = Duration::from_secs_f64(1.0 / hz);
    let start = Instant::now();
    while !rt.is_idle() && start.elapsed().as_secs_f64() < max_secs {
        rt.pump().unwrap();
        std::thread::sleep(period);
    }
    // One last pump to flush any trailing events.
    rt.pump().unwrap();
    let _ = &sink;
}

fn collector() -> (Rc<RefCell<Vec<HostEvent>>>, impl FnMut(&HostEvent) + 'static) {
    let sink = Rc::new(RefCell::new(Vec::new()));
    let s = sink.clone();
    (sink, move |ev: &HostEvent| s.borrow_mut().push(ev.clone()))
}

#[test]
fn boots_the_pi_agent_core() {
    let (sink, cb) = collector();
    let mut rt = PiRuntime::new().expect("runtime");
    rt.on_event(cb);
    rt.boot(r#"{"model":"test","scripted":{"steps":[{"text":"hello"}]}}"#)
        .expect("boot");
    let kinds: Vec<String> = sink.borrow().iter().map(|e| e.kind.clone()).collect();
    assert!(kinds.contains(&"booted".to_string()), "got {kinds:?}");
}

#[test]
fn scripted_text_turn_completes() {
    let (sink, cb) = collector();
    let mut rt = PiRuntime::new().expect("runtime");
    rt.on_event(cb);
    rt.boot(r#"{"model":"test","scripted":{"steps":[{"text":"three word answer"}]}}"#)
        .expect("boot");
    rt.prompt("say something").expect("prompt");
    run(&mut rt, sink.clone(), 2.0, 5.0);

    let events = sink.borrow();
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    assert!(kinds.contains(&"start"), "no start in {kinds:?}");
    assert!(kinds.contains(&"assistant_text"), "no assistant_text in {kinds:?}");
    assert!(kinds.contains(&"end"), "no end in {kinds:?}");

    let text = events
        .iter()
        .find(|e| e.kind == "assistant_text")
        .and_then(|e| e.value.get("text").and_then(|v| v.as_str()))
        .unwrap_or("");
    assert_eq!(text, "three word answer");
    assert!(rt.is_idle());
}

#[test]
fn scripted_tool_call_round_trips_through_native_rust() {
    let (sink, cb) = collector();
    let mut rt = PiRuntime::new().expect("runtime");
    rt.on_event(cb);

    let tool_calls = Rc::new(RefCell::new(0u32));
    let tc = tool_calls.clone();
    rt.register_tool("get_secret", move |args| {
        *tc.borrow_mut() += 1;
        let who = args.get("who").and_then(|v| v.as_str()).unwrap_or("world");
        ToolResult::text(format!("secret for {who} is 42"))
    });

    // Turn 1: the model calls the tool. Turn 2 (loop re-drives): it answers.
    rt.boot(
        r#"{"model":"test","tools":[{"name":"get_secret","description":"get the secret",
        "parameters":{"type":"object","properties":{"who":{"type":"string"}}}}],
        "scripted":{"steps":[{"toolCall":{"name":"get_secret","arguments":{"who":"cat"}}},
        {"text":"the secret is 42"}]}}"#,
    )
    .expect("boot");
    rt.prompt("what is the secret for cat?").expect("prompt");
    run(&mut rt, sink.clone(), 2.0, 5.0);

    assert_eq!(*tool_calls.borrow(), 1, "tool should run exactly once");
    let events = sink.borrow();
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    assert!(kinds.contains(&"tool_start"), "no tool_start in {kinds:?}");
    assert!(kinds.contains(&"tool_end"), "no tool_end in {kinds:?}");
    assert!(kinds.contains(&"assistant_text"), "no assistant_text in {kinds:?}");
    assert!(kinds.contains(&"end"), "no end in {kinds:?}");
}

/// The Option-A capability: an agent-authored **TypeScript** plugin is
/// transpiled natively (oxc) and loaded into the live realm, registering a new
/// tool the agent then calls — no Node, no build step.
#[test]
fn loads_a_typescript_plugin_and_calls_its_tool() {
    let (sink, cb) = collector();
    let mut rt = PiRuntime::new().expect("runtime");
    rt.on_event(cb);

    // Boot with NO tools; the plugin adds one at runtime.
    rt.boot(
        r#"{"model":"test","scripted":{"steps":[
            {"toolCall":{"name":"add","arguments":{"a":2,"b":3}}},
            {"text":"the sum is 5"}
        ]}}"#,
    )
    .expect("boot");

    // A real TypeScript plugin: types, an interface, an arrow fn with a cast.
    let plugin_ts = r#"
interface AddArgs { a: number; b: number }
export default function (api: any): void {
    api.addTool({
        name: "add",
        description: "Add two numbers",
        parameters: {
            type: "object",
            properties: { a: { type: "number" }, b: { type: "number" } },
            required: ["a", "b"],
        },
        execute: (args: AddArgs): string => {
            const sum: number = (args.a as number) + (args.b as number);
            api.log("sum=" + sum);
            return String(sum);
        },
    });
}
"#;
    rt.load_plugin_ts("adder", plugin_ts).expect("load plugin");

    rt.prompt("add 2 and 3").expect("prompt");
    run(&mut rt, sink.clone(), 4.0, 5.0);

    let events = sink.borrow();
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    assert!(kinds.contains(&"plugin_loaded"), "plugin not loaded: {kinds:?}");
    assert!(kinds.contains(&"tool_start"), "tool not called: {kinds:?}");
    // The plugin's TS logic actually ran with the right args → sum=5.
    let logged = events.iter().any(|e| {
        e.kind == "plugin_log"
            && e.value.get("message").and_then(|v| v.as_str()) == Some("sum=5")
    });
    assert!(logged, "plugin tool didn't compute correctly: {kinds:?}");
}

/// Self-extension: the agent calls `define_plugin` to author a tool in
/// TypeScript, then calls that very tool on the next turn — pi's "the agent
/// extends its own harness" loop, on the no-Node runtime.
#[test]
fn agent_writes_and_uses_its_own_plugin() {
    let (sink, cb) = collector();
    let mut rt = PiRuntime::new().expect("runtime");
    rt.on_event(cb);

    // The agent's scripted turns: (1) write a plugin in TypeScript, (2) call it.
    let plugin_src = r#"
export default (api: any) => {
    api.addTool({
        name: "shout",
        description: "uppercase a word",
        parameters: { type: "object", properties: { word: { type: "string" } }, required: ["word"] },
        execute: (a: { word: string }): string => {
            const s: string = a.word.toUpperCase();
            api.log("shouted=" + s);
            return s;
        },
    });
};
"#;
    // A self-authored tool becomes available on the NEXT prompt (tools are read
    // at run start), matching pi's extension model — so: prompt 1 defines it,
    // prompt 2 uses it.
    let cfg = serde_json::json!({
        "model": "test",
        "selfExtend": true,
        "scripted": { "steps": [
            { "toolCall": { "name": "define_plugin",
                "arguments": { "name": "shouter", "typescript": plugin_src } } },
            { "text": "made a shout tool" },
            { "toolCall": { "name": "shout", "arguments": { "word": "meow" } } },
            { "text": "MEOW" }
        ]}
    });
    rt.boot(&cfg.to_string()).expect("boot");
    rt.prompt("make a shout tool").expect("prompt");
    run(&mut rt, sink.clone(), 4.0, 6.0);
    rt.prompt("now shout meow").expect("prompt");
    run(&mut rt, sink.clone(), 4.0, 6.0);

    let events = sink.borrow();
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    assert!(kinds.contains(&"plugin_loaded"), "plugin not loaded: {kinds:?}");
    // The self-authored tool actually ran with the right arg.
    let shouted = events.iter().any(|e| {
        e.kind == "plugin_log"
            && e.value.get("message").and_then(|v| v.as_str()) == Some("shouted=MEOW")
    });
    assert!(shouted, "self-authored tool didn't run correctly: {kinds:?}");
}

/// Milestone 1 of the Node-compat runtime: the module system resolves + loads
/// real modules — a relative `.ts` file (transpiled), `node:` builtins, and a
/// bare package from `node_modules` — and they run correctly.
#[test]
fn node_module_system_loads_ts_builtins_and_a_bare_package() {
    use std::fs;
    let dir = std::env::temp_dir().join(format!("pocketpi-node-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/greet")).unwrap();
    fs::write(
        dir.join("node_modules/greet/package.json"),
        r#"{"name":"greet","version":"1.0.0","module":"index.js"}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/greet/index.js"),
        "export default (who) => \"hello \" + who;",
    )
    .unwrap();
    fs::write(
        dir.join("entry.ts"),
        r#"
import { join } from "node:path";
import { EventEmitter } from "node:events";
import { Buffer } from "node:buffer";
import { homedir } from "node:os";
import greet from "greet";

type Result = { path: string; ee: boolean; b64: string; greet: string; hasHome: boolean };
const ee = new EventEmitter();
let fired = false;
ee.on("x", () => { fired = true; });
ee.emit("x");

const out: Result = {
    path: join("a", "b", "..", "c"),
    ee: fired,
    b64: Buffer.from("hi").toString("base64"),
    greet: greet("cat"),
    hasHome: typeof homedir() === "string",
};
(globalThis as any).__nodeTest = out;
"#,
    )
    .unwrap();

    let mut rt = PiRuntime::new().expect("runtime");
    rt.run_module(dir.join("entry.ts").to_str().unwrap())
        .expect("run module");

    let out = rt.get_global_json("__nodeTest").expect("__nodeTest set");
    assert_eq!(out["path"], "a/c", "path.join wrong: {out}");
    assert_eq!(out["ee"], true, "EventEmitter didn't fire: {out}");
    assert_eq!(out["b64"], "aGk=", "Buffer base64 wrong: {out}");
    assert_eq!(out["greet"], "hello cat", "bare package import wrong: {out}");
    assert_eq!(out["hasHome"], true, "os.homedir wrong: {out}");

    let _ = fs::remove_dir_all(&dir);
}

/// Load a piece of the REAL, unmodified pi-ai (`utils/event-stream.js`) straight
/// from node_modules through the Node loader and exercise its actual class. This
/// is the "run unmodified pi" thesis, proven on real pi code — and validates the
/// ESM-first (no-CJS) decision. Skipped if node_modules isn't installed.
#[test]
fn runs_real_unmodified_pi_ai_module() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let real = format!(
        "{manifest}/../../js/node_modules/@mariozechner/pi-ai/dist/utils/event-stream.js"
    );
    if !std::path::Path::new(&real).exists() {
        eprintln!("skipping runs_real_unmodified_pi_ai_module: run `npm install` in js/ first");
        return;
    }

    let dir = std::env::temp_dir().join(format!("pocketpi-piai-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("entry.ts"),
        format!(
            r#"
import {{ AssistantMessageEventStream }} from "{real}";
const s = new AssistantMessageEventStream();
const msg = {{ role: "assistant", content: [{{ type: "text", text: "hi" }}] }};
// The real stream completes on a "done" event and resolves result() to its message.
s.push({{ type: "text_delta", delta: "hi", partial: msg }});
s.push({{ type: "done", message: msg }});
s.result().then((m: any) => {{
    (globalThis as any).__piAi = {{ resolvedText: m.content[0].text, isReal: true }};
}});
"#
        ),
    )
    .unwrap();

    let mut rt = PiRuntime::new().expect("runtime");
    rt.run_module(dir.join("entry.ts").to_str().unwrap())
        .expect("run real pi-ai module");
    // Drain so the result() promise settles.
    for _ in 0..5 {
        rt.pump().ok();
    }

    let out = rt.get_global_json("__piAi").expect("real pi-ai class ran");
    assert_eq!(out["resolvedText"], "hi", "real pi-ai EventStream misbehaved: {out}");
    assert_eq!(out["isReal"], true);
    let _ = std::fs::remove_dir_all(&dir);
}

/// WHATWG `fetch` → `Response` → `ReadableStream` → `json()`, end to end over the
/// native HTTP hub. Hits a real endpoint (needs network/proxy); skips if
/// unreachable. A 401 is fine — fetch doesn't throw on it, which is the point.
#[test]
fn whatwg_fetch_returns_a_readable_response() {
    let mut rt = PiRuntime::new().expect("runtime");
    rt.eval_script(
        r#"
        globalThis.__f = { done: false };
        fetch("https://api.anthropic.com/v1/models")
          .then(async (r) => {
            const body = await r.text();
            let parsed = null; try { parsed = JSON.parse(body); } catch {}
            globalThis.__f = {
              done: true, status: r.status, ok: r.ok,
              ct: r.headers.get("content-type"),
              len: body.length, isObject: parsed !== null && typeof parsed === "object",
            };
          })
          .catch((e) => { globalThis.__f = { done: true, err: String(e && e.message ? e.message : e) }; });
        "#,
    )
    .expect("eval fetch");

    let start = std::time::Instant::now();
    loop {
        rt.pump().unwrap();
        let f = rt.get_global_json("__f").unwrap_or(serde_json::Value::Null);
        if f.get("done").and_then(|v| v.as_bool()) == Some(true) {
            if let Some(err) = f.get("err").and_then(|v| v.as_str()) {
                eprintln!("skipping whatwg_fetch: endpoint unreachable ({err})");
                return;
            }
            eprintln!("fetch result: {f}");
            let status = f["status"].as_i64().unwrap_or(0);
            assert!(status > 0, "no status: {f}");
            assert!(f["len"].as_i64().unwrap_or(0) > 0, "empty body: {f}");
            assert_eq!(f["isObject"], true, "body wasn't JSON: {f}");
            assert!(f["ct"].as_str().unwrap_or("").contains("json"), "content-type header missing: {f}");
            return;
        }
        if start.elapsed().as_secs() > 30 {
            panic!("fetch never completed: {f}");
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}

/// Live end-to-end against OpenAI. Skipped unless OPENAI_API_KEY is set.
#[test]
fn live_openai_turn() {
    let Ok(key) = std::env::var("OPENAI_API_KEY") else {
        eprintln!("skipping live_openai_turn: OPENAI_API_KEY not set");
        return;
    };
    let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());
    let (sink, cb) = collector();
    let mut rt = PiRuntime::new().expect("runtime");
    rt.on_event(cb);
    let cfg = serde_json::json!({
        "provider": "openai",
        "model": model,
        "apiKey": key,
        // Reasoning models (gpt-5.x) spend tokens thinking before answering —
        // a tiny cap leaves nothing for the visible reply.
        "maxTokens": 2048,
        "systemPrompt": "Reply with exactly the word: pong"
    });
    rt.boot(&cfg.to_string()).expect("boot");
    rt.prompt("ping").expect("prompt");
    run(&mut rt, sink.clone(), 4.0, 90.0);

    let events = sink.borrow();
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    eprintln!("openai event kinds: {kinds:?}");
    for e in events.iter().filter(|e| e.kind == "error") {
        eprintln!("openai error raw: {}", e.raw);
    }
    assert!(kinds.contains(&"end"), "turn did not complete: {kinds:?}");
    let text = events
        .iter()
        .find(|e| e.kind == "assistant_text")
        .and_then(|e| e.value.get("text").and_then(|v| v.as_str()))
        .unwrap_or("");
    eprintln!("live openai said: {text:?}");
    assert!(!text.is_empty(), "no assistant text");
}

/// Live end-to-end against Anthropic. Skipped unless ANTHROPIC_API_KEY is set,
/// so `cargo test` is hermetic by default.
#[test]
fn live_anthropic_turn() {
    let Ok(key) = std::env::var("ANTHROPIC_API_KEY") else {
        eprintln!("skipping live_anthropic_turn: ANTHROPIC_API_KEY not set");
        return;
    };
    let (sink, cb) = collector();
    let mut rt = PiRuntime::new().expect("runtime");
    rt.on_event(cb);
    let cfg = serde_json::json!({
        "model": "claude-opus-4-8",
        "apiKey": key,
        "maxTokens": 64,
        "systemPrompt": "Reply with exactly the word: pong"
    });
    rt.boot(&cfg.to_string()).expect("boot");
    rt.prompt("ping").expect("prompt");
    run(&mut rt, sink.clone(), 4.0, 60.0);

    let events = sink.borrow();
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    assert!(kinds.contains(&"end"), "turn did not complete: {kinds:?}");
    let text = events
        .iter()
        .find(|e| e.kind == "assistant_text")
        .and_then(|e| e.value.get("text").and_then(|v| v.as_str()))
        .unwrap_or("");
    eprintln!("live model said: {text:?}");
    assert!(!text.is_empty(), "no assistant text");
}

/// Path B: load the esbuild-bundled UNMODIFIED pi-coding-agent (one module) and
/// confirm it evaluates — sidestepping the QuickJS multi-module linker crash.
/// Run with `cargo test -- --ignored loads_bundled_pi_coding_agent --nocapture`.
#[ignore]
#[test]
fn loads_bundled_pi_coding_agent() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let bundle = format!("{manifest}/js/pi-full.bundle.js");
    if !std::path::Path::new(&bundle).exists() {
        eprintln!("skip: bundle not built (node js/build-pi-full.mjs)");
        return;
    }
    let mut rt = PiRuntime::new().expect("runtime");
    match rt.run_module(&bundle) {
        Ok(()) => {
            let loaded = rt.get_global_json("__piFullLoaded");
            eprintln!("BUNDLE LOADED: __piFullLoaded = {loaded:?}");
            assert_eq!(loaded, Some(serde_json::Value::Bool(true)), "createAgentSession not exported");
        }
        Err(e) => panic!("BUNDLE LOAD FAILED: {e}"),
    }
}

/// Path B end-to-end: stand up an AgentSession from the UNMODIFIED bundled
/// pi-coding-agent and run one real turn against gpt-5.6 through Pocket Pi's
/// fetch + system proxy. Requires OPENAI_API_KEY and a reachable proxy, so it is
/// `#[ignore]`. Run with:
///   https_proxy=http://127.0.0.1:7897 OPENAI_API_KEY=... \
///     cargo test -p pocket-pi runs_bundled_pi_turn -- --ignored --nocapture
#[ignore]
#[test]
fn runs_bundled_pi_turn() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let bundle = format!("{manifest}/js/pi-full.bundle.js");
    let driver = format!("{manifest}/../../js/pi-full/driver.js");
    if !std::path::Path::new(&bundle).exists() {
        eprintln!("skip: bundle not built");
        return;
    }
    let key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
    if key.is_empty() {
        eprintln!("skip: OPENAI_API_KEY not set");
        return;
    }

    let mut rt = PiRuntime::new().expect("runtime");
    rt.run_module(&bundle).expect("bundle load");
    // Inject the key as a global (kept out of logs) then load the driver script.
    rt.eval_script(&format!("globalThis.__OPENAI_KEY = {key:?};")).expect("inject key");
    let driver_src = std::fs::read_to_string(&driver).expect("driver.js");
    rt.eval_script(&driver_src).expect("driver eval");

    // Kick off the async turn (fire-and-forget promise), then pump at 2Hz.
    rt.eval_script("globalThis.__piRun('Reply with exactly: pocket pi lives');")
        .expect("kick off");

    let start = std::time::Instant::now();
    let mut done = false;
    while start.elapsed().as_secs_f64() < 90.0 {
        rt.pump().expect("pump");
        if rt.get_global_json("__piDone") == Some(serde_json::Value::Bool(true)) {
            done = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }

    let result = rt.get_global_json("__piResult");
    let err = rt.get_global_json("__piError");
    let last = rt.get_global_json("__piLastEvent");
    let log = rt.get_global_json("__piLog");
    eprintln!("TURN done={done} last_event={last:?}");
    eprintln!("TURN error={err:?}");
    eprintln!("TURN result={result:?}");
    if let Some(serde_json::Value::Array(items)) = log {
        eprintln!("TURN log ({} events):", items.len());
        for it in items {
            eprintln!("  - {}", it.as_str().unwrap_or_default());
        }
    }
    assert!(done, "turn did not complete within budget");
    assert_eq!(err, Some(serde_json::Value::Null), "agent errored");
    let text = result.and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
    assert!(!text.trim().is_empty(), "no assistant text produced");
}

/// Path B extensions (M6): load a real, unmodified pi extension through Pocket
/// Pi's OWN module loader — the `.ts` is transpiled natively with oxc, NOT jiti
/// (which needs Node internals QuickJS lacks) — and hand its default factory to
/// pi's unmodified `loadExtensionFromFactory`. Asserts the extension's tool and
/// lifecycle hook register. Offline; only needs the built bundle. Run with:
///   cargo test -p pocket-pi loads_pi_extension_via_our_loader -- --ignored --nocapture
#[ignore]
#[test]
fn loads_pi_extension_via_our_loader() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let bundle = format!("{manifest}/js/pi-full.bundle.js");
    let probe = format!("{manifest}/../../js/pi-full/ext-probe.js");
    let ext = format!("{manifest}/../../js/pi-full/example-extension.ts");
    if !std::path::Path::new(&bundle).exists() {
        eprintln!("skip: bundle not built");
        return;
    }

    let mut rt = PiRuntime::new().expect("runtime");
    rt.run_module(&bundle).expect("bundle load");
    let probe_src = std::fs::read_to_string(&probe).expect("ext-probe.js");
    rt.eval_script(&probe_src).expect("probe eval");
    rt.eval_script(&format!("globalThis.__piLoadExtension({ext:?});"))
        .expect("kick off");

    let start = std::time::Instant::now();
    while start.elapsed().as_secs_f64() < 15.0 {
        rt.pump().expect("pump");
        if rt.get_global_json("__piExtDone") == Some(serde_json::Value::Bool(true)) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let err = rt.get_global_json("__piExtError");
    let result = rt.get_global_json("__piExtResult");
    eprintln!("EXT error={err:?}");
    eprintln!("EXT result={result:?}");
    assert_eq!(err, Some(serde_json::Value::Null), "extension load errored");
    let result = result.expect("no extension result");
    let tools = result.get("tools").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let handlers = result.get("handlers").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    assert!(
        tools.iter().any(|t| t.as_str() == Some("echo")),
        "extension tool 'echo' not registered (got {tools:?})"
    );
    assert!(
        handlers.iter().any(|h| h.as_str() == Some("agent_start")),
        "extension hook 'agent_start' not registered (got {handlers:?})"
    );
}

/// WIP integration probe toward loading unmodified pi-coding-agent. Run with
/// `cargo test -- --ignored probe_pi_coding_agent --nocapture`. Currently clears
/// the whole Node-builtin + CJS dependency surface and reaches pi-coding-agent's
/// own modules (blocked on a QuickJS ESM indirect-re-export cycle).
#[ignore]
#[test]
fn probe_pi_coding_agent() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let sdk = format!("{manifest}/../../js/node_modules/@mariozechner/pi-coding-agent/dist/core/sdk.js");
    if !std::path::Path::new(&sdk).exists() { eprintln!("skip: not installed"); return; }
    let dir = std::env::temp_dir().join(format!("pca-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("e.ts"), format!(
        "import {{ createAgentSession }} from \"{sdk}\";\n(globalThis as any).__pca = typeof createAgentSession;\n"
    )).unwrap();
    let mut rt = PiRuntime::new().expect("rt");
    match rt.run_module(dir.join("e.ts").to_str().unwrap()) {
        Ok(()) => eprintln!("PROBE OK: createAgentSession = {:?}", rt.get_global_json("__pca")),
        Err(e) => eprintln!("PROBE ERR: {e}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}
