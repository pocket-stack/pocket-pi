use std::sync::{mpsc, Arc};

use rquickjs::{CatchResultExt, Context, Function, Object, Runtime};

const AGENT_BUNDLE: &str = include_str!("../js/pi-agent.bundle.js");
const PRELUDE: &str = include_str!("../js/prelude.js");
#[cfg(target_os = "espidf")]
const QUICKJS_STACK_LIMIT: usize = 96 * 1024;
#[cfg(not(target_os = "espidf"))]
const QUICKJS_STACK_LIMIT: usize = 512 * 1024;

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
    Delta(String),
    Done,
    Failed(String),
}

pub fn spawn_agent_worker(
    config_json: String,
    backend: Arc<dyn ModelBackend>,
    tools: Arc<dyn ToolHost>,
    stack_size: Option<usize>,
) -> Result<(mpsc::Sender<String>, mpsc::Receiver<AgentEvent>), String> {
    let (prompt_tx, prompt_rx) = mpsc::channel::<String>();
    let (event_tx, event_rx) = mpsc::channel::<AgentEvent>();
    let mut builder = std::thread::Builder::new().name("pi-agent".to_owned());
    if let Some(stack_size) = stack_size {
        builder = builder.stack_size(stack_size);
    }
    builder
        .spawn(move || {
            let delta_tx = event_tx.clone();
            let runtime = PiEmbedded::new(
                &config_json,
                backend,
                tools,
                Arc::new(move |delta| {
                    delta_tx.send(AgentEvent::Delta(delta)).ok();
                }),
            );
            let runtime = match runtime {
                Ok(runtime) => runtime,
                Err(error) => {
                    event_tx.send(AgentEvent::Failed(error)).ok();
                    return;
                }
            };
            event_tx.send(AgentEvent::Ready).ok();
            for prompt in prompt_rx {
                let result = runtime.prompt(&prompt).and_then(|()| runtime.pump());
                let event = match result {
                    Ok(_) => AgentEvent::Done,
                    Err(error) => AgentEvent::Failed(error),
                };
                event_tx.send(event).ok();
            }
        })
        .map_err(|error| format!("spawn Pi Agent worker: {error}"))?;
    Ok((prompt_tx, event_rx))
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

pub struct PiEmbedded {
    runtime: Runtime,
    context: Context,
}

impl PiEmbedded {
    pub fn new(
        config_json: &str,
        backend: Arc<dyn ModelBackend>,
        tools: Arc<dyn ToolHost>,
        on_delta: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<Self, String> {
        let config_json = config_with_tools(config_json, tools.definitions())?;
        let runtime = Runtime::new().map_err(|error| error.to_string())?;
        runtime.set_max_stack_size(QUICKJS_STACK_LIMIT);
        let context = Context::full(&runtime).map_err(|error| error.to_string())?;

        context.with(|ctx| -> Result<(), String> {
            let host = Object::new(ctx.clone()).map_err(|error| error.to_string())?;
            host.set(
                "modelComplete",
                Function::new(ctx.clone(), move |request: String| -> String {
                    let mut emit = |delta: &str| on_delta(delta.to_owned());
                    match backend.complete(&request, &mut emit) {
                        Ok(response) => response,
                        Err(error) => {
                            let text = format!("model backend failed: {error}");
                            emit(&text);
                            serde_json::json!({"text":text}).to_string()
                        }
                    }
                })
                .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            host.set(
                "tool",
                Function::new(
                    ctx.clone(),
                    move |call_id: String, name: String, args: String| -> String {
                        tools.execute(&call_id, &name, &args).to_json()
                    },
                )
                .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            ctx.globals()
                .set("host", host)
                .map_err(|error| error.to_string())?;

            ctx.eval::<(), _>(PRELUDE.as_bytes())
                .catch(&ctx)
                .map_err(|error| format!("embedded prelude: {error}"))?;
            ctx.eval::<(), _>(AGENT_BUNDLE.as_bytes())
                .catch(&ctx)
                .map_err(|error| format!("embedded agent: {error}"))?;
            let agent: Object = ctx
                .globals()
                .get("PocketPiEmbedded")
                .map_err(|error| format!("PocketPiEmbedded missing: {error}"))?;
            let boot: Function = agent.get("boot").map_err(|error| error.to_string())?;
            boot.call::<_, ()>((config_json,))
                .catch(&ctx)
                .map_err(|error| format!("embedded boot: {error}"))
        })?;

        let runtime = Self { runtime, context };
        runtime.pump()?;
        Ok(runtime)
    }

    pub fn prompt(&self, text: &str) -> Result<(), String> {
        self.context.with(|ctx| {
            let agent: Object = ctx
                .globals()
                .get("PocketPiEmbedded")
                .map_err(|error| error.to_string())?;
            let prompt: Function = agent.get("prompt").map_err(|error| error.to_string())?;
            prompt
                .call::<_, ()>((text.to_owned(),))
                .catch(&ctx)
                .map_err(|error| format!("embedded prompt: {error}"))
        })
    }

    pub fn pump(&self) -> Result<String, String> {
        while self.runtime.is_job_pending() {
            self.runtime
                .execute_pending_job()
                .map_err(|error| error.to_string())?;
        }
        self.context
            .with(|ctx| {
                let agent: Object = ctx.globals().get("PocketPiEmbedded")?;
                let drain: Function = agent.get("drain")?;
                drain.call::<_, String>(())
            })
            .map_err(|error| error.to_string())
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
        assert!(events.contains("message_end"));
    }
}
