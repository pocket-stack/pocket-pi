//! A minimal Node/CommonJS-flavored **module system + builtins** for QuickJS —
//! milestone 1 of running unmodified `pi-coding-agent` on Pocket Pi.
//!
//! rquickjs lets us plug a [`Resolver`] (specifier → canonical name) and a
//! [`Loader`] (name → compiled [`Module`]). We implement Node's resolution
//! (relative paths, `node_modules` walk, `node:` builtins), transpile `.ts` on
//! the fly with oxc, and serve builtins (`path`, `os`, `fs`, …) implemented in
//! JS over a small set of native ops. This is the no-Node analogue of the
//! runtime bun/deno provide — enough that real npm packages `import` and run.
//!
//! Scope of this milestone: **ESM + TypeScript + builtins + bare-package
//! resolution.** CommonJS interop, more builtins (crypto/child_process/stream),
//! and the Web globals needed for the full pi provider stack land next.

use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::module::{Declared, Module};
use rquickjs::{Ctx, Error, Function, Object, Result};
use std::path::{Path, PathBuf};

use crate::transpile::transpile_ts;

/// The builtin modules we ship, as embedded JS source.
const BUILTINS: &[(&str, &str)] = &[
    ("path", include_str!("../js/node/path.js")),
    ("os", include_str!("../js/node/os.js")),
    ("events", include_str!("../js/node/events.js")),
    ("util", include_str!("../js/node/util.js")),
    ("buffer", include_str!("../js/node/buffer.js")),
    ("process", include_str!("../js/node/process.js")),
    ("fs", include_str!("../js/node/fs.js")),
];

fn builtin_source(name: &str) -> Option<&'static str> {
    let bare = name.strip_prefix("node:").unwrap_or(name);
    // `fs/promises` etc. map onto their parent for now.
    let root = bare.split('/').next().unwrap_or(bare);
    BUILTINS.iter().find(|(n, _)| *n == root).map(|(_, s)| *s)
}

fn is_builtin(name: &str) -> bool {
    let bare = name.strip_prefix("node:").unwrap_or(name);
    let root = bare.split('/').next().unwrap_or(bare);
    BUILTINS.iter().any(|(n, _)| *n == root)
}

// --- Resolver: Node resolution algorithm (the subset we need) ---

pub struct NodeResolver;

impl Resolver for NodeResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> Result<String> {
        if is_builtin(name) {
            let bare = name.strip_prefix("node:").unwrap_or(name);
            return Ok(format!("node:{bare}"));
        }
        if name.starts_with("./") || name.starts_with("../") || name.starts_with('/') {
            let base_dir = Path::new(base).parent().unwrap_or_else(|| Path::new("/"));
            let joined = normalize(&base_dir.join(name));
            return probe(&joined)
                .ok_or_else(|| Error::new_resolving(base.to_string(), name.to_string()));
        }
        // Bare specifier: walk up node_modules from the importer.
        resolve_bare(base, name).ok_or_else(|| Error::new_resolving(base.to_string(), name.to_string()))
    }
}

/// Probe a path for the file that actually exists (extensions, index).
fn probe(p: &Path) -> Option<String> {
    let s = p.to_string_lossy().to_string();
    let candidates = [
        s.clone(),
        format!("{s}.ts"),
        format!("{s}.tsx"),
        format!("{s}.mts"),
        format!("{s}.js"),
        format!("{s}.mjs"),
        format!("{s}.cjs"),
        format!("{s}.json"),
        format!("{s}/index.ts"),
        format!("{s}/index.js"),
        format!("{s}/index.mjs"),
    ];
    candidates.into_iter().find(|c| Path::new(c).is_file())
}

fn resolve_bare(base: &str, name: &str) -> Option<String> {
    // Split scope/pkg + subpath: "@scope/pkg/sub" or "pkg/sub".
    let (pkg, subpath) = split_package(name);
    let mut dir = Path::new(base).parent().map(|p| p.to_path_buf());
    while let Some(d) = dir {
        let pkg_dir = d.join("node_modules").join(&pkg);
        if pkg_dir.is_dir() {
            if let Some(sub) = &subpath {
                return probe(&pkg_dir.join(sub));
            }
            return resolve_package_entry(&pkg_dir);
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }
    None
}

fn split_package(name: &str) -> (String, Option<String>) {
    let parts: Vec<&str> = name.splitn(if name.starts_with('@') { 3 } else { 2 }, '/').collect();
    if name.starts_with('@') && parts.len() == 3 {
        (format!("{}/{}", parts[0], parts[1]), Some(parts[2].to_string()))
    } else if name.starts_with('@') {
        (name.to_string(), None)
    } else if parts.len() == 2 {
        (parts[0].to_string(), Some(parts[1].to_string()))
    } else {
        (name.to_string(), None)
    }
}

fn resolve_package_entry(pkg_dir: &Path) -> Option<String> {
    let manifest = pkg_dir.join("package.json");
    let text = std::fs::read_to_string(&manifest).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    // Prefer exports["."] (import/default), then module, then main.
    let entry = exports_main(&json)
        .or_else(|| json.get("module").and_then(|v| v.as_str()).map(String::from))
        .or_else(|| json.get("main").and_then(|v| v.as_str()).map(String::from))
        .unwrap_or_else(|| "index.js".to_string());
    probe(&normalize(&pkg_dir.join(entry)))
}

fn exports_main(json: &serde_json::Value) -> Option<String> {
    let exports = json.get("exports")?;
    let dot = match exports {
        serde_json::Value::String(s) => return Some(s.clone()),
        serde_json::Value::Object(o) => o.get(".").or(Some(exports)).unwrap_or(exports),
        _ => return None,
    };
    // Conditional: import > default > require.
    for key in ["import", "default", "node", "require"] {
        if let Some(v) = dot.get(key) {
            if let Some(s) = v.as_str() {
                return Some(s.to_string());
            }
            if let Some(s) = v.get("default").and_then(|x| x.as_str()) {
                return Some(s.to_string());
            }
        }
    }
    dot.as_str().map(String::from)
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        use std::path::Component::*;
        match comp {
            ParentDir => {
                out.pop();
            }
            CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

// --- Loader: read, transpile, declare ---

pub struct NodeLoader;

impl Loader for NodeLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attrs: Option<ImportAttributes<'js>>,
    ) -> Result<Module<'js, Declared>> {
        if let Some(src) = builtin_source(name) {
            return Module::declare(ctx.clone(), name, src);
        }
        let source = std::fs::read_to_string(name)
            .map_err(|e| Error::new_loading_message(name.to_string(), e.to_string()))?;
        let is_ts = name.ends_with(".ts") || name.ends_with(".tsx") || name.ends_with(".mts") || name.ends_with(".cts");
        let js = if is_ts {
            transpile_ts(name, &source).map_err(|e| Error::new_loading_message(name.to_string(), e))?
        } else if name.ends_with(".json") {
            format!("export default {source};")
        } else {
            source
        };
        Module::declare(ctx.clone(), name, js)
    }
}

// --- Native ops + process global (installed onto the realm) ---

pub fn install_node(ctx: &Ctx) -> rquickjs::Result<()> {
    let node = Object::new(ctx.clone())?;
    node.set("cwd", Function::new(ctx.clone(), || -> String {
        std::env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| "/".into())
    })?)?;
    node.set("homedir", Function::new(ctx.clone(), || -> String {
        std::env::var("HOME").unwrap_or_else(|_| "/".into())
    })?)?;
    node.set("tmpdir", Function::new(ctx.clone(), || -> String {
        std::env::temp_dir().to_string_lossy().to_string()
    })?)?;
    node.set("hostname", Function::new(ctx.clone(), || -> String { "localhost".into() })?)?;

    // node.fs.* — each returns a JSON string the JS shim parses (avoids tying
    // rquickjs Object lifetimes through closures).
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
    node.set("fs", fs)?;
    ctx.globals().set("__node", node)?;

    // process global.
    let process = Object::new(ctx.clone())?;
    let env = Object::new(ctx.clone())?;
    for (k, v) in std::env::vars() {
        env.set(k, v)?;
    }
    process.set("env", env)?;
    process.set("platform", std::env::consts::OS.replace("macos", "darwin"))?;
    process.set("arch", if std::env::consts::ARCH == "aarch64" { "arm64" } else { "x64" })?;
    process.set("cwd", Function::new(ctx.clone(), || -> String {
        std::env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| "/".into())
    })?)?;
    process.set("version", "v22.0.0")?;
    let versions = Object::new(ctx.clone())?;
    versions.set("node", "22.0.0")?;
    process.set("versions", versions)?;
    process.set("argv", vec!["node".to_string(), "pocket-pi".to_string()])?;
    process.set("exit", Function::new(ctx.clone(), |_code: Option<i32>| {})?)?;
    ctx.globals().set("process", process)?;

    // Bootstrap (JS): hoist Buffer to a global (many packages assume it) and
    // wire process.nextTick to the microtask queue.
    let boot = r#"
        import { Buffer } from "node:buffer";
        globalThis.Buffer = Buffer;
        globalThis.__nodeBuffer = Buffer;
        globalThis.process.nextTick = (fn, ...args) => queueMicrotask(() => fn(...args));
    "#;
    Module::evaluate(ctx.clone(), "pocket-pi:node-bootstrap", boot)?.finish::<()>()?;
    Ok(())
}
