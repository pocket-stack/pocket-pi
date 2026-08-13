use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde_json::{json, Map, Value};

use crate::model::ModelStreamEvent;

pub fn build_request(request_json: &str) -> Result<String, String> {
    let request: Value = serde_json::from_str(request_json)
        .map_err(|error| format!("parse Pi model request: {error}"))?;
    let model = request
        .pointer("/model/id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Pi model request is missing model.id".to_string())?;
    let context = request
        .get("context")
        .and_then(Value::as_object)
        .ok_or_else(|| "Pi model request is missing context".to_string())?;

    let mut body = Map::new();
    body.insert("model".into(), Value::String(model.into()));
    body.insert(
        "max_tokens".into(),
        Value::from(
            request
                .pointer("/model/maxTokens")
                .and_then(Value::as_u64)
                .unwrap_or(1024)
                .clamp(1, 16_384),
        ),
    );
    body.insert("stream".into(), Value::Bool(true));
    if let Some(system) = context
        .get("systemPrompt")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        body.insert("system".into(), Value::String(system.into()));
    }

    let messages = context
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(convert_message)
        .collect::<Result<Vec<_>, _>>()?;
    body.insert("messages".into(), Value::Array(messages));

    let tools = context
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|tool| {
            let name = tool
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| "Pi tool definition is missing name".to_string())?;
            Ok(json!({
                "name":name,
                "description":tool.get("description").and_then(Value::as_str).unwrap_or(""),
                "input_schema":tool.get("parameters").cloned()
                    .unwrap_or_else(|| json!({"type":"object","properties":{}}))
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    if !tools.is_empty() {
        body.insert("tools".into(), Value::Array(tools));
        body.insert("tool_choice".into(), json!({"type":"auto"}));
    }
    serde_json::to_string(&Value::Object(body))
        .map_err(|error| format!("serialize Anthropic request: {error}"))
}

#[derive(Default)]
struct PendingToolCall {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
pub struct Stream {
    text: String,
    tool_calls: BTreeMap<u64, PendingToolCall>,
    stop_reason: Option<String>,
}

impl Stream {
    pub fn push(&mut self, data_json: &str) -> Result<Vec<ModelStreamEvent>, String> {
        let event: Value = serde_json::from_str(data_json)
            .map_err(|error| format!("parse Anthropic stream event: {error}"))?;
        match event.get("type").and_then(Value::as_str) {
            Some("error") => Err(format!(
                "Anthropic stream error: {}",
                event.get("error").unwrap_or(&Value::Null)
            )),
            Some("content_block_start") => {
                let block = event
                    .get("content_block")
                    .and_then(Value::as_object)
                    .ok_or_else(|| "Anthropic content block is missing".to_string())?;
                if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                    let index = event.get("index").and_then(Value::as_u64).unwrap_or(0);
                    let call = self.tool_calls.entry(index).or_default();
                    call.id = block.get("id").and_then(Value::as_str).unwrap_or("").into();
                    call.name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into();
                }
                Ok(Vec::new())
            }
            Some("content_block_delta") => {
                let delta = event
                    .get("delta")
                    .and_then(Value::as_object)
                    .ok_or_else(|| "Anthropic content delta is missing".to_string())?;
                match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        let text = delta.get("text").and_then(Value::as_str).unwrap_or("");
                        self.text.push_str(text);
                        Ok(if text.is_empty() {
                            Vec::new()
                        } else {
                            alloc::vec![ModelStreamEvent::Text(text.to_string())]
                        })
                    }
                    Some("input_json_delta") => {
                        if let Some(json) = delta.get("partial_json").and_then(Value::as_str) {
                            let index = event.get("index").and_then(Value::as_u64).unwrap_or(0);
                            self.tool_calls
                                .entry(index)
                                .or_default()
                                .arguments
                                .push_str(json);
                        }
                        Ok(Vec::new())
                    }
                    _ => Ok(Vec::new()),
                }
            }
            Some("message_delta") => {
                self.stop_reason = event
                    .pointer("/delta/stop_reason")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                Ok(Vec::new())
            }
            _ => Ok(Vec::new()),
        }
    }

    pub fn finish(self) -> Result<String, String> {
        let stop_reason = match self.stop_reason.as_deref() {
            Some("end_turn" | "stop_sequence") => "stop",
            Some("tool_use") => "toolUse",
            Some("max_tokens") if self.tool_calls.is_empty() => "length",
            Some("max_tokens") => return Err("Anthropic truncated streamed tool calls".into()),
            Some(reason) => return Err(format!("unsupported Anthropic stop reason: {reason}")),
            None => return Err("Anthropic stream ended without stop_reason".into()),
        };
        let mut tool_calls = Vec::new();
        for (_, call) in self.tool_calls {
            if call.id.is_empty() || call.name.is_empty() {
                return Err("Anthropic tool use is missing id or name".into());
            }
            let arguments: Value = serde_json::from_str(if call.arguments.is_empty() {
                "{}"
            } else {
                &call.arguments
            })
            .map_err(|error| format!("parse Anthropic tool input: {error}"))?;
            if !arguments.is_object() {
                return Err("Anthropic tool input must be an object".into());
            }
            tool_calls.push(json!({"id":call.id,"name":call.name,"arguments":arguments}));
        }
        if self.text.is_empty() && tool_calls.is_empty() {
            return Err("Anthropic stream contained no model decision".into());
        }
        serde_json::to_string(&json!({
            "thinking":"",
            "text":self.text,
            "toolCalls":tool_calls,
            "usage":{
                "input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,
                "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}
            },
            "stopReason":stop_reason
        }))
        .map_err(|error| format!("serialize Anthropic model result: {error}"))
    }
}

fn convert_message(message: &Value) -> Result<Value, String> {
    match message.get("role").and_then(Value::as_str) {
        Some("assistant") => {
            let mut content = Vec::new();
            let text = content_text(message.get("content"));
            if !text.is_empty() {
                content.push(json!({"type":"text","text":text}));
            }
            for block in message
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if block.get("type").and_then(Value::as_str) == Some("toolCall") {
                    content.push(json!({
                        "type":"tool_use",
                        "id":block.get("id").and_then(Value::as_str)
                            .ok_or_else(|| "Pi tool call is missing id".to_string())?,
                        "name":block.get("name").and_then(Value::as_str)
                            .ok_or_else(|| "Pi tool call is missing name".to_string())?,
                        "input":block.get("arguments").cloned()
                            .unwrap_or_else(|| Value::Object(Map::new()))
                    }));
                }
            }
            Ok(json!({"role":"assistant","content":content}))
        }
        Some("toolResult") => Ok(json!({
            "role":"user",
            "content":[{
                "type":"tool_result",
                "tool_use_id":message.get("toolCallId").and_then(Value::as_str)
                    .ok_or_else(|| "Pi tool result is missing toolCallId".to_string())?,
                "content":content_text(message.get("content")),
                "is_error":message.get("isError").and_then(Value::as_bool).unwrap_or(false)
            }]
        })),
        _ => Ok(json!({
            "role":"user",
            "content":[{"type":"text","text":content_text(message.get("content"))}]
        })),
    }
}

fn content_text(content: Option<&Value>) -> String {
    let Some(content) = content else {
        return String::new();
    };
    if let Some(text) = content.as_str() {
        return text.into();
    }
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn streams_text() {
        let mut stream = Stream::default();
        assert_eq!(
            stream
                .push(r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"hi"}}"#)
                .unwrap(),
            vec![ModelStreamEvent::Text("hi".into())]
        );
        stream
            .push(r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"}}"#)
            .unwrap();
        let result: Value = serde_json::from_str(&stream.finish().unwrap()).unwrap();
        assert_eq!(result["text"], "hi");
    }

    #[test]
    fn streams_multiple_tool_calls() {
        let mut stream = Stream::default();
        for event in [
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"one","name":"first"}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{}"}}"#,
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"two","name":"second"}}"#,
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"n\":2}"}}"#,
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"}}"#,
        ] {
            stream.push(event).unwrap();
        }
        let result: Value = serde_json::from_str(&stream.finish().unwrap()).unwrap();
        assert_eq!(result["toolCalls"].as_array().unwrap().len(), 2);
        assert_eq!(result["stopReason"], "toolUse");
    }
}
