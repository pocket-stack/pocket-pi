use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use anyhow::{bail, Context, Result};

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next().as_deref()) {
        (Some("build"), Some("macos")) => cargo(&root, ["build", "-p", "pocket-pi-macos"]),
        (Some("build"), Some("esp32-p4")) => {
            build_guests(&root)?;
            command(
                Command::new("rustup")
                    .current_dir(root.join("firmware/esp32-p4"))
                    .args([
                        "run",
                        "nightly-2026-05-01",
                        "./tools/cargo-esp32p4",
                        "build",
                    ]),
                "building ESP32-P4 firmware",
            )
        }
        (Some("build"), Some("esp32-p4-sim")) => {
            build_guests(&root)?;
            cargo(&root, ["build", "-p", "pocket-pi-esp32-p4-sim"])
        }
        (Some("run"), Some("macos")) => {
            let rest = args.collect::<Vec<_>>();
            cargo_with_args(&root, ["run", "-p", "pocket-pi-macos", "--"], &rest)
        }
        (Some("run"), Some("esp32-p4-sim")) => {
            build_guests(&root)?;
            let rest = args.collect::<Vec<_>>();
            cargo_with_args(&root, ["run", "-p", "pocket-pi-esp32-p4-sim", "--"], &rest)
        }
        (Some("snapshot"), Some("esp32-p4-sim")) => {
            build_guests(&root)?;
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
                "usage:\n  cargo xtask build macos|esp32-p4|esp32-p4-sim\n  cargo xtask run macos|esp32-p4-sim [args]\n  cargo xtask snapshot esp32-p4-sim"
            );
            bail!("unknown xtask command")
        }
    }
}

fn build_guests(root: &Path) -> Result<()> {
    let embedded = root.join("crates/pocket-pi-embedded/js");
    install_if_missing(&embedded)?;
    command(
        Command::new("bun")
            .current_dir(&embedded)
            .args(["run", "build"]),
        "building embedded Pi guest",
    )?;

    let app = root.join("apps/agent-shell");
    install_if_missing(&app)?;
    std::fs::create_dir_all(root.join("artifacts/ui"))?;
    let build = app.join("node_modules/@pocketjs/framework/tools/build.ts");
    command(
        Command::new("bun")
            .current_dir(root)
            .arg(build)
            .arg(root.join("apps/agent-shell/agent-shell.tsx"))
            .arg("--framework=solid")
            .arg("--density=2")
            .arg("--no-config")
            .arg(format!("--project-root={}", root.display()))
            .arg(format!("--outdir={}", root.join("artifacts/ui").display())),
        "building PocketJS agent shell",
    )
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
