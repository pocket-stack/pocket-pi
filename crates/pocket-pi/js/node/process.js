// node:process — re-exports the global process object the runtime installs.
const p = globalThis.process;
export const env = p.env;
export const platform = p.platform;
export const argv = p.argv;
export const version = p.version;
export const versions = p.versions;
export const cwd = p.cwd;
export const nextTick = p.nextTick;
export const exit = p.exit;
export default p;
