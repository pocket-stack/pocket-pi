use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use pocket_pi_embedded::ModelBackend;
use pocket_pi_protocols::model::WirelessProvider;
use pocket_pi_protocols::{anthropic_messages, codex_decision, openai_chat};
use serde_json::{json, Value};

pub enum BackendChoice {
    Scripted,
    Wireless {
        provider: WirelessProvider,
        api_key: String,
        model: String,
    },
    Codex {
        model: Option<String>,
    },
}

impl BackendChoice {
    pub fn from_name(name: &str, model: Option<String>) -> Result<Self, String> {
        match name {
            "scripted" => Ok(Self::Scripted),
            "openai" | "openrouter" | "anthropic" => {
                let provider = match name {
                    "openai" => WirelessProvider::OpenAi,
                    "openrouter" => WirelessProvider::OpenRouter,
                    "anthropic" => WirelessProvider::Anthropic,
                    _ => unreachable!(),
                };
                let prefix = name.to_ascii_uppercase();
                let api_key = std::env::var(format!("{prefix}_API_KEY"))
                    .map_err(|_| format!("{prefix}_API_KEY is required for --backend {name}"))?;
                let model = model
                    .or_else(|| std::env::var(format!("{prefix}_MODEL")).ok())
                    .or_else(|| provider.default_model().map(str::to_owned))
                    .ok_or_else(|| format!("--backend {name} requires --model"))?;
                Ok(Self::Wireless {
                    provider,
                    api_key,
                    model,
                })
            }
            "codex" => Ok(Self::Codex {
                model: model.or_else(|| std::env::var("CODEX_MODEL").ok()),
            }),
            other => Err(format!(
                "unknown backend {other:?}; expected scripted, openai, openrouter, anthropic or codex"
            )),
        }
    }

    pub fn agent_config(&self) -> String {
        let (provider, model) = match self {
            Self::Scripted => ("openai", "simulated"),
            Self::Wireless {
                provider, model, ..
            } => (provider.id(), model.as_str()),
            Self::Codex { model } => ("codex", model.as_deref().unwrap_or("coding-plan")),
        };
        json!({
            "provider": provider,
            "model": model,
            "systemPrompt": "You are Pocket Pi in the ESP32 simulator. Be concise."
        })
        .to_string()
    }

    pub fn build(self) -> Arc<dyn ModelBackend> {
        match self {
            Self::Scripted => Arc::new(ScriptedBackend),
            Self::Wireless {
                provider, api_key, ..
            } => Arc::new(WirelessBackend { provider, api_key }),
            Self::Codex { model } => Arc::new(CodexBackend { model }),
        }
    }
}

struct ScriptedBackend;

impl ModelBackend for ScriptedBackend {
    fn complete(
        &self,
        _request_json: &str,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<String, String> {
        let text = "Embedded Pi is running the real pi-agent-core profile. Chat and workspace are rendered by the shared PocketJS UI.";
        for word in text.split_inclusive(' ') {
            on_delta(word);
            std::thread::sleep(Duration::from_millis(35));
        }
        Ok(json!({ "text": text }).to_string())
    }
}

struct WirelessBackend {
    provider: WirelessProvider,
    api_key: String,
}

impl ModelBackend for WirelessBackend {
    fn complete(
        &self,
        request_json: &str,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<String, String> {
        let (endpoint, body) = match self.provider {
            WirelessProvider::OpenAi => (
                "https://api.openai.com/v1/chat/completions",
                openai_chat::build_request(request_json)?,
            ),
            WirelessProvider::OpenRouter => (
                "https://openrouter.ai/api/v1/chat/completions",
                openai_chat::build_request_for(request_json, openai_chat::Dialect::OpenRouter)?,
            ),
            WirelessProvider::Anthropic => (
                "https://api.anthropic.com/v1/messages",
                anthropic_messages::build_request(request_json)?,
            ),
        };
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(20))
            .timeout_read(Duration::from_secs(180))
            .build();
        let mut request = agent
            .post(endpoint)
            .set("accept", "text/event-stream")
            .set("content-type", "application/json");
        request = match self.provider {
            WirelessProvider::Anthropic => request
                .set("x-api-key", &self.api_key)
                .set("anthropic-version", "2023-06-01"),
            WirelessProvider::OpenAi | WirelessProvider::OpenRouter => {
                request.set("authorization", &format!("Bearer {}", self.api_key))
            }
        };
        let response = request.send_string(&body);
        let response = match response {
            Ok(response) => response,
            Err(ureq::Error::Status(status, response)) => {
                let body = read_body(response.into_reader());
                return Err(format!(
                    "{} returned HTTP {status}: {body}",
                    self.provider.id()
                ));
            }
            Err(error) => return Err(format!("{} request failed: {error}", self.provider.id())),
        };

        let mut stream = match self.provider {
            WirelessProvider::Anthropic => SimProviderStream::Anthropic(Default::default()),
            WirelessProvider::OpenAi | WirelessProvider::OpenRouter => {
                SimProviderStream::Chat(Default::default())
            }
        };
        for line in BufReader::new(response.into_reader()).lines() {
            let line = line.map_err(|error| format!("model stream read failed: {error}"))?;
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim_start();
            if data == "[DONE]" {
                break;
            }
            if let Some(delta) = stream.push(data)? {
                on_delta(&delta);
            }
        }
        stream.finish()
    }
}

enum SimProviderStream {
    Chat(openai_chat::Stream),
    Anthropic(anthropic_messages::Stream),
}

impl SimProviderStream {
    fn push(&mut self, data: &str) -> Result<Option<String>, String> {
        match self {
            Self::Chat(stream) => stream.push(data),
            Self::Anthropic(stream) => stream.push(data),
        }
    }

    fn finish(self) -> Result<String, String> {
        match self {
            Self::Chat(stream) => stream.finish(),
            Self::Anthropic(stream) => stream.finish(),
        }
    }
}

struct CodexBackend {
    model: Option<String>,
}

impl ModelBackend for CodexBackend {
    fn complete(
        &self,
        request_json: &str,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<String, String> {
        let workspace = tempfile::tempdir().map_err(|error| error.to_string())?;
        let mut command = Command::new("codex");
        command.args([
            "exec",
            "--ephemeral",
            "--ignore-user-config",
            "--ignore-rules",
            "--sandbox",
            "read-only",
            "--skip-git-repo-check",
            "--color",
            "never",
            "-C",
        ]);
        command.arg(workspace.path());
        if let Some(model) = &self.model {
            command.args(["--model", model]);
        }
        command
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| format!("failed to start local Codex: {error}"))?;
        let (prompt, tools) = codex_decision::build_prompt(request_json)?;
        child
            .stdin
            .take()
            .ok_or("local Codex stdin is unavailable")?
            .write_all(prompt.as_bytes())
            .map_err(|error| format!("write local Codex prompt: {error}"))?;
        let output = child
            .wait_with_output()
            .map_err(|error| format!("wait for local Codex: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "local Codex exited {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let text = String::from_utf8(output.stdout)
            .map_err(|error| format!("local Codex returned non-UTF-8 output: {error}"))?;
        log::debug!("local Codex raw decision: {}", text.trim());
        let call_id = format!(
            "sim_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis())
                .unwrap_or_default()
        );
        let result = codex_decision::parse_response(&text, &tools, &call_id)?;
        log::debug!("local Codex parsed decision: {result}");
        if let Some(text) = serde_json::from_str::<Value>(&result)
            .ok()
            .and_then(|value| value.get("text").and_then(Value::as_str).map(str::to_owned))
        {
            on_delta(&text);
        }
        Ok(result)
    }
}

fn read_body(mut reader: impl Read) -> String {
    let mut body = String::new();
    reader.read_to_string(&mut body).ok();
    body
}
