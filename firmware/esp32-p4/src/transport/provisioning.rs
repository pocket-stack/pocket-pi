use std::time::Duration;

use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition};
use pocket_pi_protocols::model::{
    ModelBackendSettings, ModelSettings, ThinkingLevel, UartProvider, WirelessProvider,
};
use serde_json::{json, Value};

use super::LineTransport;
use crate::DEVICE_NVS_NAMESPACE;

const CONFIG_REQUEST: &str = "PPI-CONFIG-REQUEST";
const CONFIG_RESPONSE: &str = "PPI-CONFIG:";
const MODEL_CONFIG_KEY: &str = "model";

pub struct RuntimeConfig {
    pub wifi_ssid: Option<String>,
    pub wifi_password: Option<String>,
    pub model: ModelSettings,
    pub model_api_key: Option<String>,
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
    let value: Value = serde_json::from_str(
        frame
            .strip_prefix(CONFIG_RESPONSE)
            .ok_or_else(|| "invalid runtime config frame".to_owned())?,
    )
    .map_err(|error| format!("runtime config JSON: {error}"))?;
    parse_runtime_config(&value)
}

pub fn load_runtime_config(
    partition: EspDefaultNvsPartition,
) -> Result<Option<RuntimeConfig>, String> {
    let storage = EspDefaultNvs::new(partition, DEVICE_NVS_NAMESPACE, true)
        .map_err(|error| format!("open model config NVS: {error}"))?;
    let Some(length) = storage
        .blob_len(MODEL_CONFIG_KEY)
        .map_err(|error| format!("read model config length: {error}"))?
    else {
        return Ok(None);
    };
    let mut bytes = vec![0; length];
    let bytes = storage
        .get_blob(MODEL_CONFIG_KEY, &mut bytes)
        .map_err(|error| format!("read model config: {error}"))?
        .ok_or_else(|| "model config disappeared while reading".to_owned())?;
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("parse stored model config: {error}"))?;
    let config = parse_runtime_config(&value)?;
    if !matches!(config.model.backend, ModelBackendSettings::Wireless { .. }) {
        return Err("stored model config must use the wireless backend".into());
    }
    Ok(Some(config))
}

pub fn persist_runtime_config(
    partition: EspDefaultNvsPartition,
    config: &RuntimeConfig,
) -> Result<(), String> {
    let ModelBackendSettings::Wireless { provider } = config.model.backend else {
        return Err("only wireless model config can be persisted".into());
    };
    let mut value = json!({
        "modelBackend": "wireless",
        "modelProvider": provider.id(),
        "modelApiKey": config.model_api_key,
        "thinkingLevel": config.model.thinking_level.id(),
    });
    if let Some(model) = &config.model.model {
        value["model"] = Value::String(model.clone());
    }
    let bytes = serde_json::to_vec(&value)
        .map_err(|error| format!("encode model config: {error}"))?;
    let storage = EspDefaultNvs::new(partition, DEVICE_NVS_NAMESPACE, true)
        .map_err(|error| format!("open model config NVS: {error}"))?;
    storage
        .set_blob(MODEL_CONFIG_KEY, &bytes)
        .map_err(|error| format!("store model config: {error}"))
}

fn parse_runtime_config(value: &Value) -> Result<RuntimeConfig, String> {
    let backend = text(value, "modelBackend", 16)?
        .ok_or_else(|| "modelBackend is required".to_owned())?;
    let provider = text(value, "modelProvider", 32)?
        .ok_or_else(|| "modelProvider is required".to_owned())?;
    let model_api_key = secret(value, "modelApiKey", 512)?;
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

fn text(
    value: &Value,
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

fn string<'a>(value: &'a Value, field: &str) -> Result<Option<&'a str>, String> {
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
    value: &Value,
    field: &str,
    max_bytes: usize,
) -> Result<Option<String>, String> {
    let value = text(value, field, max_bytes)?;
    if value.as_ref().is_some_and(|value| !value.is_ascii()) {
        return Err(format!("{field} must be ASCII"));
    }
    Ok(value)
}
