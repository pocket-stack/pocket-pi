use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde_json::{json, Value};

/// Build the same single-decision prompt used by the board's Codex bridge.
/// Codex chooses one ESP tool call or a final text response; it never executes
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
         Do not call host tools or inspect Mac files. The JSON tools below run only on the ESP32 after you request one.\n\n\
         Return exactly one compact JSON object and no Markdown. Choose either \
         {{\"toolCall\":{{\"name\":\"registered.name\",\"arguments\":{{...}}}}}} to take one action, \
         or {{\"text\":\"final response\"}} when the turn is complete. After a tool result you may request the next tool. \
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
    if let Some(call) = response.get("toolCall").and_then(Value::as_object) {
        let name = call
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "Codex toolCall is missing name".to_string())?;
        if !tools.iter().any(|registered| registered == name) {
            return Err(format!("Codex requested unregistered ESP32 tool: {name}"));
        }
        let arguments = call.get("arguments").cloned().unwrap_or_else(|| json!({}));
        if !arguments.is_object() {
            return Err("Codex toolCall.arguments must be an object".into());
        }
        return Ok(json!({
            "toolCall":{"id":call_id,"name":name,"arguments":arguments}
        })
        .to_string());
    }
    if let Some(text) = response.get("text").and_then(Value::as_str) {
        return Ok(json!({"text":text}).to_string());
    }
    Err("Codex model response needs text or toolCall".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_registered_tools() {
        let tools = alloc::vec!["read".to_string()];
        let result = parse_response(
            r#"{"toolCall":{"name":"read","arguments":{"path":"a.txt"}}}"#,
            &tools,
            "call_1",
        )
        .unwrap();
        assert!(result.contains("call_1"));
        assert!(parse_response(
            r#"{"toolCall":{"name":"bash","arguments":{}}}"#,
            &tools,
            "call_2"
        )
        .is_err());
    }
}
