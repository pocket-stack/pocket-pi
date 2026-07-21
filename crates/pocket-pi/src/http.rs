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

    /// Begin a streaming request described by `{url, apiKey, body}` JSON.
    /// Returns the turn id, or an error string the caller turns into a JS throw.
    pub fn start(&self, request_json: &str) -> Result<u64, String> {
        let req: serde_json::Value =
            serde_json::from_str(request_json).map_err(|e| format!("bad request json: {e}"))?;
        let url = req
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or("missing url")?
            .to_string();
        let api_key = req
            .get("apiKey")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let body = req.get("body").cloned().unwrap_or(serde_json::json!({}));
        let body_str = serde_json::to_string(&body).map_err(|e| e.to_string())?;
        // "bearer" (OpenAI-style) vs "x-api-key" (Anthropic, default).
        let auth = req
            .get("auth")
            .and_then(|v| v.as_str())
            .unwrap_or("x-api-key")
            .to_string();

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut inner = self.inner.lock().unwrap();
            inner.turns.insert(
                id,
                Turn {
                    cancel: cancel.clone(),
                    ..Default::default()
                },
            );
        }

        let hub = self.inner.clone();
        std::thread::Builder::new()
            .name(format!("pocket-pi-http-{id}"))
            .spawn(move || run_request(hub, id, url, api_key, auth, body_str, cancel))
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

fn run_request(
    hub: Arc<Mutex<Inner>>,
    id: u64,
    url: String,
    api_key: String,
    auth: String,
    body: String,
    cancel: Arc<AtomicBool>,
) {
    let mut builder =
        ureq::AgentBuilder::new().timeout_connect(std::time::Duration::from_secs(20));
    // Respect a system proxy (Clash/mihomo, corporate egress, …) the way curl
    // does — many desktops route all external HTTP through one.
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

    let mut req = agent.post(&url).set("content-type", "application/json");
    if auth == "bearer" {
        // OpenAI-style: Authorization: Bearer <key>.
        req = req.set("authorization", &format!("Bearer {api_key}"));
    } else if api_key.starts_with("sk-ant-oat") {
        // Anthropic OAuth token → Bearer + Claude Code identity beta.
        req = req
            .set("anthropic-version", "2023-06-01")
            .set("authorization", &format!("Bearer {api_key}"))
            .set("anthropic-beta", "oauth-2025-04-20");
    } else {
        // Anthropic API key.
        req = req
            .set("anthropic-version", "2023-06-01")
            .set("x-api-key", &api_key);
    }

    let resp = match req.send_string(&body) {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let msg = r
                .into_string()
                .unwrap_or_else(|_| String::from("(no body)"));
            finish(&hub, id, Some(format!("http {code}: {}", truncate(&msg, 400))));
            return;
        }
        Err(e) => {
            finish(&hub, id, Some(format!("request error: {e}")));
            return;
        }
    };

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
        // Emit complete lines; keep the trailing partial in the buffer.
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\r', '\n']);
            // Anthropic SSE: `event: <type>` then `data: <json>`. The JSON
            // carries its own `type`, so we forward only `data:` payloads.
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

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}
