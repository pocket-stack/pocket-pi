use alloc::string::String;
use serde::{Deserialize, Serialize};

pub const OPENAI_API_BASE_URL: &str = "https://api.openai.com/v1";
pub const CODEX_BACKEND_BASE_URL: &str = "https://chatgpt.com/backend-api";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    OpenAi,
    OpenRouter,
    Anthropic,
    CodexCodingPlan,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelRequest {
    pub provider: Provider,
    pub model: String,
    pub payload: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelEvent {
    TextDelta { text: String },
    Completed { text: String },
    Failed { message: String },
}
