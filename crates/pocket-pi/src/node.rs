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
    ("fs/promises", include_str!("../js/node/fs-promises.js")),
    ("child_process", include_str!("../js/node/child_process.js")),
    ("crypto", include_str!("../js/node/crypto.js")),
    ("url", include_str!("../js/node/url.js")),
    ("module", include_str!("../js/node/module.js")),
    ("stream", include_str!("../js/node/stream.js")),
    ("stream/promises", include_str!("../js/node/stream-promises.js")),
    ("string_decoder", include_str!("../js/node/string_decoder.js")),
    ("readline", include_str!("../js/node/readline.js")),
    ("perf_hooks", include_str!("../js/node/perf_hooks.js")),
    ("tty", include_str!("../js/node/tty.js")),
    ("http", include_str!("../js/node/http.js")),
    ("https", include_str!("../js/node/https.js")),
    ("net", include_str!("../js/node/net.js")),
    ("tls", include_str!("../js/node/tls.js")),
    ("zlib", include_str!("../js/node/zlib.js")),
    ("dns", include_str!("../js/node/dns.js")),
    ("querystring", include_str!("../js/node/querystring.js")),
    ("assert", include_str!("../js/node/assert.js")),
    ("timers", include_str!("../js/node/timers.js")),
    ("worker_threads", include_str!("../js/node/worker_threads.js")),
    ("v8", include_str!("../js/node/v8.js")),
    ("vm", include_str!("../js/node/vm.js")),
    ("constants", include_str!("../js/node/constants.js")),
    ("async_hooks", include_str!("../js/node/async_hooks.js")),
    ("diagnostics_channel", include_str!("../js/node/diagnostics_channel.js")),
];

fn builtin_source(name: &str) -> Option<&'static str> {
    let bare = name.strip_prefix("node:").unwrap_or(name);
    // Exact match first (so `fs/promises` gets its own module), then the root.
    if let Some((_, s)) = BUILTINS.iter().find(|(n, _)| *n == bare) {
        return Some(s);
    }
    let root = bare.split('/').next().unwrap_or(bare);
    BUILTINS.iter().find(|(n, _)| *n == root).map(|(_, s)| *s)
}

fn is_builtin(name: &str) -> bool {
    let bare = name.strip_prefix("node:").unwrap_or(name);
    if BUILTINS.iter().any(|(n, _)| *n == bare) {
        return true;
    }
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
        resolve_spec(base, name)
            .ok_or_else(|| Error::new_resolving(base.to_string(), name.to_string()))
    }
}

/// The core Node resolution, shared by the rquickjs `Resolver` and the native
/// `require` op. Returns the canonical module name (`node:X` or an abs path).
pub fn resolve_spec(base: &str, name: &str) -> Option<String> {
    if is_builtin(name) {
        let bare = name.strip_prefix("node:").unwrap_or(name);
        return Some(format!("node:{bare}"));
    }
    if name.starts_with('#') {
        return resolve_internal(base, name);
    }
    if name.starts_with("./") || name.starts_with("../") || name.starts_with('/') {
        let base_dir = Path::new(base).parent().unwrap_or_else(|| Path::new("/"));
        return probe(&normalize(&base_dir.join(name)));
    }
    resolve_bare(base, name)
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
            return resolve_in_package(&pkg_dir, subpath.as_deref());
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }
    None
}

/// Resolve a `#internal` import via the nearest package.json's `imports` map.
fn resolve_internal(base: &str, name: &str) -> Option<String> {
    let mut dir = Path::new(base).parent();
    while let Some(d) = dir {
        let pj = d.join("package.json");
        if pj.is_file() {
            let text = std::fs::read_to_string(&pj).ok()?;
            let json: serde_json::Value = serde_json::from_str(&text).ok()?;
            let imports = json.get("imports")?;
            // Exact match, then `*` wildcard.
            if let Some(v) = imports.get(name) {
                let target = resolve_conditions(v)?;
                return probe(&normalize(&d.join(target)));
            }
            if let Some(obj) = imports.as_object() {
                for (pat, target) in obj {
                    if let Some(pre) = pat.strip_suffix('*') {
                        if let Some(rest) = name.strip_prefix(pre) {
                            let tgt = resolve_conditions(target)?.replace('*', rest);
                            return probe(&normalize(&d.join(tgt)));
                        }
                    }
                }
            }
            return None; // nearest package.json is the resolution boundary
        }
        dir = d.parent();
    }
    None
}

/// Resolve a specifier within a package dir, honoring the `exports` map (exact,
/// conditional, and `*` wildcard subpaths), then falling back to module/main or
/// a literal probe.
fn resolve_in_package(pkg_dir: &Path, subpath: Option<&str>) -> Option<String> {
    let manifest = pkg_dir.join("package.json");
    if let Ok(text) = std::fs::read_to_string(&manifest) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
            let key = match subpath {
                None => ".".to_string(),
                Some(s) => format!("./{s}"),
            };
            if let Some(target) = resolve_export(&json, &key) {
                return probe(&normalize(&pkg_dir.join(target)));
            }
            // No exports match: use module/main for the root, else literal probe.
            if subpath.is_none() {
                let entry = json
                    .get("module")
                    .and_then(|v| v.as_str())
                    .or_else(|| json.get("main").and_then(|v| v.as_str()))
                    .unwrap_or("index.js");
                return probe(&normalize(&pkg_dir.join(entry)));
            }
        }
    }
    probe(&normalize(&pkg_dir.join(subpath.unwrap_or("index.js"))))
}

/// Look up `key` (`.` or `./sub`) in a package's `exports`, resolving conditions
/// and `*` wildcards. Returns the target path relative to the package dir.
fn resolve_export(json: &serde_json::Value, key: &str) -> Option<String> {
    let exports = json.get("exports")?;
    // `"exports": "./x.js"` — only valid for the root.
    if let Some(s) = exports.as_str() {
        return if key == "." { Some(s.to_string()) } else { None };
    }
    let obj = exports.as_object()?;
    if let Some(v) = obj.get(key) {
        return resolve_conditions(v);
    }
    // Wildcard: a key like "./*" or "./dist/*" mapping to "./build/*.js".
    for (pat, target) in obj {
        if let (Some(pre), Some(_post)) = (pat.strip_suffix('*'), Some("")) {
            if let Some(rest) = key.strip_prefix(pre) {
                let tgt = resolve_conditions(target)?;
                return Some(tgt.replace('*', rest));
            }
        }
    }
    None
}

fn resolve_conditions(value: &serde_json::Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }
    for cond in ["import", "module", "default", "node", "require"] {
        if let Some(v) = value.get(cond) {
            if let Some(s) = resolve_conditions(v) {
                return Some(s);
            }
        }
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
        if std::env::var("POCKET_PI_DEBUG_MODULES").is_ok() {
            eprintln!("LOAD {name}");
        }
        if let Some(src) = builtin_source(name) {
            let m = Module::declare(ctx.clone(), name, src)?;
            set_import_meta(&m, name);
            return Ok(m);
        }
        let source = std::fs::read_to_string(name)
            .map_err(|e| Error::new_loading_message(name.to_string(), e.to_string()))?;
        let is_ts = name.ends_with(".ts") || name.ends_with(".tsx") || name.ends_with(".mts") || name.ends_with(".cts");
        let js = if is_ts {
            let t = transpile_ts(name, &source).map_err(|e| Error::new_loading_message(name.to_string(), e))?;
            rewrite_reexports(&t)
        } else if name.ends_with(".json") {
            format!("export default {source};")
        } else if name.ends_with(".cjs") || (!name.ends_with(".mjs") && is_cjs(&source)) {
            // CommonJS: wrap as an ESM module so `import {x}` works, and route
            // its own `require(...)` to the synchronous native require.
            wrap_cjs(name, &source)
        } else {
            rewrite_reexports(&source)
        };
        let m = Module::declare(ctx.clone(), name, js)?;
        set_import_meta(&m, name);
        Ok(m)
    }
}

/// Populate `import.meta.url` (and a no-op `resolve`) so code that derives
/// `__filename`/`__dirname` from `import.meta.url` works.
fn set_import_meta(module: &Module<'_, Declared>, name: &str) {
    if let Ok(meta) = module.meta() {
        let url = if name.starts_with("node:") {
            format!("file:///node/{}", name.strip_prefix("node:").unwrap())
        } else {
            format!("file://{name}")
        };
        let _ = meta.set("url", url);
    }
}

/// Rewrite indirect named re-exports into an import + a local export:
///   `export { a, b as c } from "./y"`  →  `import { a, b as c } from "./y";
///    export { a, c };`
/// QuickJS resolves an indirect export by chaining through modules, which trips
/// "circular reference" inside dependency cycles (pi-coding-agent's sdk.js). A
/// local export sidesteps that chain. Semantically identical outside cycles.
fn rewrite_reexports(src: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"(?m)^\s*export\s*\{([^}]*)\}\s*from\s*(["'][^"']+["'])\s*;?"#).unwrap()
    });
    let counter = std::cell::Cell::new(0usize);
    re.replace_all(src, |caps: &regex::Captures| {
        let list = &caps[1];
        let spec = &caps[2];
        let mut imports = Vec::new();
        let mut exports = Vec::new();
        for item in list.split(',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            // `orig` is the name in the target module, `exported` the outer name.
            let (orig, exported) = match item.split_once(" as ") {
                Some((a, b)) => (a.trim(), b.trim()),
                None => (item, item),
            };
            // Unique local avoids clashing with an existing `import { orig }`.
            let n = counter.get();
            counter.set(n + 1);
            let local = format!("__rx{n}_{}", exported.replace(|c: char| !c.is_alphanumeric(), "_"));
            imports.push(format!("{orig} as {local}"));
            exports.push(format!("{local} as {exported}"));
        }
        format!(
            "import {{ {} }} from {spec}; export {{ {} }};",
            imports.join(", "),
            exports.join(", ")
        )
    })
    .into_owned()
}

/// Heuristic CJS detection: has `require(`/`module.exports`/`exports.` and no
/// top-level ESM `import`/`export` statements.
fn is_cjs(src: &str) -> bool {
    // Any top-level ESM statement → treat as ESM. `import(` (dynamic) is fine in
    // CJS and deliberately not matched here.
    for l in src.lines() {
        let t = l.trim_start();
        if t.starts_with("import ")
            || t.starts_with("import{")
            || t.starts_with("import *")
            || t.starts_with("import*")
            || t.starts_with("export ")
            || t.starts_with("export{")
            || t.starts_with("export*")
            || t.starts_with("export default")
        {
            return false;
        }
    }
    src.contains("require(") || src.contains("module.exports") || src.contains("exports.") || src.contains("exports[")
}

/// Wrap a CommonJS module as an ES module: run its body with module/exports/
/// require, cache the result, and re-export `default` plus the named exports we
/// can detect (a lightweight cjs-module-lexer).
fn wrap_cjs(name: &str, src: &str) -> String {
    let dir = Path::new(name).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    let names = cjs_named_exports(src);
    let mut named = String::new();
    for n in &names {
        named.push_str(&format!("export const {n} = __m[{:?}];\n", n));
    }
    // The body runs inside a function so its `var`/`function` decls don't leak
    // and `return` is legal; JSON.stringify keeps the source intact as a string
    // is unnecessary — we inline it directly.
    format!(
        "const module = {{ exports: {{}} }};\n\
         let exports = module.exports;\n\
         const __filename = {name:?};\n\
         const __dirname = {dir:?};\n\
         const require = (s) => globalThis.__cjsRequire(__filename, s);\n\
         globalThis.__cjsCache = globalThis.__cjsCache || new Map();\n\
         globalThis.__cjsCache.set(__filename, module.exports);\n\
         (function (module, exports, require, __filename, __dirname) {{\n{src}\n}})(module, exports, require, __filename, __dirname);\n\
         const __m = module.exports;\n\
         globalThis.__cjsCache.set(__filename, __m);\n\
         export default __m;\n{named}"
    )
}

/// Scan CJS source for its named exports (best-effort, covers TS-compiled CJS).
/// Uses `match_indices` so slicing always lands on char boundaries.
fn cjs_named_exports(src: &str) -> Vec<String> {
    fn ident(s: &str) -> String {
        s.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect()
    }
    let mut set: Vec<String> = Vec::new();
    let mut push = |n: String| {
        if n != "default" && n != "__esModule" && !n.is_empty() && !set.iter().any(|x| *x == n) {
            set.push(n);
        }
    };
    // `exports.NAME =` / `exports.NAME[`
    for (idx, _) in src.match_indices("exports.") {
        let rest = &src[idx + 8..];
        let name = ident(rest);
        if !name.is_empty() {
            let after = rest[name.len()..].trim_start();
            if after.starts_with('=') || after.starts_with('[') {
                push(name);
            }
        }
    }
    // `Object.defineProperty(exports, "NAME"` and `__createBinding(exports, …, "NAME"`
    for pat in ["defineProperty(exports,", "__createBinding(exports,"] {
        for (idx, _) in src.match_indices(pat) {
            let rest = &src[idx + pat.len()..];
            if let Some(q) = rest.find(['"', '\'']) {
                push(ident(&rest[q + 1..]));
            }
        }
    }
    set
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

    // node.spawnSync(cmd, argsJson, optsJson) -> JSON {stdout, stderr, status, error?}.
    node.set("spawnSync", Function::new(ctx.clone(), |cmd: String, args_json: String, opts_json: String| -> String {
        let args: Vec<String> = serde_json::from_str(&args_json).unwrap_or_default();
        let opts: serde_json::Value = serde_json::from_str(&opts_json).unwrap_or(serde_json::Value::Null);
        let mut command = std::process::Command::new(&cmd);
        command.args(&args);
        if let Some(cwd) = opts.get("cwd").and_then(|v| v.as_str()) {
            command.current_dir(cwd);
        }
        match command.output() {
            Ok(o) => serde_json::json!({
                "stdout": String::from_utf8_lossy(&o.stdout),
                "stderr": String::from_utf8_lossy(&o.stderr),
                "status": o.status.code(),
            }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string(), "status": serde_json::Value::Null }).to_string(),
        }
    })?)?;
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
    // Minimal stdout/stderr so the TUI + logging code can write() headlessly.
    for (name, is_err) in [("stdout", false), ("stderr", true)] {
        let stream = Object::new(ctx.clone())?;
        stream.set("isTTY", false)?;
        stream.set("columns", 80)?;
        stream.set("rows", 24)?;
        stream.set(
            "write",
            Function::new(ctx.clone(), move |s: String| -> bool {
                if is_err {
                    eprint!("{s}");
                } else {
                    print!("{s}");
                }
                true
            })?,
        )?;
        stream.set("on", Function::new(ctx.clone(), || {})?)?;
        stream.set("end", Function::new(ctx.clone(), || {})?)?;
        process.set(name, stream)?;
    }
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

    // CJS bridge: capture each builtin's exports for the synchronous `require`,
    // and define __cjsRequire (used by CJS modules wrapped by the loader).
    let cjs_boot = r#"
        import * as _fs from "node:fs";
        import * as _path from "node:path";
        import * as _os from "node:os";
        import * as _events from "node:events";
        import * as _util from "node:util";
        import * as _buffer from "node:buffer";
        import * as _process from "node:process";
        import * as _crypto from "node:crypto";
        import * as _url from "node:url";
        import * as _module from "node:module";
        import * as _stream from "node:stream";
        import * as _sd from "node:string_decoder";
        import * as _readline from "node:readline";
        import * as _fsp from "node:fs/promises";
        import * as _perf from "node:perf_hooks";
        import * as _tty from "node:tty";
        import * as _http from "node:http";
        import * as _https from "node:https";
        import * as _net from "node:net";
        import * as _tls from "node:tls";
        import * as _zlib from "node:zlib";
        import * as _dns from "node:dns";
        import * as _qs from "node:querystring";
        import * as _assert from "node:assert";
        import * as _timers from "node:timers";
        import * as _wt from "node:worker_threads";
        import * as _v8 from "node:v8";
        import * as _vm from "node:vm";
        import * as _constants from "node:constants";
        import * as _ah from "node:async_hooks";
        import * as _dc from "node:diagnostics_channel";
        import * as _sp from "node:stream/promises";
        const pick = (ns) => (ns && ns.default !== undefined ? ns.default : ns);
        globalThis.__builtinExports = {
            fs: pick(_fs), path: pick(_path), os: pick(_os), events: pick(_events),
            util: pick(_util), buffer: pick(_buffer), process: pick(_process),
            crypto: pick(_crypto), url: pick(_url), module: pick(_module), stream: pick(_stream),
            string_decoder: pick(_sd), readline: pick(_readline),
            "fs/promises": pick(_fsp), perf_hooks: pick(_perf), tty: pick(_tty),
            http: pick(_http), https: pick(_https), net: pick(_net), tls: pick(_tls),
            zlib: pick(_zlib), dns: pick(_dns), querystring: pick(_qs), assert: pick(_assert),
            timers: pick(_timers), worker_threads: pick(_wt), v8: pick(_v8), vm: pick(_vm),
            constants: pick(_constants), async_hooks: pick(_ah), "stream/promises": pick(_sp),
            diagnostics_channel: pick(_dc),
        };
        globalThis.__cjsCache = globalThis.__cjsCache || new Map();
        globalThis.__cjsRequire = function (fromFile, spec) {
            const r = JSON.parse(globalThis.__node.resolve(fromFile, spec));
            if (r.builtin != null) {
                const b = globalThis.__builtinExports[r.builtin] ?? globalThis.__builtinExports[r.builtin.split("/")[0]];
                if (b === undefined) throw new Error("builtin not available: " + r.builtin);
                return b;
            }
            if (r.err) throw new Error("Cannot find module '" + spec + "' from '" + fromFile + "'");
            const p = r.path;
            if (globalThis.__cjsCache.has(p)) return globalThis.__cjsCache.get(p);
            let src = globalThis.__node.readText(p);
            if (p.endsWith(".ts") || p.endsWith(".cts")) src = host.transpile(p, src);
            const module = { exports: {} };
            globalThis.__cjsCache.set(p, module.exports);
            const dir = p.replace(/\/[^/]*$/, "");
            const fn = new Function("module", "exports", "require", "__filename", "__dirname", src);
            fn(module, module.exports, (s) => globalThis.__cjsRequire(p, s), p, dir);
            globalThis.__cjsCache.set(p, module.exports);
            return module.exports;
        };
        // esbuild-bundled CJS calls a runtime `require` for external builtins;
        // delegate it to our synchronous require.
        globalThis.require = (spec) => globalThis.__cjsRequire("/pocket-pi-bundle", spec);
    "#;
    Module::evaluate(ctx.clone(), "pocket-pi:cjs-bootstrap", cjs_boot)?.finish::<()>()?;
    Ok(())
}
