//! Source transforms applied by the loader before a module is declared:
//! TypeScript type-erasure, JSON wrapping, CommonJS→ESM bridging, and an ESM
//! re-export rewrite that breaks QuickJS linker cycles. One entry point,
//! [`prepare_module_source`], picks the transform from the file extension (and,
//! for ambiguous `.js`, a CJS/ESM classification).

use crate::transpile::transpile_ts;
use std::path::Path;

/// Turn a raw module `source` (already known to be a non-builtin file at `name`)
/// into the JS the loader declares. Errors only if TypeScript transpile fails.
pub fn prepare_module_source(name: &str, source: &str) -> std::result::Result<String, String> {
    let js = if is_typescript(name) {
        rewrite_reexports(&transpile_ts(name, source)?)
    } else if name.ends_with(".json") {
        format!("export default {source};")
    } else if name.ends_with(".cjs") || (!name.ends_with(".mjs") && is_cjs(source)) {
        // CommonJS: wrap as an ESM module so `import { x }` works, routing its own
        // `require(...)` through the synchronous native require.
        wrap_cjs(name, source)
    } else {
        rewrite_reexports(source)
    };
    Ok(js)
}

fn is_typescript(name: &str) -> bool {
    name.ends_with(".ts") || name.ends_with(".tsx") || name.ends_with(".mts") || name.ends_with(".cts")
}

/// Heuristic CJS classification for ambiguous `.js`/extensionless files: any
/// top-level ESM statement marks it ESM; otherwise a `require`/`exports` marker
/// marks it CJS. Deliberately a scan, not a parse — the loader runs this on every
/// file including the multi-megabyte Path B bundle, and `.cjs`/`.mjs`/`.ts` are
/// classified by extension without reaching here.
fn is_cjs(src: &str) -> bool {
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
    src.contains("require(")
        || src.contains("module.exports")
        || src.contains("exports.")
        || src.contains("exports[")
}

/// Wrap a CommonJS module as an ES module: run its body with `module`/`exports`/
/// `require`, cache the result, and re-export `default` plus the named exports we
/// can detect (a lightweight cjs-module-lexer, [`cjs_named_exports`]).
fn wrap_cjs(name: &str, src: &str) -> String {
    let dir = Path::new(name)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut named = String::new();
    for n in cjs_named_exports(src) {
        named.push_str(&format!("export const {n} = __m[{:?}];\n", n));
    }
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
/// Uses `match_indices` so slicing always lands on char boundaries (a byte-index
/// slice here once panicked in the non-unwinding native op → SIGABRT).
fn cjs_named_exports(src: &str) -> Vec<String> {
    fn ident(s: &str) -> String {
        s.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect()
    }
    let mut set: Vec<String> = Vec::new();
    let mut push = |n: String| {
        if n != "default" && n != "__esModule" && !n.is_empty() && !set.contains(&n) {
            set.push(n);
        }
    };
    // `exports.NAME =` / `exports.NAME[`
    for (idx, _) in src.match_indices("exports.") {
        let rest = &src[idx + "exports.".len()..];
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

/// Rewrite indirect named re-exports into an import + a local export:
///   `export { a, b as c } from "./y"`  →
///   `import { a, b as c } from "./y"; export { a, c };`
/// QuickJS resolves an indirect export by chaining through modules, which trips
/// "circular reference" inside dependency cycles. A local binding sidesteps that
/// chain and is semantically identical outside cycles.
fn rewrite_reexports(src: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"(?m)^\s*export\s*\{([^}]*)\}\s*from\s*(["'][^"']+["'])\s*;?"#).unwrap()
    });
    let counter = std::cell::Cell::new(0usize);
    re.replace_all(src, |caps: &regex::Captures| {
        let (list, spec) = (&caps[1], &caps[2]);
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
            // A unique local avoids clashing with an existing `import { orig }`.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_cjs_and_esm() {
        assert!(is_cjs("const x = require('y');\nmodule.exports = x;"));
        assert!(!is_cjs("import x from 'y';\nexport default x;"));
        assert!(!is_cjs("export const a = 1;"));
    }

    #[test]
    fn detects_cjs_named_exports() {
        let names = cjs_named_exports("exports.foo = 1;\nObject.defineProperty(exports, \"bar\", {});");
        assert!(names.contains(&"foo".to_string()));
        assert!(names.contains(&"bar".to_string()));
        assert!(!names.contains(&"default".to_string()));
    }

    #[test]
    fn rewrites_indirect_reexports_to_local_bindings() {
        let out = rewrite_reexports("export { a, b as c } from \"./y\";");
        assert!(out.contains("import {"), "got: {out}");
        assert!(out.contains("as c"), "got: {out}");
        assert!(!out.contains("export { a, b as c } from"), "still indirect: {out}");
    }

    #[test]
    fn prepare_dispatches_on_extension() {
        assert!(prepare_module_source("/x.json", "{\"a\":1}").unwrap().starts_with("export default"));
        assert!(prepare_module_source("/x.cjs", "module.exports = 1;").unwrap().contains("__cjsRequire"));
    }
}
