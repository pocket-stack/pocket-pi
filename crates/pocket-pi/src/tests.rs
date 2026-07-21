use super::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Pump until the runtime goes idle or a frame budget is hit. Returns the
/// collected host events. `hz` is the pump cadence — we deliberately run slow to
/// prove the coalesced scheduler carries a real turn to completion.
fn run(rt: &mut PiRuntime, sink: Rc<RefCell<Vec<HostEvent>>>, hz: f64, max_secs: f64) {
    let period = Duration::from_secs_f64(1.0 / hz);
    let start = Instant::now();
    while !rt.is_idle() && start.elapsed().as_secs_f64() < max_secs {
        rt.pump().unwrap();
        std::thread::sleep(period);
    }
    // One last pump to flush any trailing events.
    rt.pump().unwrap();
    let _ = &sink;
}

fn collector() -> (Rc<RefCell<Vec<HostEvent>>>, impl FnMut(&HostEvent) + 'static) {
    let sink = Rc::new(RefCell::new(Vec::new()));
    let s = sink.clone();
    (sink, move |ev: &HostEvent| s.borrow_mut().push(ev.clone()))
}

#[test]
fn boots_the_pi_agent_core() {
    let (sink, cb) = collector();
    let mut rt = PiRuntime::new().expect("runtime");
    rt.on_event(cb);
    rt.boot(r#"{"model":"test","scripted":{"steps":[{"text":"hello"}]}}"#)
        .expect("boot");
    let kinds: Vec<String> = sink.borrow().iter().map(|e| e.kind.clone()).collect();
    assert!(kinds.contains(&"booted".to_string()), "got {kinds:?}");
}

#[test]
fn scripted_text_turn_completes() {
    let (sink, cb) = collector();
    let mut rt = PiRuntime::new().expect("runtime");
    rt.on_event(cb);
    rt.boot(r#"{"model":"test","scripted":{"steps":[{"text":"three word answer"}]}}"#)
        .expect("boot");
    rt.prompt("say something").expect("prompt");
    run(&mut rt, sink.clone(), 2.0, 5.0);

    let events = sink.borrow();
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    assert!(kinds.contains(&"start"), "no start in {kinds:?}");
    assert!(kinds.contains(&"assistant_text"), "no assistant_text in {kinds:?}");
    assert!(kinds.contains(&"end"), "no end in {kinds:?}");

    let text = events
        .iter()
        .find(|e| e.kind == "assistant_text")
        .and_then(|e| e.value.get("text").and_then(|v| v.as_str()))
        .unwrap_or("");
    assert_eq!(text, "three word answer");
    assert!(rt.is_idle());
}

#[test]
fn scripted_tool_call_round_trips_through_native_rust() {
    let (sink, cb) = collector();
    let mut rt = PiRuntime::new().expect("runtime");
    rt.on_event(cb);

    let tool_calls = Rc::new(RefCell::new(0u32));
    let tc = tool_calls.clone();
    rt.register_tool("get_secret", move |args| {
        *tc.borrow_mut() += 1;
        let who = args.get("who").and_then(|v| v.as_str()).unwrap_or("world");
        ToolResult::text(format!("secret for {who} is 42"))
    });

    // Turn 1: the model calls the tool. Turn 2 (loop re-drives): it answers.
    rt.boot(
        r#"{"model":"test","tools":[{"name":"get_secret","description":"get the secret",
        "parameters":{"type":"object","properties":{"who":{"type":"string"}}}}],
        "scripted":{"steps":[{"toolCall":{"name":"get_secret","arguments":{"who":"cat"}}},
        {"text":"the secret is 42"}]}}"#,
    )
    .expect("boot");
    rt.prompt("what is the secret for cat?").expect("prompt");
    run(&mut rt, sink.clone(), 2.0, 5.0);

    assert_eq!(*tool_calls.borrow(), 1, "tool should run exactly once");
    let events = sink.borrow();
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    assert!(kinds.contains(&"tool_start"), "no tool_start in {kinds:?}");
    assert!(kinds.contains(&"tool_end"), "no tool_end in {kinds:?}");
    assert!(kinds.contains(&"assistant_text"), "no assistant_text in {kinds:?}");
    assert!(kinds.contains(&"end"), "no end in {kinds:?}");
}

/// Live end-to-end against Anthropic. Skipped unless ANTHROPIC_API_KEY is set,
/// so `cargo test` is hermetic by default.
#[test]
fn live_anthropic_turn() {
    let Ok(key) = std::env::var("ANTHROPIC_API_KEY") else {
        eprintln!("skipping live_anthropic_turn: ANTHROPIC_API_KEY not set");
        return;
    };
    let (sink, cb) = collector();
    let mut rt = PiRuntime::new().expect("runtime");
    rt.on_event(cb);
    let cfg = serde_json::json!({
        "model": "claude-opus-4-8",
        "apiKey": key,
        "maxTokens": 64,
        "systemPrompt": "Reply with exactly the word: pong"
    });
    rt.boot(&cfg.to_string()).expect("boot");
    rt.prompt("ping").expect("prompt");
    run(&mut rt, sink.clone(), 4.0, 60.0);

    let events = sink.borrow();
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    assert!(kinds.contains(&"end"), "turn did not complete: {kinds:?}");
    let text = events
        .iter()
        .find(|e| e.kind == "assistant_text")
        .and_then(|e| e.value.get("text").and_then(|v| v.as_str()))
        .unwrap_or("");
    eprintln!("live model said: {text:?}");
    assert!(!text.is_empty(), "no assistant text");
}
