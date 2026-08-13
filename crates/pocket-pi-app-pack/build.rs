use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const OPTIONAL_APPS: &[&str] = &["robinhood", "exa"];

fn main() {
    println!("cargo:rerun-if-env-changed=POCKET_PI_APPS");
    let selected = selected_apps();
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("../..")
        .canonicalize()
        .unwrap();
    let mut source = String::from("pub fn embedded_apps() -> Vec<EmbeddedApp> {\n    vec![\n");
    source.push_str(&bundle(&root, "pi-agent", true));
    for app in selected {
        source.push_str(&bundle(&root, app, false));
    }
    source.push_str("    ]\n}\n");
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("apps.rs"),
        source,
    )
    .unwrap();
}

fn selected_apps() -> Vec<&'static str> {
    let value = env::var("POCKET_PI_APPS").unwrap_or_else(|_| OPTIONAL_APPS.join(","));
    if value == "none" || value.is_empty() {
        return Vec::new();
    }
    let mut selected = Vec::new();
    for name in value.split(',') {
        let name = name.trim();
        let app = OPTIONAL_APPS
            .iter()
            .copied()
            .find(|candidate| *candidate == name)
            .unwrap_or_else(|| panic!("unknown App {name}; expected robinhood, exa, or none"));
        if selected.contains(&app) {
            panic!("duplicate App {app}");
        }
        selected.push(app);
    }
    selected
}

fn bundle(root: &Path, app: &str, system: bool) -> String {
    let app = root.join("apps").join(app);
    for path in [
        app.join("agent-app.json"),
        app.join("pocket.json"),
        app.join("dist/app.js"),
        app.join("dist/app.pak"),
    ] {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let data = app.join("dist/data-action.js");
    let agent = app.join("dist/agent.js");
    println!("cargo:rerun-if-changed={}", data.display());
    if system {
        println!("cargo:rerun-if-changed={}", agent.display());
    }
    format!(
        "        EmbeddedApp::new(include_str!({descriptor:?}), include_str!({pocket:?}), include_str!({js:?}), {data}, {agent}, include_bytes!({pak:?})),\n",
        descriptor = app.join("agent-app.json"),
        pocket = app.join("pocket.json"),
        js = app.join("dist/app.js"),
        data = option_include_str(&data),
        agent = if system { option_include_str(&agent) } else { "None".into() },
        pak = app.join("dist/app.pak"),
    )
}

fn option_include_str(path: &Path) -> String {
    if path.is_file() {
        format!("Some(include_str!({path:?}))")
    } else {
        "None".into()
    }
}
