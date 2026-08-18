use std::collections::{BTreeMap, BTreeSet};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use anyhow::{bail, Context, Result};
use serde_json::Value;

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
        (Some("build"), Some("pi-agent")) => build_system_assets(&root),
        (Some("build"), Some("view-sdk")) => {
            let pocketjs = pocketjs_checkout(&root)?;
            build_view_sdk(&root, &pocketjs)
        }
        (Some("package"), Some("app")) => {
            let app = rest.first().context("package app requires an App id")?;
            package_source_app(&root, app, rest.get(1).map(PathBuf::from).as_deref())
        }
        (Some("build"), Some("esp32-p4")) => {
            build_system_assets(&root)?;
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
            build_system_assets(&root)?;
            cargo(&root, ["build", "-p", "pocket-pi-esp32-p4-sim"])
        }
        (Some("run"), Some("esp32-p4-sim")) => {
            build_system_assets(&root)?;
            cargo_with_args(&root, ["run", "-p", "pocket-pi-esp32-p4-sim", "--"], &rest)
        }
        (Some("snapshot"), Some("esp32-p4-sim")) => {
            build_system_assets(&root)?;
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
                "usage:\n  cargo xtask build pi-agent|view-sdk|esp32-p4|esp32-p4-sim\n  cargo xtask package app <id> [credentials.json]\n  cargo xtask run esp32-p4-sim [args]\n  cargo xtask snapshot esp32-p4-sim"
            );
            bail!("unknown xtask command")
        }
    }
}

fn build_system_assets(root: &Path) -> Result<()> {
    build_embedded_guest(root)?;
    let pocketjs = pocketjs_checkout(root)?;
    build_view_sdk(root, &pocketjs)
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

fn build_view_sdk(root: &Path, pocketjs: &Path) -> Result<()> {
    let output = root.join("target/view-sdk");
    std::fs::create_dir_all(&output)?;
    command(
        Command::new("bun")
            .current_dir(pocketjs)
            .arg("tools/build.ts")
            .arg(root.join("system/view-sdk-pack.ts"))
            .arg(format!("--outdir={}", output.display()))
            .arg("--framework=solid")
            .arg("--no-config"),
        "building Pocket Pi View SDK resources",
    )?;
    let source = output.join("view-sdk-pack.pak");
    let destination = root.join("system/view-sdk.pak");
    std::fs::copy(&source, &destination).with_context(|| {
        format!(
            "install Pocket Pi View SDK resources {} -> {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

fn package_source_app(root: &Path, app: &str, credentials: Option<&Path>) -> Result<()> {
    let app_root = app_root(root, app)?;
    let descriptor: Value = serde_json::from_slice(&std::fs::read(app_root.join("app.json"))?)?;
    anyhow::ensure!(
        descriptor["format"] == 1
            && descriptor["frameworkApi"] == SYSTEM_FRAMEWORK_API
            && descriptor["id"] == app,
        "App does not target this source runtime"
    );
    if let Some(path) = credentials {
        let credentials_map =
            serde_json::from_slice::<BTreeMap<String, String>>(&std::fs::read(path)?)
                .context("parse credentials.json")?;
        anyhow::ensure!(
            credentials_map.keys().cloned().collect::<BTreeSet<_>>()
                == descriptor_credential_ids(&descriptor),
            "credentials.json ids do not match app.json"
        );
    }
    let mut assets = BTreeSet::new();
    for resource in descriptor["resources"]
        .as_object()
        .into_iter()
        .flat_map(|resources| resources.values())
    {
        let path = resource["path"]
            .as_str()
            .context("App resource is missing path")?;
        anyhow::ensure!(
            resource["type"] == "json" && valid_asset_path(path) && assets.insert(path),
            "invalid App resource {path}"
        );
        anyhow::ensure!(app_root.join(path).is_file(), "missing App resource {path}");
    }

    let schema_version = descriptor["schemaVersion"]
        .as_u64()
        .context("App is missing schemaVersion")?;
    let mut migrations = BTreeMap::new();
    let migrations_root = app_root.join("migrations");
    if migrations_root.exists() {
        for entry in std::fs::read_dir(&migrations_root)? {
            let entry = entry?;
            anyhow::ensure!(entry.file_type()?.is_file(), "App migration is not a file");
            let name = entry
                .file_name()
                .to_str()
                .context("App migration name is not UTF-8")?
                .to_owned();
            let version = name
                .strip_suffix(".sql")
                .context("invalid App migration filename")?
                .parse::<u64>()
                .context("invalid App migration filename")?;
            anyhow::ensure!(
                version >= 2 && version <= schema_version && name == format!("{version}.sql"),
                "invalid App migration: {name}"
            );
            anyhow::ensure!(
                migrations.insert(version, entry.path()).is_none(),
                "duplicate App migration for schema {version}"
            );
        }
    }

    let output_dir = root.join("target/pocketapps");
    std::fs::create_dir_all(&output_dir)?;
    let output = output_dir.join(format!("{app}.pocketapp"));
    let file = std::fs::File::create(&output)?;
    let mut archive = tar::Builder::new(file);
    for name in ["app.json", "schema.sql", "actions.js", "view.js"] {
        archive.append_path_with_name(app_root.join(name), name)?;
    }
    for path in assets {
        archive.append_path_with_name(app_root.join(path), path)?;
    }
    for (version, path) in migrations {
        archive.append_path_with_name(path, format!("migrations/{version}.sql"))?;
    }
    if let Some(credentials) = credentials {
        archive.append_path_with_name(credentials, "credentials.json")?;
    }
    archive.finish()?;
    #[cfg(unix)]
    std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o600))?;
    println!("packaged {}", output.display());
    Ok(())
}

fn valid_asset_path(path: &str) -> bool {
    path.len() <= 100
        && path.strip_prefix("assets/").is_some_and(|path| {
            !path.is_empty()
                && !path.contains('\\')
                && path.split('/').all(|component| {
                    !component.is_empty()
                        && component != "."
                        && component != ".."
                        && component.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
                        })
                })
        })
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
