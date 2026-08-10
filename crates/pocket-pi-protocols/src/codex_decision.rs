use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde_json::{json, Value};

/// Build the same decision prompt used by the board's Codex bridge.
/// Codex chooses ESP tool calls or a final text response; it never executes
/// host tools itself.
pub fn build_prompt(request_json: &str) -> Result<(String, Vec<String>), String> {
    let request: Value = serde_json::from_str(request_json)
        .map_err(|error| format!("parse Pi model request: {error}"))?;
    let context = request
        .get("context")
        .and_then(Value::as_object)
        .ok_or_else(|| "Pi model request is missing context".to_string())?;
    let messages = context
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let tools = context
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let recent = messages
        .iter()
        .skip(messages.len().saturating_sub(24))
        .cloned()
        .collect::<Vec<_>>();
    let system = context
        .get("systemPrompt")
        .and_then(Value::as_str)
        .unwrap_or("");
    let prompt = format!(
        "You are the model decision backend for a Pi Agent running on an ESP32-P4.\n\n\
         Do not call host tools or inspect Mac files. The JSON tools below run only on the ESP32 after you request them.\n\n\
         Return exactly one compact JSON object and no Markdown. Choose either \
         {{\"toolCalls\":[{{\"name\":\"registered.name\",\"arguments\":{{...}}}}]}} to take actions, \
         or {{\"text\":\"final response\"}} when the turn is complete. \
         Never claim an action succeeded until its tool result appears in the conversation.\n\n\
         System instruction: {system}\n\n\
         Registered ESP32 tools: {}\n\n\
         Conversation: {}",
        Value::Array(tools),
        Value::Array(recent)
    );
    Ok((prompt, names))
}

pub fn parse_response(raw: &str, tools: &[String], call_id: &str) -> Result<String, String> {
    let raw = raw.trim();
    let raw = raw
        .strip_prefix("```json")
        .or_else(|| raw.strip_prefix("```"))
        .unwrap_or(raw)
        .trim()
        .strip_suffix("```")
        .unwrap_or(raw)
        .trim();
    let response: Value = serde_json::from_str(raw)
        .map_err(|error| format!("Codex returned invalid model JSON: {error}"))?;
    if let Some(calls) = response.get("toolCalls").and_then(Value::as_array) {
        if calls.is_empty() {
            return Err("Codex model response has empty toolCalls".into());
        }
        let mut result_calls = Vec::with_capacity(calls.len());
        for (index, call) in calls.iter().enumerate() {
            let call = call
                .as_object()
                .ok_or_else(|| "Codex toolCalls entries must be objects".to_string())?;
            let name = call
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| "Codex toolCalls entry is missing name".to_string())?;
            if !tools.iter().any(|registered| registered == name) {
                return Err(format!("Codex requested unregistered ESP32 tool: {name}"));
            }
            let arguments = call.get("arguments").cloned().unwrap_or_else(|| json!({}));
            if !arguments.is_object() {
                return Err("Codex toolCalls arguments must be objects".into());
            }
            result_calls.push(json!({
                "id":format!("{call_id}_{index}"),
                "name":name,
                "arguments":arguments
            }));
        }
        return Ok(json!({
            "thinking":"",
            "text":"",
            "toolCalls":result_calls,
            "usage":{
                "input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,
                "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}
            },
            "stopReason":"toolUse"
        })
        .to_string());
    }
    if let Some(text) = response.get("text").and_then(Value::as_str) {
        return Ok(json!({
            "thinking":"",
            "text":text,
            "toolCalls":[],
            "usage":{
                "input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,
                "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}
            },
            "stopReason":"stop"
        })
        .to_string());
    }
    Err("Codex model response needs text or toolCalls".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_registered_tools() {
        let tools = alloc::vec!["read".to_string()];
        let result = parse_response(
            r#"{"toolCalls":[{"name":"read","arguments":{"path":"a.txt"}},{"name":"read","arguments":{"path":"b.txt"}}]}"#,
            &tools,
            "call_1",
        )
        .unwrap();
        assert!(result.contains("call_1_0"));
        assert!(result.contains("call_1_1"));
        assert!(parse_response(
            r#"{"toolCalls":[{"name":"bash","arguments":{}}]}"#,
            &tools,
            "call_2"
        )
        .is_err());
    }
}
