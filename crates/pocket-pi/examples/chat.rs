//! A one-shot Pocket Pi chat demo.
//!
//!   cargo run -p pocket-pi --example chat -- "Say hi in three words."
//!
//! With ANTHROPIC_API_KEY set it streams a real Anthropic turn; otherwise it
//! replays a scripted answer so the demo runs fully offline. Either way the
//! runtime is driven by a deliberately slow 2 Hz pump — proof that an agent
//! runtime doesn't need a hot loop; it needs a heartbeat.

use pocket_pi::{PiRuntime, ToolResult};
use std::io::Write;
use std::time::{Duration, Instant};

fn main() {
    let prompt = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    let prompt = if prompt.is_empty() {
        "In one short sentence, what are you?".to_string()
    } else {
        prompt
    };

    let mut rt = PiRuntime::new().expect("runtime");

    // A trivial native tool, to show the agent can reach host capabilities.
    rt.register_tool("current_time", |_args| {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        ToolResult::text(format!("unix seconds: {secs}"))
    });

    rt.on_event(|ev| match ev.kind.as_str() {
        "text" => {
            if let Some(d) = ev.value.get("delta").and_then(|v| v.as_str()) {
                print!("{d}");
                let _ = std::io::stdout().flush();
            }
        }
        "tool_start" => {
            let name = ev.value.get("name").and_then(|v| v.as_str()).unwrap_or("");
            eprint!("\n[tool: {name}] ");
        }
        "end" => println!("\n[done]"),
        "error" => eprintln!("\n[error] {}", ev.raw),
        _ => {}
    });

    let cfg = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) => serde_json::json!({
            "model": "claude-opus-4-8",
            "apiKey": key,
            "maxTokens": 256,
            "systemPrompt": "You are Pocket Pi, a tiny agent living in a QuickJS runtime. Be brief.",
            "tools": [{
                "name": "current_time",
                "description": "Get the current unix time in seconds.",
                "parameters": {"type": "object", "properties": {}}
            }]
        }),
        Err(_) => {
            eprintln!("(no ANTHROPIC_API_KEY — running the offline scripted assistant)\n");
            serde_json::json!({
                "model": "offline",
                "scripted": { "steps": [
                    { "text": "I'm Pocket Pi — pi's agent core running inside QuickJS, no Node, no bun." }
                ]}
            })
        }
    };

    rt.boot(&cfg.to_string()).expect("boot");
    println!("you> {prompt}\npi > ");
    rt.prompt(&prompt).expect("prompt");

    // The whole point: pump slowly. LLM latency dwarfs a 500 ms frame.
    let start = Instant::now();
    while !rt.is_idle() && start.elapsed() < Duration::from_secs(120) {
        rt.pump().expect("pump");
        std::thread::sleep(Duration::from_millis(500));
    }
    rt.pump().ok();
}
