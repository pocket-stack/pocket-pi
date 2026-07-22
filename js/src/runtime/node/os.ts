// node:os — backed by native ops for the machine-specific bits.
const n = globalThis.__node;

export function platform() {
  return (globalThis.process && globalThis.process.platform) || "linux";
}
export function homedir() {
  return n ? n.homedir() : "/";
}
export function tmpdir() {
  return n ? n.tmpdir() : "/tmp";
}
export function hostname() {
  return n && n.hostname ? n.hostname() : "localhost";
}
export function arch() {
  return (globalThis.process && globalThis.process.arch) || "x64";
}
export function type() {
  return platform() === "darwin" ? "Darwin" : platform() === "win32" ? "Windows_NT" : "Linux";
}
export function release() {
  return "0.0.0";
}
export function cpus() {
  return [];
}
export function totalmem() {
  return 0;
}
export function freemem() {
  return 0;
}
export function uptime() {
  return 0;
}
export function userInfo() {
  return { username: "user", homedir: homedir(), shell: null, uid: -1, gid: -1 };
}
export const EOL = "\n";
export const constants = { signals: {}, errno: {} };
export default { platform, homedir, tmpdir, hostname, arch, type, release, cpus, totalmem, freemem, uptime, userInfo, EOL, constants };
