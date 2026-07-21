//! A minimal Node/CommonJS-flavored **module system + builtins** for QuickJS —
//! the layer that lets unmodified npm packages (ultimately `pi-coding-agent`)
//! `import`/`require` and run with no Node and no bun.
//!
//! The design is deliberately factored so it stays extensible:
//! - [`builtins`] is the **single source of truth** for `node:*` modules; the
//!   resolver, loader, and CJS `require` bridge all derive from its one list.
//! - [`resolve`] implements Node resolution (relative, `node_modules`,
//!   `exports`/`imports`, `node:` builtins) for both the ESM [`Resolver`] and the
//!   native `require` op.
//! - [`transform`] owns every source rewrite (TS erasure, JSON, CJS→ESM, the ESM
//!   cycle-breaking re-export rewrite) behind one entry point.
//! - [`ops`] mounts the native `__node`/`process` surface the JS shims call.
//!
//! `mod.rs` itself only wires these together: the [`NodeLoader`] and the
//! [`install_node`] bootstrap.

mod builtins;
mod ops;
mod resolve;
mod transform;

pub use resolve::NodeResolver;

use rquickjs::loader::{ImportAttributes, Loader};
use rquickjs::module::{Declared, Module};
use rquickjs::{Ctx, Error, Result};

/// The rquickjs loader hook: serve a builtin from the registry, or read the file,
/// transform it per its extension, and declare it.
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
        if let Some(src) = builtins::builtin_source(name) {
            let m = Module::declare(ctx.clone(), name, src)?;
            set_import_meta(&m, name);
            return Ok(m);
        }
        let source = std::fs::read_to_string(name)
            .map_err(|e| Error::new_loading_message(name.to_string(), e.to_string()))?;
        let js = transform::prepare_module_source(name, &source)
            .map_err(|e| Error::new_loading_message(name.to_string(), e))?;
        let m = Module::declare(ctx.clone(), name, js)?;
        set_import_meta(&m, name);
        Ok(m)
    }
}

/// Populate `import.meta.url` so code that derives `__filename`/`__dirname` from
/// it works. Builtins get a synthetic `file:///node/<name>` url.
fn set_import_meta(module: &Module<'_, Declared>, name: &str) {
    if let Ok(meta) = module.meta() {
        let url = match name.strip_prefix("node:") {
            Some(bare) => format!("file:///node/{bare}"),
            None => format!("file://{name}"),
        };
        let _ = meta.set("url", url);
    }
}

const NODE_BOOTSTRAP: &str = include_str!("../../js/node/_bootstrap.js");

/// Install the Node layer onto a realm: native ops + `process`, the runtime
/// bootstrap (Buffer global, `nextTick`), and the CJS `require` bridge (whose
/// `__builtinExports` map is generated from the builtin registry). Call once,
/// after the resolver/loader are set and the prelude has run.
pub fn install_node(ctx: &Ctx) -> Result<()> {
    ops::install_ops(ctx)?;
    Module::evaluate(ctx.clone(), "pocket-pi:node-bootstrap", NODE_BOOTSTRAP)?.finish::<()>()?;
    Module::evaluate(
        ctx.clone(),
        "pocket-pi:cjs-bootstrap",
        builtins::cjs_bootstrap_source().as_str(),
    )?
    .finish::<()>()?;
    Ok(())
}
