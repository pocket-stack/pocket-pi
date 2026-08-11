use std::collections::BTreeMap;
use std::time::Duration;

use pocket_pi_protocols::model::{
    ModelBackendSettings, ModelSettings, ThinkingLevel, UartProvider, WirelessProvider,
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
    pub app_credentials: BTreeMap<String, String>,
    pub initial_prompt: Option<String>,
    pub initial_prompt_delay_seconds: u64,
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
        ("wireless", "openai" | "openrouter" | "anthropic" | "deepseek") => {
            if model_api_key.is_none() {
                return Err(format!("{provider} requires modelApiKey"));
            }
            ModelBackendSettings::Wireless {
                provider: match provider.as_str() {
                    "openai" => WirelessProvider::OpenAi,
                    "openrouter" => WirelessProvider::OpenRouter,
                    "anthropic" => WirelessProvider::Anthropic,
                    "deepseek" => WirelessProvider::DeepSeek,
                    _ => unreachable!(),
                },
            }
        }
        ("uart", _) => return Err("UART provider must be codex or claude-code".into()),
        ("wireless", _) => {
            return Err(
                "wireless provider must be openai, openrouter, anthropic or deepseek".into(),
            )
        }
        _ => return Err("model backend must be uart or wireless".into()),
    };
    let model = ModelSettings {
        backend: model_backend,
        model: text(&value, "model", 128)?,
        thinking_level: match text(&value, "thinkingLevel", 8)?.as_deref() {
            None | Some("high") => ThinkingLevel::High,
            Some("xhigh") => ThinkingLevel::Xhigh,
            Some(_) => return Err("thinkingLevel must be high or xhigh".into()),
        },
    };
    model.resolved_model()?;
    Ok(RuntimeConfig {
        wifi_ssid: text(&value, "wifiSsid", 32)?,
        wifi_password: secret(&value, "wifiPassword", 63)?,
        model,
        model_api_key,
        app_credentials: app_credentials(&value)?,
        initial_prompt: string(&value, "initialPrompt")?.map(str::to_owned),
        initial_prompt_delay_seconds: value
            .get("initialPromptDelaySeconds")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
            .min(120),
        unix_time_seconds: value
            .get("unixTimeSeconds")
            .and_then(serde_json::Value::as_u64),
    })
}

fn app_credentials(value: &serde_json::Value) -> Result<BTreeMap<String, String>, String> {
    let Some(credentials) = value.get("appCredentials") else {
        return Ok(BTreeMap::new());
    };
    let credentials = credentials
        .as_object()
        .ok_or_else(|| "appCredentials must be an object".to_owned())?;
    if credentials.len() > 16 {
        return Err("appCredentials has too many entries".into());
    }
    credentials
        .iter()
        .map(|(id, value)| {
            if id.is_empty() || id.len() > 128 || !id.is_ascii() {
                return Err("appCredentials contains an invalid id".into());
            }
            let secret = value
                .as_str()
                .filter(|secret| !secret.is_empty() && secret.len() <= 4096 && secret.is_ascii())
                .ok_or_else(|| format!("appCredentials.{id} is invalid"))?;
            Ok((id.clone(), secret.to_owned()))
        })
        .collect()
}

fn text(
    value: &serde_json::Value,
    field: &str,
    max_bytes: usize,
) -> Result<Option<String>, String> {
    let Some(value) = string(value, field)? else {
        return Ok(None);
    };
    if value.len() > max_bytes {
        return Err(format!("{field} has invalid length"));
    }
    Ok(Some(value.to_owned()))
}

fn string<'a>(value: &'a serde_json::Value, field: &str) -> Result<Option<&'a str>, String> {
    let Some(value) = value.get(field) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .ok_or_else(|| format!("{field} must be a string"))?;
    if value.is_empty() {
        return Err(format!("{field} has invalid length"));
    }
    Ok(Some(value))
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
