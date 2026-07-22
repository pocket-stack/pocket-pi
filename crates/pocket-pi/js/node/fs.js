const raw = globalThis.__node.fs;
const fs = {
  readFile: (p) => JSON.parse(raw.readFile(p)),
  writeFile: (p, b) => JSON.parse(raw.writeFile(p, b)),
  exists: (p) => raw.exists(p),
  readdir: (p) => JSON.parse(raw.readdir(p)),
  mkdir: (p, r) => raw.mkdir(p, r),
  stat: (p) => JSON.parse(raw.stat(p)),
  realpath: (p) => JSON.parse(raw.realpath(p)),
  unlink: (p) => raw.unlink(p)
};
function decode(bytesJson, encoding) {
  const bytes = bytesJson;
  if (!encoding) return globalThis.Buffer ? globalThis.Buffer.from(bytes) : Uint8Array.from(bytes);
  const B = globalThis.__nodeBuffer;
  return B ? B.from(bytes).toString(encoding) : String.fromCharCode(...bytes);
}
function readFileSync(path, options) {
  const encoding = typeof options === "string" ? options : options && options.encoding;
  const res = fs.readFile(String(path));
  if (res.err) throw enoent(res.err, path);
  return decode(res.bytes, encoding);
}
function writeFileSync(pathOrFd, data, options) {
  if (typeof pathOrFd === "number") {
    fdWrite(pathOrFd, data, options);
    return;
  }
  const encoding = typeof options === "string" ? options : options && options.encoding || "utf8";
  let bytes;
  if (typeof data === "string") {
    const B = globalThis.__nodeBuffer;
    bytes = B ? Array.from(B.from(data, encoding)) : Array.from(data, (c) => c.charCodeAt(0));
  } else bytes = Array.from(data);
  const res = fs.writeFile(String(pathOrFd), bytes);
  if (res.err) throw new Error(res.err);
}
function existsSync(path) {
  return fs.exists(String(path));
}
function readdirSync(path) {
  const res = fs.readdir(String(path));
  if (res.err) throw enoent(res.err, path);
  return res.entries;
}
function mkdirSync(path, options) {
  fs.mkdir(String(path), !!(options && options.recursive));
}
function statSync(path) {
  const res = fs.stat(String(path));
  if (res.err) throw enoent(res.err, path);
  return makeStat(res);
}
const lstatSync = statSync;
function realpathSync(path) {
  const res = fs.realpath(String(path));
  return res.err ? String(path) : res.path;
}
function unlinkSync(path) {
  fs.unlink(String(path));
}
function rmSync(path) {
  fs.unlink(String(path));
}
const constants = { F_OK: 0, R_OK: 4, W_OK: 2, X_OK: 1 };
function accessSync(path, _mode) {
  if (!existsSync(path)) throw enoent("no access", path);
}
const nope = (name) => () => {
  throw new Error(`fs.${name} is not implemented in Pocket Pi`);
};
const __fds = /* @__PURE__ */ new Map();
let __nextFd = 3;
function ebadf() {
  const e = new Error("EBADF: bad file descriptor");
  e.code = "EBADF";
  return e;
}
function parseFlags(flags) {
  const f = String(flags || "r");
  return {
    write: /[wa+]/.test(f),
    append: /a/.test(f),
    create: /[wa]/.test(f),
    excl: /x/.test(f),
    truncate: /w/.test(f) && !/\+/.test(f),
    read: /r|\+/.test(f)
  };
}
function openSync(path, flags, _mode) {
  path = String(path);
  const f = parseFlags(flags);
  const exists2 = existsSync(path);
  if (f.excl && exists2) {
    const e = new Error(`EEXIST: file already exists, open '${path}'`);
    e.code = "EEXIST";
    throw e;
  }
  if (!f.create && !exists2) throw enoent("open", path);
  if (f.truncate || f.create && !exists2) writeFileSync(path, "");
  const bytes = f.read && !f.truncate && existsSync(path) ? fs.readFile(path).bytes || [] : [];
  const fd = __nextFd++;
  __fds.set(fd, { path, flags: f, bytes, pos: 0 });
  return fd;
}
function readSync(fd, buffer, offset, length, position) {
  const e = __fds.get(fd);
  if (!e) throw ebadf();
  const start = position == null || position < 0 ? e.pos : position;
  let n = 0;
  for (; n < length && start + n < e.bytes.length; n++) buffer[offset + n] = e.bytes[start + n];
  if (position == null || position < 0) e.pos = start + n;
  return n;
}
function toStr(data, options) {
  if (typeof data === "string") return data;
  const enc = (typeof options === "string" ? options : options && options.encoding) || "utf8";
  const B = globalThis.__nodeBuffer;
  return B ? B.from(data).toString(enc) : String.fromCharCode(...data);
}
function fdWrite(fd, data, options) {
  const e = __fds.get(fd);
  if (!e) throw ebadf();
  const prev = existsSync(e.path) ? readFileSync(e.path, "utf8") : "";
  writeFileSync(e.path, prev + toStr(data, options), "utf8");
}
function writeSync(fd, data, _offOrPos, _length, _position) {
  const s = typeof data === "string" ? data : toStr(data);
  fdWrite(fd, s);
  return typeof data === "string" ? s.length : data.length;
}
function closeSync(fd) {
  __fds.delete(fd);
}
const fsyncSync = () => {
};
const fdatasyncSync = () => {
};
const ftruncateSync = () => {
};
function appendFileSync(path, data, options) {
  const prev = existsSync(path) ? readFileSync(path, "utf8") : "";
  writeFileSync(path, prev + (typeof data === "string" ? data : ""), options);
}
const copyFileSync = nope("copyFileSync");
function renameSync(a, b) {
  const d = readFileSync(a);
  writeFileSync(b, d);
  unlinkSync(a);
}
const rmdirSync = (p) => unlinkSync(p);
const cpSync = nope("cpSync");
const chmodSync = () => {
};
const symlinkSync = nope("symlinkSync");
const readlinkSync = (p) => realpathSync(p);
const truncateSync = nope("truncateSync");
const utimesSync = () => {
};
const createReadStream = nope("createReadStream");
const createWriteStream = nope("createWriteStream");
const watchFile = () => {
};
const unwatchFile = () => {
};
function watch() {
  return { close() {
  }, on() {
  }, unref() {
    return this;
  } };
}
const opendirSync = nope("opendirSync");
const mkdtempSync = (prefix) => {
  const p = prefix + Math.random().toString(36).slice(2, 8);
  mkdirSync(p, { recursive: true });
  return p;
};
function makeStat(res) {
  return {
    size: res.size || 0,
    mtimeMs: res.mtimeMs || 0,
    isFile: () => res.isFile,
    isDirectory: () => res.isDir,
    isSymbolicLink: () => false
  };
}
function enoent(msg, path) {
  const e = new Error(`ENOENT: ${msg}, '${path}'`);
  e.code = "ENOENT";
  e.path = String(path);
  return e;
}
const cbify = (fn) => (...args) => {
  const cb = typeof args[args.length - 1] === "function" ? args.pop() : () => {
  };
  queueMicrotask(() => {
    try {
      cb(null, fn(...args));
    } catch (e) {
      cb(e);
    }
  });
};
const readFile = cbify(readFileSync);
const writeFile = cbify(writeFileSync);
const readdir = cbify(readdirSync);
const stat = cbify(statSync);
const lstat = cbify(statSync);
const mkdir = cbify(mkdirSync);
const access = cbify(accessSync);
const unlink = cbify(unlinkSync);
const realpath = cbify(realpathSync);
const rename = cbify(renameSync);
const rm = cbify(unlinkSync);
const exists = (p, cb) => queueMicrotask(() => cb(existsSync(p)));
const P = (fn) => (...args) => new Promise((res, rej) => {
  try {
    res(fn(...args));
  } catch (e) {
    rej(e);
  }
});
const promises = {
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
    close: () => {
    },
    read: () => ({ bytesRead: 0, buffer: null }),
    write: () => ({ bytesWritten: 0 })
  }))
};
var fs_default = {
  readFileSync,
  writeFileSync,
  existsSync,
  readdirSync,
  mkdirSync,
  statSync,
  lstatSync,
  realpathSync,
  unlinkSync,
  rmSync,
  promises,
  constants: { F_OK: 0, R_OK: 4, W_OK: 2, X_OK: 1 }
};
export {
  access,
  accessSync,
  appendFileSync,
  chmodSync,
  closeSync,
  constants,
  copyFileSync,
  cpSync,
  createReadStream,
  createWriteStream,
  fs_default as default,
  exists,
  existsSync,
  fdatasyncSync,
  fsyncSync,
  ftruncateSync,
  lstat,
  lstatSync,
  mkdir,
  mkdirSync,
  mkdtempSync,
  openSync,
  opendirSync,
  promises,
  readFile,
  readFileSync,
  readSync,
  readdir,
  readdirSync,
  readlinkSync,
  realpath,
  realpathSync,
  rename,
  renameSync,
  rm,
  rmSync,
  rmdirSync,
  stat,
  statSync,
  symlinkSync,
  truncateSync,
  unlink,
  unlinkSync,
  unwatchFile,
  utimesSync,
  watch,
  watchFile,
  writeFile,
  writeFileSync,
  writeSync
};
