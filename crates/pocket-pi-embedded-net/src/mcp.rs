use alloc::string::String;

use serde::Serialize;

pub const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: &'static str,
    pub params: T,
}

impl<T> JsonRpcRequest<T> {
    pub const fn new(id: u64, method: &'static str, params: T) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method,
            params,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams<'a> {
    pub protocol_version: &'static str,
    pub capabilities: EmptyParams,
    pub client_info: ClientInfo<'a>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ClientInfo<'a> {
    pub name: &'a str,
    pub version: &'a str,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct EmptyParams {}

pub fn initialize(id: u64, version: &str) -> JsonRpcRequest<InitializeParams<'_>> {
    JsonRpcRequest::new(
        id,
        "initialize",
        InitializeParams {
            protocol_version: PROTOCOL_VERSION,
            capabilities: EmptyParams {},
            client_info: ClientInfo {
                name: "pocket-pi-p4",
                version,
            },
        },
    )
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ToolCallParams<A> {
    pub name: &'static str,
    pub arguments: A,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AccountArgs<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_number: Option<&'a str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadOnlyRobinhoodCall<'a> {
    GetAccounts,
    GetPortfolio { account_number: Option<&'a str> },
}

impl<'a> ReadOnlyRobinhoodCall<'a> {
    pub fn into_request(self, id: u64) -> JsonRpcRequest<ToolCallParams<AccountArgs<'a>>> {
        let (name, account_number) = match self {
            Self::GetAccounts => ("get_accounts", None),
            Self::GetPortfolio { account_number } => ("get_portfolio", account_number),
        };
        JsonRpcRequest::new(
            id,
            "tools/call",
            ToolCallParams {
                name,
                arguments: AccountArgs { account_number },
            },
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct McpSession {
    pub id: Option<String>,
    pub next_request_id: u64,
}

impl Default for McpSession {
    fn default() -> Self {
        Self {
            id: None,
            next_request_id: 1,
        }
    }
}

impl McpSession {
    pub fn take_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_uses_pinned_protocol_version() {
        let json = serde_json::to_string(&initialize(1, "0.1.0")).unwrap();
        assert!(json.contains(r#""protocolVersion":"2025-06-18""#));
        assert!(json.contains(r#""name":"pocket-pi-p4""#));
    }

    #[test]
    fn read_only_surface_serializes_only_named_portfolio_tools() {
        let accounts =
            serde_json::to_string(&ReadOnlyRobinhoodCall::GetAccounts.into_request(1)).unwrap();
        let portfolio = serde_json::to_string(
            &ReadOnlyRobinhoodCall::GetPortfolio {
                account_number: Some("masked-account"),
            }
            .into_request(2),
        )
        .unwrap();

        assert!(accounts.contains("get_accounts"));
        assert!(portfolio.contains("get_portfolio"));
        assert!(!accounts.contains("order"));
        assert!(!portfolio.contains("order"));
    }
}
