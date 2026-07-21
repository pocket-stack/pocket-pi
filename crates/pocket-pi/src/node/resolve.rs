//! Node module resolution — the subset Pocket Pi needs: relative paths, the
//! `node_modules` walk, `exports`/`imports` maps (conditions + a single `*`
//! wildcard), and `node:` builtins. Shared by the rquickjs [`Resolver`] and the
//! native `require` op (via [`resolve_spec`]).

use rquickjs::loader::{ImportAttributes, Resolver};
use rquickjs::{Ctx, Error, Result};
use std::path::{Path, PathBuf};

use super::builtins::is_builtin;

/// The rquickjs resolver hook — a thin wrapper over [`resolve_spec`].
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

/// Resolve `name` imported from `base` to a canonical module id: `node:X` for a
/// builtin, or an absolute file path. Shared by the ESM resolver and the CJS
/// `require` op so both agree on resolution.
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

/// Probe a path for the file that actually exists, trying the Node extension and
/// index-file candidates in priority order.
fn probe(p: &Path) -> Option<String> {
    let s = p.to_string_lossy().to_string();
    const EXTS: &[&str] = &["", ".ts", ".tsx", ".mts", ".js", ".mjs", ".cjs", ".json"];
    const INDEX: &[&str] = &["/index.ts", "/index.js", "/index.mjs"];
    EXTS.iter()
        .map(|ext| format!("{s}{ext}"))
        .chain(INDEX.iter().map(|idx| format!("{s}{idx}")))
        .find(|c| Path::new(c).is_file())
}

/// Walk `node_modules` from `base` upward, resolving a bare specifier
/// (`pkg` or `@scope/pkg` with an optional subpath) in the first package found.
fn resolve_bare(base: &str, name: &str) -> Option<String> {
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
            let json = read_json(&pj)?;
            let imports = json.get("imports")?;
            let obj = imports.as_object()?;
            // Exact match first, then a single-`*` wildcard pattern.
            if let Some(v) = obj.get(name) {
                let target = resolve_conditions(v)?;
                return probe(&normalize(&d.join(target)));
            }
            for (pat, target) in obj {
                if let Some(cap) = wildcard_capture(pat, name) {
                    let tgt = resolve_conditions(target)?.replace('*', &cap);
                    return probe(&normalize(&d.join(tgt)));
                }
            }
            return None; // the nearest package.json is the resolution boundary
        }
        dir = d.parent();
    }
    None
}

/// Resolve a specifier within a package dir, honoring the `exports` map (exact,
/// conditional, and single-`*` wildcard subpaths), else `module`/`main`, else a
/// literal probe.
fn resolve_in_package(pkg_dir: &Path, subpath: Option<&str>) -> Option<String> {
    if let Some(json) = read_json(&pkg_dir.join("package.json")) {
        let key = match subpath {
            None => ".".to_string(),
            Some(s) => format!("./{s}"),
        };
        if let Some(target) = resolve_export(&json, &key) {
            return probe(&normalize(&pkg_dir.join(target)));
        }
        // No exports match: use module/main for the root, else a literal probe.
        if subpath.is_none() {
            let entry = json
                .get("module")
                .and_then(|v| v.as_str())
                .or_else(|| json.get("main").and_then(|v| v.as_str()))
                .unwrap_or("index.js");
            return probe(&normalize(&pkg_dir.join(entry)));
        }
    }
    probe(&normalize(&pkg_dir.join(subpath.unwrap_or("index.js"))))
}

/// Look up `key` (`.` or `./sub`) in a package's `exports`, resolving conditions
/// and a single-`*` wildcard. Returns the target path relative to the package dir.
fn resolve_export(json: &serde_json::Value, key: &str) -> Option<String> {
    let exports = json.get("exports")?;
    // `"exports": "./x.js"` — sugar for the root entry only.
    if let Some(s) = exports.as_str() {
        return if key == "." { Some(s.to_string()) } else { None };
    }
    let obj = exports.as_object()?;
    // Exact subpath wins over any wildcard (Node's precedence).
    if let Some(v) = obj.get(key) {
        return resolve_conditions(v);
    }
    for (pat, target) in obj {
        if let Some(cap) = wildcard_capture(pat, key) {
            return Some(resolve_conditions(target)?.replace('*', &cap));
        }
    }
    None
}

/// Pick a target string out of a conditional-exports value. Strings resolve
/// directly; objects are probed in a fixed priority (ESM-leaning, with `default`
/// and `require` as fallbacks). Nested objects recurse.
fn resolve_conditions(value: &serde_json::Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }
    // Arrays: first resolvable entry wins (Node's fallback-array semantics).
    if let Some(arr) = value.as_array() {
        return arr.iter().find_map(resolve_conditions);
    }
    for cond in ["import", "module", "browser", "default", "node", "require"] {
        if let Some(v) = value.get(cond) {
            if let Some(s) = resolve_conditions(v) {
                return Some(s);
            }
        }
    }
    None
}

/// Match a subpath pattern containing at most one `*` against `key`, returning the
/// `*` capture. A pattern without `*` matches only its exact equal.
fn wildcard_capture(pattern: &str, key: &str) -> Option<String> {
    match pattern.find('*') {
        None => (pattern == key).then(String::new),
        Some(i) => {
            let (pre, post) = (&pattern[..i], &pattern[i + 1..]);
            if key.len() >= pre.len() + post.len() && key.starts_with(pre) && key.ends_with(post) {
                Some(key[pre.len()..key.len() - post.len()].to_string())
            } else {
                None
            }
        }
    }
}

/// Split a bare specifier into `(package, subpath)`, handling `@scope/pkg`.
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

/// Collapse `.`/`..` components without touching the filesystem.
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

fn read_json(path: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_scoped_and_plain_packages() {
        assert_eq!(split_package("react"), ("react".into(), None));
        assert_eq!(split_package("react/jsx-runtime"), ("react".into(), Some("jsx-runtime".into())));
        assert_eq!(split_package("@scope/pkg"), ("@scope/pkg".into(), None));
        assert_eq!(split_package("@scope/pkg/sub"), ("@scope/pkg".into(), Some("sub".into())));
    }

    #[test]
    fn wildcard_matches_prefix_and_suffix() {
        assert_eq!(wildcard_capture("./*", "./foo"), Some("foo".into()));
        assert_eq!(wildcard_capture("./features/*.js", "./features/x.js"), Some("x".into()));
        assert_eq!(wildcard_capture("./features/*.js", "./other/x.js"), None);
        assert_eq!(wildcard_capture("./exact", "./exact"), Some(String::new()));
        assert_eq!(wildcard_capture("./exact", "./nope"), None);
    }

    #[test]
    fn exports_exact_beats_wildcard() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"exports":{"./a":"./exact.js","./*":"./src/*.js"}}"#,
        )
        .unwrap();
        assert_eq!(resolve_export(&json, "./a").as_deref(), Some("./exact.js"));
        assert_eq!(resolve_export(&json, "./b").as_deref(), Some("./src/b.js"));
    }

    #[test]
    fn conditions_prefer_import_then_fall_back() {
        let dual: serde_json::Value =
            serde_json::from_str(r#"{"import":"./m.mjs","require":"./m.cjs"}"#).unwrap();
        assert_eq!(resolve_conditions(&dual).as_deref(), Some("./m.mjs"));
        let only_req: serde_json::Value = serde_json::from_str(r#"{"require":"./m.cjs"}"#).unwrap();
        assert_eq!(resolve_conditions(&only_req).as_deref(), Some("./m.cjs"));
        let nested: serde_json::Value =
            serde_json::from_str(r#"{"node":{"import":"./n.mjs"}}"#).unwrap();
        assert_eq!(resolve_conditions(&nested).as_deref(), Some("./n.mjs"));
    }

    #[test]
    fn builtins_resolve_with_node_prefix() {
        assert_eq!(resolve_spec("/x.js", "fs").as_deref(), Some("node:fs"));
        assert_eq!(resolve_spec("/x.js", "node:path").as_deref(), Some("node:path"));
    }
}
