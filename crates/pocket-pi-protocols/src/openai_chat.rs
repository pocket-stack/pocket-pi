use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde_json::{json, Map, Value};

/// Convert the provider-neutral request emitted by embedded Pi into OpenAI's
/// streaming Chat Completions wire format. Both the ESP host and simulator use
/// this code; only their HTTP clients differ.
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

    let mut messages = Vec::new();
    if let Some(system_prompt) = context
        .get("systemPrompt")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        messages.push(json!({"role":"system","content":system_prompt}));
    }
    for message in context
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(message) = convert_message(message)? {
            messages.push(message);
        }
    }

    let mut tools = Vec::new();
    for tool in context
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "Pi tool definition is missing name".to_string())?;
        tools.push(json!({
            "type":"function",
            "function":{
                "name":name,
                "description":tool.get("description").and_then(Value::as_str).unwrap_or(""),
                "parameters":tool.get("parameters").cloned()
                    .unwrap_or_else(|| json!({"type":"object","properties":{}}))
            }
        }));
    }

    let mut body = Map::new();
    body.insert("model".into(), Value::String(model.into()));
    body.insert("messages".into(), Value::Array(messages));
    body.insert(
        "max_completion_tokens".into(),
        Value::from(
            request
                .pointer("/model/maxTokens")
                .and_then(Value::as_u64)
                .unwrap_or(1024)
                .clamp(1, 16_384),
        ),
    );
    body.insert("stream".into(), Value::Bool(true));
    body.insert("parallel_tool_calls".into(), Value::Bool(false));
    if !tools.is_empty() {
        body.insert("tools".into(), Value::Array(tools));
        body.insert("tool_choice".into(), Value::String("auto".into()));
    }
    serde_json::to_string(&Value::Object(body))
        .map_err(|error| format!("serialize OpenAI request: {error}"))
}

#[derive(Default)]
pub struct Stream {
    text: String,
    tool_id: String,
    tool_name: String,
    tool_arguments: String,
    stop_reason: Option<String>,
}

impl Stream {
    pub fn push(&mut self, data_json: &str) -> Result<Option<String>, String> {
        let event: Value = serde_json::from_str(data_json)
            .map_err(|error| format!("parse OpenAI stream event: {error}"))?;
        if let Some(error) = event.get("error") {
            return Err(format!("OpenAI stream error: {error}"));
        }
        let Some(choice) = event
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
        else {
            return Ok(None);
        };
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.stop_reason = Some(reason.into());
        }
        let Some(delta) = choice.get("delta").and_then(Value::as_object) else {
            return Ok(None);
        };
        if let Some(content) = delta.get("content").and_then(Value::as_str) {
            self.text.push_str(content);
            return Ok(Some(content.into()));
        }
        for call in delta
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if call.get("index").and_then(Value::as_u64).unwrap_or(0) != 0 {
                return Err("provider streamed multiple tool calls".into());
            }
            if let Some(id) = call.get("id").and_then(Value::as_str) {
                self.tool_id.push_str(id);
            }
            if let Some(function) = call.get("function").and_then(Value::as_object) {
                if let Some(name) = function.get("name").and_then(Value::as_str) {
                    self.tool_name.push_str(name);
                }
                if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
                    self.tool_arguments.push_str(arguments);
                }
            }
        }
        Ok(None)
    }

    pub fn finish(self) -> Result<String, String> {
        if !self.tool_name.is_empty() {
            if self.tool_id.is_empty() {
                return Err("streamed tool call is missing id".into());
            }
            let arguments: Value = serde_json::from_str(if self.tool_arguments.is_empty() {
                "{}"
            } else {
                &self.tool_arguments
            })
            .map_err(|error| format!("parse streamed tool arguments: {error}"))?;
            if !arguments.is_object() {
                return Err("streamed tool arguments must be a JSON object".into());
            }
            return serde_json::to_string(&json!({
                "toolCall":{"id":self.tool_id,"name":self.tool_name,"arguments":arguments}
            }))
            .map_err(|error| format!("serialize Pi tool call: {error}"));
        }
        if self.text.is_empty() {
            return Err("provider stream contained neither text nor a tool call".into());
        }
        let stop_reason = if self.stop_reason.as_deref() == Some("length") {
            "length"
        } else {
            "stop"
        };
        serde_json::to_string(&json!({"text":self.text,"stopReason":stop_reason}))
            .map_err(|error| format!("serialize Pi text result: {error}"))
    }
}

fn convert_message(message: &Value) -> Result<Option<Value>, String> {
    let Some(role) = message.get("role").and_then(Value::as_str) else {
        return Ok(None);
    };
    match role {
        "user" => Ok(Some(json!({
            "role":"user",
            "content":content_text(message.get("content"))
        }))),
        "assistant" => {
            let text = content_text(message.get("content"));
            let mut tool_calls = Vec::new();
            for block in message
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if block.get("type").and_then(Value::as_str) != Some("toolCall") {
                    continue;
                }
                let id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Pi assistant toolCall is missing id".to_string())?;
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Pi assistant toolCall is missing name".to_string())?;
                let arguments = serde_json::to_string(
                    block.get("arguments").unwrap_or(&Value::Object(Map::new())),
                )
                .map_err(|error| format!("serialize prior tool arguments: {error}"))?;
                tool_calls.push(json!({
                    "id":id,
                    "type":"function",
                    "function":{"name":name,"arguments":arguments}
                }));
            }
            let mut converted = Map::new();
            converted.insert("role".into(), Value::String("assistant".into()));
            converted.insert(
                "content".into(),
                if text.is_empty() {
                    Value::Null
                } else {
                    Value::String(text)
                },
            );
            if !tool_calls.is_empty() {
                converted.insert("tool_calls".into(), Value::Array(tool_calls));
            }
            Ok(Some(Value::Object(converted)))
        }
        "toolResult" => {
            let call_id = message
                .get("toolCallId")
                .and_then(Value::as_str)
                .ok_or_else(|| "Pi toolResult is missing toolCallId".to_string())?;
            Ok(Some(json!({
                "role":"tool",
                "tool_call_id":call_id,
                "content":content_text(message.get("content"))
            })))
        }
        _ => Ok(None),
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

    #[test]
    fn maps_history_and_tools() {
        let request = json!({
            "model":{"id":"gpt-5.6","maxTokens":768},
            "context":{
                "systemPrompt":"Use tools when needed.",
                "messages":[{"role":"user","content":"list files"}],
                "tools":[{"name":"ls","description":"List files","parameters":{"type":"object"}}]
            }
        });
        let body: Value =
            serde_json::from_str(&build_request(&request.to_string()).unwrap()).unwrap();
        assert_eq!(body["model"], "gpt-5.6");
        assert_eq!(body["parallel_tool_calls"], false);
        assert_eq!(body["tools"][0]["function"]["name"], "ls");
    }

    #[test]
    fn decodes_streamed_text() {
        let mut stream = Stream::default();
        assert_eq!(
            stream
                .push(r#"{"choices":[{"delta":{"content":"hel"}}]}"#)
                .unwrap(),
            Some("hel".into())
        );
        stream
            .push(r#"{"choices":[{"delta":{"content":"lo"},"finish_reason":"stop"}]}"#)
            .unwrap();
        let result: Value = serde_json::from_str(&stream.finish().unwrap()).unwrap();
        assert_eq!(result["text"], "hello");
    }
}
