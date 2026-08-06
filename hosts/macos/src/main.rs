use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;
use std::time::{Duration, Instant};

use pocket_pi::{PiRuntime, ToolResult};

fn main() -> Result<(), String> {
    let prompt = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    let prompt = if prompt.is_empty() {
        "In one short sentence, what are you?".to_owned()
    } else {
        prompt
    };

    let mut runtime = PiRuntime::new()?;
    runtime.register_tool("current_time", |_| {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        ToolResult::text(format!("unix seconds: {seconds}"))
    });
    let error = Rc::new(RefCell::new(None));
    let callback_error = error.clone();
    runtime.on_event(move |event| match event.kind.as_str() {
        "text" => {
            if let Some(delta) = event.value.get("delta").and_then(|value| value.as_str()) {
                print!("{delta}");
                std::io::stdout().flush().ok();
            }
        }
        "end" => println!(),
        "error" => {
            let message = event
                .value
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or(&event.raw)
                .to_owned();
            eprintln!("\n{message}");
            *callback_error.borrow_mut() = Some(message);
        }
        _ => {}
    });

    let config = if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
        serde_json::json!({
            "provider": "openai",
            "model": std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-5.6".into()),
            "apiKey": api_key,
            "systemPrompt": "You are Pocket Pi on macOS. Be concise."
        })
    } else if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        serde_json::json!({
            "provider": "anthropic",
            "model": std::env::var("ANTHROPIC_MODEL")
                .unwrap_or_else(|_| "claude-opus-4-8".into()),
            "apiKey": api_key,
            "systemPrompt": "You are Pocket Pi on macOS. Be concise."
        })
    } else {
        serde_json::json!({
            "model": "offline",
            "scripted": {"steps": [{"text": "Pocket Pi desktop runtime is ready."}]}
        })
    };
    runtime.boot(&config.to_string())?;
    println!("you> {prompt}\npi > ");
    runtime.prompt(&prompt)?;

    let started = Instant::now();
    while !runtime.is_idle() && started.elapsed() < Duration::from_secs(120) {
        runtime.pump()?;
        std::thread::sleep(Duration::from_millis(33));
    }
    runtime.pump().ok();
    if let Some(message) = error.borrow_mut().take() {
        return Err(format!("model backend failed: {message}"));
    }
    if !runtime.is_idle() {
        return Err("model backend timed out after 120 seconds".into());
    }
    Ok(())
}
