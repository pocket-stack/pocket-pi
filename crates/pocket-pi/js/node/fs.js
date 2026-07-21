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

const P = (fn) => (...args) => new Promise((res, rej) => { try { res(fn(...args)); } catch (e) { rej(e); } });
export const promises = {
  readFile: P(readFileSync),
  writeFile: P(writeFileSync),
  readdir: P(readdirSync),
  mkdir: P(mkdirSync),
  stat: P(statSync),
  lstat: P(statSync),
  realpath: P(realpathSync),
  unlink: P(unlinkSync),
  rm: P(rmSync),
  access: P((p) => { if (!existsSync(p)) throw enoent("no access", p); }),
};

export default {
  readFileSync, writeFileSync, existsSync, readdirSync, mkdirSync, statSync, lstatSync,
  realpathSync, unlinkSync, rmSync, promises,
  constants: { F_OK: 0, R_OK: 4, W_OK: 2, X_OK: 1 },
};
