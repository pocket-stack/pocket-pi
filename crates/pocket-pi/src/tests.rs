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

fn collector() -> (
    Rc<RefCell<Vec<HostEvent>>>,
    impl FnMut(&HostEvent) + 'static,
) {
    let sink = Rc::new(RefCell::new(Vec::new()));
    let s = sink.clone();
    (sink, move |ev: &HostEvent| s.borrow_mut().push(ev.clone()))
}

/// Assemble the assistant reply the way the host does: streamed `text` deltas,
/// with an `assistant_text` full-text emit as a fallback.
fn reply_text(events: &[HostEvent]) -> String {
    let mut out = String::new();
    for e in events {
        match e.kind.as_str() {
            "text" => {
                if let Some(d) = e.value.get("delta").and_then(|v| v.as_str()) {
                    out.push_str(d);
                }
            }
            "assistant_text" if out.is_empty() => {
                if let Some(t) = e.value.get("text").and_then(|v| v.as_str()) {
                    out = t.to_string();
                }
            }
            _ => {}
        }
    }
    out
}

/// The module system resolves and loads a relative transpiled `.ts` file,
/// `node:` builtins, and a bare package from `node_modules`.
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
    assert_eq!(
        out["greet"], "hello cat",
        "bare package import wrong: {out}"
    );
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
    let real =
        format!("{manifest}/../../js/node_modules/@mariozechner/pi-ai/dist/utils/event-stream.js");
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
    assert_eq!(
        out["resolvedText"], "hi",
        "real pi-ai EventStream misbehaved: {out}"
    );
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
            assert!(
                f["ct"].as_str().unwrap_or("").contains("json"),
                "content-type header missing: {f}"
            );
            return;
        }
        if start.elapsed().as_secs() > 30 {
            panic!("fetch never completed: {f}");
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}

/// The full, unmodified pi-coding-agent is embedded and loaded by every
/// `PiRuntime::new()`: `globalThis.PiFull` (createAgentSession, …) and the
/// `PocketPi` host harness are both present with no external bundle, no network.
#[test]
fn new_embeds_and_loads_full_pi() {
    let mut rt = PiRuntime::new().expect("runtime");
    assert_eq!(
        rt.get_global_json("__piFullLoaded"),
        Some(serde_json::Value::Bool(true)),
        "full pi did not initialize in new()"
    );
    // The host harness is wired too.
    rt.eval_script("globalThis.__hasPocketPi = typeof globalThis.PocketPi?.boot === 'function';")
        .expect("probe");
    assert_eq!(
        rt.get_global_json("__hasPocketPi"),
        Some(serde_json::Value::Bool(true))
    );
}

/// The deterministic Mac fallback still runs a complete, unmodified
/// pi-coding-agent AgentSession; only its model provider is local and scripted.
#[test]
fn full_pi_runs_with_offline_provider() {
    let (sink, cb) = collector();
    let mut rt = PiRuntime::new().expect("runtime");
    rt.on_event(cb);
    let cfg = serde_json::json!({
        "model": "offline",
        "scripted": {"steps": [{"text": "OFFLINE-E2E-OK"}]}
    });
    rt.boot(&cfg.to_string()).expect("boot");
    rt.prompt("reply with the scripted response")
        .expect("prompt");
    run(&mut rt, sink.clone(), 30.0, 5.0);

    let events = sink.borrow();
    let errors: Vec<&HostEvent> = events
        .iter()
        .filter(|event| event.kind == "error")
        .collect();
    assert!(errors.is_empty(), "offline turn failed: {errors:?}");
    assert!(
        events.iter().any(|event| event.kind == "end"),
        "turn did not end"
    );
    assert_eq!(reply_text(&events), "OFFLINE-E2E-OK");
}

/// Load a real, unmodified pi extension through Pocket
/// Pi's OWN module loader — the `.ts` is transpiled natively with oxc, NOT jiti
/// (which needs Node internals QuickJS lacks) — and hand its default factory to
/// pi's unmodified `loadExtensionFromFactory`.
#[test]
fn loads_pi_extension_via_our_loader() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let probe = format!("{manifest}/js/pi-full/ext-probe.js");
    let ext = format!("{manifest}/../../js/src/pi-full/example-extension.ts");

    let mut rt = PiRuntime::new().expect("runtime");
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
    let tools = result
        .get("tools")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let handlers = result
        .get("handlers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        tools.iter().any(|t| t.as_str() == Some("echo")),
        "extension tool 'echo' not registered (got {tools:?})"
    );
    assert!(
        handlers.iter().any(|h| h.as_str() == Some("agent_start")),
        "extension hook 'agent_start' not registered (got {handlers:?})"
    );
}

/// Pump `__piRun(optsJson)` (see driver.js) until it finishes or the budget is
/// hit. Returns nothing; results are read from globals by the caller.
fn drive_session(rt: &mut PiRuntime, opts_json: &str, max_secs: f64) -> bool {
    rt.eval_script(&format!("globalThis.__piRun({opts_json:?});"))
        .expect("kick off");
    let start = std::time::Instant::now();
    while start.elapsed().as_secs_f64() < max_secs {
        rt.pump().expect("pump");
        if rt.get_global_json("__piDone") == Some(serde_json::Value::Bool(true)) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

/// Bind an unmodified pi extension into a real AgentSession through the
/// first-class `extensionFactories` seam and confirm the ExtensionRunner picks
/// up its hook and tool.
#[test]
fn binds_extension_into_session() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let driver = format!("{manifest}/js/pi-full/driver.js");
    let ext = format!("{manifest}/../../js/src/pi-full/example-extension.ts");
    let mut rt = PiRuntime::new().expect("runtime");
    rt.eval_script(&std::fs::read_to_string(&driver).expect("driver.js"))
        .expect("driver eval");

    let opts = serde_json::json!({ "extensionPath": ext }).to_string();
    let done = drive_session(&mut rt, &opts, 15.0);
    let err = rt.get_global_json("__piError");
    let bind = rt.get_global_json("__piBind");
    eprintln!("BIND done={done} error={err:?}");
    eprintln!("BIND result={bind:?}");
    assert!(done, "session build did not finish");
    assert_eq!(err, Some(serde_json::Value::Null), "session build errored");
    let bind = bind.expect("no bind info");
    assert_eq!(
        bind.get("hasAgentStart"),
        Some(&serde_json::Value::Bool(true)),
        "agent_start hook not bound into session"
    );
    let tools = bind
        .get("registeredTools")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        tools.iter().any(|t| t.as_str() == Some("echo")),
        "echo tool not registered in session (got {tools:?})"
    );
}

/// Persist a session to disk with pi's unmodified SessionManager (backed by
/// Pocket Pi's fs builtin), then resume it in a fresh manager and confirm the
/// history round-trips.
#[test]
fn persists_and_resumes_session() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let probe = format!("{manifest}/js/pi-full/persist-probe.js");
    let dir = std::env::temp_dir().join(format!("pocket-pi-sess-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut rt = PiRuntime::new().expect("runtime");
    rt.eval_script(&std::fs::read_to_string(&probe).expect("persist-probe.js"))
        .expect("probe eval");
    rt.eval_script(&format!(
        "globalThis.__piPersist({:?});",
        dir.to_str().unwrap()
    ))
    .expect("run persist");
    rt.pump().expect("pump");

    let err = rt.get_global_json("__piPersistError");
    let result = rt.get_global_json("__piPersistResult");
    eprintln!("PERSIST error={err:?}");
    eprintln!("PERSIST result={result:?}");
    // Confirm the session file actually hit disk.
    let files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect();
    eprintln!("PERSIST files={files:?}");
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(err, Some(serde_json::Value::Null), "persistence errored");
    let result = result.expect("no persist result");
    assert_eq!(
        result.get("wrote").and_then(|v| v.as_u64()),
        Some(2),
        "did not write 2 messages"
    );
    assert_eq!(
        result.get("resumedCount").and_then(|v| v.as_u64()),
        Some(2),
        "resumed session lost messages"
    );
    let texts = result
        .get("texts")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        texts
            .iter()
            .any(|t| t.as_str().is_some_and(|s| s.contains("42"))),
        "resumed history missing content (got {texts:?})"
    );
    assert!(
        files
            .iter()
            .any(|f| f.to_string_lossy().ends_with(".jsonl")),
        "no .jsonl session file on disk"
    );
}
