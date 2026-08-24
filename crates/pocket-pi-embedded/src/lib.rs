use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;

use pocket_mod::qjs::{CatchResultExt, Function, Object};
use pocket_mod::Guest;
pub use pocket_pi_protocols::model::ModelStreamEvent;

const PRELUDE: &str = include_str!("../js/prelude.js");
pub const MODEL_WORKER_STACK_BYTES: usize = 64 * 1024;
const TOOL_WORKER_STACK_BYTES: usize = 16 * 1024;

pub trait ModelBackend: Send + Sync {
    fn complete(
        &self,
        request_json: &str,
        on_event: &mut dyn FnMut(ModelStreamEvent),
    ) -> Result<String, String>;
}

pub trait ToolHost: Send + Sync {
    fn definitions(&self) -> Vec<serde_json::Value>;
    fn execute(&self, call_id: &str, name: &str, args_json: &str) -> ToolResult;
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ToolResult {
    pub text: String,
    pub details: serde_json::Value,
    pub is_error: bool,
    pub terminate: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentEvent {
    Ready,
    ResponseText(String),
    Done,
    Failed(String),
}

impl ToolResult {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            details: serde_json::Value::Null,
            ..Self::default()
        }
    }

    fn to_json(&self) -> String {
        serde_json::json!({
            "text": self.text,
            "details": self.details,
            "isError": self.is_error,
            "terminate": self.terminate,
        })
        .to_string()
    }
}

/// The Pi Agent half of the Pi Agent System App.
///
/// This object does not own a QuickJS runtime. It mounts pi-agent-core into
/// the Root App's existing PocketJS Guest, so the Agent Loop, context, tools
/// and Root View have one App lifecycle. Slow model and tool work happens on
/// worker threads; `tick` only delivers completed events into the Guest.
pub struct GuestAgent;

impl GuestAgent {
    pub fn mount_source(
        guest: &Guest,
        config_json: &str,
        backend: Arc<dyn ModelBackend>,
        tools: Arc<dyn ToolHost>,
        agent_source: &str,
    ) -> Result<Self, String> {
        let mount_started = Instant::now();
        let config_json = config_with_tools(config_json, tools.definitions())?;
        log::info!(
            "diag agent.loop phase=mount_start config_bytes={} source_bytes={}",
            config_json.len(),
            agent_source.len()
        );
        let (host_tx, host_rx) = mpsc::channel::<serde_json::Value>();
        let host_rx = Arc::new(Mutex::new(host_rx));
        let next_request = Arc::new(AtomicI32::new(1));
        let (model_tx, model_rx) = mpsc::channel::<(i32, String)>();
        let model_host_tx = host_tx.clone();
        std::thread::Builder::new()
            .name("pi-model".into())
            .stack_size(MODEL_WORKER_STACK_BYTES)
            .spawn(move || {
                while let Ok((id, request)) = model_rx.recv() {
                    let started = Instant::now();
                    let request_bytes = request.len();
                    let mut stream_events = 0usize;
                    let mut thinking_bytes = 0usize;
                    let mut text_bytes = 0usize;
                    log::info!(
                        "diag agent.model phase=start request_id={id} request_bytes={request_bytes}"
                    );
                    let worker_tx = model_host_tx.clone();
                    let completion = {
                        let mut emit = |event| {
                            let event = match event {
                                ModelStreamEvent::Thinking(delta) => {
                                    stream_events += 1;
                                    thinking_bytes += delta.len();
                                    serde_json::json!({
                                        "type":"model_progress",
                                        "id":id,
                                        "thinkingDelta":delta,
                                        "textDelta":"",
                                    })
                                }
                                ModelStreamEvent::Text(delta) => {
                                    stream_events += 1;
                                    text_bytes += delta.len();
                                    serde_json::json!({
                                        "type":"model_progress",
                                        "id":id,
                                        "thinkingDelta":"",
                                        "textDelta":delta,
                                    })
                                }
                            };
                            let _ = worker_tx.send(event);
                        };
                        backend.complete(&request, &mut emit)
                    };
                    match completion {
                        Ok(result) => {
                            log::info!(
                                "diag agent.model phase=done request_id={id} elapsed_ms={} result_bytes={} stream_events={stream_events} thinking_bytes={thinking_bytes} text_bytes={text_bytes}",
                                started.elapsed().as_millis(),
                                result.len()
                            );
                            let _ = worker_tx.send(serde_json::json!({
                                "type":"model_done",
                                "id":id,
                                "result":result,
                            }));
                        }
                        Err(error) => {
                            log::warn!(
                                "diag agent.model phase=failed request_id={id} elapsed_ms={} stream_events={stream_events} thinking_bytes={thinking_bytes} text_bytes={text_bytes} error_bytes={}",
                                started.elapsed().as_millis(),
                                error.len()
                            );
                            let _ = worker_tx.send(serde_json::json!({
                                "type":"model_error",
                                "id":id,
                                "error":format!("model backend failed: {error}"),
                            }));
                        }
                    }
                }
            })
            .map_err(|error| format!("spawn model worker: {error}"))?;

        guest
            .mount("host", {
                let tools = tools.clone();
                move |ctx, host| {
                    host.set(
                        "startModel",
                        Function::new(ctx.clone(), {
                            let model_tx = model_tx.clone();
                            let host_tx = host_tx.clone();
                            let next_request = next_request.clone();
                            move |request: String| -> i32 {
                                let id = next_request.fetch_add(1, Ordering::Relaxed);
                                log::info!(
                                    "diag agent.model phase=queued request_id={id} request_bytes={}",
                                    request.len()
                                );
                                if model_tx.send((id, request)).is_err() {
                                    log::error!(
                                        "diag agent.model phase=queue_failed request_id={id}"
                                    );
                                    let _ = host_tx.send(serde_json::json!({
                                        "type":"model_error",
                                        "id":id,
                                        "error":"model worker stopped",
                                    }));
                                }
                                id
                            }
                        })?,
                    )?;
                    host.set(
                        "startTool",
                        Function::new(ctx.clone(), {
                            let tools = tools.clone();
                            let host_tx = host_tx.clone();
                            let next_request = next_request.clone();
                            move |call_id: String, name: String, args: String| -> i32 {
                                let id = next_request.fetch_add(1, Ordering::Relaxed);
                                let queued_at = Instant::now();
                                log::info!(
                                    "diag agent.tool phase=queued request_id={id} name={name} args_bytes={} call_id_bytes={}",
                                    args.len(),
                                    call_id.len()
                                );
                                let tools = tools.clone();
                                let host_tx = host_tx.clone();
                                let worker_tx = host_tx.clone();
                                let spawn = std::thread::Builder::new()
                                    .name(format!("pi-tool-{id}"))
                                    .stack_size(TOOL_WORKER_STACK_BYTES)
                                    .spawn(move || {
                                        let started = Instant::now();
                                        log::info!(
                                            "diag agent.tool phase=start request_id={id} name={name} queue_ms={}",
                                            queued_at.elapsed().as_millis()
                                        );
                                        let result = tools.execute(&call_id, &name, &args);
                                        log::info!(
                                            "diag agent.tool phase=done request_id={id} name={name} elapsed_ms={} error={} terminate={} result_bytes={}",
                                            started.elapsed().as_millis(),
                                            result.is_error,
                                            result.terminate,
                                            result.text.len()
                                        );
                                        let _ = worker_tx.send(serde_json::json!({
                                            "type":"tool_done",
                                            "id":id,
                                            "result":result.to_json(),
                                        }));
                                    });
                                if let Err(error) = spawn {
                                    log::error!(
                                        "diag agent.tool phase=spawn_failed request_id={id} error_bytes={}",
                                        error.to_string().len()
                                    );
                                    let result = ToolResult {
                                        text: format!("spawn tool worker: {error}"),
                                        is_error: true,
                                        ..ToolResult::default()
                                    };
                                    let _ = host_tx.send(serde_json::json!({
                                        "type":"tool_done",
                                        "id":id,
                                        "result":result.to_json(),
                                    }));
                                }
                                id
                            }
                        })?,
                    )?;
                    host.set(
                        "poll",
                        Function::new(ctx.clone(), {
                            let host_rx = host_rx.clone();
                            move || -> String {
                                let batch = host_rx
                                    .lock()
                                    .map(|receiver| coalesce_host_events(receiver.try_iter()))
                                    .unwrap_or_default();
                                serde_json::to_string(&batch).unwrap_or_else(|_| "[]".to_owned())
                            }
                        })?,
                    )?;
                    Ok(())
                }
            })
            .map_err(|error| error.to_string())?;

        guest
            .eval("pi-agent-prelude", PRELUDE)
            .map_err(|error| error.to_string())?;
        guest
            .eval("pi-agent-core", agent_source)
            .map_err(|error| error.to_string())?;
        call_agent::<_, ()>(guest, "boot", (config_json,))?;
        guest.drain_jobs();

        log::info!(
            "diag agent.loop phase=mount_done elapsed_ms={}",
            mount_started.elapsed().as_millis()
        );

        Ok(Self)
    }

    pub fn prompt(&self, guest: &Guest, text: &str) -> Result<(), String> {
        log::info!(
            "diag agent.loop phase=prompt_start prompt_bytes={}",
            text.len()
        );
        let started = Instant::now();
        let result = call_agent(guest, "prompt", (text.to_owned(),));
        log::info!(
            "diag agent.loop phase=prompt_dispatched elapsed_ms={} ok={}",
            started.elapsed().as_millis(),
            result.is_ok()
        );
        result
    }

    pub fn replace_tools(
        &self,
        guest: &Guest,
        definitions: Vec<serde_json::Value>,
    ) -> Result<(), String> {
        validate_tool_definitions(&definitions)?;
        let definitions = serde_json::to_string(&definitions)
            .map_err(|error| format!("serialize Agent tools: {error}"))?;
        call_agent(guest, "replaceTools", (definitions,))
    }

    pub fn tick(&self, guest: &Guest) -> Result<Vec<AgentEvent>, String> {
        call_agent::<_, ()>(guest, "tick", ())?;
        guest.drain_jobs();
        let raw: String = call_agent(guest, "drain", ())?;
        let payload: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|error| format!("parse Pi Agent events: {error}"))?;
        let mut events = Vec::new();
        let mut terminal_error = None;
        for event in payload["events"].as_array().into_iter().flatten() {
            match event["type"].as_str() {
                Some("agent_ready") => events.push(AgentEvent::Ready),
                Some("message_update") if event["kind"] == "text_delta" => {
                    if let Some(delta) = event["delta"].as_str().filter(|delta| !delta.is_empty()) {
                        events.push(AgentEvent::ResponseText(delta.to_owned()));
                    }
                }
                Some("message_end") => {
                    if event["role"] == "assistant" && event["stopReason"] == "error" {
                        terminal_error = Some(
                            event["errorMessage"]
                                .as_str()
                                .unwrap_or("model request failed")
                                .to_owned(),
                        );
                    }
                }
                Some("agent_end") => {
                    events.push(
                        terminal_error
                            .take()
                            .map_or(AgentEvent::Done, AgentEvent::Failed),
                    );
                }
                Some("agent_error") => events.push(AgentEvent::Failed(
                    event["message"]
                        .as_str()
                        .unwrap_or("Pi Agent failed")
                        .to_owned(),
                )),
                _ => {}
            }
        }
        if !events.is_empty() {
            let response_bytes = events
                .iter()
                .filter_map(|event| match event {
                    AgentEvent::ResponseText(text) => Some(text.len()),
                    _ => None,
                })
                .sum::<usize>();
            let terminal = events
                .iter()
                .any(|event| matches!(event, AgentEvent::Done | AgentEvent::Failed(_)));
            log::info!(
                "diag agent.loop phase=events count={} response_bytes={response_bytes} terminal={terminal}",
                events.len()
            );
        }
        Ok(events)
    }
}

fn coalesce_host_events(
    source: impl IntoIterator<Item = serde_json::Value>,
) -> Vec<serde_json::Value> {
    let mut batch = Vec::<serde_json::Value>::new();
    let mut progress = std::collections::BTreeMap::<i64, (usize, String, String)>::new();
    for event in source {
        if event["type"] != "model_progress" {
            batch.push(event);
            continue;
        }
        let id = event["id"].as_i64().unwrap_or_default();
        let entry = progress.entry(id).or_insert_with(|| {
            batch.push(serde_json::Value::Null);
            (batch.len() - 1, String::new(), String::new())
        });
        entry
            .1
            .push_str(event["thinkingDelta"].as_str().unwrap_or(""));
        entry.2.push_str(event["textDelta"].as_str().unwrap_or(""));
    }
    for (id, (index, thinking, text)) in progress {
        batch[index] = serde_json::json!({
            "type":"model_progress",
            "id":id,
            "thinkingDelta":thinking,
            "textDelta":text,
        });
    }
    batch
}

fn call_agent<A, R>(guest: &Guest, name: &str, args: A) -> Result<R, String>
where
    A: for<'js> pocket_mod::qjs::function::IntoArgs<'js>,
    R: for<'js> pocket_mod::qjs::FromJs<'js>,
{
    guest.with(|ctx| {
        let agent: Object = ctx
            .globals()
            .get("PocketPiEmbedded")
            .map_err(|error| format!("PocketPiEmbedded missing: {error}"))?;
        let function: Function = agent.get(name).map_err(|error| error.to_string())?;
        function
            .call::<_, R>(args)
            .catch(&ctx)
            .map_err(|error| format!("PocketPiEmbedded.{name}: {error}"))
    })
}

fn config_with_tools(
    config_json: &str,
    definitions: Vec<serde_json::Value>,
) -> Result<String, String> {
    let mut config: serde_json::Value =
        serde_json::from_str(config_json).map_err(|error| format!("embedded config: {error}"))?;
    let object = config
        .as_object_mut()
        .ok_or_else(|| "embedded config must be a JSON object".to_owned())?;
    validate_tool_definitions(&definitions)?;
    object.insert("tools".to_owned(), serde_json::Value::Array(definitions));
    serde_json::to_string(&config).map_err(|error| format!("serialize embedded config: {error}"))
}

fn validate_tool_definitions(definitions: &[serde_json::Value]) -> Result<(), String> {
    let mut names = std::collections::BTreeSet::new();
    for definition in definitions {
        let name = definition
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "tool definition is missing a string name".to_owned())?;
        if !names.insert(name.to_owned()) {
            return Err(format!("duplicate tool definition: {name}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    const TEST_AGENT_BUNDLE: &str = include_str!("../js/pi-agent.bundle.js");

    struct BurstBackend;

    impl ModelBackend for BurstBackend {
        fn complete(
            &self,
            _request_json: &str,
            on_event: &mut dyn FnMut(ModelStreamEvent),
        ) -> Result<String, String> {
            for _ in 0..100 {
                on_event(ModelStreamEvent::Text("x".into()));
            }
            Ok(serde_json::json!({
                "thinking":"",
                "text":"x".repeat(100),
                "toolCalls":[],
                "usage":{},
                "stopReason":"stop"
            })
            .to_string())
        }
    }

    struct FailingBackend;

    impl ModelBackend for FailingBackend {
        fn complete(
            &self,
            _request_json: &str,
            _on_event: &mut dyn FnMut(ModelStreamEvent),
        ) -> Result<String, String> {
            Err("network unavailable".into())
        }
    }

    struct Tools;

    impl ToolHost for Tools {
        fn definitions(&self) -> Vec<serde_json::Value> {
            vec![serde_json::json!({
                "name":"echo",
                "description":"Echo",
                "parameters":{"type":"object","properties":{}}
            })]
        }

        fn execute(&self, _call_id: &str, name: &str, _args_json: &str) -> ToolResult {
            ToolResult::text(format!("called {name}"))
        }
    }

    #[test]
    fn tool_definitions_replace_caller_supplied_tools() {
        let config = config_with_tools(
            r#"{"model":"offline","tools":[{"name":"wrong"}]}"#,
            Tools.definitions(),
        )
        .unwrap();
        let config: serde_json::Value = serde_json::from_str(&config).unwrap();
        assert_eq!(config["tools"][0]["name"], "echo");
        assert_eq!(config["tools"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn model_progress_is_coalesced_before_reaching_the_embedded_ui() {
        let guest = Guest::new().unwrap();
        let agent = GuestAgent::mount_source(
            &guest,
            r#"{"model":"offline"}"#,
            Arc::new(BurstBackend),
            Arc::new(Tools),
            TEST_AGENT_BUNDLE,
        )
        .unwrap();
        agent.prompt(&guest, "burst").unwrap();

        let mut deltas = Vec::new();
        let mut done = false;
        for _ in 0..200 {
            let events = agent.tick(&guest).unwrap();
            for event in events {
                match event {
                    AgentEvent::ResponseText(text) => deltas.push(text),
                    AgentEvent::Done => {
                        assert_eq!(deltas, ["x".repeat(100)]);
                        done = true;
                    }
                    _ => {}
                }
            }
            if done {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(done);
        assert_eq!(deltas, ["x".repeat(100)]);
    }

    #[test]
    fn model_failure_ends_the_turn_as_failed() {
        let guest = Guest::new().unwrap();
        let agent = GuestAgent::mount_source(
            &guest,
            r#"{"model":"offline"}"#,
            Arc::new(FailingBackend),
            Arc::new(Tools),
            TEST_AGENT_BUNDLE,
        )
        .unwrap();
        agent.prompt(&guest, "fail").unwrap();

        for _ in 0..200 {
            let events = agent.tick(&guest).unwrap();
            assert!(!events.contains(&AgentEvent::Done));
            if events.contains(&AgentEvent::Failed(
                "model backend failed: network unavailable".into(),
            )) {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("model failure did not terminate the turn");
    }

    struct ThinkingToolBackend {
        calls: AtomicUsize,
        requests: Arc<Mutex<Vec<serde_json::Value>>>,
        threads: Arc<Mutex<Vec<std::thread::ThreadId>>>,
    }

    impl ModelBackend for ThinkingToolBackend {
        fn complete(
            &self,
            request_json: &str,
            on_event: &mut dyn FnMut(ModelStreamEvent),
        ) -> Result<String, String> {
            self.threads
                .lock()
                .unwrap()
                .push(std::thread::current().id());
            self.requests
                .lock()
                .unwrap()
                .push(serde_json::from_str(request_json).unwrap());
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                on_event(ModelStreamEvent::Thinking("use both tools".into()));
                return Ok(serde_json::json!({
                    "thinking":"use both tools",
                    "thinkingSignature":"reasoning_content",
                    "text":"",
                    "toolCalls":[
                        {"id":"call_first","name":"first","arguments":{"n":1}},
                        {"id":"call_second","name":"second","arguments":{"n":2}}
                    ],
                    "usage":{"reasoning":3},
                    "stopReason":"toolUse"
                })
                .to_string());
            }
            on_event(ModelStreamEvent::Thinking("both finished".into()));
            on_event(ModelStreamEvent::Text("complete".into()));
            Ok(serde_json::json!({
                "thinking":"both finished",
                "thinkingSignature":"reasoning_content",
                "text":"complete",
                "toolCalls":[],
                "usage":{"reasoning":2},
                "stopReason":"stop"
            })
            .to_string())
        }
    }

    struct OrderedTools(Arc<Mutex<Vec<String>>>);

    impl ToolHost for OrderedTools {
        fn definitions(&self) -> Vec<serde_json::Value> {
            ["first", "second"]
                .into_iter()
                .map(|name| {
                    serde_json::json!({
                        "name":name,
                        "description":name,
                        "parameters":{"type":"object","properties":{"n":{"type":"number"}}}
                    })
                })
                .collect()
        }

        fn execute(&self, _call_id: &str, name: &str, _args_json: &str) -> ToolResult {
            self.0.lock().unwrap().push(name.into());
            ToolResult::text(format!("{name}-done"))
        }
    }

    #[test]
    fn one_model_worker_preserves_thinking_and_sequential_tools() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let threads = Arc::new(Mutex::new(Vec::new()));
        let executed = Arc::new(Mutex::new(Vec::new()));
        let guest = Guest::new().unwrap();
        let agent = GuestAgent::mount_source(
            &guest,
            r#"{"provider":"deepseek","model":"deepseek-v4-pro","thinkingLevel":"high"}"#,
            Arc::new(ThinkingToolBackend {
                calls: AtomicUsize::new(0),
                requests: requests.clone(),
                threads: threads.clone(),
            }),
            Arc::new(OrderedTools(executed.clone())),
            TEST_AGENT_BUNDLE,
        )
        .unwrap();
        agent.prompt(&guest, "run both").unwrap();

        let mut text = String::new();
        let mut done = false;
        for _ in 0..500 {
            for event in agent.tick(&guest).unwrap() {
                match event {
                    AgentEvent::ResponseText(delta) => text.push_str(&delta),
                    AgentEvent::Done => done = true,
                    AgentEvent::Failed(error) => panic!("agent failed: {error}"),
                    AgentEvent::Ready => {}
                }
            }
            if done {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }

        assert!(done);
        assert_eq!(text, "complete");
        assert_eq!(*executed.lock().unwrap(), ["first", "second"]);
        let threads = threads.lock().unwrap();
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0], threads[1]);
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0]["options"]["reasoning"], "high");
        let messages = requests[1]["context"]["messages"].as_array().unwrap();
        let assistant = messages
            .iter()
            .find(|message| message["role"] == "assistant")
            .unwrap();
        assert_eq!(assistant["content"][0]["type"], "thinking");
        assert_eq!(assistant["content"][0]["thinking"], "use both tools");
        assert_eq!(
            assistant["content"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|block| block["type"] == "toolCall")
                .count(),
            2
        );
        assert_eq!(
            messages
                .iter()
                .filter(|message| message["role"] == "toolResult")
                .count(),
            2
        );
    }

    struct RecordingBackend(Arc<Mutex<Vec<serde_json::Value>>>);

    impl ModelBackend for RecordingBackend {
        fn complete(
            &self,
            request_json: &str,
            _on_event: &mut dyn FnMut(ModelStreamEvent),
        ) -> Result<String, String> {
            self.0
                .lock()
                .unwrap()
                .push(serde_json::from_str(request_json).unwrap());
            Ok(serde_json::json!({
                "thinking":"",
                "text":"done",
                "toolCalls":[],
                "usage":{},
                "stopReason":"stop"
            })
            .to_string())
        }
    }

    #[test]
    fn replacing_tools_keeps_the_guest_and_conversation() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let guest = Guest::new().unwrap();
        let agent = GuestAgent::mount_source(
            &guest,
            r#"{"model":"offline"}"#,
            Arc::new(RecordingBackend(requests.clone())),
            Arc::new(Tools),
            TEST_AGENT_BUNDLE,
        )
        .unwrap();

        for prompt in ["before install", "after install"] {
            if prompt == "after install" {
                agent
                    .replace_tools(
                        &guest,
                        vec![serde_json::json!({
                            "name":"new.tool",
                            "description":"New App Tool",
                            "parameters":{"type":"object","properties":{}}
                        })],
                    )
                    .unwrap();
            }
            agent.prompt(&guest, prompt).unwrap();
            let mut done = false;
            for _ in 0..200 {
                done |= agent
                    .tick(&guest)
                    .unwrap()
                    .iter()
                    .any(|event| matches!(event, AgentEvent::Done));
                if done {
                    break;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(done);
        }

        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0]["context"]["tools"][0]["name"], "echo");
        assert_eq!(requests[1]["context"]["tools"][0]["name"], "new.tool");
        assert!(requests[1]["context"]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["role"] == "user"
                && message["content"].to_string().contains("before install")));
    }
}
