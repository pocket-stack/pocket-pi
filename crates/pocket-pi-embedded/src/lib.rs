use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use pocket_mod::qjs::{CatchResultExt, Function, Object};
use pocket_mod::Guest;

const AGENT_BUNDLE: &str = include_str!("../js/pi-agent.bundle.js");
const PRELUDE: &str = include_str!("../js/prelude.js");

pub trait ModelBackend: Send + Sync {
    fn complete(
        &self,
        request_json: &str,
        on_delta: &mut dyn FnMut(&str),
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
pub struct GuestAgent {
    events: mpsc::Receiver<AgentEvent>,
}

impl GuestAgent {
    pub fn mount(
        guest: &Guest,
        config_json: &str,
        backend: Arc<dyn ModelBackend>,
        tools: Arc<dyn ToolHost>,
    ) -> Result<Self, String> {
        Self::mount_source(
            guest,
            config_json,
            backend,
            tools,
            AGENT_BUNDLE,
            Arc::new(|_| {}),
        )
    }

    pub fn mount_source(
        guest: &Guest,
        config_json: &str,
        backend: Arc<dyn ModelBackend>,
        tools: Arc<dyn ToolHost>,
        agent_source: &str,
        on_delta: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<Self, String> {
        let config_json = config_with_tools(config_json, tools.definitions())?;
        let (host_tx, host_rx) = mpsc::channel::<serde_json::Value>();
        let host_rx = Arc::new(Mutex::new(host_rx));
        let (event_tx, event_rx) = mpsc::channel::<AgentEvent>();
        let next_request = Arc::new(AtomicI32::new(1));

        guest
            .mount("host", {
                let backend = backend.clone();
                let tools = tools.clone();
                move |ctx, host| {
                    host.set(
                        "startModel",
                        Function::new(ctx.clone(), {
                            let backend = backend.clone();
                            let host_tx = host_tx.clone();
                            let event_tx = event_tx.clone();
                            let next_request = next_request.clone();
                            let on_delta = on_delta.clone();
                            move |request: String| -> i32 {
                                let id = next_request.fetch_add(1, Ordering::Relaxed);
                                let backend = backend.clone();
                                let host_tx = host_tx.clone();
                                let worker_tx = host_tx.clone();
                                let event_tx = event_tx.clone();
                                let on_delta = on_delta.clone();
                                let spawn = std::thread::Builder::new()
                                    .name(format!("pi-model-{id}"))
                                    .spawn(move || {
                                        // Providers may stream internally, but the embedded UI
                                        // receives one complete text update per model request.
                                        // Fine-grained deltas are prohibitively expensive on the
                                        // ESP32 and add no value to the fixed Chat projection.
                                        let mut buffered = String::new();
                                        let mut collect = |delta: &str| buffered.push_str(delta);
                                        match backend.complete(&request, &mut collect) {
                                            Ok(result) => {
                                                let final_text = model_result_text(&result)
                                                    .or_else(|| (!buffered.is_empty()).then_some(buffered));
                                                if let Some(text) = final_text {
                                                    on_delta(text.clone());
                                                    let _ = event_tx
                                                        .send(AgentEvent::ResponseText(text.clone()));
                                                    let _ = worker_tx.send(serde_json::json!({
                                                        "type":"model_delta",
                                                        "id":id,
                                                        "delta":text,
                                                    }));
                                                }
                                                let _ = worker_tx.send(serde_json::json!({
                                                    "type":"model_done",
                                                    "id":id,
                                                    "result":result,
                                                }));
                                            }
                                            Err(error) => {
                                                let _ = worker_tx.send(serde_json::json!({
                                                    "type":"model_error",
                                                    "id":id,
                                                    "error":format!("model backend failed: {error}"),
                                                }));
                                            }
                                        }
                                    });
                                if let Err(error) = spawn {
                                    let _ = host_tx.send(serde_json::json!({
                                        "type":"model_error",
                                        "id":id,
                                        "error":format!("spawn model worker: {error}"),
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
                                let tools = tools.clone();
                                let host_tx = host_tx.clone();
                                let worker_tx = host_tx.clone();
                                let spawn = std::thread::Builder::new()
                                    .name(format!("pi-tool-{id}"))
                                    .spawn(move || {
                                        let result = tools.execute(&call_id, &name, &args);
                                        let _ = worker_tx.send(serde_json::json!({
                                            "type":"tool_done",
                                            "id":id,
                                            "result":result.to_json(),
                                        }));
                                    });
                                if let Err(error) = spawn {
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
                                    .map(|receiver| receiver.try_iter().collect::<Vec<_>>())
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

        Ok(Self { events: event_rx })
    }

    pub fn prompt(&self, guest: &Guest, text: &str) -> Result<(), String> {
        call_agent(guest, "prompt", (text.to_owned(),))
    }

    pub fn abort(&self, guest: &Guest) -> Result<(), String> {
        call_agent(guest, "abort", ())
    }

    pub fn tick(&self, guest: &Guest) -> Result<Vec<AgentEvent>, String> {
        call_agent::<_, ()>(guest, "tick", ())?;
        guest.drain_jobs();
        let raw: String = call_agent(guest, "drain", ())?;
        let payload: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|error| format!("parse Pi Agent events: {error}"))?;
        let mut events = self.events.try_iter().collect::<Vec<_>>();
        for event in payload["events"].as_array().into_iter().flatten() {
            match event["type"].as_str() {
                Some("agent_ready") => events.push(AgentEvent::Ready),
                Some("agent_end") => events.push(AgentEvent::Done),
                Some("agent_error") => events.push(AgentEvent::Failed(
                    event["message"]
                        .as_str()
                        .unwrap_or("Pi Agent failed")
                        .to_owned(),
                )),
                _ => {}
            }
        }
        Ok(events)
    }
}

fn model_result_text(result: &str) -> Option<String> {
    let result = serde_json::from_str::<serde_json::Value>(result).ok()?;
    if result
        .get("toolCall")
        .is_some_and(serde_json::Value::is_object)
    {
        return None;
    }
    result
        .get("text")?
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
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

/// Standalone compatibility harness. Product hosts should mount `GuestAgent`
/// through `AppSupervisor`, which keeps the System App alive across View
/// navigation.
pub struct PiEmbedded {
    guest: Guest,
    agent: GuestAgent,
}

impl PiEmbedded {
    pub fn new(
        config_json: &str,
        backend: Arc<dyn ModelBackend>,
        tools: Arc<dyn ToolHost>,
        on_delta: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<Self, String> {
        let guest = Guest::new().map_err(|error| error.to_string())?;
        let agent =
            GuestAgent::mount_source(&guest, config_json, backend, tools, AGENT_BUNDLE, on_delta)?;
        Ok(Self { guest, agent })
    }

    pub fn prompt(&self, text: &str) -> Result<(), String> {
        self.agent.prompt(&self.guest, text)
    }

    pub fn pump(&self) -> Result<String, String> {
        let mut observed = Vec::new();
        loop {
            self.guest.frame(0).map_err(|error| error.to_string())?;
            let events = self.agent.tick(&self.guest)?;
            let mut finished = false;
            for event in events {
                match event {
                    AgentEvent::Ready => observed.push("agent_ready"),
                    AgentEvent::ResponseText(_) => observed.push("message_update"),
                    AgentEvent::Done => {
                        observed.push("agent_end");
                        finished = true;
                    }
                    AgentEvent::Failed(error) => return Err(error),
                }
            }
            if finished {
                return serde_json::to_string(&observed).map_err(|error| error.to_string());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
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
    let mut names = std::collections::BTreeSet::new();
    for definition in &definitions {
        let name = definition
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "tool definition is missing a string name".to_owned())?;
        if !names.insert(name.to_owned()) {
            return Err(format!("duplicate tool definition: {name}"));
        }
    }
    object.insert("tools".to_owned(), serde_json::Value::Array(definitions));
    serde_json::to_string(&config).map_err(|error| format!("serialize embedded config: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Backend;

    impl ModelBackend for Backend {
        fn complete(
            &self,
            _request_json: &str,
            on_delta: &mut dyn FnMut(&str),
        ) -> Result<String, String> {
            on_delta("embedded-ok");
            Ok(r#"{"text":"embedded-ok"}"#.into())
        }
    }

    struct BurstBackend;

    impl ModelBackend for BurstBackend {
        fn complete(
            &self,
            _request_json: &str,
            on_delta: &mut dyn FnMut(&str),
        ) -> Result<String, String> {
            for _ in 0..100 {
                on_delta("x");
            }
            Ok(serde_json::json!({"text":"x".repeat(100)}).to_string())
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
    fn tool_call_results_do_not_emit_assistant_text() {
        let result = serde_json::json!({
            "text":"provider preamble",
            "toolCall":{"name":"echo","arguments":{}}
        })
        .to_string();
        assert_eq!(model_result_text(&result), None);
    }

    #[test]
    fn boots_and_runs_a_real_pi_agent_turn() {
        let runtime = PiEmbedded::new(
            r#"{"model":"offline"}"#,
            Arc::new(Backend),
            Arc::new(Tools),
            Arc::new(|_| {}),
        )
        .unwrap();
        runtime.prompt("hello").unwrap();
        let events = runtime.pump().unwrap();
        assert!(events.contains("agent_end"));
    }

    #[test]
    fn model_deltas_are_coalesced_before_reaching_the_embedded_ui() {
        let guest = Guest::new().unwrap();
        let agent = GuestAgent::mount(
            &guest,
            r#"{"model":"offline"}"#,
            Arc::new(BurstBackend),
            Arc::new(Tools),
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
}
