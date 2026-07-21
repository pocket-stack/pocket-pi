//! The native HTTP bridge for Pocket Pi.
//!
//! QuickJS is single-threaded and can't do blocking TLS, so each streaming
//! request runs on its own OS thread. The thread reads the Anthropic SSE body
//! line by line and pushes each decoded `data:` JSON into a per-turn mailbox.
//! The JS side never sees bytes — only complete event payloads it drains once
//! per host frame. This is the PocketJS `svc` mailbox pattern applied to LLM
//! streaming: the frame scheduler is the pump that moves the agent forward.

use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Turn {
    lines: VecDeque<String>,
    done: bool,
    error: Option<String>,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
struct Inner {
    turns: HashMap<u64, Turn>,
}

/// Shared, thread-safe registry of in-flight requests.
#[derive(Clone, Default)]
pub struct HttpHub {
    inner: Arc<Mutex<Inner>>,
    next_id: Arc<AtomicU64>,
}

/// What the JS side sees each time it drains a turn.
struct Drained {
    lines: Vec<String>,
    done: bool,
    error: Option<String>,
}

impl HttpHub {
    pub fn new() -> Self {
        HttpHub {
            inner: Arc::new(Mutex::new(Inner::default())),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Begin a request. Two shapes share this path:
    /// - provider streaming: `{url, apiKey, auth, body}` → POST, SSE `data:` lines.
    /// - WHATWG fetch: `{url, method, headers, body, raw:true}` → a meta line then
    ///   base64 body chunks (backing `Response`/`ReadableStream`).
    pub fn start(&self, request_json: &str) -> Result<u64, String> {
        let v: serde_json::Value =
            serde_json::from_str(request_json).map_err(|e| format!("bad request json: {e}"))?;
        let url = v.get("url").and_then(|x| x.as_str()).ok_or("missing url")?.to_string();
        let raw = v.get("raw").and_then(|x| x.as_bool()).unwrap_or(false);
        let method = v
            .get("method")
            .and_then(|x| x.as_str())
            .map(|s| s.to_uppercase())
            .unwrap_or_else(|| "POST".into());
        let api_key = v.get("apiKey").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let auth = v.get("auth").and_then(|x| x.as_str()).unwrap_or("x-api-key").to_string();
        // Body: fetch passes a string; providers pass a JSON object to serialize.
        let body = match v.get("body") {
            Some(serde_json::Value::String(s)) => Some(s.clone()),
            Some(other) if !other.is_null() => Some(other.to_string()),
            _ => None,
        };
        let mut headers: Vec<(String, String)> = Vec::new();
        if let Some(serde_json::Value::Object(h)) = v.get("headers") {
            for (k, val) in h {
                if let Some(s) = val.as_str() {
                    headers.push((k.clone(), s.to_string()));
                }
            }
        }
        let req = Req { url, method, headers, body, api_key, auth, raw };

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut inner = self.inner.lock().unwrap();
            inner.turns.insert(id, Turn { cancel: cancel.clone(), ..Default::default() });
        }
        let hub = self.inner.clone();
        std::thread::Builder::new()
            .name(format!("pocket-pi-http-{id}"))
            .spawn(move || run_request(hub, id, req, cancel))
            .map_err(|e| format!("spawn failed: {e}"))?;
        Ok(id)
    }

    /// Take everything the turn has accumulated so far. Returns a JSON string
    /// `{lines:[...], done, error}` (the JS side JSON.parses it).
    pub fn drain(&self, id: u64) -> String {
        let mut inner = self.inner.lock().unwrap();
        let out = match inner.turns.get_mut(&id) {
            Some(turn) => Drained {
                lines: turn.lines.drain(..).collect(),
                done: turn.done,
                error: turn.error.clone(),
            },
            None => Drained {
                lines: Vec::new(),
                done: true,
                error: Some("unknown turn".into()),
            },
        };
        // Once terminal and drained, forget the turn.
        if out.done {
            if let Some(turn) = inner.turns.get(&id) {
                if turn.lines.is_empty() {
                    inner.turns.remove(&id);
                }
            }
        }
        serde_json::json!({
            "lines": out.lines,
            "done": out.done,
            "error": out.error,
        })
        .to_string()
    }

    pub fn cancel(&self, id: u64) {
        let inner = self.inner.lock().unwrap();
        if let Some(turn) = inner.turns.get(&id) {
            turn.cancel.store(true, Ordering::SeqCst);
        }
    }
}

fn push_line(hub: &Arc<Mutex<Inner>>, id: u64, line: String) {
    if let Some(turn) = hub.lock().unwrap().turns.get_mut(&id) {
        turn.lines.push_back(line);
    }
}

fn finish(hub: &Arc<Mutex<Inner>>, id: u64, error: Option<String>) {
    if let Some(turn) = hub.lock().unwrap().turns.get_mut(&id) {
        turn.done = true;
        if error.is_some() {
            turn.error = error;
        }
    }
}

struct Req {
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    api_key: String,
    auth: String,
    raw: bool,
}

fn run_request(hub: Arc<Mutex<Inner>>, id: u64, req: Req, cancel: Arc<AtomicBool>) {
    let mut builder =
        ureq::AgentBuilder::new().timeout_connect(std::time::Duration::from_secs(20));
    // Respect a system proxy (Clash/mihomo, corporate egress, …) like curl does.
    if let Some(proxy_url) = std::env::var("HTTPS_PROXY")
        .or_else(|_| std::env::var("https_proxy"))
        .or_else(|_| std::env::var("ALL_PROXY"))
        .or_else(|_| std::env::var("all_proxy"))
        .ok()
        .filter(|s| !s.is_empty())
    {
        if let Ok(proxy) = ureq::Proxy::new(&proxy_url) {
            builder = builder.proxy(proxy);
        }
    }
    let agent = builder.build();

    let mut r = agent.request(&req.method, &req.url);
    let has_ct = req.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type"));
    if req.body.is_some() && !has_ct {
        r = r.set("content-type", "application/json");
    }
    for (k, v) in &req.headers {
        r = r.set(k, v);
    }
    // Provider auth convenience (skipped for a plain fetch that sets its own).
    if !req.api_key.is_empty() {
        if req.auth == "bearer" {
            r = r.set("authorization", &format!("Bearer {}", req.api_key));
        } else if req.api_key.starts_with("sk-ant-oat") {
            r = r
                .set("anthropic-version", "2023-06-01")
                .set("authorization", &format!("Bearer {}", req.api_key))
                .set("anthropic-beta", "oauth-2025-04-20");
        } else {
            r = r.set("anthropic-version", "2023-06-01").set("x-api-key", &req.api_key);
        }
    }

    let send = match &req.body {
        Some(b) => r.send_string(b),
        None => r.call(),
    };
    let resp = match send {
        Ok(r) => r,
        // For raw fetch we forward non-2xx as a real Response (fetch doesn't
        // throw on 4xx/5xx); for SSE providers, surface it as an error.
        Err(ureq::Error::Status(code, r)) if req.raw => r_or_status(r, code),
        Err(ureq::Error::Status(code, r)) => {
            let msg = r.into_string().unwrap_or_else(|_| String::from("(no body)"));
            finish(&hub, id, Some(format!("http {code}: {}", truncate(&msg, 400))));
            return;
        }
        Err(e) => {
            finish(&hub, id, Some(format!("request error: {e}")));
            return;
        }
    };

    if req.raw {
        // Deliver a meta line (status/headers) then base64 body chunks.
        let mut headers = serde_json::Map::new();
        for name in resp.headers_names() {
            if let Some(val) = resp.header(&name) {
                headers.insert(name.to_lowercase(), serde_json::Value::String(val.to_string()));
            }
        }
        let meta = serde_json::json!({ "__meta": {
            "status": resp.status(), "statusText": resp.status_text(), "headers": headers,
        }});
        push_line(&hub, id, meta.to_string());
        let mut reader = resp.into_reader();
        let mut chunk = [0u8; 8192];
        loop {
            if cancel.load(Ordering::SeqCst) {
                finish(&hub, id, Some("aborted".into()));
                return;
            }
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    let msg = serde_json::json!({ "__chunk": base64_encode(&chunk[..n]) });
                    push_line(&hub, id, msg.to_string());
                }
                Err(e) => {
                    finish(&hub, id, Some(format!("read error: {e}")));
                    return;
                }
            }
        }
        finish(&hub, id, None);
        return;
    }

    // SSE mode: forward `data:` payloads line by line.
    let mut reader = resp.into_reader();
    let mut buf: Vec<u8> = Vec::with_capacity(8192);
    let mut chunk = [0u8; 4096];
    loop {
        if cancel.load(Ordering::SeqCst) {
            finish(&hub, id, Some("aborted".into()));
            return;
        }
        let n = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                finish(&hub, id, Some(format!("read error: {e}")));
                return;
            }
        };
        buf.extend_from_slice(&chunk[..n]);
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(rest) = line.strip_prefix("data:") {
                let payload = rest.trim();
                if payload.is_empty() || payload == "[DONE]" {
                    continue;
                }
                if std::env::var("POCKET_PI_DEBUG_SSE").is_ok() {
                    eprintln!("SSE< {}", &payload[..payload.len().min(300)]);
                }
                push_line(&hub, id, payload.to_string());
            }
        }
    }
    finish(&hub, id, None);
}

/// ureq consumes the Response in the Status error; return it unchanged for the
/// raw path (fetch surfaces non-2xx as a normal Response).
fn r_or_status(resp: ureq::Response, _code: u16) -> ureq::Response {
    resp
}

fn base64_encode(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for c in bytes.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(A[(n >> 18 & 63) as usize] as char);
        out.push(A[(n >> 12 & 63) as usize] as char);
        out.push(if c.len() > 1 { A[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if c.len() > 2 { A[(n & 63) as usize] as char } else { '=' });
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}
