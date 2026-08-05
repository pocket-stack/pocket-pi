mod clock;
mod coding;
mod schedule;
mod shell;
mod workspace;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pocket_pi_embedded::{ToolHost, ToolResult};
use serde_json::{json, Value};

pub use schedule::{ScheduleProjection, ScheduledWake};

pub trait PlatformTools: Send + Sync {
    fn device_status(&self) -> Value;
    fn wifi_status(&self) -> Value;
    fn reboot(&self) -> Result<Value, String>;
}

pub struct CoreToolHost {
    root: PathBuf,
    coding: coding::CodingTools,
    schedule: schedule::ScheduleStore,
    workspace: workspace::WorkspaceContext,
    platform: Arc<dyn PlatformTools>,
}

impl CoreToolHost {
    pub fn new(root: impl Into<PathBuf>, platform: Arc<dyn PlatformTools>) -> Self {
        let root = root.into();
        let _ = std::fs::create_dir_all(&root);
        Self {
            coding: coding::CodingTools::new(&root),
            schedule: schedule::ScheduleStore::load(&root),
            workspace: workspace::WorkspaceContext::new(&root),
            root,
            platform,
        }
    }

    pub fn workspace_root(&self) -> &Path {
        &self.root
    }

    pub fn schedule_projection(&self) -> ScheduleProjection {
        self.schedule.projection()
    }

    pub fn claim_due(&self) -> Option<ScheduledWake> {
        self.schedule.claim_due()
    }

    fn execute_value(
        &self,
        call_id: &str,
        name: &str,
        args: &Value,
    ) -> Result<NativeToolResult, String> {
        if let Some(result) = self.coding.execute(call_id, name, args)? {
            return Ok(result);
        }
        if let Some(result) = shell::execute(
            &self.coding,
            self.root.as_path(),
            self.platform.as_ref(),
            call_id,
            name,
            args,
        )? {
            return Ok(result);
        }
        if let Some(result) = self.schedule.execute(name, args)? {
            return Ok(NativeToolResult::json(result));
        }
        if let Some(result) = self.workspace.execute(name)? {
            return Ok(NativeToolResult::json(result));
        }
        if let Some(result) = clock::execute(name) {
            return Ok(NativeToolResult::json(result));
        }
        if name == "device.status" {
            return Ok(NativeToolResult::json(self.platform.device_status()));
        }
        Err(format!("unknown tool: {name}"))
    }
}

impl ToolHost for CoreToolHost {
    fn definitions(&self) -> Vec<Value> {
        let mut definitions = coding::definitions();
        definitions.extend(shell::definitions());
        definitions.push(json!({
            "name":"device.status",
            "description":"Read the embedded host runtime and memory status.",
            "parameters":{"type":"object","properties":{},"additionalProperties":false}
        }));
        definitions.extend(workspace::WorkspaceContext::definitions());
        definitions.extend(clock::definitions());
        definitions.extend(schedule::ScheduleStore::definitions());
        definitions
    }

    fn execute(&self, call_id: &str, name: &str, args_json: &str) -> ToolResult {
        let args = match serde_json::from_str::<Value>(args_json) {
            Ok(Value::Object(args)) => Value::Object(args),
            Ok(_) => return ToolResult::error("tool arguments must be a JSON object"),
            Err(error) => return ToolResult::error(format!("invalid tool arguments: {error}")),
        };
        match self.execute_value(call_id, name, &args) {
            Ok(result) => ToolResult {
                text: result.text,
                details: result.details,
                is_error: false,
                terminate: result.terminate,
            },
            Err(error) => ToolResult::error(error),
        }
    }
}

pub(crate) struct NativeToolResult {
    pub text: String,
    pub details: Value,
    pub terminate: bool,
}

impl NativeToolResult {
    fn json(value: Value) -> Self {
        Self {
            text: value.to_string(),
            details: value,
            terminate: false,
        }
    }
}

trait ToolResultExt {
    fn error(text: impl Into<String>) -> ToolResult;
}

impl ToolResultExt for ToolResult {
    fn error(text: impl Into<String>) -> ToolResult {
        ToolResult {
            text: text.into(),
            details: Value::Null,
            is_error: true,
            terminate: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use pocket_pi_embedded::{ModelBackend, PiEmbedded};

    struct TestPlatform;

    impl PlatformTools for TestPlatform {
        fn device_status(&self) -> Value {
            json!({"status":"ok","board":"esp32-p4-sim"})
        }

        fn wifi_status(&self) -> Value {
            json!({"status":"connected","simulated":true})
        }

        fn reboot(&self) -> Result<Value, String> {
            Ok(json!({"status":"scheduled","simulated":true}))
        }
    }

    fn host(root: &Path) -> CoreToolHost {
        CoreToolHost::new(root, Arc::new(TestPlatform))
    }

    #[test]
    fn exposes_the_embedded_core_tool_set() {
        let temp = tempfile::tempdir().unwrap();
        let names = host(temp.path())
            .definitions()
            .into_iter()
            .map(|value| value["name"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        for expected in [
            "read",
            "write",
            "edit",
            "find",
            "grep",
            "ls",
            "bash",
            "device.status",
            "time.now",
            "workspace.context",
            "schedule.set",
            "schedule.list",
            "schedule.cancel",
            "schedule.clear",
        ] {
            assert!(
                names.iter().any(|name| name == expected),
                "missing {expected}"
            );
        }
    }

    #[test]
    fn file_tools_share_real_workspace_state() {
        let temp = tempfile::tempdir().unwrap();
        let host = host(temp.path());
        let written = host.execute(
            "call-1",
            "write",
            r#"{"path":"memory/note.md","content":"alpha beta"}"#,
        );
        assert!(!written.is_error, "{written:?}");
        let grep = host.execute("call-2", "grep", r#"{"pattern":"beta","path":"memory"}"#);
        assert!(!grep.is_error, "{grep:?}");
        assert!(grep.text.contains("note.md:1"));
    }

    #[test]
    fn recurring_schedule_persists_and_lists() {
        let temp = tempfile::tempdir().unwrap();
        let tools = host(temp.path());
        let set = tools.execute(
            "call-1",
            "schedule.set",
            r#"{"name":"market","prompt":"check market","every_minutes":30}"#,
        );
        assert!(!set.is_error, "{set:?}");
        let list = tools.execute("call-2", "schedule.list", "{}");
        assert!(list.text.contains("market"));
        drop(tools);
        let restored = host(temp.path()).execute("call-3", "schedule.list", "{}");
        assert!(restored.text.contains("market"));
    }

    struct ToolCallingBackend {
        calls: AtomicUsize,
    }

    impl ModelBackend for ToolCallingBackend {
        fn complete(
            &self,
            request_json: &str,
            on_delta: &mut dyn FnMut(&str),
        ) -> Result<String, String> {
            let request: Value = serde_json::from_str(request_json).unwrap();
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                let names = request["context"]["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|tool| tool["name"].as_str())
                    .collect::<Vec<_>>();
                assert!(names.contains(&"write"));
                return Ok(json!({
                    "toolCall":{
                        "id":"write-1",
                        "name":"write",
                        "arguments":{"path":"agent-created.txt","content":"created through pi-agent-core"}
                    }
                })
                .to_string());
            }
            assert!(request["context"]["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|message| message["role"] == "toolResult"));
            on_delta("tool complete");
            Ok(json!({"text":"tool complete"}).to_string())
        }
    }

    #[test]
    fn pi_agent_executes_the_real_registered_tool() {
        let temp = tempfile::tempdir().unwrap();
        let tools = Arc::new(host(temp.path()));
        let runtime = PiEmbedded::new(
            r#"{"provider":"test","model":"tool-test"}"#,
            Arc::new(ToolCallingBackend {
                calls: AtomicUsize::new(0),
            }),
            tools,
            Arc::new(|_| {}),
        )
        .unwrap();

        runtime.prompt("create a file").unwrap();
        runtime.pump().unwrap();

        assert_eq!(
            std::fs::read_to_string(temp.path().join("agent-created.txt")).unwrap(),
            "created through pi-agent-core"
        );
    }
}
