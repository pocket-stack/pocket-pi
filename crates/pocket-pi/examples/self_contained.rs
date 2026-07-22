//! A self-contained agent binary: the full, unmodified pi-coding-agent is
//! embedded (gzip) in the executable — no external .js. Build with:
//!   node js/build-pi-full.mjs
//!   cargo run --release --features embed-full-pi --example self_contained
use pocket_pi::PiRuntime;

fn main() {
    let mut rt = PiRuntime::new().expect("runtime");
    rt.load_full_pi().expect("load embedded full pi");
    println!("full pi loaded: {:?}", rt.get_global_json("__piFullLoaded"));
}
