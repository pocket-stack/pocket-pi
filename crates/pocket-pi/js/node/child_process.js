// node:child_process — spawnSync/execSync real (native), async forms best-effort.
import { EventEmitter } from "node:events";
const n = globalThis.__node;
export function spawnSync(cmd, args = [], options = {}) {
  const res = JSON.parse(n.spawnSync(String(cmd), JSON.stringify(args || []), JSON.stringify(options || {})));
  const enc = options.encoding;
  const wrap = (s) => (enc && enc !== "buffer" ? s : globalThis.Buffer.from(s));
  return {
    status: res.status ?? null,
    signal: null,
    stdout: wrap(res.stdout || ""),
    stderr: wrap(res.stderr || ""),
    error: res.error ? new Error(res.error) : undefined,
    pid: 0,
  };
}
export function execSync(command, options = {}) {
  const shell = options.shell || "/bin/sh";
  const res = spawnSync(shell, ["-c", String(command)], options);
  if (res.status && res.status !== 0) {
    const e = new Error(`Command failed: ${command}\n${res.stderr}`);
    e.status = res.status;
    e.stdout = res.stdout;
    e.stderr = res.stderr;
    throw e;
  }
  return res.stdout;
}
export function execFileSync(file, args = [], options = {}) {
  return spawnSync(file, args, options).stdout;
}
// Async forms: minimal EventEmitter-ish; adequate for code that imports but
// rarely runs them on the session path. (Real async spawn is a later milestone.)
export function spawn(cmd, args = [], options = {}) {
  const ee = new EventEmitter();
  queueMicrotask(() => {
    try {
      const r = spawnSync(cmd, args, options);
      ee.stdout && ee.stdout.emit && ee.stdout.emit("data", r.stdout);
      ee.emit("close", r.status ?? 0);
    } catch (e) { ee.emit("error", e); }
  });
  ee.stdout = { on() {} };
  ee.stderr = { on() {} };
  ee.stdin = { write() {}, end() {} };
  return ee;
}
export function exec(command, options, cb) {
  if (typeof options === "function") { cb = options; options = {}; }
  queueMicrotask(() => {
    try { const out = execSync(command, options); cb && cb(null, out, ""); }
    catch (e) { cb && cb(e, e.stdout || "", e.stderr || ""); }
  });
  return { on() {} };
}
export const execFile = exec;
export function fork() { throw new Error("child_process.fork is not supported in Pocket Pi"); }
export default { spawnSync, execSync, execFileSync, spawn, exec, execFile, fork };
