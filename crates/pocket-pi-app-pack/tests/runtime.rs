use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pocket_pi_agentos::{
    AppServiceHost, AppSupervisor, HttpRequest, NetFailure, RoutedToolHost, TransportCompletion,
    ROOT_APP_ID,
};
use pocket_pi_app_pack::catalog;
use pocket_pi_embedded::{AgentEvent, ModelBackend, ToolHost, ToolResult};
use serde_json::Value;

struct Services;

impl AppServiceHost for Services {
    fn call(
        &self,
        _app_id: &str,
        _service: &str,
        _operation: &str,
        _args: &Value,
    ) -> Result<Value, String> {
        Err("unexpected App service call".into())
    }

    fn http(&self, app_id: &str, request: HttpRequest) -> Result<TransportCompletion, NetFailure> {
        assert_eq!(app_id, "exa");
        assert_eq!(request.url, "https://api.exa.ai/search");
        assert_eq!(request.method, "POST");
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["query"], "Pocket Pi architecture");
        let response = serde_json::json!({
            "results": [{
                "title": "Evidence result",
                "url": "https://example.com/evidence"
            }]
        });
        Ok(TransportCompletion::Done {
            handle: request.handle,
            status: 200,
            url: request.url,
            headers: BTreeMap::from([("content-type".into(), "application/json".into())]),
            body: serde_json::to_vec(&response).unwrap(),
        })
    }
}

struct Backend(AtomicUsize);

impl ModelBackend for Backend {
    fn complete(
        &self,
        request_json: &str,
        on_event: &mut dyn FnMut(pocket_pi_embedded::ModelStreamEvent),
    ) -> Result<String, String> {
        if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
            return Ok(serde_json::json!({
                "thinking": "",
                "text": "",
                "toolCalls": [{
                    "id": "call_search",
                    "name": "research.search",
                    "arguments": {"query": "Pocket Pi architecture"}
                }],
                "usage": {},
                "stopReason": "toolUse"
            })
            .to_string());
        }
        assert!(request_json.contains("Evidence result"), "{request_json}");
        on_event(pocket_pi_embedded::ModelStreamEvent::Text(
            "research complete".into(),
        ));
        Ok(serde_json::json!({
            "thinking": "",
            "text": "research complete",
            "toolCalls": [],
            "usage": {},
            "stopReason": "stop"
        })
        .to_string())
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
fn agent_routes_a_background_app_tool_through_http_and_sqlite() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = catalog().unwrap();
    assert!(catalog.descriptor("exa").is_some());
    assert!(catalog.descriptor("robinhood").is_some());
    let mut supervisor = AppSupervisor::new(temp.path(), catalog, Arc::new(Services)).unwrap();
    let (tools, requests) = RoutedToolHost::new(Arc::new(NoTools), supervisor.catalog().clone());
    supervisor
        .boot_agent(
            r#"{"model":"offline"}"#,
            Arc::new(Backend(AtomicUsize::new(0))),
            Arc::new(tools),
        )
        .unwrap();
    supervisor.prompt_agent("research Pocket Pi").unwrap();
    supervisor.open("robinhood").unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut response = String::new();
    let mut finished = false;
    while Instant::now() < deadline && !finished {
        while let Ok(request) = requests.try_recv() {
            request.handle(&mut supervisor);
        }
        for event in supervisor.frame().unwrap() {
            match event {
                AgentEvent::ResponseText(delta) => response.push_str(&delta),
                AgentEvent::Done => finished = true,
                AgentEvent::Failed(error) => panic!("Agent failed: {error}"),
                AgentEvent::Ready => {}
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }

    assert!(finished);
    assert_eq!(response, "research complete");
    assert_eq!(supervisor.active_id(), "robinhood");
    let storage = supervisor.invoke_tool("research.storage_status", "{}");
    assert!(!storage.is_error, "{}", storage.text);
    assert_eq!(storage.details["searches"], 1);
    supervisor.open(ROOT_APP_ID).unwrap();
    assert_eq!(supervisor.active_id(), ROOT_APP_ID);
}
