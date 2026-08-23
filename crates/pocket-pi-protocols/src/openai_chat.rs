use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde_json::{json, Map, Value};

use crate::model::ModelStreamEvent;

const DEEPSEEK_TOOL_SEPARATOR: &str = "_dot_";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dialect {
    OpenAi,
    OpenRouter,
    DeepSeek,
}

pub fn build_request(request_json: &str) -> Result<String, String> {
    build_request_for(request_json, Dialect::OpenAi)
}

pub fn build_request_for(request_json: &str, dialect: Dialect) -> Result<String, String> {
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
        if let Some(message) = convert_message(message, dialect)? {
            messages.push(message);
        }
    }

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
            let provider_name = encode_tool_name(name, dialect)?;
            Ok(json!({
                "type":"function",
                "function":{
                    "name":provider_name,
                    "description":tool.get("description").and_then(Value::as_str).unwrap_or(""),
                    "parameters":tool.get("parameters").cloned()
                        .unwrap_or_else(|| json!({"type":"object","properties":{}}))
                }
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let max_tokens = request
        .pointer("/options/maxTokens")
        .and_then(Value::as_u64)
        .or_else(|| request.pointer("/model/maxTokens").and_then(Value::as_u64))
        .unwrap_or(1024)
        .clamp(1, 384_000);
    let mut body = Map::new();
    body.insert("model".into(), Value::String(model.into()));
    body.insert("messages".into(), Value::Array(messages));
    body.insert(
        if dialect == Dialect::OpenAi {
            "max_completion_tokens"
        } else {
            "max_tokens"
        }
        .into(),
        Value::from(max_tokens),
    );
    body.insert("stream".into(), Value::Bool(true));

    if dialect == Dialect::DeepSeek {
        let reasoning = request
            .pointer("/options/reasoning")
            .and_then(Value::as_str)
            .unwrap_or("high");
        body.insert("thinking".into(), json!({"type":"enabled"}));
        body.insert(
            "reasoning_effort".into(),
            Value::String(if matches!(reasoning, "xhigh" | "max") {
                "max".into()
            } else {
                "high".into()
            }),
        );
        body.insert("stream_options".into(), json!({"include_usage":true}));
    } else {
        body.insert("parallel_tool_calls".into(), Value::Bool(false));
    }

    if !tools.is_empty() {
        body.insert("tools".into(), Value::Array(tools));
        if dialect != Dialect::DeepSeek {
            body.insert("tool_choice".into(), Value::String("auto".into()));
        }
    }

    serde_json::to_string(&Value::Object(body))
        .map_err(|error| format!("serialize chat completions request: {error}"))
}

#[derive(Default)]
struct PendingToolCall {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
struct Usage {
    input: u64,
    output: u64,
    cache_read: u64,
    reasoning: u64,
    total: u64,
}

pub struct Stream {
    dialect: Dialect,
    emit_progress: bool,
    thinking: String,
    text: String,
    tool_calls: BTreeMap<u64, PendingToolCall>,
    usage: Usage,
    stop_reason: Option<String>,
}

impl Stream {
    pub fn new(dialect: Dialect, emit_progress: bool) -> Self {
        Self {
            dialect,
            emit_progress,
            thinking: String::new(),
            text: String::new(),
            tool_calls: BTreeMap::new(),
            usage: Usage::default(),
            stop_reason: None,
        }
    }

    pub fn push(&mut self, data_json: &str) -> Result<Vec<ModelStreamEvent>, String> {
        let event: Value = serde_json::from_str(data_json)
            .map_err(|error| format!("parse chat completions stream event: {error}"))?;
        if let Some(error) = event.get("error") {
            return Err(format!("chat completions stream error: {error}"));
        }
        if let Some(usage) = event.get("usage").and_then(Value::as_object) {
            self.usage.input = usage
                .get("prompt_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            self.usage.output = usage
                .get("completion_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            self.usage.total = usage
                .get("total_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(self.usage.input.saturating_add(self.usage.output));
            self.usage.cache_read = usage
                .get("prompt_tokens_details")
                .and_then(|details| details.get("cached_tokens"))
                .and_then(Value::as_u64)
                .or_else(|| usage.get("prompt_cache_hit_tokens").and_then(Value::as_u64))
                .unwrap_or(0);
            self.usage.reasoning = usage
                .get("completion_tokens_details")
                .and_then(|details| details.get("reasoning_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
        }

        let Some(choice) = event
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
        else {
            return Ok(Vec::new());
        };
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.stop_reason = Some(reason.into());
        }
        let Some(delta) = choice.get("delta").and_then(Value::as_object) else {
            return Ok(Vec::new());
        };

        let mut events = Vec::new();
        if let Some(thinking) = delta.get("reasoning_content").and_then(Value::as_str) {
            if !thinking.is_empty() {
                self.thinking.push_str(thinking);
                if self.emit_progress {
                    events.push(ModelStreamEvent::Thinking(thinking.into()));
                }
            }
        }
        if let Some(content) = delta.get("content").and_then(Value::as_str) {
            if !content.is_empty() {
                self.text.push_str(content);
                if self.emit_progress {
                    events.push(ModelStreamEvent::Text(content.into()));
                }
            }
        }
        for call in delta
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let index = call.get("index").and_then(Value::as_u64).unwrap_or(0);
            let pending = self.tool_calls.entry(index).or_default();
            if let Some(id) = call.get("id").and_then(Value::as_str) {
                pending.id.push_str(id);
            }
            if let Some(function) = call.get("function").and_then(Value::as_object) {
                if let Some(name) = function.get("name").and_then(Value::as_str) {
                    pending.name.push_str(name);
                }
                if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
                    pending.arguments.push_str(arguments);
                }
            }
        }
        Ok(events)
    }

    pub fn finish(self) -> Result<String, String> {
        let stop_reason = match self.stop_reason.as_deref() {
            Some("stop") => "stop",
            Some("tool_calls") => "toolUse",
            Some("length") if self.tool_calls.is_empty() => "length",
            Some("length") => return Err("provider truncated streamed tool calls".into()),
            Some("content_filter") => {
                return Err("provider content filter stopped the response".into())
            }
            Some("insufficient_system_resource") => {
                return Err("provider had insufficient system resources".into())
            }
            Some(other) => return Err(format!("unsupported provider finish reason: {other}")),
            None => return Err("provider stream ended without finish_reason".into()),
        };

        let mut tool_calls = Vec::new();
        for (_, call) in self.tool_calls {
            if call.id.is_empty() || call.name.is_empty() {
                return Err("streamed tool call is missing id or name".into());
            }
            let arguments: Value = serde_json::from_str(if call.arguments.is_empty() {
                "{}"
            } else {
                &call.arguments
            })
            .map_err(|error| format!("parse streamed tool arguments: {error}"))?;
            if !arguments.is_object() {
                return Err("streamed tool arguments must be a JSON object".into());
            }
            tool_calls.push(json!({
                "id":call.id,
                "name":decode_tool_name(&call.name, self.dialect),
                "arguments":arguments
            }));
        }
        if self.text.is_empty() && tool_calls.is_empty() {
            return Err("provider stream contained no model decision".into());
        }

        let mut result = Map::new();
        result.insert("thinking".into(), Value::String(self.thinking.clone()));
        if !self.thinking.is_empty() {
            result.insert(
                "thinkingSignature".into(),
                Value::String("reasoning_content".into()),
            );
        }
        result.insert("text".into(), Value::String(self.text));
        result.insert("toolCalls".into(), Value::Array(tool_calls));
        result.insert(
            "usage".into(),
            json!({
                "input":self.usage.input,
                "output":self.usage.output,
                "cacheRead":self.usage.cache_read,
                "cacheWrite":0,
                "reasoning":self.usage.reasoning,
                "totalTokens":self.usage.total,
                "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}
            }),
        );
        result.insert("stopReason".into(), Value::String(stop_reason.into()));
        serde_json::to_string(&Value::Object(result))
            .map_err(|error| format!("serialize model result: {error}"))
    }
}

fn convert_message(message: &Value, dialect: Dialect) -> Result<Option<Value>, String> {
    let Some(role) = message.get("role").and_then(Value::as_str) else {
        return Ok(None);
    };
    match role {
        "user" => Ok(Some(json!({
            "role":"user",
            "content":content_text(message.get("content"))
        }))),
        "assistant" => {
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
                let provider_name = encode_tool_name(name, dialect)?;
                let arguments = serde_json::to_string(
                    block.get("arguments").unwrap_or(&Value::Object(Map::new())),
                )
                .map_err(|error| format!("serialize prior tool arguments: {error}"))?;
                tool_calls.push(json!({
                    "id":id,
                    "type":"function",
                    "function":{"name":provider_name,"arguments":arguments}
                }));
            }
            let mut converted = Map::new();
            converted.insert("role".into(), Value::String("assistant".into()));
            converted.insert(
                "content".into(),
                Value::String(content_text(message.get("content"))),
            );
            if dialect == Dialect::DeepSeek {
                let thinking = thinking_text(message.get("content"));
                if !thinking.is_empty() {
                    converted.insert("reasoning_content".into(), Value::String(thinking));
                }
            }
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

fn encode_tool_name(name: &str, dialect: Dialect) -> Result<String, String> {
    if dialect != Dialect::DeepSeek {
        return Ok(name.into());
    }
    if name.contains(DEEPSEEK_TOOL_SEPARATOR) {
        return Err(format!(
            "tool name {name:?} contains reserved DeepSeek separator {DEEPSEEK_TOOL_SEPARATOR:?}"
        ));
    }
    let encoded = name.replace('.', DEEPSEEK_TOOL_SEPARATOR);
    if encoded.len() > 64
        || !encoded
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(format!(
            "tool name {name:?} cannot be represented for DeepSeek"
        ));
    }
    Ok(encoded)
}

fn decode_tool_name(name: &str, dialect: Dialect) -> String {
    if dialect == Dialect::DeepSeek {
        name.replace(DEEPSEEK_TOOL_SEPARATOR, ".")
    } else {
        name.into()
    }
}

fn content_text(content: Option<&Value>) -> String {
    block_text(content, "text", "text")
}

fn thinking_text(content: Option<&Value>) -> String {
    block_text(content, "thinking", "thinking")
}

fn block_text(content: Option<&Value>, block_type: &str, field: &str) -> String {
    let Some(content) = content else {
        return String::new();
    };
    if block_type == "text" {
        if let Some(text) = content.as_str() {
            return text.into();
        }
    }
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some(block_type))
        .filter_map(|block| block.get(field).and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn deepseek_request(reasoning: &str) -> Value {
        json!({
            "model":{"id":"deepseek-v4-pro","maxTokens":384000},
            "context":{
                "systemPrompt":"Use tools when needed.",
                "messages":[{"role":"user","content":"list files"}],
                "tools":[{"name":"ls","description":"List files","parameters":{"type":"object"}}]
            },
            "options":{"reasoning":reasoning}
        })
    }

    #[test]
    fn deepseek_request_encodes_thinking_tools_and_replay() {
        let mut request = deepseek_request("xhigh");
        request["context"]["tools"][0]["name"] = json!("time.now");
        request["context"]["messages"] = json!([{
            "role":"assistant",
            "content":[
                {"type":"thinking","thinking":"need both tools","thinkingSignature":"reasoning_content"},
                {"type":"toolCall","id":"call_1","name":"time.now","arguments":{}},
                {"type":"toolCall","id":"call_2","name":"device.status","arguments":{"full":true}}
            ]
        }]);
        let body: Value = serde_json::from_str(
            &build_request_for(&request.to_string(), Dialect::DeepSeek).unwrap(),
        )
        .unwrap();
        assert_eq!(body["model"], "deepseek-v4-pro");
        assert_eq!(body["max_tokens"], 384000);
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["reasoning_effort"], "max");
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert!(body.get("tool_choice").is_none());
        assert!(body.get("parallel_tool_calls").is_none());
        assert!(body.get("temperature").is_none());
        assert_eq!(body["tools"][0]["function"]["name"], "time_dot_now");
        assert_eq!(body["messages"][1]["content"], "");
        assert_eq!(body["messages"][1]["reasoning_content"], "need both tools");
        assert_eq!(
            body["messages"][1]["tool_calls"].as_array().unwrap().len(),
            2
        );
        assert_eq!(
            body["messages"][1]["tool_calls"][1]["function"]["name"],
            "device_dot_status"
        );
    }

    #[test]
    fn deepseek_stream_decodes_thinking_text_multiple_tools_and_usage() {
        let mut stream = Stream::new(Dialect::DeepSeek, true);
        assert_eq!(
            stream
                .push(r#"{"choices":[{"delta":{"reasoning_content":"think "}}]}"#)
                .unwrap(),
            vec![ModelStreamEvent::Thinking("think ".into())]
        );
        assert_eq!(
            stream
                .push(r#"{"choices":[{"delta":{"reasoning_content":"more","content":"answer"}}]}"#)
                .unwrap(),
            vec![
                ModelStreamEvent::Thinking("more".into()),
                ModelStreamEvent::Text("answer".into())
            ]
        );
        stream.push(r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"id":"call_2","function":{"name":"device_dot_status","arguments":"{\"b\":"}},{"index":0,"id":"call_1","function":{"name":"time_dot_now","arguments":"{}"}}]}}]}"#).unwrap();
        stream.push(r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"arguments":"2}"}}]},"finish_reason":"tool_calls"}]}"#).unwrap();
        stream.push(r#"{"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":20,"total_tokens":30,"prompt_cache_hit_tokens":4,"completion_tokens_details":{"reasoning_tokens":12}}}"#).unwrap();
        let result: Value = serde_json::from_str(&stream.finish().unwrap()).unwrap();
        assert_eq!(result["thinking"], "think more");
        assert_eq!(result["text"], "answer");
        assert_eq!(result["toolCalls"][0]["name"], "time.now");
        assert_eq!(result["toolCalls"][1]["name"], "device.status");
        assert_eq!(result["toolCalls"][1]["arguments"]["b"], 2);
        assert_eq!(result["usage"]["reasoning"], 12);
        assert_eq!(result["stopReason"], "toolUse");

        let mut truncated = Stream::new(Dialect::DeepSeek, true);
        truncated.push(r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"time_dot_now","arguments":"{"}}]},"finish_reason":"length"}]}"#).unwrap();
        assert!(truncated.finish().unwrap_err().contains("truncated"));
    }

    #[test]
    fn openrouter_uses_max_tokens() {
        let request = json!({
            "model":{"id":"vendor/model"},
            "context":{"messages":[{"role":"user","content":"hi"}]}
        });
        let body: Value = serde_json::from_str(
            &build_request_for(&request.to_string(), Dialect::OpenRouter).unwrap(),
        )
        .unwrap();
        assert!(body.get("max_tokens").is_some());
        assert!(body.get("max_completion_tokens").is_none());
    }
}
