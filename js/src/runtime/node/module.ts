// node:module — createRequire delegates to the synchronous CJS require.
export function createRequire(from) {
  let base = "/pocket-pi-bundle";
  if (typeof from === "string") base = from.startsWith("file://") ? from.slice(7) : from;
  else if (from && from.href) base = String(from.href).replace(/^file:\/\//, "");
  return function require(spec) { return globalThis.__cjsRequire(base, spec); };
}
export const builtinModules = ["fs", "path", "os", "events", "util", "buffer", "process", "crypto", "url", "child_process", "stream", "string_decoder", "module", "readline", "http", "https", "net", "tls", "zlib", "assert", "querystring"];
export function isBuiltin(m) { return builtinModules.includes(String(m).replace(/^node:/, "")); }
export class Module {}
Module.createRequire = createRequire;
Module.builtinModules = builtinModules;
export default { createRequire, builtinModules, isBuiltin, Module };
