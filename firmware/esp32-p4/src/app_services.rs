use core::time::Duration;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use embedded_svc::http::{client::Client as HttpClient, Method};
use embedded_svc::io::Write as _;
use esp_idf_svc::http::client::{Configuration, EspHttpConnection};
use esp_idf_svc::io::EspIOError;
use pocket_pi_agentos::{AppServiceHost, HttpRequest, NetFailure, TransportCompletion};
use serde_json::{json, Value};

const ROBINHOOD_MCP_URL: &str = "https://agent.robinhood.com/mcp/trading";
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const MAX_MCP_RESPONSE: usize = 160 * 1024;

pub struct EspAppServices {
    inner: Arc<EspAppServicesInner>,
}

struct EspAppServicesInner {
    network_ready: Arc<AtomicBool>,
    exa_api_key: Option<String>,
    robinhood_access_token: Option<String>,
    robinhood: Mutex<McpState>,
}

#[derive(Default)]
struct McpState {
    session_id: Option<String>,
    next_id: u64,
}

impl EspAppServices {
    pub fn new(
        network_ready: Arc<AtomicBool>,
        exa_api_key: Option<String>,
        robinhood_access_token: Option<String>,
    ) -> Self {
        let inner = Arc::new(EspAppServicesInner {
            network_ready,
            exa_api_key,
            robinhood_access_token,
            robinhood: Mutex::new(McpState {
                session_id: None,
                next_id: 1,
            }),
        });
        Self { inner }
    }
}

impl EspAppServicesInner {
    fn robinhood_operation_allowed(operation: &str) -> bool {
        matches!(
            operation,
            "get_accounts"
                | "get_portfolio"
                | "get_equity_positions"
                | "get_equity_orders"
                | "get_equity_historicals"
                | "get_realized_pnl"
                | "get_pnl_trade_history"
                | "review_equity_order"
        )
    }

    fn exa_http(
        &self,
        request: HttpRequest,
    ) -> std::result::Result<TransportCompletion, NetFailure> {
        if request.method != "POST"
            || !matches!(
                request.url.as_str(),
                "https://api.exa.ai/search" | "https://api.exa.ai/contents"
            )
        {
            return Err(NetFailure::new(
                "invalid_request",
                "Exa NET capability permits only POST /search and /contents",
            ));
        }
        if request
            .headers
            .keys()
            .any(|name| !matches!(name.as_str(), "accept" | "content-type"))
        {
            return Err(NetFailure::new(
                "invalid_request",
                "Exa App supplied a forbidden HTTP header",
            ));
        }
        let body: Value = serde_json::from_slice(&request.body)
            .map_err(|error| NetFailure::new("invalid_request", error.to_string()))?;
        if !body.is_object() {
            return Err(NetFailure::new(
                "invalid_request",
                "Exa request body must be a JSON object",
            ));
        }
        let api_key = self
            .exa_api_key
            .as_deref()
            .ok_or_else(|| NetFailure::new("unavailable", "Exa API key was not provisioned"))?;
        execute_exa_http(request, api_key)
    }

    fn robinhood_call(&self, args: &Value) -> Result<Value, String> {
        if args.get("connection").and_then(Value::as_str) != Some("robinhood") {
            return Err("Robinhood App requested an unknown MCP connection".to_owned());
        }
        let operation = args
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "mcp.client callTool requires name".to_owned())?;
        let arguments = args.get("arguments").unwrap_or(&Value::Null);
        if !Self::robinhood_operation_allowed(operation) {
            return Err(format!(
                "Robinhood operation is not allowlisted: {operation}"
            ));
        }
        let token = self
            .robinhood_access_token
            .as_deref()
            .ok_or_else(|| "Robinhood OAuth token was not provisioned for this boot".to_owned())?;
        let mut state = self
            .robinhood
            .lock()
            .map_err(|_| "Robinhood MCP state lock was poisoned".to_owned())?;
        for attempt in 0..2 {
            let result = self.robinhood_once(token, &mut state, operation, arguments);
            match result {
                Ok(value) => return Ok(value),
                Err(error) if attempt == 0 && stale_mcp_session(&error) => {
                    log::warn!("Robinhood MCP reconnecting after: {error}");
                    state.session_id = None;
                    std::thread::sleep(Duration::from_millis(250));
                }
                Err(error) if attempt == 0 && transient_mcp_connect(&error) => {
                    log::warn!("Robinhood MCP transport retry after: {error}");
                    std::thread::sleep(Duration::from_millis(250));
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!()
    }

    fn robinhood_calls(&self, args: &Value) -> Result<Value, String> {
        if args.get("connection").and_then(Value::as_str) != Some("robinhood") {
            return Err("Robinhood App requested an unknown MCP connection".to_owned());
        }
        let calls = args
            .get("calls")
            .and_then(Value::as_array)
            .filter(|calls| !calls.is_empty() && calls.len() <= 16)
            .ok_or_else(|| "mcp.client callTools requires 1 to 16 calls".to_owned())?;
        for call in calls {
            let name = call
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| "mcp.client callTools requires a name for every call".to_owned())?;
            if !Self::robinhood_operation_allowed(name) {
                return Err(format!("Robinhood operation is not allowlisted: {name}"));
            }
        }
        let token = self
            .robinhood_access_token
            .as_deref()
            .ok_or_else(|| "Robinhood OAuth token was not provisioned for this boot".to_owned())?;
        let mut state = self
            .robinhood
            .lock()
            .map_err(|_| "Robinhood MCP state lock was poisoned".to_owned())?;
        for attempt in 0..2 {
            let result = self.robinhood_batch_once(token, &mut state, calls);
            match result {
                Ok(value) => return Ok(value),
                Err(error) if attempt == 0 && stale_mcp_session(&error) => {
                    log::warn!("Robinhood MCP batch reconnecting after: {error}");
                    state.session_id = None;
                    std::thread::sleep(Duration::from_millis(250));
                }
                Err(error) if attempt == 0 && transient_mcp_connect(&error) => {
                    log::warn!("Robinhood MCP batch transport retry after: {error}");
                    std::thread::sleep(Duration::from_millis(250));
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!()
    }

    fn robinhood_batch_once(
        &self,
        token: &str,
        state: &mut McpState,
        calls: &[Value],
    ) -> Result<Value, String> {
        self.ensure_robinhood_session(token, state)?;
        let mut requests = Vec::with_capacity(calls.len());
        let mut request_ids = Vec::with_capacity(calls.len());
        for call in calls {
            let id = state.next_id;
            state.next_id = state.next_id.saturating_add(1);
            request_ids.push(id);
            requests.push(json!({
                "jsonrpc":"2.0",
                "id":id,
                "method":"tools/call",
                "params":{
                    "name":call.get("name").and_then(Value::as_str).unwrap_or_default(),
                    "arguments":call.get("arguments").unwrap_or(&Value::Null)
                }
            }));
        }
        let started = Instant::now();
        let (responses, returned_session) = post_mcp_batch(
            token,
            state.session_id.as_deref(),
            &requests,
            request_ids.len(),
        )?;
        if returned_session.is_some() {
            state.session_id = returned_session;
        }
        let mut by_id = BTreeMap::new();
        for response in responses {
            if let Some(id) = response.get("id").and_then(Value::as_u64) {
                by_id.insert(id, response);
            }
        }
        let mut results = Vec::with_capacity(calls.len());
        for (index, call) in calls.iter().enumerate() {
            let name = call.get("name").and_then(Value::as_str).unwrap_or_default();
            let response = by_id
                .remove(&request_ids[index])
                .ok_or_else(|| format!("Robinhood MCP batch omitted response for {name}"))?;
            match normalize_mcp_result(&response) {
                Ok(value) => results.push(json!({"name":name,"ok":true,"value":value})),
                Err(error) => results.push(json!({"name":name,"ok":false,"error":error})),
            }
        }
        log::info!(
            "Robinhood MCP batch calls={} completed in {}ms",
            calls.len(),
            started.elapsed().as_millis()
        );
        Ok(json!({"results":results}))
    }

    fn ensure_robinhood_session(
        &self,
        token: &str,
        state: &mut McpState,
    ) -> Result<(), String> {
        if state.session_id.is_some() {
            return Ok(());
        }
        let request = json!({
            "jsonrpc":"2.0",
            "id":state.next_id,
            "method":"initialize",
            "params":{"protocolVersion":MCP_PROTOCOL_VERSION,"capabilities":{},"clientInfo":{"name":"pocket-pi-agentos","version":"0.1.0"}}
        });
        state.next_id = state.next_id.saturating_add(1);
        let (body, session) = post_mcp(token, None, &request)?;
        if let Some(error) = body.get("error") {
            return Err(format!("Robinhood MCP initialize failed: {error}"));
        }
        let session = session.ok_or_else(|| "Robinhood MCP omitted session id".to_owned())?;
        let notification = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        let _ = post_mcp(token, Some(&session), &notification)?;
        state.session_id = Some(session);
        log::info!("Robinhood MCP session initialized");
        Ok(())
    }

    fn robinhood_once(
        &self,
        token: &str,
        state: &mut McpState,
        operation: &str,
        args: &Value,
    ) -> Result<Value, String> {
        let operation_started = Instant::now();
        log::info!("Robinhood MCP {operation} started");
        self.ensure_robinhood_session(token, state)?;
        let request = json!({
            "jsonrpc":"2.0",
            "id":state.next_id,
            "method":"tools/call",
            "params":{"name":operation,"arguments":args}
        });
        state.next_id = state.next_id.saturating_add(1);
        let (body, returned_session) = post_mcp(token, state.session_id.as_deref(), &request)?;
        if returned_session.is_some() {
            state.session_id = returned_session;
        }
        let value = normalize_mcp_result(&body)?;
        log::info!(
            "Robinhood MCP {operation} completed in {}ms",
            operation_started.elapsed().as_millis()
        );
        Ok(value)
    }
}

impl AppServiceHost for EspAppServices {
    fn call(
        &self,
        app_id: &str,
        service: &str,
        operation: &str,
        args: &Value,
    ) -> Result<Value, String> {
        if !self.inner.network_ready.load(Ordering::Acquire) {
            return Err("Network is not connected; App data was not changed".to_owned());
        }
        match (app_id, service, operation) {
            ("robinhood", "mcp.client", "callTool") => self.inner.robinhood_call(args),
            ("robinhood", "mcp.client", "callTools") => self.inner.robinhood_calls(args),
            _ => Err(format!("App {app_id} cannot access service {service}")),
        }
    }

    fn http(
        &self,
        app_id: &str,
        request: HttpRequest,
    ) -> std::result::Result<TransportCompletion, NetFailure> {
        if !self.inner.network_ready.load(Ordering::Acquire) {
            return Err(NetFailure::new("unavailable", "network is not connected"));
        }
        if app_id != "exa" {
            return Err(NetFailure::new(
                "invalid_request",
                format!("App {app_id} has no NET capability"),
            ));
        }
        self.inner.exa_http(request)
    }
}

fn client(timeout: Duration) -> Result<HttpClient<EspHttpConnection>, String> {
    EspHttpConnection::new(&Configuration {
        buffer_size: Some(8 * 1024),
        buffer_size_tx: Some(4 * 1024),
        timeout: Some(timeout),
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        ..Default::default()
    })
    .map(HttpClient::wrap)
    .map_err(|error| format!("initialize HTTPS client: {error}"))
}

fn stale_mcp_session(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    (lower.contains("mcp http 400") || lower.contains("mcp http 404")) && lower.contains("session")
}

fn transient_mcp_connect(error: &str) -> bool {
    error.contains("ESP_ERR_HTTP_CONNECT")
}

fn execute_exa_http(
    meta: HttpRequest,
    api_key: &str,
) -> std::result::Result<TransportCompletion, NetFailure> {
    let length = meta.body.len().to_string();
    let headers = [
        ("accept", "application/json"),
        ("content-type", "application/json"),
        ("content-length", length.as_str()),
        ("connection", "close"),
        ("user-agent", "pocket-pi-agentos/0.1"),
        ("x-api-key", api_key),
    ];
    let mut client =
        client(Duration::from_millis(u64::from(meta.timeout_ms))).map_err(net_failure)?;
    let mut request = client
        .request(Method::Post, &meta.url, &headers)
        .map_err(|error| net_failure(format!("create HTTPS request: {error}")))?;
    request
        .write_all(&meta.body)
        .map_err(|error| net_failure(format!("write HTTPS request: {error}")))?;
    request
        .flush()
        .map_err(|error| net_failure(format!("flush HTTPS request: {error}")))?;
    let mut response = request
        .submit()
        .map_err(|error| net_failure(format!("send HTTPS request: {error}")))?;
    let status = response.status();
    let content_type = response.header("content-type").map(str::to_owned);
    let expected_length = response
        .header("content-length")
        .and_then(|value| value.parse::<usize>().ok());
    let body = read_bounded(&mut response, meta.max_bytes, expected_length).map_err(net_failure)?;
    let mut response_headers = BTreeMap::new();
    if let Some(content_type) = content_type {
        response_headers.insert("content-type".to_owned(), content_type);
    }
    Ok(TransportCompletion::Done {
        handle: meta.handle,
        status,
        url: meta.url,
        headers: response_headers,
        body,
    })
}

fn net_failure(message: String) -> NetFailure {
    let lower = message.to_ascii_lowercase();
    let code = if lower.contains("exceeded") {
        "response_too_large"
    } else if lower.contains("timeout") {
        "timeout"
    } else if lower.contains("tls") || lower.contains("certificate") {
        "tls"
    } else if lower.contains("connect") {
        "connect"
    } else {
        "other"
    };
    NetFailure::new(code, message)
}

fn post_mcp(
    token: &str,
    session: Option<&str>,
    body: &Value,
) -> Result<(Value, Option<String>), String> {
    let request_label = body
        .get("params")
        .and_then(|params| params.get("name"))
        .and_then(Value::as_str)
        .or_else(|| body.get("method").and_then(Value::as_str))
        .unwrap_or("unknown");
    let payload = body.to_string();
    let length = payload.len().to_string();
    let authorization = format!("Bearer {token}");
    let mut headers = vec![
        ("accept", "application/json, text/event-stream"),
        ("content-type", "application/json"),
        ("content-length", length.as_str()),
        ("authorization", authorization.as_str()),
        ("user-agent", "pocket-pi-agentos/0.1"),
    ];
    if let Some(session) = session {
        headers.push(("mcp-session-id", session));
    }
    let mut client = client(Duration::from_secs(12))?;
    let mut request = client
        .request(Method::Post, ROBINHOOD_MCP_URL, &headers)
        .map_err(|error| format!("create Robinhood MCP request: {error}"))?;
    request
        .write_all(payload.as_bytes())
        .map_err(|error| format!("write Robinhood MCP request: {error}"))?;
    request
        .flush()
        .map_err(|error| format!("flush Robinhood MCP request: {error}"))?;
    let submit_started = Instant::now();
    let mut response = request
        .submit()
        .map_err(|error| format!("send Robinhood MCP request: {error}"))?;
    let status = response.status();
    log::info!(
        "Robinhood MCP HTTP {request_label} headers status={status} in {}ms",
        submit_started.elapsed().as_millis()
    );
    let returned_session = response.header("Mcp-Session-Id").map(str::to_owned);
    let is_event_stream = response
        .header("content-type")
        .is_some_and(|value| value.to_ascii_lowercase().contains("text/event-stream"));
    // JSON-RPC notifications have no response payload. The MCP endpoint keeps
    // its HTTP connection alive, so waiting for EOF here would block forever.
    if body.get("id").is_none() && (200..300).contains(&status) {
        return Ok((Value::Null, returned_session));
    }
    let body_started = Instant::now();
    let bytes = if is_event_stream {
        read_sse_event(&mut response, MAX_MCP_RESPONSE)?
    } else {
        let expected_length = response
            .header("content-length")
            .and_then(|value| value.parse::<usize>().ok());
        read_bounded(&mut response, MAX_MCP_RESPONSE, expected_length)?
    };
    log::info!(
        "Robinhood MCP HTTP {request_label} body bytes={} in {}ms",
        bytes.len(),
        body_started.elapsed().as_millis()
    );
    if !(200..300).contains(&status) {
        return Err(format!(
            "Robinhood MCP HTTP {status}: {}",
            String::from_utf8_lossy(&bytes)
        ));
    }
    let body = parse_json_or_sse(&bytes)?;
    Ok((body, returned_session))
}

fn post_mcp_batch(
    token: &str,
    session: Option<&str>,
    bodies: &[Value],
    expected_responses: usize,
) -> Result<(Vec<Value>, Option<String>), String> {
    let payload = serde_json::to_string(bodies)
        .map_err(|error| format!("encode Robinhood MCP batch: {error}"))?;
    let length = payload.len().to_string();
    let authorization = format!("Bearer {token}");
    let mut headers = vec![
        ("accept", "application/json, text/event-stream"),
        ("content-type", "application/json"),
        ("content-length", length.as_str()),
        ("authorization", authorization.as_str()),
        ("user-agent", "pocket-pi-agentos/0.1"),
    ];
    if let Some(session) = session {
        headers.push(("mcp-session-id", session));
    }
    let mut client = client(Duration::from_secs(12))?;
    let mut request = client
        .request(Method::Post, ROBINHOOD_MCP_URL, &headers)
        .map_err(|error| format!("create Robinhood MCP batch request: {error}"))?;
    request
        .write_all(payload.as_bytes())
        .map_err(|error| format!("write Robinhood MCP batch request: {error}"))?;
    request
        .flush()
        .map_err(|error| format!("flush Robinhood MCP batch request: {error}"))?;
    let submit_started = Instant::now();
    let mut response = request
        .submit()
        .map_err(|error| format!("send Robinhood MCP batch request: {error}"))?;
    let status = response.status();
    log::info!(
        "Robinhood MCP HTTP batch headers status={status} in {}ms",
        submit_started.elapsed().as_millis()
    );
    let returned_session = response.header("Mcp-Session-Id").map(str::to_owned);
    if !(200..300).contains(&status) {
        let bytes = read_bounded(&mut response, MAX_MCP_RESPONSE, None)?;
        return Err(format!(
            "Robinhood MCP batch HTTP {status}: {}",
            String::from_utf8_lossy(&bytes)
        ));
    }
    let body_started = Instant::now();
    let responses = read_sse_values(&mut response, MAX_MCP_RESPONSE, expected_responses)?;
    log::info!(
        "Robinhood MCP HTTP batch responses={} in {}ms",
        responses.len(),
        body_started.elapsed().as_millis()
    );
    Ok((responses, returned_session))
}

fn read_bounded(
    reader: &mut impl embedded_svc::io::Read,
    limit: usize,
    expected_length: Option<usize>,
) -> Result<Vec<u8>, String> {
    if expected_length.is_some_and(|length| length > limit) {
        return Err(format!("HTTPS response exceeded {limit} bytes"));
    }
    let mut out = Vec::with_capacity(8 * 1024);
    let mut chunk = [0u8; 2048];
    loop {
        let count = reader
            .read(&mut chunk)
            .map_err(|error| format!("read HTTPS response: {error:?}"))?;
        if count == 0 {
            if let Some(expected) = expected_length {
                if out.len() < expected {
                    return Err(format!(
                        "HTTPS response ended after {} of {expected} bytes",
                        out.len()
                    ));
                }
            }
            break;
        }
        if out.len().saturating_add(count) > limit {
            return Err(format!("HTTPS response exceeded {limit} bytes"));
        }
        out.extend_from_slice(&chunk[..count]);
        if expected_length.is_some_and(|expected| out.len() >= expected) {
            break;
        }
    }
    Ok(out)
}

fn read_sse_event(
    reader: &mut impl embedded_svc::io::Read,
    limit: usize,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(8 * 1024);
    // Stop after the first complete SSE event because the server keeps this
    // connection alive. Reading one byte at a time made a normal MCP payload
    // expensive enough to trip the ESP32 task watchdog, so consume the bytes
    // already available from esp_http_client in bounded chunks instead.
    let mut chunk = [0u8; 512];
    loop {
        let count = reader
            .read(&mut chunk)
            .map_err(|error| format!("read MCP SSE response: {error:?}"))?;
        if count == 0 {
            break;
        }
        if out.len().saturating_add(count) > limit {
            return Err(format!("MCP SSE response exceeded {limit} bytes"));
        }
        out.extend_from_slice(&chunk[..count]);
        if let Some(end) = sse_event_end(&out) {
            out.truncate(end);
            return Ok(out);
        }
    }
    Ok(out)
}

fn read_sse_values<R>(
    reader: &mut R,
    limit: usize,
    expected: usize,
) -> Result<Vec<Value>, String>
where
    R: embedded_svc::io::Read<Error = EspIOError>,
{
    let mut buffered = Vec::with_capacity(16 * 1024);
    let mut consumed = 0;
    let mut values = Vec::with_capacity(expected);
    let mut chunk = [0u8; 2048];
    let deadline = Instant::now() + Duration::from_secs(120);
    while values.len() < expected {
        let count = match reader.read(&mut chunk) {
            Ok(count) => count,
            Err(error)
                if error.0.code() == -esp_idf_svc::sys::ESP_ERR_HTTP_EAGAIN
                    && Instant::now() < deadline =>
            {
                log::info!(
                    "Robinhood MCP batch waiting for SSE responses ({}/{expected})",
                    values.len()
                );
                continue;
            }
            Err(error) if error.0.code() == -esp_idf_svc::sys::ESP_ERR_HTTP_EAGAIN => {
                return Err(format!(
                    "Robinhood MCP batch timed out after receiving {} of {expected} responses",
                    values.len()
                ));
            }
            Err(error) => return Err(format!("read MCP batch SSE response: {error:?}")),
        };
        if count == 0 {
            break;
        }
        if buffered.len().saturating_add(count) > limit {
            return Err(format!("MCP batch SSE response exceeded {limit} bytes"));
        }
        buffered.extend_from_slice(&chunk[..count]);
        while let Some(relative_end) = buffered[consumed..].iter().position(|byte| *byte == b'\n') {
            let end = consumed + relative_end;
            let line = &buffered[consumed..end];
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if let Some(data) = line.strip_prefix(b"data:") {
                let data = data.strip_prefix(b" ").unwrap_or(data);
                values.push(
                    serde_json::from_slice(data)
                        .map_err(|error| format!("parse MCP batch SSE JSON: {error}"))?,
                );
            }
            consumed = end + 1;
        }
    }
    if values.len() != expected {
        return Err(format!(
            "Robinhood MCP batch returned {} of {expected} responses",
            values.len()
        ));
    }
    Ok(values)
}

fn sse_event_end(bytes: &[u8]) -> Option<usize> {
    // Robinhood sends one complete JSON value on a `data:` line but may keep
    // the chunked stream open without promptly sending the optional blank
    // event separator. A terminated data line is already a complete response.
    for start in 0..bytes.len() {
        if (start == 0 || bytes[start - 1] == b'\n') && bytes[start..].starts_with(b"data:") {
            if let Some(relative_end) = bytes[start..].iter().position(|byte| *byte == b'\n') {
                return Some(start + relative_end + 1);
            }
        }
    }
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .or_else(|| {
            bytes
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|index| index + 2)
        })
}

fn parse_json_or_sse(bytes: &[u8]) -> Result<Value, String> {
    // JSON-RPC notifications (notably MCP notifications/initialized) have no
    // response body on success. Treat that as a valid null result; tools/call
    // still goes through normalize_mcp_result and therefore requires result.
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Value::Null);
    }
    if let Ok(value) = serde_json::from_slice(bytes) {
        return Ok(value);
    }
    for line in String::from_utf8_lossy(bytes).lines().rev() {
        if let Some(data) = line.strip_prefix("data: ") {
            return serde_json::from_str(data)
                .map_err(|error| format!("parse MCP SSE JSON: {error}"));
        }
    }
    Err("MCP response was neither JSON nor SSE".to_owned())
}

fn normalize_mcp_result(body: &Value) -> Result<Value, String> {
    if let Some(error) = body.get("error") {
        return Err(format!("Robinhood MCP error: {error}"));
    }
    let result = body
        .get("result")
        .ok_or_else(|| "Robinhood MCP response omitted result".to_owned())?;
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|item| item.get("type").and_then(Value::as_str) == Some("text"))
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .ok_or_else(|| "Robinhood MCP tool returned no text".to_owned())?;
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(text.to_owned());
    }
    Ok(serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned())))
}
