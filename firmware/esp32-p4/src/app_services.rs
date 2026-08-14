use core::time::Duration;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use embedded_svc::http::{
    client::{Client as HttpClient, Request, Response},
    Method,
};
use embedded_svc::io::Write as _;
use esp_idf_svc::http::client::{Configuration, EspHttpConnection};
use esp_idf_svc::io::EspIOError;
use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition};
use pocket_pi_agentos::{
    AppServiceHost, CredentialBinding, HttpRequest, InstalledAppIndex, McpServicePolicy,
    NetFailure, TransportCompletion,
};
use serde_json::{json, Value};

use super::delay_current_task;

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const MAX_MCP_RESPONSE: usize = 160 * 1024;
const MCP_RETRY_DELAY: Duration = Duration::from_millis(250);
const APP_NVS_NAMESPACE: &str = "pocket_apps";
const APP_CREDENTIALS_KEY: &str = "credentials";

pub struct EspAppServices {
    inner: Arc<EspAppServicesInner>,
}

struct EspAppServicesInner {
    network_ready: Arc<AtomicBool>,
    catalog: InstalledAppIndex,
    credentials: Mutex<BTreeMap<String, String>>,
    nvs: Option<EspDefaultNvsPartition>,
    mcp: Mutex<BTreeMap<(String, String), McpState>>,
}

#[derive(Default)]
struct McpState {
    session_id: Option<String>,
    next_id: u64,
}

impl EspAppServices {
    pub fn new(
        network_ready: Arc<AtomicBool>,
        catalog: InstalledAppIndex,
        nvs: Option<EspDefaultNvsPartition>,
    ) -> Self {
        let mut credentials = nvs
            .as_ref()
            .and_then(|partition| load_credentials(partition.clone()).ok())
            .unwrap_or_default();
        let required = catalog.credential_ids();
        credentials.retain(|id, _| required.contains(id));
        let inner = Arc::new(EspAppServicesInner {
            network_ready,
            catalog,
            credentials: Mutex::new(credentials),
            nvs,
            mcp: Mutex::new(BTreeMap::new()),
        });
        Self { inner }
    }
}

impl EspAppServicesInner {
    fn http(
        &self,
        app_id: &str,
        request: HttpRequest,
        deadline: Instant,
    ) -> std::result::Result<TransportCompletion, NetFailure> {
        let policy = self
            .catalog
            .http_policy(app_id, &request.method, &request.url)
            .ok_or_else(|| NetFailure::new("invalid_request", "HTTP request is not allowed"))?;
        if request.headers.keys().any(|name| {
            !policy
                .allowed_request_headers
                .iter()
                .any(|item| item == name)
        }) {
            return Err(NetFailure::new(
                "invalid_request",
                "App supplied a forbidden HTTP header",
            ));
        }
        let credentials = self
            .credentials
            .lock()
            .map_err(|_| NetFailure::new("unavailable", "credential store lock was poisoned"))?;
        execute_http(request, policy.credential.as_ref(), &credentials, deadline)
    }

    fn mcp_call(&self, app_id: &str, args: &Value, deadline: Instant) -> Result<Value, String> {
        let connection = args
            .get("connection")
            .and_then(Value::as_str)
            .ok_or_else(|| "mcp.client requires connection".to_owned())?;
        let policy = self
            .catalog
            .mcp_policy(app_id, connection)
            .ok_or_else(|| "App requested an unknown MCP connection".to_owned())?;
        let operation = args
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "mcp.client callTool requires name".to_owned())?;
        let arguments = args.get("arguments").unwrap_or(&Value::Null);
        if !self.catalog.provider_operation_allowed(app_id, operation) {
            return Err(format!("MCP operation is not allowlisted: {operation}"));
        }
        let credential = self.credential(&policy.credential)?;
        let retryable = args
            .get("retryable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut state = self
            .mcp
            .lock()
            .map_err(|_| "MCP state lock was poisoned".to_owned())?;
        let state = state
            .entry((app_id.to_owned(), connection.to_owned()))
            .or_insert_with(|| McpState {
                session_id: None,
                next_id: 1,
            });
        for attempt in 0..2 {
            let result = self.mcp_once(
                &policy,
                &credential,
                state,
                operation,
                arguments,
                retryable,
                deadline,
            );
            match result {
                Ok(value) => return Ok(value),
                Err(error) if attempt == 0 && stale_mcp_session(&error) => {
                    log::warn!("MCP reconnecting after: {error}");
                    state.session_id = None;
                    wait_to_retry(deadline)?;
                }
                Err(error) if attempt == 0 && retryable && transient_mcp_connect(&error) => {
                    log::warn!("MCP transport retry after: {error}");
                    wait_to_retry(deadline)?;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!()
    }

    fn mcp_calls(&self, app_id: &str, args: &Value, deadline: Instant) -> Result<Value, String> {
        let connection = args
            .get("connection")
            .and_then(Value::as_str)
            .ok_or_else(|| "mcp.client requires connection".to_owned())?;
        let policy = self
            .catalog
            .mcp_policy(app_id, connection)
            .ok_or_else(|| "App requested an unknown MCP connection".to_owned())?;
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
            if !self.catalog.provider_operation_allowed(app_id, name) {
                return Err(format!("MCP operation is not allowlisted: {name}"));
            }
        }
        let credential = self.credential(&policy.credential)?;
        let retryable = args
            .get("retryable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut state = self
            .mcp
            .lock()
            .map_err(|_| "MCP state lock was poisoned".to_owned())?;
        let state = state
            .entry((app_id.to_owned(), connection.to_owned()))
            .or_insert_with(|| McpState {
                session_id: None,
                next_id: 1,
            });
        for attempt in 0..2 {
            let result =
                self.mcp_batch_once(&policy, &credential, state, calls, retryable, deadline);
            match result {
                Ok(value) => return Ok(value),
                Err(error) if attempt == 0 && stale_mcp_session(&error) => {
                    log::warn!("MCP batch reconnecting after: {error}");
                    state.session_id = None;
                    wait_to_retry(deadline)?;
                }
                Err(error) if attempt == 0 && retryable && transient_mcp_connect(&error) => {
                    log::warn!("MCP batch transport retry after: {error}");
                    wait_to_retry(deadline)?;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!()
    }

    fn credential(&self, binding: &CredentialBinding) -> Result<String, String> {
        self.credentials
            .lock()
            .map_err(|_| "credential store lock was poisoned".to_owned())?
            .get(&binding.id)
            .map(|secret| format!("{}{}", binding.prefix, secret))
            .ok_or_else(|| format!("credential {} was not provisioned", binding.id))
    }

    fn mcp_batch_once(
        &self,
        policy: &McpServicePolicy,
        credential: &str,
        state: &mut McpState,
        calls: &[Value],
        retryable: bool,
        deadline: Instant,
    ) -> Result<Value, String> {
        self.ensure_mcp_session(policy, credential, state, deadline)?;
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
            policy,
            credential,
            state.session_id.as_deref(),
            &requests,
            request_ids.len(),
            retryable,
            deadline,
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
                .ok_or_else(|| format!("MCP batch omitted response for {name}"))?;
            match normalize_mcp_result(&response) {
                Ok(value) => results.push(json!({"name":name,"ok":true,"value":value})),
                Err(error) => results.push(json!({"name":name,"ok":false,"error":error})),
            }
        }
        log::info!(
            "MCP batch calls={} completed in {}ms",
            calls.len(),
            started.elapsed().as_millis()
        );
        Ok(json!({"results":results}))
    }

    fn ensure_mcp_session(
        &self,
        policy: &McpServicePolicy,
        credential: &str,
        state: &mut McpState,
        deadline: Instant,
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
        let (body, session) = post_mcp(policy, credential, None, &request, true, deadline)?;
        if let Some(error) = body.get("error") {
            return Err(format!("MCP initialize failed: {error}"));
        }
        let session = session.ok_or_else(|| "MCP omitted session id".to_owned())?;
        let notification = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        let _ = post_mcp(
            policy,
            credential,
            Some(&session),
            &notification,
            true,
            deadline,
        )?;
        state.session_id = Some(session);
        log::info!("MCP session initialized");
        Ok(())
    }

    fn mcp_once(
        &self,
        policy: &McpServicePolicy,
        credential: &str,
        state: &mut McpState,
        operation: &str,
        args: &Value,
        retryable: bool,
        deadline: Instant,
    ) -> Result<Value, String> {
        let operation_started = Instant::now();
        log::info!("MCP {operation} started");
        self.ensure_mcp_session(policy, credential, state, deadline)?;
        let request = json!({
            "jsonrpc":"2.0",
            "id":state.next_id,
            "method":"tools/call",
            "params":{"name":operation,"arguments":args}
        });
        state.next_id = state.next_id.saturating_add(1);
        let (body, returned_session) = post_mcp(
            policy,
            credential,
            state.session_id.as_deref(),
            &request,
            retryable,
            deadline,
        )?;
        if returned_session.is_some() {
            state.session_id = returned_session;
        }
        let value = normalize_mcp_result(&body)?;
        log::info!(
            "MCP {operation} completed in {}ms",
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
        deadline: Instant,
    ) -> Result<Value, String> {
        if !self.inner.network_ready.load(Ordering::Acquire) {
            return Err("Network is not connected; App data was not changed".to_owned());
        }
        match (service, operation) {
            ("mcp.client", "callTool") => self.inner.mcp_call(app_id, args, deadline),
            ("mcp.client", "callTools") => self.inner.mcp_calls(app_id, args, deadline),
            _ => Err(format!("App {app_id} cannot access service {service}")),
        }
    }

    fn http(
        &self,
        app_id: &str,
        request: HttpRequest,
        deadline: Instant,
    ) -> std::result::Result<TransportCompletion, NetFailure> {
        if !self.inner.network_ready.load(Ordering::Acquire) {
            return Err(NetFailure::new("unavailable", "network is not connected"));
        }
        self.inner.http(app_id, request, deadline)
    }

    fn store_credentials(&self, credentials: &BTreeMap<String, String>) -> Result<(), String> {
        if credentials.is_empty() {
            return Ok(());
        }
        let mut stored = self
            .inner
            .credentials
            .lock()
            .map_err(|_| "credential store lock was poisoned".to_owned())?;
        stored.extend(credentials.clone());
        if let Some(partition) = &self.inner.nvs {
            persist_credentials(partition.clone(), &stored)?;
        }
        Ok(())
    }

    fn remove_app_state(&self, app_id: &str, credential_ids: &[String]) -> Result<(), String> {
        let mut stored = self
            .inner
            .credentials
            .lock()
            .map_err(|_| "credential store lock was poisoned".to_owned())?;
        for id in credential_ids {
            stored.remove(id);
        }
        if let Some(partition) = &self.inner.nvs {
            persist_credentials(partition.clone(), &stored)?;
        }
        drop(stored);
        self.inner
            .mcp
            .lock()
            .map_err(|_| "MCP state lock was poisoned".to_owned())?
            .retain(|(owner, _), _| owner != app_id);
        Ok(())
    }
}

fn load_credentials(partition: EspDefaultNvsPartition) -> Result<BTreeMap<String, String>, String> {
    let storage = EspDefaultNvs::new(partition, APP_NVS_NAMESPACE, true)
        .map_err(|error| format!("open App credential NVS: {error}"))?;
    let Some(length) = storage
        .blob_len(APP_CREDENTIALS_KEY)
        .map_err(|error| format!("read App credential length: {error}"))?
    else {
        return Ok(BTreeMap::new());
    };
    let mut bytes = vec![0; length];
    let bytes = storage
        .get_blob(APP_CREDENTIALS_KEY, &mut bytes)
        .map_err(|error| format!("read App credentials: {error}"))?
        .unwrap_or_default();
    serde_json::from_slice(bytes).map_err(|error| format!("parse App credentials: {error}"))
}

fn persist_credentials(
    partition: EspDefaultNvsPartition,
    credentials: &BTreeMap<String, String>,
) -> Result<(), String> {
    let storage = EspDefaultNvs::new(partition, APP_NVS_NAMESPACE, true)
        .map_err(|error| format!("open App credential NVS: {error}"))?;
    let bytes = serde_json::to_vec(credentials)
        .map_err(|error| format!("encode App credentials: {error}"))?;
    storage
        .set_blob(APP_CREDENTIALS_KEY, &bytes)
        .map_err(|error| format!("store App credentials: {error}"))
}

fn connection(timeout: Duration) -> Result<EspHttpConnection, String> {
    EspHttpConnection::new(&Configuration {
        buffer_size: Some(8 * 1024),
        buffer_size_tx: Some(4 * 1024),
        timeout: Some(timeout),
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        ..Default::default()
    })
    .map_err(|error| format!("initialize HTTPS client: {error}"))
}

fn client(timeout: Duration) -> Result<HttpClient<EspHttpConnection>, String> {
    connection(timeout).map(HttpClient::wrap)
}

fn stale_mcp_session(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    (lower.contains("mcp http 400") || lower.contains("mcp http 404")) && lower.contains("session")
}

fn transient_mcp_connect(error: &str) -> bool {
    error.contains("ESP_ERR_HTTP_CONNECT")
}

fn remaining(deadline: Instant) -> Result<Duration, String> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| "App Data Action deadline expired".to_owned())
}

fn yield_current_task() {
    unsafe { esp_idf_svc::sys::vTaskDelay(1) };
}

fn wait_to_retry(deadline: Instant) -> Result<(), String> {
    let delay = remaining(deadline)?.min(MCP_RETRY_DELAY);
    delay_current_task(delay);
    remaining(deadline).map(|_| ())
}

fn execute_http(
    meta: HttpRequest,
    credential: Option<&CredentialBinding>,
    credentials: &BTreeMap<String, String>,
    deadline: Instant,
) -> std::result::Result<TransportCompletion, NetFailure> {
    let length = meta.body.len().to_string();
    let mut values = meta.headers;
    values.insert("content-length".into(), length);
    values.insert("connection".into(), "close".into());
    values.insert("user-agent".into(), "pocket-pi-agentos/0.1".into());
    if let Some(binding) = credential {
        let secret = credentials.get(&binding.id).ok_or_else(|| {
            NetFailure::new(
                "unavailable",
                format!("credential {} was not provisioned", binding.id),
            )
        })?;
        values.insert(
            binding.header.clone(),
            format!("{}{}", binding.prefix, secret),
        );
    }
    let headers = values
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    let mut client = client(remaining(deadline).map_err(net_failure)?).map_err(net_failure)?;
    let mut request = client
        .request(http_method(&meta.method)?, &meta.url, &headers)
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
    let body = read_bounded(&mut response, meta.max_bytes, expected_length, deadline)
        .map_err(net_failure)?;
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

fn http_method(method: &str) -> std::result::Result<Method, NetFailure> {
    match method {
        "GET" => Ok(Method::Get),
        "POST" => Ok(Method::Post),
        "PUT" => Ok(Method::Put),
        "DELETE" => Ok(Method::Delete),
        "PATCH" => Ok(Method::Patch),
        _ => Err(NetFailure::new(
            "invalid_request",
            format!("unsupported HTTP method {method}"),
        )),
    }
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

fn submit_mcp(
    policy: &McpServicePolicy,
    credential: &str,
    session: Option<&str>,
    payload: &str,
    request_label: &str,
    retryable: bool,
    deadline: Instant,
) -> Result<Response<EspHttpConnection>, String> {
    let length = payload.len().to_string();
    for attempt in 0..2 {
        let mut headers = vec![
            ("accept", "application/json, text/event-stream"),
            ("content-type", "application/json"),
            ("content-length", length.as_str()),
            (policy.credential.header.as_str(), credential),
            ("user-agent", "pocket-pi-agentos/0.1"),
        ];
        if let Some(session) = session {
            headers.push(("mcp-session-id", session));
        }
        let mut connection = connection(remaining(deadline)?)?;
        connection
            .initiate_request(Method::Post, &policy.url, &headers)
            .map_err(|error| format!("create MCP request: {error:?}"))?;
        let mut request = Request::wrap(connection);
        request
            .write_all(payload.as_bytes())
            .map_err(|error| format!("write MCP request: {error:?}"))?;
        request
            .flush()
            .map_err(|error| format!("flush MCP request: {error:?}"))?;
        let started = Instant::now();
        match request.submit() {
            Ok(response) => {
                log::info!(
                    "MCP HTTP {request_label} headers status={} in {}ms",
                    response.status(),
                    started.elapsed().as_millis()
                );
                return Ok(response);
            }
            Err(error)
                if attempt == 0
                    && retryable
                    && error.0.code() == -esp_idf_svc::sys::ESP_ERR_HTTP_EAGAIN =>
            {
                log::warn!(
                    "MCP {request_label} received no response headers in {}ms; reconnecting once",
                    started.elapsed().as_millis()
                );
                wait_to_retry(deadline)?;
            }
            Err(error) => {
                return Err(format!("send MCP request: {error:?}"));
            }
        }
    }
    unreachable!()
}

fn post_mcp(
    policy: &McpServicePolicy,
    credential: &str,
    session: Option<&str>,
    body: &Value,
    retryable: bool,
    deadline: Instant,
) -> Result<(Value, Option<String>), String> {
    let request_label = body
        .get("params")
        .and_then(|params| params.get("name"))
        .and_then(Value::as_str)
        .or_else(|| body.get("method").and_then(Value::as_str))
        .unwrap_or("unknown");
    let payload = body.to_string();
    let mut response = submit_mcp(
        policy,
        credential,
        session,
        &payload,
        request_label,
        retryable,
        deadline,
    )?;
    let status = response.status();
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
        read_sse_event(&mut response, MAX_MCP_RESPONSE, deadline)?
    } else {
        let expected_length = response
            .header("content-length")
            .and_then(|value| value.parse::<usize>().ok());
        read_bounded(&mut response, MAX_MCP_RESPONSE, expected_length, deadline)?
    };
    log::info!(
        "MCP HTTP {request_label} body bytes={} in {}ms",
        bytes.len(),
        body_started.elapsed().as_millis()
    );
    if !(200..300).contains(&status) {
        return Err(format!(
            "MCP HTTP {status}: {}",
            String::from_utf8_lossy(&bytes)
        ));
    }
    let body = parse_json_or_sse(&bytes)?;
    Ok((body, returned_session))
}

fn post_mcp_batch(
    policy: &McpServicePolicy,
    credential: &str,
    session: Option<&str>,
    bodies: &[Value],
    expected_responses: usize,
    retryable: bool,
    deadline: Instant,
) -> Result<(Vec<Value>, Option<String>), String> {
    let payload =
        serde_json::to_string(bodies).map_err(|error| format!("encode MCP batch: {error}"))?;
    let mut response = submit_mcp(
        policy, credential, session, &payload, "batch", retryable, deadline,
    )?;
    let status = response.status();
    let returned_session = response.header("Mcp-Session-Id").map(str::to_owned);
    if !(200..300).contains(&status) {
        let bytes = read_bounded(&mut response, MAX_MCP_RESPONSE, None, deadline)?;
        return Err(format!(
            "MCP batch HTTP {status}: {}",
            String::from_utf8_lossy(&bytes)
        ));
    }
    let body_started = Instant::now();
    let responses = read_sse_values(
        &mut response,
        MAX_MCP_RESPONSE,
        expected_responses,
        deadline,
    )?;
    log::info!(
        "MCP HTTP batch responses={} in {}ms",
        responses.len(),
        body_started.elapsed().as_millis()
    );
    Ok((responses, returned_session))
}

fn read_bounded(
    reader: &mut impl embedded_svc::io::Read,
    limit: usize,
    expected_length: Option<usize>,
    deadline: Instant,
) -> Result<Vec<u8>, String> {
    if expected_length.is_some_and(|length| length > limit) {
        return Err(format!("HTTPS response exceeded {limit} bytes"));
    }
    let mut out = Vec::with_capacity(8 * 1024);
    let mut chunk = [0u8; 2048];
    loop {
        remaining(deadline)?;
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
    deadline: Instant,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(8 * 1024);
    // Stop after the first complete SSE event because the server keeps this
    // connection alive. Reading one byte at a time made a normal MCP payload
    // expensive enough to trip the ESP32 task watchdog, so consume the bytes
    // already available from esp_http_client in bounded chunks instead.
    let mut chunk = [0u8; 512];
    loop {
        remaining(deadline)?;
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
    deadline: Instant,
) -> Result<Vec<Value>, String>
where
    R: embedded_svc::io::Read<Error = EspIOError>,
{
    let mut buffered = Vec::with_capacity(16 * 1024);
    let mut consumed = 0;
    let mut values = Vec::with_capacity(expected);
    let mut chunk = [0u8; 2048];
    let mut waiting_logged = false;
    while values.len() < expected {
        remaining(deadline)?;
        let count = match reader.read(&mut chunk) {
            Ok(count) => count,
            Err(error)
                if error.0.code() == -esp_idf_svc::sys::ESP_ERR_HTTP_EAGAIN
                    && Instant::now() < deadline =>
            {
                if !waiting_logged {
                    log::info!(
                        "MCP batch waiting for SSE responses ({}/{expected})",
                        values.len()
                    );
                    waiting_logged = true;
                }
                yield_current_task();
                continue;
            }
            Err(error) if error.0.code() == -esp_idf_svc::sys::ESP_ERR_HTTP_EAGAIN => {
                return Err(format!(
                    "MCP batch timed out after receiving {} of {expected} responses",
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
            "MCP batch returned {} of {expected} responses",
            values.len()
        ));
    }
    Ok(values)
}

fn sse_event_end(bytes: &[u8]) -> Option<usize> {
    // Some MCP servers send one complete JSON value on a `data:` line but keep
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
        return Err(format!("MCP error: {error}"));
    }
    let result = body
        .get("result")
        .ok_or_else(|| "MCP response omitted result".to_owned())?;
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|item| item.get("type").and_then(Value::as_str) == Some("text"))
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .ok_or_else(|| "MCP tool returned no text".to_owned())?;
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(text.to_owned());
    }
    Ok(serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned())))
}
