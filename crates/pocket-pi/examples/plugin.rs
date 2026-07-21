//! Load an agent-authored TypeScript plugin at runtime, then let the agent use
//! the tool it registered — no Node, no build step, all offline.
//!
//!   cargo run -p pocket-pi --example plugin

use pocket_pi::PiRuntime;

fn main() {
    let mut rt = PiRuntime::new().expect("runtime");
    rt.on_event(|ev| match ev.kind.as_str() {
        "plugin_loaded" => println!("[loaded plugin] {}", ev.value.get("name").and_then(|v| v.as_str()).unwrap_or("")),
        "plugin_log" => println!("[plugin] {}", ev.value.get("message").and_then(|v| v.as_str()).unwrap_or("")),
        "tool_start" => println!("[tool] {}", ev.value.get("name").and_then(|v| v.as_str()).unwrap_or("")),
        "assistant_text" => println!("pi > {}", ev.value.get("text").and_then(|v| v.as_str()).unwrap_or("")),
        _ => {}
    });

    // A scripted agent that will call the plugin's `slugify` tool, so the demo
    // runs with no API key.
    rt.boot(
        r#"{"model":"offline","scripted":{"steps":[
            {"toolCall":{"name":"slugify","arguments":{"text":"Hello, Pocket Pi!"}}},
            {"text":"the slug is hello-pocket-pi"}
        ]}}"#,
    )
    .expect("boot");

    // The agent's own words could have written this. It's real TypeScript —
    // interface, typed params, a cast — transpiled natively at load.
    rt.load_plugin_ts(
        "slugger",
        r#"
interface Args { text: string }
export default function (api: any): void {
    api.addTool({
        name: "slugify",
        description: "Turn text into a url slug",
        parameters: { type: "object", properties: { text: { type: "string" } }, required: ["text"] },
        execute: (args: Args): string => {
            const slug: string = (args.text as string)
                .toLowerCase()
                .replace(/[^a-z0-9]+/g, "-")
                .replace(/^-+|-+$/g, "");
            api.log("slugified: " + slug);
            return slug;
        },
    });
}
"#,
    )
    .expect("load plugin");

    rt.prompt("slugify 'Hello, Pocket Pi!'").expect("prompt");
    let start = std::time::Instant::now();
    while !rt.is_idle() && start.elapsed().as_secs() < 5 {
        rt.pump().ok();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    rt.pump().ok();
}
