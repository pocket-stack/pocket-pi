use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

pub const MCP_URL: &str = "https://agent.robinhood.com/mcp/trading";
pub const PROTECTED_RESOURCE_METADATA_URL: &str =
    "https://agent.robinhood.com/.well-known/oauth-protected-resource/mcp/trading";
pub const AUTHORIZATION_SERVER_METADATA_URL: &str =
    "https://agent.robinhood.com/.well-known/oauth-authorization-server/mcp/trading";

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct AuthorizationServerMetadata {
    pub issuer: alloc::string::String,
    pub authorization_endpoint: alloc::string::String,
    pub token_endpoint: alloc::string::String,
    pub registration_endpoint: alloc::string::String,
    pub code_challenge_methods_supported: Vec<alloc::string::String>,
    pub grant_types_supported: Vec<alloc::string::String>,
}

impl AuthorizationServerMetadata {
    pub fn supports_required_flow(&self) -> bool {
        self.code_challenge_methods_supported
            .iter()
            .any(|method| method == "S256")
            && self
                .grant_types_supported
                .iter()
                .any(|grant| grant == "authorization_code")
            && self
                .grant_types_supported
                .iter()
                .any(|grant| grant == "refresh_token")
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ClientRegistrationRequest<'a> {
    pub client_name: &'a str,
    pub redirect_uris: [&'a str; 1],
    pub grant_types: [&'a str; 2],
    pub response_types: [&'a str; 1],
    pub token_endpoint_auth_method: &'a str,
}

impl<'a> ClientRegistrationRequest<'a> {
    pub const fn pocket_pi(redirect_uri: &'a str) -> Self {
        Self {
            client_name: "Pocket Pi ESP32-P4",
            redirect_uris: [redirect_uri],
            grant_types: ["authorization_code", "refresh_token"],
            response_types: ["code"],
            token_endpoint_auth_method: "none",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_robinhood_metadata_requirements() {
        let metadata: AuthorizationServerMetadata = serde_json::from_str(
            r#"{
                "issuer":"https://agent.robinhood.com/mcp/trading",
                "authorization_endpoint":"https://robinhood.com/oauth",
                "token_endpoint":"https://api.robinhood.com/oauth2/token/",
                "registration_endpoint":"https://agent.robinhood.com/oauth/trading/register",
                "code_challenge_methods_supported":["S256"],
                "grant_types_supported":["authorization_code","refresh_token"]
            }"#,
        )
        .unwrap();

        assert!(metadata.supports_required_flow());
    }

    #[test]
    fn rejects_metadata_without_refresh_tokens() {
        let metadata = AuthorizationServerMetadata {
            issuer: MCP_URL.into(),
            authorization_endpoint: "https://robinhood.com/oauth".into(),
            token_endpoint: "https://api.robinhood.com/oauth2/token/".into(),
            registration_endpoint: "https://agent.robinhood.com/oauth/trading/register".into(),
            code_challenge_methods_supported: alloc::vec!["S256".into()],
            grant_types_supported: alloc::vec!["authorization_code".into()],
        };

        assert!(!metadata.supports_required_flow());
    }
}
