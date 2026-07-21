// node:module — createRequire returns a require that handles node: builtins.
export function createRequire(_from) {
  return function require(spec) {
    throw new Error("require('" + spec + "') is not supported (ESM-only runtime)");
  };
}
export const builtinModules = ["fs", "path", "os", "events", "util", "buffer", "process", "crypto", "url", "child_process", "stream", "string_decoder", "module", "readline"];
export default { createRequire, builtinModules };
export class Module {}
