use std::sync::Arc;

use rquickjs::{CatchResultExt, Context, Function, Object, Runtime};

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
    fn execute(&self, call_id: &str, name: &str, args_json: &str) -> ToolResult;
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ToolResult {
    pub text: String,
    pub is_error: bool,
    pub terminate: bool,
}

impl ToolResult {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    fn to_json(&self) -> String {
        serde_json::json!({
            "text": self.text,
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
        let runtime = Runtime::new().map_err(|error| error.to_string())?;
        // The limit bounds QuickJS recursion; it does not pre-allocate this
        // memory. ESP hosts may lower it after measuring their native task.
        runtime.set_max_stack_size(512 * 1024);
        let context = Context::full(&runtime).map_err(|error| error.to_string())?;

        context.with(|ctx| -> Result<(), String> {
            let host = Object::new(ctx.clone()).map_err(|error| error.to_string())?;
            host.set(
                "modelComplete",
                Function::new(ctx.clone(), move |request: String| -> String {
                    let mut emit = |delta: &str| on_delta(delta.to_owned());
                    backend
                        .complete(&request, &mut emit)
                        .unwrap_or_else(|error| {
                            serde_json::json!({"text": format!("model backend failed: {error}")})
                                .to_string()
                        })
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
            boot.call::<_, ()>((config_json.to_owned(),))
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

    pub fn memory_used_bytes(&self) -> i64 {
        self.runtime.memory_usage().memory_used_size
    }
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
        fn execute(&self, _call_id: &str, name: &str, _args_json: &str) -> ToolResult {
            ToolResult::text(format!("called {name}"))
        }
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
