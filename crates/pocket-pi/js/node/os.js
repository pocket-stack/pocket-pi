const n = globalThis.__node;
function platform() {
  return globalThis.process && globalThis.process.platform || "linux";
}
function homedir() {
  return n ? n.homedir() : "/";
}
function tmpdir() {
  return n ? n.tmpdir() : "/tmp";
}
function hostname() {
  return n && n.hostname ? n.hostname() : "localhost";
}
function arch() {
  return globalThis.process && globalThis.process.arch || "x64";
}
function type() {
  return platform() === "darwin" ? "Darwin" : platform() === "win32" ? "Windows_NT" : "Linux";
}
function release() {
  return "0.0.0";
}
function cpus() {
  return [];
}
function totalmem() {
  return 0;
}
function freemem() {
  return 0;
}
function uptime() {
  return 0;
}
function userInfo() {
  return { username: "user", homedir: homedir(), shell: null, uid: -1, gid: -1 };
}
const EOL = "\n";
const constants = { signals: {}, errno: {} };
var os_default = { platform, homedir, tmpdir, hostname, arch, type, release, cpus, totalmem, freemem, uptime, userInfo, EOL, constants };
export {
  EOL,
  arch,
  constants,
  cpus,
  os_default as default,
  freemem,
  homedir,
  hostname,
  platform,
  release,
  tmpdir,
  totalmem,
  type,
  uptime,
  userInfo
};
