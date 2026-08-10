use alloc::format;
use alloc::string::{String, ToString};
use serde::{Deserialize, Serialize};

pub const OPENAI_API_BASE_URL: &str = "https://api.openai.com/v1";
pub const CODEX_BACKEND_BASE_URL: &str = "https://chatgpt.com/backend-api";
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelStreamEvent {
    Thinking(String),
    Text(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WirelessProvider {
    OpenAi,
    OpenRouter,
    Anthropic,
    DeepSeek,
}

impl WirelessProvider {
    pub fn id(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::OpenRouter => "openrouter",
            Self::Anthropic => "anthropic",
            Self::DeepSeek => "deepseek",
        }
    }

    pub fn default_model(self) -> Option<&'static str> {
        match self {
            Self::OpenAi => Some("gpt-5-mini"),
            Self::DeepSeek => Some("deepseek-v4-pro"),
            Self::OpenRouter | Self::Anthropic => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UartProvider {
    #[default]
    Codex,
    ClaudeCode,
}

impl UartProvider {
    pub fn id(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum ModelBackendSettings {
    Wireless { provider: WirelessProvider },
    Uart { provider: UartProvider },
}

impl Default for ModelBackendSettings {
    fn default() -> Self {
        Self::Uart {
            provider: UartProvider::Codex,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelSettings {
    pub backend: ModelBackendSettings,
    pub model: Option<String>,
    pub thinking_level: ThinkingLevel,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel {
    #[default]
    High,
    Xhigh,
}

impl ThinkingLevel {
    pub fn id(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Xhigh => "xhigh",
        }
    }
}

impl ModelSettings {
    pub fn resolved_model(&self) -> Result<String, String> {
        if let Some(model) = self.model.as_ref().filter(|model| !model.is_empty()) {
            return Ok(model.clone());
        }
        match self.backend {
            ModelBackendSettings::Wireless { provider } => provider
                .default_model()
                .map(ToString::to_string)
                .ok_or_else(|| format!("{} requires an explicit model", provider.id())),
            ModelBackendSettings::Uart {
                provider: UartProvider::Codex,
            } => Ok("codex".to_string()),
            ModelBackendSettings::Uart {
                provider: UartProvider::ClaudeCode,
            } => Ok("claude-code".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_backend_defaults_without_secrets() {
        let settings = ModelSettings::default();
        assert_eq!(settings.resolved_model().unwrap(), "codex");
        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains("key"));
        let settings = ModelSettings {
            backend: ModelBackendSettings::Wireless {
                provider: WirelessProvider::DeepSeek,
            },
            model: None,
            thinking_level: ThinkingLevel::Xhigh,
        };
        assert_eq!(settings.resolved_model().unwrap(), "deepseek-v4-pro");
        assert_eq!(settings.thinking_level.id(), "xhigh");
    }
}
