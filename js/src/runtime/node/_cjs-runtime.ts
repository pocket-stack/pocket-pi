// Synchronous CommonJS `require`, over the native resolver + file reader. This is
// the static tail of the CJS bootstrap: the generated head (see
// builtins::cjs_bootstrap_source) has already populated globalThis.__builtinExports
// from the builtin registry, so this file only implements the require semantics.
// Kept separate from the generated part so the logic is readable and lintable.

globalThis.__cjsCache = globalThis.__cjsCache || new Map();

// Resolve + load a CommonJS (or JSON, or transpiled-TS) module synchronously.
// `fromFile` is the requiring module's path; `spec` is the import specifier.
globalThis.__cjsRequire = function (fromFile, spec) {
  const r = JSON.parse(globalThis.__node.resolve(fromFile, spec));

  // A node: builtin — served straight from the registry-populated map.
  if (r.builtin != null) {
    const exports =
      globalThis.__builtinExports[r.builtin] ??
      globalThis.__builtinExports[r.builtin.split("/")[0]];
    if (exports === undefined) throw new Error("builtin not available: " + r.builtin);
    return exports;
  }
  if (r.err) throw new Error("Cannot find module '" + spec + "' from '" + fromFile + "'");

  const p = r.path;
  if (globalThis.__cjsCache.has(p)) return globalThis.__cjsCache.get(p);

  // JSON modules resolve to their parsed value.
  if (p.endsWith(".json")) {
    const val = JSON.parse(globalThis.__node.readText(p));
    globalThis.__cjsCache.set(p, val);
    return val;
  }

  let src = globalThis.__node.readText(p);
  if (p.endsWith(".ts") || p.endsWith(".cts")) src = host.transpile(p, src);

  const module = { exports: {} };
  // Seed the cache before running the body so require cycles terminate.
  globalThis.__cjsCache.set(p, module.exports);
  const dir = p.replace(/\/[^/]*$/, "");
  const fn = new Function("module", "exports", "require", "__filename", "__dirname", src);
  fn(module, module.exports, (s) => globalThis.__cjsRequire(p, s), p, dir);
  globalThis.__cjsCache.set(p, module.exports);
  return module.exports;
};

// esbuild-bundled CJS emits a runtime `require(...)` for anything left external
// (our node: builtins); delegate it to the synchronous require above.
globalThis.require = (spec) => globalThis.__cjsRequire("/pocket-pi-bundle", spec);
