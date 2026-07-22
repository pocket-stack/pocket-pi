//! Dead-simple acceptance: boot the full, embedded pi on a real model and say
//! hello. This binary carries the WHOLE unmodified pi-coding-agent inside it.
//!
//!   OPENAI_API_KEY=…  cargo run --release --example hello
//!   OPENAI_API_KEY=…  OPENAI_MODEL=gpt-5.6  cargo run --release --example hello
use pocket_pi::{HostEvent, PiRuntime};
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

fn main() {
    let key = std::env::var("OPENAI_API_KEY").expect("set OPENAI_API_KEY");
    let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-5.6".into());

    // The whole pi is embedded — new() stands it up with no external files.
    let mut rt = PiRuntime::new().expect("runtime");

    let reply = Rc::new(RefCell::new(String::new()));
    let r = reply.clone();
    rt.on_event(move |ev: &HostEvent| match ev.kind.as_str() {
        "text" => {
            if let Some(d) = ev.value.get("delta").and_then(|v| v.as_str()) {
                r.borrow_mut().push_str(d);
                print!("{d}");
                std::io::stdout().flush().ok();
            }
        }
        "assistant_text" if r.borrow().is_empty() => {
            if let Some(t) = ev.value.get("text").and_then(|v| v.as_str()) {
                *r.borrow_mut() = t.to_string();
                print!("{t}");
            }
        }
        "error" => eprintln!("\n[error] {}", ev.raw),
        _ => {}
    });

    let cfg = serde_json::json!({
        "provider": "openai", "model": model, "apiKey": key,
        "maxTokens": 2048,
        "systemPrompt": "You are a friendly assistant. Be very brief."
    });
    eprintln!("booting full pi on {model} …");
    rt.boot(&cfg.to_string()).expect("boot");
    eprint!("pi > ");
    rt.prompt("Say hello in a few words.").expect("prompt");
    while !rt.is_idle() {
        rt.pump().expect("pump");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    println!();
    if reply.borrow().trim().is_empty() {
        eprintln!("(no reply — check OPENAI_API_KEY / proxy)");
        std::process::exit(1);
    }
}
