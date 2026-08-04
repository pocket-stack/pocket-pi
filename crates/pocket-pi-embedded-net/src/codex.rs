use alloc::string::String;

use serde::{Deserialize, Serialize};

pub const OPENAI_API_BASE_URL: &str = "https://api.openai.com/v1";
pub const CODEX_BACKEND_BASE_URL: &str = "https://chatgpt.com/backend-api";

/// These endpoints and client ID match the `openai-codex` provider shipped by
/// Pocket Pi's pinned Pi 0.81.1 dependency. They are intentionally isolated so
/// a provider update cannot silently change the public API-key path.
pub mod coding_plan_experiment {
    pub const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
    pub const DEVICE_USER_CODE_URL: &str =
        "https://auth.openai.com/api/accounts/deviceauth/usercode";
    pub const DEVICE_TOKEN_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";
    pub const DEVICE_VERIFICATION_URL: &str = "https://auth.openai.com/codex/device";
    pub const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
    pub const REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeviceCodeRequest<'a> {
    pub client_id: &'a str,
}

impl Default for DeviceCodeRequest<'static> {
    fn default() -> Self {
        Self {
            client_id: coding_plan_experiment::CLIENT_ID,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_auth_id: String,
    pub user_code: String,
    #[serde(deserialize_with = "number_from_number_or_string")]
    pub interval: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeviceTokenPollRequest<'a> {
    pub device_auth_id: &'a str,
    pub user_code: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct DeviceAuthorizationResponse {
    pub authorization_code: String,
    pub code_verifier: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "grant_type")]
pub enum TokenRequest<'a> {
    #[serde(rename = "authorization_code")]
    AuthorizationCode {
        client_id: &'a str,
        code: &'a str,
        code_verifier: &'a str,
        redirect_uri: &'a str,
    },
    #[serde(rename = "refresh_token")]
    RefreshToken {
        client_id: &'a str,
        refresh_token: &'a str,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

fn number_from_number_or_string<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumberOrString {
        Number(u32),
        String(String),
    }

    match NumberOrString::deserialize(deserializer)? {
        NumberOrString::Number(value) => Ok(value),
        NumberOrString::String(value) => value.parse().map_err(serde::de::Error::custom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_device_interval_in_both_observed_shapes() {
        let number: DeviceCodeResponse = serde_json::from_str(
            r#"{"device_auth_id":"device","user_code":"ABCD-EFGH","interval":5}"#,
        )
        .unwrap();
        let string: DeviceCodeResponse = serde_json::from_str(
            r#"{"device_auth_id":"device","user_code":"ABCD-EFGH","interval":"5"}"#,
        )
        .unwrap();

        assert_eq!(number.interval, 5);
        assert_eq!(string.interval, 5);
    }

    #[test]
    fn api_key_and_coding_plan_use_different_origins() {
        assert_ne!(OPENAI_API_BASE_URL, CODEX_BACKEND_BASE_URL);
    }
}
