use std::collections::{BTreeMap, BTreeSet};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

const POCKETJS_REVISION: &str = "e12cf12f82cc60b636368119d49a06eb9ed2a3d5";
const SYSTEM_FRAMEWORK_API: u32 = 1;

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let mut args = std::env::args().skip(1);
    let command_name = args.next();
    let target = args.next();
    let rest = args.collect::<Vec<_>>();
    match (command_name.as_deref(), target.as_deref()) {
        (Some("build"), Some("pi-agent")) => {
            build_embedded_guest(&root)?;
            build_pi_agent(&root)
        }
        (Some("build"), Some("app")) => {
            let app = rest.first().context("build app requires an App id")?;
            anyhow::ensure!(
                app != "pi-agent",
                "the System App is built with `cargo xtask build pi-agent`"
            );
            build_app(&root, app)?;
            package_app(&root, app, rest.get(1).map(PathBuf::from).as_deref())
        }
        (Some("build"), Some("esp32-p4")) => {
            build_embedded_guest(&root)?;
            build_pi_agent(&root)?;
            command(
                Command::new("rustup")
                    .current_dir(root.join("firmware/esp32-p4"))
                    .args([
                        "run",
                        "nightly-2026-05-01",
                        "./tools/cargo-esp32p4",
                        "build",
                        "--release",
                    ]),
                "building ESP32-P4 firmware",
            )
        }
        (Some("build"), Some("esp32-p4-sim")) => {
            build_embedded_guest(&root)?;
            build_pi_agent(&root)?;
            cargo(&root, ["build", "-p", "pocket-pi-esp32-p4-sim"])
        }
        (Some("run"), Some("esp32-p4-sim")) => {
            build_embedded_guest(&root)?;
            build_pi_agent(&root)?;
            cargo_with_args(&root, ["run", "-p", "pocket-pi-esp32-p4-sim", "--"], &rest)
        }
        (Some("snapshot"), Some("esp32-p4-sim")) => {
            build_embedded_guest(&root)?;
            build_pi_agent(&root)?;
            let output = root.join("artifacts/screenshots/esp32-p4-sim.png");
            std::fs::create_dir_all(output.parent().unwrap())?;
            cargo_with_args(
                &root,
                ["run", "-p", "pocket-pi-esp32-p4-sim", "--"],
                &["--screenshot".into(), output.display().to_string()],
            )
        }
        _ => {
            eprintln!(
                "usage:\n  cargo xtask build pi-agent|esp32-p4|esp32-p4-sim\n  cargo xtask build app <id> [credentials.json]\n  cargo xtask run esp32-p4-sim [args]\n  cargo xtask snapshot esp32-p4-sim"
            );
            bail!("unknown xtask command")
        }
    }
}

fn build_pi_agent(root: &Path) -> Result<()> {
    let pocketjs = pocketjs_checkout(root)?;
    build_app_with_pocketjs(root, &pocketjs, "pi-agent")
}

fn pocketjs_checkout(root: &Path) -> Result<PathBuf> {
    let pocketjs = std::env::var_os("POCKETJS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.parent().unwrap_or(root).join("pocketjs"));
    let revision = Command::new("git")
        .current_dir(&pocketjs)
        .args(["rev-parse", "HEAD"])
        .output()
        .with_context(|| format!("inspect PocketJS checkout at {}", pocketjs.display()))?;
    let actual = String::from_utf8_lossy(&revision.stdout).trim().to_owned();
    if !revision.status.success() || actual != POCKETJS_REVISION {
        bail!(
            "POCKETJS_ROOT={} must be checked out at the pinned upstream PocketJS revision {POCKETJS_REVISION}; found {actual}",
            pocketjs.display()
        );
    }
    install_if_missing(&pocketjs)?;
    Ok(pocketjs)
}

fn build_app(root: &Path, app: &str) -> Result<()> {
    let pocketjs = pocketjs_checkout(root)?;
    build_app_with_pocketjs(root, &pocketjs, app)
}

fn build_app_with_pocketjs(root: &Path, pocketjs: &Path, app: &str) -> Result<()> {
    let app_root = app_root(root, app)?;
    command(
        Command::new("bun")
            .current_dir(pocketjs)
            .arg("tools/build.ts")
            .arg(app_root.join("app.tsx"))
            .arg(format!("--outdir={}", app_root.join("dist").display()))
            .arg("--framework=solid"),
        &format!("building AgentOS App {app}"),
    )?;
    minify_agentos_bundle(&app_root)?;
    let actions_entry = app_root.join("actions.ts");
    if actions_entry.is_file() {
        command(
            Command::new("bun")
                .arg(root.join("tools/build-agentos-actions.ts"))
                .arg(&actions_entry)
                .arg(app_root.join("dist/actions.js"))
                .arg(pocketjs),
            &format!("building AgentOS Actions {app}"),
        )?;
    }
    Ok(())
}

fn package_app(root: &Path, app: &str, credentials: Option<&Path>) -> Result<()> {
    let app_root = app_root(root, app)?;
    let descriptor: Value = serde_json::from_slice(&std::fs::read(app_root.join("app.json"))?)?;
    let manifest: Value = serde_json::from_slice(&std::fs::read(app_root.join("pocket.json"))?)?;
    let credentials_map = credentials.map_or_else(
        || Ok(BTreeMap::new()),
        |path| {
            serde_json::from_slice::<BTreeMap<String, String>>(&std::fs::read(path)?)
                .context("parse credentials.json")
        },
    )?;
    anyhow::ensure!(
        descriptor["id"] == app && manifest["name"] == app,
        "App id mismatch"
    );
    anyhow::ensure!(
        descriptor["version"] == manifest["version"],
        "App version mismatch"
    );
    anyhow::ensure!(
        credentials_map.keys().cloned().collect::<BTreeSet<_>>()
            == descriptor_credential_ids(&descriptor),
        "credentials.json ids do not match app.json"
    );
    let output_dir = root.join("target/pocketapps");
    std::fs::create_dir_all(&output_dir)?;
    let output = output_dir.join(format!("{app}.pocketapp"));
    let file = std::fs::File::create(&output)?;
    let mut archive = tar::Builder::new(file);
    for (source, name) in [
        (app_root.join("app.json"), "app.json"),
        (app_root.join("pocket.json"), "pocket.json"),
        (app_root.join("dist/app.js"), "app.js"),
        (app_root.join("dist/app.pak"), "app.pak"),
    ] {
        archive.append_path_with_name(source, name)?;
    }
    let actions = app_root.join("dist/actions.js");
    if actions.is_file() {
        archive.append_path_with_name(actions, "actions.js")?;
    }
    if let Some(credentials) = credentials {
        archive.append_path_with_name(credentials, "credentials.json")?;
    }
    let plan = serde_json::to_vec_pretty(&json!({
        "runtime":"pocket-pi-agentos",
        "pocketjsRevision":POCKETJS_REVISION,
        "frameworkApi":SYSTEM_FRAMEWORK_API,
        "app":app,
        "modules":manifest.pointer("/engine/capabilities/requires").cloned().unwrap_or_else(|| json!([]))
    }))?;
    let mut header = tar::Header::new_gnu();
    header.set_size(plan.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    archive.append_data(&mut header, "plan.json", plan.as_slice())?;
    archive.finish()?;
    #[cfg(unix)]
    std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o600))?;
    println!("packaged {}", output.display());
    Ok(())
}

fn descriptor_credential_ids(descriptor: &Value) -> BTreeSet<String> {
    ["http", "mcp"]
        .into_iter()
        .flat_map(|kind| {
            descriptor
                .pointer(&format!("/nativeServices/{kind}"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|policy| policy.pointer("/credential/id").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
}

fn app_root(root: &Path, app: &str) -> Result<PathBuf> {
    anyhow::ensure!(
        !app.is_empty()
            && app != "."
            && app != ".."
            && app
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')),
        "invalid App id {app}"
    );
    let path = root.join("apps").join(app);
    anyhow::ensure!(
        path.is_dir(),
        "App source does not exist: {}",
        path.display()
    );
    Ok(path)
}

fn minify_agentos_bundle(app_root: &Path) -> Result<()> {
    let bundle = app_root.join("dist/app.js");
    let minified = app_root.join("dist/app.min.js");
    command(
        Command::new("bun")
            .arg("build")
            .arg(&bundle)
            .arg(format!("--outfile={}", minified.display()))
            .arg("--minify"),
        &format!("minifying {} for the device image", bundle.display()),
    )?;
    std::fs::rename(&minified, &bundle)
        .with_context(|| format!("replace {} with minified bundle", bundle.display()))?;
    Ok(())
}

fn build_embedded_guest(root: &Path) -> Result<()> {
    let embedded = root.join("crates/pocket-pi-embedded/js");
    install_if_missing(&embedded)?;
    command(
        Command::new("bun")
            .current_dir(&embedded)
            .args(["run", "build"]),
        "building embedded Pi guest",
    )?;
    let source = embedded.join("pi-agent.bundle.js");
    let destination = root.join("apps/pi-agent/dist/agent.js");
    std::fs::create_dir_all(destination.parent().unwrap())?;
    std::fs::copy(&source, &destination).with_context(|| {
        format!(
            "install Pi Agent loop bundle {} -> {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

fn install_if_missing(directory: &Path) -> Result<()> {
    if !directory.join("node_modules").is_dir() {
        command(
            Command::new("bun").current_dir(directory).arg("install"),
            "installing guest dependencies",
        )?;
    }
    Ok(())
}

fn cargo<const N: usize>(root: &Path, args: [&str; N]) -> Result<()> {
    command(
        Command::new("cargo").current_dir(root).args(args),
        "running cargo",
    )
}

fn cargo_with_args<const N: usize>(root: &Path, base: [&str; N], rest: &[String]) -> Result<()> {
    command(
        Command::new("cargo")
            .current_dir(root)
            .args(base)
            .args(rest),
        "running cargo",
    )
}

fn command(command: &mut Command, action: &str) -> Result<()> {
    let status: ExitStatus = command.status().with_context(|| action.to_owned())?;
    if !status.success() {
        bail!("{action} failed with {status}")
    }
    Ok(())
}
