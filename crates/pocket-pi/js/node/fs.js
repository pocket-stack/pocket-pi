// node:fs — sync + promise subset, backed by native ops (globalThis.__node.fs).
// Native ops return JSON strings; wrap them so callers see plain objects.
const raw = globalThis.__node.fs;
const fs = {
  readFile: (p) => JSON.parse(raw.readFile(p)),
  writeFile: (p, b) => JSON.parse(raw.writeFile(p, b)),
  exists: (p) => raw.exists(p),
  readdir: (p) => JSON.parse(raw.readdir(p)),
  mkdir: (p, r) => raw.mkdir(p, r),
  stat: (p) => JSON.parse(raw.stat(p)),
  realpath: (p) => JSON.parse(raw.realpath(p)),
  unlink: (p) => raw.unlink(p),
};

function decode(bytesJson, encoding) {
  // Native returns a JSON array of bytes; decode per requested encoding.
  const bytes = bytesJson;
  if (!encoding) return globalThis.Buffer ? globalThis.Buffer.from(bytes) : Uint8Array.from(bytes);
  const B = globalThis.__nodeBuffer;
  return B ? B.from(bytes).toString(encoding) : String.fromCharCode(...bytes);
}

export function readFileSync(path, options) {
  const encoding = typeof options === "string" ? options : options && options.encoding;
  const res = fs.readFile(String(path));
  if (res.err) throw enoent(res.err, path);
  return decode(res.bytes, encoding);
}
export function writeFileSync(path, data, options) {
  const encoding = typeof options === "string" ? options : (options && options.encoding) || "utf8";
  let bytes;
  if (typeof data === "string") {
    const B = globalThis.__nodeBuffer;
    bytes = B ? Array.from(B.from(data, encoding)) : Array.from(data, (c) => c.charCodeAt(0));
  } else bytes = Array.from(data);
  const res = fs.writeFile(String(path), bytes);
  if (res.err) throw new Error(res.err);
}
export function existsSync(path) {
  return fs.exists(String(path));
}
export function readdirSync(path) {
  const res = fs.readdir(String(path));
  if (res.err) throw enoent(res.err, path);
  return res.entries;
}
export function mkdirSync(path, options) {
  fs.mkdir(String(path), !!(options && options.recursive));
}
export function statSync(path) {
  const res = fs.stat(String(path));
  if (res.err) throw enoent(res.err, path);
  return makeStat(res);
}
export const lstatSync = statSync;
export function realpathSync(path) {
  const res = fs.realpath(String(path));
  return res.err ? String(path) : res.path;
}
export function unlinkSync(path) {
  fs.unlink(String(path));
}
export function rmSync(path) {
  fs.unlink(String(path));
}
export const constants = { F_OK: 0, R_OK: 4, W_OK: 2, X_OK: 1 };
export function accessSync(path, _mode) {
  if (!existsSync(path)) throw enoent("no access", path);
}

// Broader fs surface as stubs so any `import { … } from "fs"` resolves. The ones
// pi actually exercises headlessly are implemented above; the rest throw or no-op
// until a milestone needs them.
const nope = (name) => () => { throw new Error(`fs.${name} is not implemented in Pocket Pi`); };
export const openSync = nope("openSync");
export const closeSync = () => {};
export const readSync = nope("readSync");
export const writeSync = nope("writeSync");
export function appendFileSync(path, data, options) {
  const prev = existsSync(path) ? readFileSync(path, "utf8") : "";
  writeFileSync(path, prev + (typeof data === "string" ? data : ""), options);
}
export const copyFileSync = nope("copyFileSync");
export function renameSync(a, b) { const d = readFileSync(a); writeFileSync(b, d); unlinkSync(a); }
export const rmdirSync = (p) => unlinkSync(p);
export const cpSync = nope("cpSync");
export const chmodSync = () => {};
export const symlinkSync = nope("symlinkSync");
export const readlinkSync = (p) => realpathSync(p);
export const truncateSync = nope("truncateSync");
export const utimesSync = () => {};
export const createReadStream = nope("createReadStream");
export const createWriteStream = nope("createWriteStream");
export const watchFile = () => {};
export const unwatchFile = () => {};
export function watch() { return { close() {}, on() {}, unref() { return this; } }; }
export const opendirSync = nope("opendirSync");
export const mkdtempSync = (prefix) => { const p = prefix + Math.random().toString(36).slice(2, 8); mkdirSync(p, { recursive: true }); return p; };

function makeStat(res) {
  return {
    size: res.size || 0,
    mtimeMs: res.mtimeMs || 0,
    isFile: () => res.isFile,
    isDirectory: () => res.isDir,
    isSymbolicLink: () => false,
  };
}
function enoent(msg, path) {
  const e = new Error(`ENOENT: ${msg}, '${path}'`);
  e.code = "ENOENT";
  e.path = String(path);
  return e;
}

// Callback-style async API (fs.readFile(path, cb), etc.) — wrap the sync forms.
const cbify = (fn) => (...args) => {
  const cb = typeof args[args.length - 1] === "function" ? args.pop() : () => {};
  queueMicrotask(() => { try { cb(null, fn(...args)); } catch (e) { cb(e); } });
};
export const readFile = cbify(readFileSync);
export const writeFile = cbify(writeFileSync);
export const readdir = cbify(readdirSync);
export const stat = cbify(statSync);
export const lstat = cbify(statSync);
export const mkdir = cbify(mkdirSync);
export const access = cbify(accessSync);
export const unlink = cbify(unlinkSync);
export const realpath = cbify(realpathSync);
export const rename = cbify(renameSync);
export const rm = cbify(unlinkSync);
export const exists = (p, cb) => queueMicrotask(() => cb(existsSync(p)));

const P = (fn) => (...args) => new Promise((res, rej) => { try { res(fn(...args)); } catch (e) { rej(e); } });
export const promises = {
  readFile: P(readFileSync),
  writeFile: P(writeFileSync),
  readdir: P(readdirSync),
  mkdir: P(mkdirSync),
  stat: P(statSync),
  lstat: P(statSync),
  realpath: P(realpathSync),
  readlink: P(readlinkSync),
  unlink: P(unlinkSync),
  rm: P(rmSync),
  rmdir: P(rmdirSync),
  rename: P(renameSync),
  copyFile: P(copyFileSync),
  cp: P(cpSync),
  appendFile: P(appendFileSync),
  chmod: P(chmodSync),
  symlink: P(symlinkSync),
  utimes: P(utimesSync),
  truncate: P(truncateSync),
  mkdtemp: P(mkdtempSync),
  access: P(accessSync),
  open: P((path) => ({
    fd: 0,
    readFile: (opts) => readFileSync(path, opts),
    writeFile: (data, opts) => writeFileSync(path, data, opts),
    stat: () => statSync(path),
    close: () => {},
    read: () => ({ bytesRead: 0, buffer: null }),
    write: () => ({ bytesWritten: 0 }),
  })),
};

export default {
  readFileSync, writeFileSync, existsSync, readdirSync, mkdirSync, statSync, lstatSync,
  realpathSync, unlinkSync, rmSync, promises,
  constants: { F_OK: 0, R_OK: 4, W_OK: 2, X_OK: 1 },
};
