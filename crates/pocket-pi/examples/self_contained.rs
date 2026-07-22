//! Pocket Pi is self-contained: the whole unmodified pi-coding-agent is embedded
//! in the binary, so `PiRuntime::new()` stands it up with no external files and
//! no Node. Run: `cargo run --release --example self_contained`.
use pocket_pi::PiRuntime;

fn main() {
    let rt = PiRuntime::new().expect("runtime");
    println!("full pi loaded: {:?}", rt.get_global_json("__piFullLoaded"));
}
