use std::sync::Arc;
use std::time::Duration;

use pocket_pi_agentos::{AppServiceHost, AppSupervisor, ROOT_APP_ID};
use pocket_pi_app_pack::catalog;
use pocket_pi_embedded::{AgentEvent, ModelBackend, ToolHost, ToolResult};
use serde_json::Value;

struct NoServices;

impl AppServiceHost for NoServices {
    fn call(
        &self,
        _app_id: &str,
        _service: &str,
        _operation: &str,
        _args: &Value,
    ) -> Result<Value, String> {
        Err("unexpected App service call".into())
    }
}

struct Backend;

impl ModelBackend for Backend {
    fn complete(
        &self,
        _request_json: &str,
        on_event: &mut dyn FnMut(pocket_pi_embedded::ModelStreamEvent),
    ) -> Result<String, String> {
        on_event(pocket_pi_embedded::ModelStreamEvent::Text("done".into()));
        Ok(r#"{"thinking":"","text":"done","toolCalls":[],"usage":{},"stopReason":"stop"}"#.into())
    }
}

struct NoTools;

impl ToolHost for NoTools {
    fn definitions(&self) -> Vec<Value> {
        Vec::new()
    }

    fn execute(&self, _call_id: &str, name: &str, _args_json: &str) -> ToolResult {
        ToolResult {
            text: format!("unexpected Tool {name}"),
            is_error: true,
            ..ToolResult::default()
        }
    }
}

#[test]
fn system_agent_keeps_running_while_an_ordinary_app_is_foreground() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = catalog().unwrap();
    let ordinary = catalog
        .descriptors()
        .find(|app| app.id != ROOT_APP_ID)
        .unwrap()
        .id
        .clone();
    let mut supervisor = AppSupervisor::new(temp.path(), catalog, Arc::new(NoServices)).unwrap();
    supervisor
        .boot_agent(
            r#"{"model":"offline"}"#,
            Arc::new(Backend),
            Arc::new(NoTools),
        )
        .unwrap();
    supervisor.prompt_agent("continue").unwrap();
    supervisor.open(&ordinary).unwrap();

    for _ in 0..100 {
        if supervisor
            .frame()
            .unwrap()
            .into_iter()
            .any(|event| event == AgentEvent::Done)
        {
            assert_eq!(supervisor.active_id(), ordinary);
            supervisor.open(ROOT_APP_ID).unwrap();
            assert_eq!(supervisor.active_id(), ROOT_APP_ID);
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("Agent did not finish while an ordinary App was foreground");
}
