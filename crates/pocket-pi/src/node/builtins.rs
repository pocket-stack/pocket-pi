//! The builtin-module registry — the **single source of truth** for the `node:*`
//! modules Pocket Pi ships.
//!
//! The resolver ([`super::resolve`]), the loader ([`super`]), and the synchronous
//! CommonJS `require` bridge all derive from this one list, so adding a builtin is
//! a single line in [`BUILTINS`]. In particular the `require` bridge's
//! `__builtinExports` map is **generated** from this list by
//! [`cjs_bootstrap_source`] — there is no second place to keep in sync (a unit
//! test enforces that invariant, which is exactly the failure mode this design
//! removes).

/// A builtin module: its Node name (`fs`, `fs/promises`) and its embedded JS
/// source. `source` is compiled straight into the binary via `include_str!`, so
/// the runtime needs no filesystem to serve builtins.
pub struct Builtin {
    pub name: &'static str,
    pub source: &'static str,
}

/// Register a builtin by Node name and `js/node/<file>`.
macro_rules! builtin {
    ($name:literal, $file:literal) => {
        Builtin { name: $name, source: include_str!(concat!("../../js/node/", $file)) }
    };
}

/// Every builtin Pocket Pi ships. Add a row to expose a new `node:*` module — the
/// resolver, loader, and `require` bridge all pick it up automatically.
pub static BUILTINS: &[Builtin] = &[
    builtin!("path", "path.js"),
    builtin!("os", "os.js"),
    builtin!("events", "events.js"),
    builtin!("util", "util.js"),
    builtin!("buffer", "buffer.js"),
    builtin!("process", "process.js"),
    builtin!("fs", "fs.js"),
    builtin!("fs/promises", "fs-promises.js"),
    builtin!("child_process", "child_process.js"),
    builtin!("crypto", "crypto.js"),
    builtin!("url", "url.js"),
    builtin!("module", "module.js"),
    builtin!("stream", "stream.js"),
    builtin!("stream/promises", "stream-promises.js"),
    builtin!("string_decoder", "string_decoder.js"),
    builtin!("readline", "readline.js"),
    builtin!("perf_hooks", "perf_hooks.js"),
    builtin!("tty", "tty.js"),
    builtin!("http", "http.js"),
    builtin!("https", "https.js"),
    builtin!("net", "net.js"),
    builtin!("tls", "tls.js"),
    builtin!("zlib", "zlib.js"),
    builtin!("dns", "dns.js"),
    builtin!("querystring", "querystring.js"),
    builtin!("assert", "assert.js"),
    builtin!("timers", "timers.js"),
    builtin!("worker_threads", "worker_threads.js"),
    builtin!("v8", "v8.js"),
    builtin!("vm", "vm.js"),
    builtin!("constants", "constants.js"),
    builtin!("async_hooks", "async_hooks.js"),
    builtin!("diagnostics_channel", "diagnostics_channel.js"),
    builtin!("console", "console.js"),
];

/// The static `require` implementation appended after the generated builtin map.
const CJS_RUNTIME: &str = include_str!("../../js/node/_cjs-runtime.js");

/// Source for a builtin by name (`node:` prefix optional). Tries the exact name
/// first — so `fs/promises` gets its own module — then the root segment, so a
/// subpath like `path/win32` falls back to the `path` module.
pub fn builtin_source(name: &str) -> Option<&'static str> {
    let bare = name.strip_prefix("node:").unwrap_or(name);
    if let Some(b) = BUILTINS.iter().find(|b| b.name == bare) {
        return Some(b.source);
    }
    let root = bare.split('/').next().unwrap_or(bare);
    BUILTINS.iter().find(|b| b.name == root).map(|b| b.source)
}

/// Whether `name` names a builtin we can serve.
pub fn is_builtin(name: &str) -> bool {
    builtin_source(name).is_some()
}

/// Build the CJS bootstrap module: `import * as` every builtin's namespace, expose
/// them on `globalThis.__builtinExports`, then append the static `require`
/// implementation. Because the imports and the map are generated from [`BUILTINS`],
/// the registry is the only place the builtin list lives.
pub fn cjs_bootstrap_source() -> String {
    let mut imports = String::new();
    let mut map = String::from(
        "const __pick = (ns) => (ns && ns.default !== undefined ? ns.default : ns);\n\
         globalThis.__builtinExports = {\n",
    );
    for (i, b) in BUILTINS.iter().enumerate() {
        imports.push_str(&format!("import * as __b{i} from \"node:{}\";\n", b.name));
        map.push_str(&format!("  {:?}: __pick(__b{i}),\n", b.name));
    }
    map.push_str("};\n");
    format!("{imports}{map}{CJS_RUNTIME}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_and_root_lookup() {
        assert!(builtin_source("fs/promises").is_some());
        assert!(builtin_source("node:fs/promises").is_some());
        assert!(builtin_source("path/win32").is_some()); // root fallback
        assert!(builtin_source("nonexistent").is_none());
    }

    /// The single-source-of-truth invariant: the generated `require` bridge must
    /// expose *every* builtin — this is the exact bug (a builtin registered but
    /// missing from `__builtinExports`) that the generated map eliminates.
    #[test]
    fn bootstrap_covers_every_builtin() {
        let src = cjs_bootstrap_source();
        for b in BUILTINS {
            let key = format!("{:?}: __pick(", b.name);
            assert!(src.contains(&key), "builtin {:?} missing from __builtinExports", b.name);
        }
        // One import per builtin.
        assert_eq!(src.matches("import * as __b").count(), BUILTINS.len());
    }
}
