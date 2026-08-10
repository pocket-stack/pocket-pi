use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use anyhow::{bail, Context, Result};

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next().as_deref()) {
        (Some("build"), Some("macos")) => cargo(&root, ["build", "-p", "pocket-pi-macos"]),
        (Some("build"), Some("agentos-apps")) => build_agentos_apps(&root),
        (Some("build"), Some("esp32-p4")) => {
            build_embedded_guest(&root)?;
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
            cargo(&root, ["build", "-p", "pocket-pi-esp32-p4-sim"])
        }
        (Some("run"), Some("macos")) => {
            let rest = args.collect::<Vec<_>>();
            cargo_with_args(&root, ["run", "-p", "pocket-pi-macos", "--"], &rest)
        }
        (Some("run"), Some("esp32-p4-sim")) => {
            build_embedded_guest(&root)?;
            let rest = args.collect::<Vec<_>>();
            cargo_with_args(&root, ["run", "-p", "pocket-pi-esp32-p4-sim", "--"], &rest)
        }
        (Some("snapshot"), Some("esp32-p4-sim")) => {
            build_embedded_guest(&root)?;
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
                "usage:\n  cargo xtask build macos|agentos-apps|esp32-p4|esp32-p4-sim\n  cargo xtask run macos|esp32-p4-sim [args]\n  cargo xtask snapshot esp32-p4-sim"
            );
            bail!("unknown xtask command")
        }
    }
}

fn build_agentos_apps(root: &Path) -> Result<()> {
    const POCKETJS_REV: &str = "afc8d4e8e877dac7f9b0c01b5c0d667642009fc0";
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
            "POCKETJS_ROOT={} must be checked out at feat/fs-surface revision {POCKETJS_REV}; found {actual}",
            pocketjs.display()
        );
    }
    install_if_missing(&pocketjs)?;
    for app in ["pi-agent", "robinhood", "exa"] {
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
                    .arg("build")
                    .arg(&data_entry)
                    .arg(format!(
                        "--outfile={}",
                        app_root.join("dist/data-action.js").display()
                    ))
                    .arg("--target=browser")
                    .arg("--format=iife")
                    .arg("--minify"),
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
