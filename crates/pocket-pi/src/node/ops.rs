//! Native ops backing the Node builtins: the `__node` namespace (resolution, file
//! I/O, subprocess) and the `process` global. The JS builtins in `js/node/*.js`
//! are thin shims over these — anything that must touch the OS lives here.
//!
//! Each fs op returns a JSON string the JS shim parses, rather than an rquickjs
//! `Object`, so no `Object` lifetime is threaded through a closure (that fails
//! borrowck in rquickjs).

use rquickjs::{Ctx, Function, Object};
use std::path::Path;

use super::resolve::resolve_spec;

/// Mount `globalThis.__node` and `globalThis.process`.
pub fn install_ops(ctx: &Ctx) -> rquickjs::Result<()> {
    install_node_ns(ctx)?;
    install_process(ctx)?;
    Ok(())
}

fn install_node_ns(ctx: &Ctx) -> rquickjs::Result<()> {
    let node = Object::new(ctx.clone())?;

    node.set("cwd", Function::new(ctx.clone(), cwd)?)?;
    node.set("homedir", Function::new(ctx.clone(), || -> String {
        std::env::var("HOME").unwrap_or_else(|_| "/".into())
    })?)?;
    node.set("tmpdir", Function::new(ctx.clone(), || -> String {
        std::env::temp_dir().to_string_lossy().to_string()
    })?)?;
    node.set("hostname", Function::new(ctx.clone(), || -> String { "localhost".into() })?)?;

    // Resolution + file read for the synchronous CJS `require`.
    node.set("resolve", Function::new(ctx.clone(), |from: String, spec: String| -> String {
        match resolve_spec(&from, &spec) {
            Some(r) if r.starts_with("node:") => {
                serde_json::json!({ "builtin": r.strip_prefix("node:").unwrap() }).to_string()
            }
            Some(path) => serde_json::json!({ "path": path }).to_string(),
            None => serde_json::json!({ "err": "not found" }).to_string(),
        }
    })?)?;
    node.set("readText", Function::new(ctx.clone(), |path: String| -> String {
        std::fs::read_to_string(&path).unwrap_or_default()
    })?)?;

    node.set("fs", make_fs(ctx)?)?;
    node.set("spawnSync", Function::new(ctx.clone(), spawn_sync)?)?;

    ctx.globals().set("__node", node)?;
    Ok(())
}

fn make_fs<'js>(ctx: &Ctx<'js>) -> rquickjs::Result<Object<'js>> {
    let fs = Object::new(ctx.clone())?;
    fs.set("readFile", Function::new(ctx.clone(), |path: String| -> String {
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::json!({ "bytes": bytes }).to_string(),
            Err(e) => serde_json::json!({ "err": e.to_string() }).to_string(),
        }
    })?)?;
    fs.set("writeFile", Function::new(ctx.clone(), |path: String, bytes: Vec<u8>| -> String {
        match std::fs::write(&path, &bytes) {
            Ok(()) => "{}".into(),
            Err(e) => serde_json::json!({ "err": e.to_string() }).to_string(),
        }
    })?)?;
    fs.set("exists", Function::new(ctx.clone(), |path: String| -> bool { Path::new(&path).exists() })?)?;
    fs.set("readdir", Function::new(ctx.clone(), |path: String| -> String {
        match std::fs::read_dir(&path) {
            Ok(rd) => {
                let names: Vec<String> = rd
                    .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
                    .collect();
                serde_json::json!({ "entries": names }).to_string()
            }
            Err(e) => serde_json::json!({ "err": e.to_string() }).to_string(),
        }
    })?)?;
    fs.set("mkdir", Function::new(ctx.clone(), |path: String, recursive: bool| {
        let _ = if recursive { std::fs::create_dir_all(&path) } else { std::fs::create_dir(&path) };
    })?)?;
    fs.set("stat", Function::new(ctx.clone(), |path: String| -> String {
        match std::fs::metadata(&path) {
            Ok(m) => serde_json::json!({
                "isFile": m.is_file(), "isDir": m.is_dir(), "size": m.len() as f64,
            })
            .to_string(),
            Err(e) => serde_json::json!({ "err": e.to_string() }).to_string(),
        }
    })?)?;
    fs.set("realpath", Function::new(ctx.clone(), |path: String| -> String {
        match std::fs::canonicalize(&path) {
            Ok(p) => serde_json::json!({ "path": p.to_string_lossy() }).to_string(),
            Err(e) => serde_json::json!({ "err": e.to_string() }).to_string(),
        }
    })?)?;
    fs.set("unlink", Function::new(ctx.clone(), |path: String| { let _ = std::fs::remove_file(&path); })?)?;
    Ok(fs)
}

fn install_process(ctx: &Ctx) -> rquickjs::Result<()> {
    let process = Object::new(ctx.clone())?;

    let env = Object::new(ctx.clone())?;
    for (k, v) in std::env::vars() {
        env.set(k, v)?;
    }
    process.set("env", env)?;
    process.set("platform", std::env::consts::OS.replace("macos", "darwin"))?;
    process.set("arch", if std::env::consts::ARCH == "aarch64" { "arm64" } else { "x64" })?;
    process.set("cwd", Function::new(ctx.clone(), cwd)?)?;
    process.set("version", "v22.0.0")?;
    let versions = Object::new(ctx.clone())?;
    versions.set("node", "22.0.0")?;
    process.set("versions", versions)?;
    process.set("argv", vec!["node".to_string(), "pocket-pi".to_string()])?;
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "/pocket-pi".into());
    process.set("execPath", exe.clone())?;
    process.set("argv0", exe)?;
    process.set("execArgv", Vec::<String>::new())?;
    process.set("pid", std::process::id() as f64)?;
    process.set("ppid", 0)?;
    process.set("exit", Function::new(ctx.clone(), |_code: Option<i32>| {})?)?;

    // Minimal stdout/stderr so TUI + logging code can write() headlessly.
    for (name, is_err) in [("stdout", false), ("stderr", true)] {
        let stream = Object::new(ctx.clone())?;
        stream.set("isTTY", false)?;
        stream.set("columns", 80)?;
        stream.set("rows", 24)?;
        stream.set("write", Function::new(ctx.clone(), move |s: String| -> bool {
            if is_err { eprint!("{s}"); } else { print!("{s}"); }
            true
        })?)?;
        stream.set("on", Function::new(ctx.clone(), || {})?)?;
        stream.set("end", Function::new(ctx.clone(), || {})?)?;
        process.set(name, stream)?;
    }

    ctx.globals().set("process", process)?;
    Ok(())
}

fn cwd() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "/".into())
}

/// `spawnSync(cmd, argsJson, optsJson) -> JSON {stdout, stderr, status, error?}`.
fn spawn_sync(cmd: String, args_json: String, opts_json: String) -> String {
    let args: Vec<String> = serde_json::from_str(&args_json).unwrap_or_default();
    let opts: serde_json::Value = serde_json::from_str(&opts_json).unwrap_or(serde_json::Value::Null);
    let mut command = std::process::Command::new(&cmd);
    command.args(&args);
    if let Some(dir) = opts.get("cwd").and_then(|v| v.as_str()) {
        command.current_dir(dir);
    }
    match command.output() {
        Ok(o) => serde_json::json!({
            "stdout": String::from_utf8_lossy(&o.stdout),
            "stderr": String::from_utf8_lossy(&o.stderr),
            "status": o.status.code(),
        })
        .to_string(),
        Err(e) => serde_json::json!({ "error": e.to_string(), "status": serde_json::Value::Null }).to_string(),
    }
}
