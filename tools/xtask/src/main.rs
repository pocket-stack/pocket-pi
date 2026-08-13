use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use anyhow::{bail, Context, Result};

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let mut args = std::env::args().skip(1);
    let command_name = args.next();
    let target = args.next();
    let mut rest = args.collect::<Vec<_>>();
    let apps = take_apps(&mut rest)?;
    match (command_name.as_deref(), target.as_deref()) {
        (Some("build"), Some("agentos-apps")) => {
            build_embedded_guest(&root)?;
            build_agentos_apps(&root, &apps)
        }
        (Some("build"), Some("esp32-p4")) => {
            build_embedded_guest(&root)?;
            build_agentos_apps(&root, &apps)?;
            command(
                Command::new("rustup")
                    .current_dir(root.join("firmware/esp32-p4"))
                    .env("POCKET_PI_APPS", &apps)
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
            build_agentos_apps(&root, &apps)?;
            cargo(&root, &apps, ["build", "-p", "pocket-pi-esp32-p4-sim"])
        }
        (Some("run"), Some("esp32-p4-sim")) => {
            build_embedded_guest(&root)?;
            build_agentos_apps(&root, &apps)?;
            cargo_with_args(
                &root,
                &apps,
                ["run", "-p", "pocket-pi-esp32-p4-sim", "--"],
                &rest,
            )
        }
        (Some("snapshot"), Some("esp32-p4-sim")) => {
            build_embedded_guest(&root)?;
            build_agentos_apps(&root, &apps)?;
            let output = root.join("artifacts/screenshots/esp32-p4-sim.png");
            std::fs::create_dir_all(output.parent().unwrap())?;
            cargo_with_args(
                &root,
                &apps,
                ["run", "-p", "pocket-pi-esp32-p4-sim", "--"],
                &["--screenshot".into(), output.display().to_string()],
            )
        }
        _ => {
            eprintln!(
                "usage:\n  cargo xtask build agentos-apps|esp32-p4|esp32-p4-sim [--apps robinhood,exa|exa|robinhood|none]\n  cargo xtask run esp32-p4-sim [--apps ...] [args]\n  cargo xtask snapshot esp32-p4-sim [--apps ...]"
            );
            bail!("unknown xtask command")
        }
    }
}

fn take_apps(args: &mut Vec<String>) -> Result<String> {
    let Some(index) = args.iter().position(|arg| arg == "--apps") else {
        return Ok("robinhood,exa".into());
    };
    args.remove(index);
    let value = args
        .get(index)
        .cloned()
        .context("--apps requires a value")?;
    args.remove(index);
    selected_apps(&value)?;
    Ok(value)
}

fn selected_apps(value: &str) -> Result<Vec<&str>> {
    if value == "none" {
        return Ok(Vec::new());
    }
    let mut selected = Vec::new();
    for app in value.split(',') {
        if !matches!(app, "robinhood" | "exa") {
            bail!("unknown App {app}; expected robinhood, exa, or none");
        }
        if selected.contains(&app) {
            bail!("duplicate App {app}");
        }
        selected.push(app);
    }
    Ok(selected)
}

fn build_agentos_apps(root: &Path, apps: &str) -> Result<()> {
    const POCKETJS_REV: &str = "9c809bbd047ddc75c27caa4990951a78d942477a";
    let pocketjs = std::env::var_os("POCKETJS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.parent().unwrap_or(root).join("pocketjs"));
    let revision = Command::new("git")
        .current_dir(&pocketjs)
        .args(["rev-parse", "HEAD"])
        .output()
        .with_context(|| format!("inspect PocketJS checkout at {}", pocketjs.display()))?;
    let actual = String::from_utf8_lossy(&revision.stdout).trim().to_owned();
    if !revision.status.success() || actual != POCKETJS_REV {
        bail!(
            "POCKETJS_ROOT={} must be checked out at the pinned upstream PocketJS revision {POCKETJS_REV}; found {actual}",
            pocketjs.display()
        );
    }
    install_if_missing(&pocketjs)?;
    for app in std::iter::once("pi-agent").chain(selected_apps(apps)?) {
        let app_root = root.join("apps").join(app);
        command(
            Command::new("bun")
                .current_dir(&pocketjs)
                .arg("tools/build.ts")
                .arg(app_root.join("app.tsx"))
                .arg(format!("--outdir={}", app_root.join("dist").display()))
                .arg("--framework=solid"),
            &format!("building AgentOS App {app}"),
        )?;
        minify_agentos_bundle(&app_root)?;
        let data_entry = app_root.join("data-action.ts");
        if data_entry.is_file() {
            command(
                Command::new("bun")
                    .arg(root.join("tools/build-agentos-data.ts"))
                    .arg(&data_entry)
                    .arg(app_root.join("dist/data-action.js"))
                    .arg(&pocketjs),
                &format!("building AgentOS data action {app}"),
            )?;
        }
    }
    Ok(())
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

fn cargo<const N: usize>(root: &Path, apps: &str, args: [&str; N]) -> Result<()> {
    command(
        Command::new("cargo")
            .current_dir(root)
            .env("POCKET_PI_APPS", apps)
            .args(args),
        "running cargo",
    )
}

fn cargo_with_args<const N: usize>(
    root: &Path,
    apps: &str,
    base: [&str; N],
    rest: &[String],
) -> Result<()> {
    command(
        Command::new("cargo")
            .current_dir(root)
            .env("POCKET_PI_APPS", apps)
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
