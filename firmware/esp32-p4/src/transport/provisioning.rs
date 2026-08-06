use std::time::Duration;

use pocket_pi_protocols::model::{
    ModelBackendSettings, ModelSettings, UartProvider, WirelessProvider,
};

use super::LineTransport;

const CONFIG_REQUEST: &str = "PPI-CONFIG-REQUEST";
const CONFIG_RESPONSE: &str = "PPI-CONFIG:";

#[derive(Default)]
pub struct RuntimeConfig {
    pub wifi_ssid: Option<String>,
    pub wifi_password: Option<String>,
    pub model: ModelSettings,
    pub model_api_key: Option<String>,
    pub initial_prompt: Option<String>,
    pub unix_time_seconds: Option<u64>,
}

pub fn request_runtime_config(
    transport: &dyn LineTransport,
    timeout: Duration,
) -> Result<RuntimeConfig, String> {
    transport.write_line(CONFIG_REQUEST);
    let frame = transport.read_frame(CONFIG_RESPONSE, timeout)?;
    let value: serde_json::Value = serde_json::from_str(
        frame
            .strip_prefix(CONFIG_RESPONSE)
            .ok_or_else(|| "invalid runtime config frame".to_owned())?,
    )
    .map_err(|error| format!("runtime config JSON: {error}"))?;
    let backend = text(&value, "modelBackend", 16)?.unwrap_or_else(|| "uart".into());
    let provider = text(&value, "modelProvider", 32)?.unwrap_or_else(|| "codex".into());
    let model_api_key = secret(&value, "modelApiKey", 512)?;
    let model_backend = match (backend.as_str(), provider.as_str()) {
        ("uart", "codex") => ModelBackendSettings::Uart {
            provider: UartProvider::Codex,
        },
        ("uart", "claude-code") => ModelBackendSettings::Uart {
            provider: UartProvider::ClaudeCode,
        },
        ("wireless", "openai" | "openrouter" | "anthropic") => {
            if model_api_key.is_none() {
                return Err(format!("{provider} requires modelApiKey"));
            }
            ModelBackendSettings::Wireless {
                provider: match provider.as_str() {
                    "openai" => WirelessProvider::OpenAi,
                    "openrouter" => WirelessProvider::OpenRouter,
                    "anthropic" => WirelessProvider::Anthropic,
                    _ => unreachable!(),
                },
            }
        }
        ("uart", _) => return Err("UART provider must be codex or claude-code".into()),
        ("wireless", _) => {
            return Err("wireless provider must be openai, openrouter or anthropic".into())
        }
        _ => return Err("model backend must be uart or wireless".into()),
    };
    let model = ModelSettings {
        backend: model_backend,
        model: text(&value, "model", 128)?,
    };
    model.resolved_model()?;
    Ok(RuntimeConfig {
        wifi_ssid: text(&value, "wifiSsid", 32)?,
        wifi_password: secret(&value, "wifiPassword", 63)?,
        model,
        model_api_key,
        initial_prompt: text(&value, "initialPrompt", 4_000)?,
        unix_time_seconds: value
            .get("unixTimeSeconds")
            .and_then(serde_json::Value::as_u64),
    })
}

fn text(
    value: &serde_json::Value,
    field: &str,
    max_bytes: usize,
) -> Result<Option<String>, String> {
    let Some(value) = value.get(field) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .ok_or_else(|| format!("{field} must be a string"))?;
    if value.is_empty() || value.len() > max_bytes {
        return Err(format!("{field} has invalid length"));
    }
    Ok(Some(value.to_owned()))
}

fn secret(
    value: &serde_json::Value,
    field: &str,
    max_bytes: usize,
) -> Result<Option<String>, String> {
    let value = text(value, field, max_bytes)?;
    if value.as_ref().is_some_and(|value| !value.is_ascii()) {
        return Err(format!("{field} must be ASCII"));
    }
    Ok(value)
}
