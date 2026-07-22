function createRequire(from) {
  let base = "/pocket-pi-bundle";
  if (typeof from === "string") base = from.startsWith("file://") ? from.slice(7) : from;
  else if (from && from.href) base = String(from.href).replace(/^file:\/\//, "");
  return function require2(spec) {
    return globalThis.__cjsRequire(base, spec);
  };
}
const builtinModules = ["fs", "path", "os", "events", "util", "buffer", "process", "crypto", "url", "child_process", "stream", "string_decoder", "module", "readline", "http", "https", "net", "tls", "zlib", "assert", "querystring"];
function isBuiltin(m) {
  return builtinModules.includes(String(m).replace(/^node:/, ""));
}
class Module {
}
Module.createRequire = createRequire;
Module.builtinModules = builtinModules;
var module_default = { createRequire, builtinModules, isBuiltin, Module };
export {
  Module,
  builtinModules,
  createRequire,
  module_default as default,
  isBuiltin
};
