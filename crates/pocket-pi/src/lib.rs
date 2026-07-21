//! # Pocket Pi
//!
//! A QuickJS runtime that runs the **pi coding-agent core** — `@mariozechner/pi-agent-core`
//! — with **no Node and no bun**, on a PocketJS-style **coalesced frame scheduler**.
//!
//! The agent loop, LLM streaming, tool dispatch, and message state are pi's own
//! JS, evaluated in one QuickJS realm. Everything Node-ish is provided natively:
//! HTTPS + SSE streaming runs on background threads and lands in a per-turn
//! mailbox ([`http`]), and the realm is driven one **frame** at a time.
//!
//! ## Why a frame scheduler for an agent
//!
//! [`PiRuntime::pump`] is one frame: deliver any streamed LLM events, fire due
//! timers, then drain the QuickJS microtask/job queue so the agent loop advances.
//! Because LLM latency (seconds) dwarfs a frame period, the host can pump as
//! slowly as **2 Hz** when idle and lose nothing — the whole runtime coalesces
//! to near-zero CPU between token bursts. That is the PocketJS "demand-render"
//! idea applied to an agent: do work only when there is work.
//!
//! ```no_run
//! use pocket_pi::{PiRuntime, HostEvent};
//! let mut rt = PiRuntime::new().unwrap();
//! rt.on_event(|ev: &HostEvent| println!("{}: {}", ev.kind, ev.raw));
//! rt.boot(r#"{"model":"claude-opus-4-8","apiKey":"...","systemPrompt":"Be terse."}"#).unwrap();
//! rt.prompt("Say hi in three words.").unwrap();
//! for _ in 0..600 { rt.pump().unwrap(); if rt.is_idle() { break } std::thread::sleep(std::time::Duration::from_millis(50)); }
//! ```

mod http;
mod node;
mod transpile;

use rquickjs::{CatchResultExt, Context, Ctx, Function, Object, Runtime};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub use http::HttpHub;

const PRELUDE: &str = include_str!("../js/prelude.js");
const AGENT_BUNDLE: &str = include_str!("../js/agent.bundle.js");

/// One event surfaced from the agent to the host, already decoded from the
/// guest's compact JSON vocabulary (`start`, `text`, `thinking`,
/// `assistant_text`, `tool_start`, `tool_end`, `end`, `error`, `booted`).
#[derive(Debug, Clone)]
pub struct HostEvent {
    pub kind: String,
    pub raw: String,
    pub value: serde_json::Value,
}

/// What a host tool returns. Text is fed back to the model; an image (base64)
/// becomes an image tool-result block — this is how the cat's "look at the
/// screen" tool hands a screenshot to the agent.
#[derive(Debug, Clone, Default)]
pub struct ToolResult {
    pub text: Option<String>,
    pub image_base64: Option<String>,
    pub mime_type: Option<String>,
    pub terminate: bool,
}

impl ToolResult {
    pub fn text(s: impl Into<String>) -> Self {
        ToolResult {
            text: Some(s.into()),
            ..Default::default()
        }
    }
    fn to_json(&self) -> serde_json::Value {
        let mut o = serde_json::Map::new();
        if let Some(t) = &self.text {
            o.insert("text".into(), serde_json::Value::String(t.clone()));
        }
        if let Some(img) = &self.image_base64 {
            o.insert("image".into(), serde_json::Value::String(img.clone()));
            o.insert(
                "mimeType".into(),
                serde_json::Value::String(self.mime_type.clone().unwrap_or_else(|| "image/jpeg".into())),
            );
        }
        o.insert("terminate".into(), serde_json::Value::Bool(self.terminate));
        serde_json::Value::Object(o)
    }
}

type ToolFn = Box<dyn FnMut(serde_json::Value) -> ToolResult>;

struct HostState {
    tools: HashMap<String, ToolFn>,
    emitted: Vec<String>,
}

/// The runtime. Not `Send` — it owns a QuickJS realm and must be driven from a
/// single thread. Cross-thread work (HTTP) lives behind the [`HttpHub`].
pub struct PiRuntime {
    rt: Runtime,
    ctx: Context,
    state: Rc<RefCell<HostState>>,
    on_event: Rc<RefCell<Option<Box<dyn FnMut(&HostEvent)>>>>,
    active: Rc<RefCell<bool>>,
}

impl PiRuntime {
    /// Build a realm, install the prelude shims and the `host` native namespace,
    /// then evaluate the bundled pi agent core.
    pub fn new() -> Result<PiRuntime, String> {
        let rt = Runtime::new().map_err(|e| e.to_string())?;
        let ctx = Context::full(&rt).map_err(|e| e.to_string())?;

        // Node-flavored module resolution + loading (relative, node_modules,
        // node: builtins, on-the-fly TS transpile).
        rt.set_loader(node::NodeResolver, node::NodeLoader);

        let state = Rc::new(RefCell::new(HostState {
            tools: HashMap::new(),
            emitted: Vec::new(),
        }));
        let on_event: Rc<RefCell<Option<Box<dyn FnMut(&HostEvent)>>>> = Rc::new(RefCell::new(None));
        let active = Rc::new(RefCell::new(false));
        let hub = HttpHub::new();

        ctx.with(|ctx| -> Result<(), String> {
            install_console(&ctx).map_err(|e| e.to_string())?;
            install_host(&ctx, &state, &hub).map_err(|e| e.to_string())?;
            Ok(())
        })?;

        // Prelude first (defines globals the bundle relies on), then the agent.
        ctx.with(|ctx| -> Result<(), String> {
            ctx.eval::<(), _>(PRELUDE.as_bytes())
                .catch(&ctx)
                .map_err(|e| format!("prelude eval: {e}"))?;
            // Node globals (process, Buffer, __node fs ops) before the bundle.
            node::install_node(&ctx)
                .catch(&ctx)
                .map_err(|e| format!("install_node: {e}"))?;
            ctx.eval::<(), _>(AGENT_BUNDLE.as_bytes())
                .catch(&ctx)
                .map_err(|e| format!("agent bundle eval: {e}"))?;
            Ok(())
        })?;

        let mut this = PiRuntime {
            rt,
            ctx,
            state,
            on_event,
            active,
        };
        this.drain_jobs();
        this.flush_events();
        Ok(this)
    }

    /// Register the host-side callback for agent events.
    pub fn on_event(&mut self, cb: impl FnMut(&HostEvent) + 'static) {
        *self.on_event.borrow_mut() = Some(Box::new(cb));
    }

    /// Register a native tool the agent can call. `run` executes synchronously
    /// during a pump; keep it fast (or stash work and answer on a later turn).
    pub fn register_tool(
        &mut self,
        name: impl Into<String>,
        run: impl FnMut(serde_json::Value) -> ToolResult + 'static,
    ) {
        self.state
            .borrow_mut()
            .tools
            .insert(name.into(), Box::new(run));
    }

    /// Boot the agent with a JSON config (`model`, `apiKey`, `systemPrompt`,
    /// `tools:[{name,description,parameters}]`, or `scripted` for offline runs).
    pub fn boot(&mut self, config_json: &str) -> Result<(), String> {
        let cfg = config_json.to_string();
        self.ctx.with(|ctx| -> Result<(), String> {
            let pp: Object = ctx
                .globals()
                .get("PocketPi")
                .map_err(|e| format!("PocketPi missing: {e}"))?;
            let boot: Function = pp.get("boot").map_err(|e| e.to_string())?;
            boot.call::<_, ()>((cfg,))
                .catch(&ctx)
                .map_err(|e| format!("boot: {e}"))?;
            Ok(())
        })?;
        self.drain_jobs();
        self.flush_events();
        Ok(())
    }

    /// Send a user prompt. Returns immediately; results arrive as events across
    /// subsequent [`pump`](Self::pump) calls.
    pub fn prompt(&mut self, text: &str) -> Result<(), String> {
        *self.active.borrow_mut() = true;
        let text = text.to_string();
        self.ctx.with(|ctx| -> Result<(), String> {
            let pp: Object = ctx.globals().get("PocketPi").map_err(|e| e.to_string())?;
            let f: Function = pp.get("prompt").map_err(|e| e.to_string())?;
            f.call::<_, ()>((text,))
                .catch(&ctx)
                .map_err(|e| format!("prompt: {e}"))?;
            Ok(())
        })?;
        self.pump()
    }

    /// Load an agent-authored **TypeScript** plugin at runtime. The source is
    /// transpiled natively (oxc) and evaluated in the live realm; its default
    /// export is a factory `(api) => void` that can register tools and hooks on
    /// the running agent. This is the no-Node analogue of pi's jiti extension
    /// loader — the agent can write and install its own tools mid-session.
    pub fn load_plugin_ts(&mut self, name: &str, ts_source: &str) -> Result<(), String> {
        let (name, ts) = (name.to_string(), ts_source.to_string());
        self.ctx.with(|ctx| -> Result<(), String> {
            let pp: Object = ctx.globals().get("PocketPi").map_err(|e| e.to_string())?;
            let f: Function = pp.get("loadPlugin").map_err(|e| e.to_string())?;
            f.call::<_, ()>((name, ts))
                .catch(&ctx)
                .map_err(|e| format!("loadPlugin: {e}"))?;
            Ok(())
        })?;
        self.drain_jobs();
        self.flush_events();
        Ok(())
    }

    /// Import and evaluate an ES module by specifier (absolute path, or a
    /// `node:` builtin), driving the Node resolver/loader — relative imports,
    /// `node_modules` packages, and `.ts` transpile all work. Milestone toward
    /// running unmodified pi-coding-agent. Returns after the module settles.
    pub fn run_module(&mut self, specifier: &str) -> Result<(), String> {
        let spec = specifier.to_string();
        self.ctx.with(|ctx| -> Result<(), String> {
            let promise = rquickjs::module::Module::import(&ctx, spec)
                .catch(&ctx)
                .map_err(|e| format!("import: {e}"))?;
            promise
                .finish::<rquickjs::Value>()
                .catch(&ctx)
                .map_err(|e| format!("module eval: {e}"))?;
            Ok(())
        })?;
        self.drain_jobs();
        Ok(())
    }

    /// Read a global (JSON-serialized) — for tests/introspection.
    pub fn get_global_json(&self, name: &str) -> Option<serde_json::Value> {
        self.ctx.with(|ctx| {
            let v: rquickjs::Value = ctx.globals().get(name).ok()?;
            let s = ctx.json_stringify(v).ok()??;
            serde_json::from_str(&s.to_string().ok()?).ok()
        })
    }

    /// Abort the in-flight turn, if any.
    pub fn abort(&mut self) -> Result<(), String> {
        self.ctx.with(|ctx| -> Result<(), String> {
            let pp: Object = ctx.globals().get("PocketPi").map_err(|e| e.to_string())?;
            let f: Function = pp.get("abort").map_err(|e| e.to_string())?;
            f.call::<_, ()>(()).catch(&ctx).map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    /// One frame: fire due timers, deliver streamed LLM events, drain the job
    /// queue so the agent loop advances, then flush host events to the callback.
    pub fn pump(&mut self) -> Result<(), String> {
        self.ctx.with(|ctx| -> Result<(), String> {
            call_global_void(&ctx, "__catpiTimers")?;
            call_global_void(&ctx, "__catpiPump")?;
            Ok(())
        })?;
        self.drain_jobs();
        self.flush_events();
        Ok(())
    }

    /// True when no turn is in flight (the last `end`/`error` event has fired).
    pub fn is_idle(&self) -> bool {
        !*self.active.borrow()
    }

    fn drain_jobs(&self) {
        loop {
            match self.rt.execute_pending_job() {
                Ok(true) => continue,
                Ok(false) => break,
                Err(e) => {
                    log::warn!("pocket-pi: pending job threw: {e:?}");
                }
            }
        }
    }

    fn flush_events(&mut self) {
        let drained: Vec<String> = {
            let mut st = self.state.borrow_mut();
            if st.emitted.is_empty() {
                return;
            }
            std::mem::take(&mut st.emitted)
        };
        for raw in drained {
            let value: serde_json::Value =
                serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null);
            let kind = value
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if kind == "end" || kind == "error" {
                *self.active.borrow_mut() = false;
            }
            let ev = HostEvent { kind, raw, value };
            if let Some(cb) = self.on_event.borrow_mut().as_mut() {
                cb(&ev);
            }
        }
    }
}

fn call_global_void(ctx: &Ctx, name: &str) -> Result<(), String> {
    if let Ok(f) = ctx.globals().get::<_, Function>(name) {
        f.call::<_, ()>(()).catch(ctx).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Mount `globalThis.host` — the entire native capability surface the guest can
/// reach. Deliberately small: HTTP streaming, tool dispatch, event emit, uuid.
fn install_host(
    ctx: &Ctx,
    state: &Rc<RefCell<HostState>>,
    hub: &HttpHub,
) -> rquickjs::Result<()> {
    let host = Object::new(ctx.clone())?;

    // host.http.{start,drain,cancel}
    let http = Object::new(ctx.clone())?;
    let h = hub.clone();
    http.set(
        "start",
        Function::new(ctx.clone(), move |ctx: Ctx, req: String| -> rquickjs::Result<f64> {
            match h.start(&req) {
                Ok(id) => Ok(id as f64),
                Err(e) => Err(ctx.throw(rquickjs::String::from_str(ctx.clone(), &e)?.into())),
            }
        })?,
    )?;
    let h = hub.clone();
    http.set(
        "drain",
        Function::new(ctx.clone(), move |id: f64| -> String { h.drain(id as u64) })?,
    )?;
    let h = hub.clone();
    http.set(
        "cancel",
        Function::new(ctx.clone(), move |id: f64| h.cancel(id as u64))?,
    )?;
    host.set("http", http)?;

    // host.tool(name, argsJson) -> resultJson  (synchronous dispatch to Rust)
    let st = state.clone();
    host.set(
        "tool",
        Function::new(ctx.clone(), move |name: String, args_json: String| -> String {
            let args: serde_json::Value =
                serde_json::from_str(&args_json).unwrap_or(serde_json::Value::Null);
            let mut st = st.borrow_mut();
            let result = match st.tools.get_mut(&name) {
                Some(f) => f(args),
                None => ToolResult::text(format!("(no such tool: {name})")),
            };
            result.to_json().to_string()
        })?,
    )?;

    // host.emit(jsonLine) -> buffers an agent event for the host to flush
    let st = state.clone();
    host.set(
        "emit",
        Function::new(ctx.clone(), move |line: String| {
            st.borrow_mut().emitted.push(line);
        })?,
    )?;

    // host.uuid() -> string
    host.set(
        "uuid",
        Function::new(ctx.clone(), move || -> String { simple_uuid() })?,
    )?;

    // host.transpile(name, tsSource) -> jsSource  (agent-authored TS plugins)
    host.set(
        "transpile",
        Function::new(
            ctx.clone(),
            move |ctx: Ctx, name: String, source: String| -> rquickjs::Result<String> {
                transpile::transpile_ts(&name, &source)
                    .map_err(|e| ctx.throw(rquickjs::String::from_str(ctx.clone(), &e).unwrap().into()))
            },
        )?,
    )?;

    ctx.globals().set("host", host)?;
    Ok(())
}

fn simple_uuid() -> String {
    // Not cryptographic; sufficient for tool-call ids in a PoC. Seeds from the
    // system clock and address-space entropy so ids don't collide within a run.
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let salt = &nanos as *const _ as usize;
    let mut x = nanos ^ (salt as u128).wrapping_mul(0x9E3779B97F4A7C15);
    let mut hex = String::with_capacity(36);
    for i in 0..32 {
        if i == 8 || i == 12 || i == 16 || i == 20 {
            hex.push('-');
        }
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let nibble = ((x >> 64) & 0xF) as u8;
        hex.push(char::from_digit(nibble as u32, 16).unwrap());
    }
    hex
}

/// `console.*` → the `log` crate, target "pocket-pi.guest".
fn install_console(ctx: &Ctx) -> rquickjs::Result<()> {
    let console = Object::new(ctx.clone())?;
    for level in ["log", "info", "debug", "warn", "error"] {
        console.set(
            level,
            Function::new(ctx.clone(), move |args: rquickjs::function::Rest<rquickjs::Value>| {
                let mut out = String::new();
                for (i, v) in args.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    if let Some(s) = v.as_string() {
                        out.push_str(&s.to_string().unwrap_or_default());
                    } else if let Ok(s) = v.ctx().json_stringify(v.clone()) {
                        out.push_str(&s.map(|s| s.to_string().unwrap_or_default()).unwrap_or_default());
                    }
                }
                log::info!(target: "pocket-pi.guest", "{out}");
            })?,
        )?;
    }
    ctx.globals().set("console", console)?;
    Ok(())
}

#[cfg(test)]
mod tests;
