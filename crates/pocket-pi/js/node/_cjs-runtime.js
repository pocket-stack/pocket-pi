globalThis.__cjsCache = globalThis.__cjsCache || /* @__PURE__ */ new Map();
globalThis.__cjsRequire = function(fromFile, spec) {
  const r = JSON.parse(globalThis.__node.resolve(fromFile, spec));
  if (r.builtin != null) {
    const exports = globalThis.__builtinExports[r.builtin] ?? globalThis.__builtinExports[r.builtin.split("/")[0]];
    if (exports === void 0) throw new Error("builtin not available: " + r.builtin);
    return exports;
  }
  if (r.err) throw new Error("Cannot find module '" + spec + "' from '" + fromFile + "'");
  const p = r.path;
  if (globalThis.__cjsCache.has(p)) return globalThis.__cjsCache.get(p);
  if (p.endsWith(".json")) {
    const val = JSON.parse(globalThis.__node.readText(p));
    globalThis.__cjsCache.set(p, val);
    return val;
  }
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
globalThis.require = (spec) => globalThis.__cjsRequire("/pocket-pi-bundle", spec);
