use std::path::Path;

use serde_json::{json, Value};

use crate::coding::CodingTools;
use crate::{NativeToolResult, PlatformTools};

const MAX_COMMAND_BYTES: usize = 512;

pub fn definitions() -> Vec<Value> {
    vec![json!({
        "name":"bash",
        "description":"Run one native embedded command. Supported commands: help, status, pwd, ls [path], cat <path>, grep <pattern> [path], wifi status, agent status, reboot. Pipes, redirects, command substitution, environment expansion and command chaining are rejected.",
        "parameters":{"type":"object","properties":{
            "command":{"type":"string","maxLength":MAX_COMMAND_BYTES}
        },"required":["command"],"additionalProperties":false}
    })]
}

pub fn execute(
    coding: &CodingTools,
    root: &Path,
    platform: &dyn PlatformTools,
    call_id: &str,
    name: &str,
    args: &Value,
) -> Result<Option<NativeToolResult>, String> {
    if name != "bash" {
        return Ok(None);
    }
    let command = args
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| "bash.command must be a string".to_owned())?;
    let words = parse_command(command)?;
    let Some(program) = words.first().map(String::as_str) else {
        return Err("bash.command must not be empty".to_owned());
    };
    let tail = &words[1..];

    let result = match (program, tail) {
        ("help", []) => text(
            "Supported: help, status, pwd, ls [path], cat <path>, grep <pattern> [path], wifi status, agent status, reboot",
            json!({"shell":"native-allowlist","arbitraryExecution":false}),
        ),
        ("status", []) => json_result(platform.device_status()),
        ("pwd", []) => text(
            "/workspace",
            json!({"path":"/workspace","hostPath":root.display().to_string()}),
        ),
        ("ls", []) => return coding.execute(call_id, "ls", &json!({})),
        ("ls", [path]) => return coding.execute(call_id, "ls", &json!({"path":path})),
        ("cat", [path]) => return coding.execute(call_id, "read", &json!({"path":path})),
        ("grep", [pattern]) => {
            return coding.execute(call_id, "grep", &json!({"pattern":pattern}))
        }
        ("grep", [pattern, path]) => {
            return coding.execute(
                call_id,
                "grep",
                &json!({"pattern":pattern,"path":path}),
            )
        }
        ("wifi", [subcommand]) if subcommand == "status" => json_result(platform.wifi_status()),
        ("agent", [subcommand]) if subcommand == "status" => text(
            "Pi Agent is running",
            json!({
                "status":"running",
                "harness":"pi-agent-core",
                "runtime":"QuickJS"
            }),
        ),
        ("reboot", []) => NativeToolResult {
            text: "Reboot scheduled".to_owned(),
            details: platform.reboot()?,
            terminate: true,
        },
        _ => return Err("unsupported bash command; run help for the native allowlist".to_owned()),
    };
    Ok(Some(result))
}

fn text(text: impl Into<String>, details: Value) -> NativeToolResult {
    NativeToolResult {
        text: text.into(),
        details,
        terminate: false,
    }
}

fn json_result(details: Value) -> NativeToolResult {
    NativeToolResult {
        text: details.to_string(),
        details,
        terminate: false,
    }
}

fn parse_command(command: &str) -> Result<Vec<String>, String> {
    if command.len() > MAX_COMMAND_BYTES {
        return Err(format!("bash.command exceeds {MAX_COMMAND_BYTES} bytes"));
    }
    if command.chars().any(|character| {
        matches!(
            character,
            '|' | '&' | ';' | '<' | '>' | '$' | '`' | '\\' | '\n' | '\r'
        )
    }) {
        return Err("bash.command contains a forbidden shell operator".to_owned());
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    for character in command.chars() {
        match quote {
            Some(delimiter) if character == delimiter => quote = None,
            Some(_) => word.push(character),
            None if matches!(character, '\'' | '"') => quote = Some(character),
            None if character.is_whitespace() => {
                if !word.is_empty() {
                    words.push(core::mem::take(&mut word));
                }
            }
            None if character.is_control() => {
                return Err("bash.command contains a control character".to_owned())
            }
            None => word.push(character),
        }
    }
    if quote.is_some() {
        return Err("bash.command contains an unterminated quote".to_owned());
    }
    if !word.is_empty() {
        words.push(word);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_shell_operators() {
        assert!(parse_command("ls | cat").is_err());
        assert!(parse_command("echo $(pwd)").is_err());
    }

    #[test]
    fn supports_quoted_arguments() {
        assert_eq!(
            parse_command("grep \"hello world\" memory").unwrap(),
            ["grep", "hello world", "memory"]
        );
    }
}
