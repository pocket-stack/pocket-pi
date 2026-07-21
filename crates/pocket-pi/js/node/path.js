// node:path (POSIX subset) — pure JS, enough for pi + its deps.
const sep = "/";

function assertPath(p) {
  if (typeof p !== "string") throw new TypeError("Path must be a string. Received " + typeof p);
}

function normalizeArray(parts, allowAboveRoot) {
  const res = [];
  for (const p of parts) {
    if (!p || p === ".") continue;
    if (p === "..") {
      if (res.length && res[res.length - 1] !== "..") res.pop();
      else if (allowAboveRoot) res.push("..");
    } else res.push(p);
  }
  return res;
}

export function normalize(path) {
  assertPath(path);
  if (path.length === 0) return ".";
  const isAbs = path.charCodeAt(0) === 47;
  const trailing = path.charCodeAt(path.length - 1) === 47;
  let out = normalizeArray(path.split("/"), !isAbs).join("/");
  if (!out && !isAbs) out = ".";
  if (out && trailing) out += "/";
  return (isAbs ? "/" : "") + out;
}

export function isAbsolute(path) {
  assertPath(path);
  return path.length > 0 && path.charCodeAt(0) === 47;
}

export function join(...args) {
  if (args.length === 0) return ".";
  let joined;
  for (const arg of args) {
    assertPath(arg);
    if (arg.length > 0) joined = joined === undefined ? arg : joined + "/" + arg;
  }
  if (joined === undefined) return ".";
  return normalize(joined);
}

export function resolve(...args) {
  let resolved = "";
  let isAbs = false;
  for (let i = args.length - 1; i >= -1 && !isAbs; i--) {
    const path = i >= 0 ? args[i] : cwd();
    assertPath(path);
    if (path.length === 0) continue;
    resolved = path + "/" + resolved;
    isAbs = path.charCodeAt(0) === 47;
  }
  const parts = normalizeArray(resolved.split("/"), !isAbs);
  resolved = parts.join("/");
  if (isAbs) return "/" + resolved;
  return resolved.length > 0 ? resolved : ".";
}

function cwd() {
  return (globalThis.process && globalThis.process.cwd && globalThis.process.cwd()) || "/";
}

export function dirname(path) {
  assertPath(path);
  if (path.length === 0) return ".";
  let end = -1;
  let matchedSlash = true;
  for (let i = path.length - 1; i >= 1; --i) {
    if (path.charCodeAt(i) === 47) {
      if (!matchedSlash) { end = i; break; }
    } else matchedSlash = false;
  }
  if (end === -1) return path.charCodeAt(0) === 47 ? "/" : ".";
  if (end === 0 && path.charCodeAt(0) === 47) return "/";
  return path.slice(0, end);
}

export function basename(path, ext) {
  assertPath(path);
  let start = 0, end = -1, matchedSlash = true;
  for (let i = path.length - 1; i >= 0; --i) {
    if (path.charCodeAt(i) === 47) {
      if (!matchedSlash) { start = i + 1; break; }
    } else if (end === -1) { matchedSlash = false; end = i + 1; }
  }
  let base = end === -1 ? "" : path.slice(start, end);
  if (ext && base.endsWith(ext) && base !== ext) base = base.slice(0, -ext.length);
  return base;
}

export function extname(path) {
  assertPath(path);
  let startDot = -1, startPart = 0, end = -1, matchedSlash = true, preDotState = 0;
  for (let i = path.length - 1; i >= 0; --i) {
    const code = path.charCodeAt(i);
    if (code === 47) { if (!matchedSlash) { startPart = i + 1; break; } continue; }
    if (end === -1) { matchedSlash = false; end = i + 1; }
    if (code === 46) { if (startDot === -1) startDot = i; else if (preDotState !== 1) preDotState = 1; }
    else if (startDot !== -1) preDotState = -1;
  }
  if (startDot === -1 || end === -1 || preDotState === 0 || (preDotState === 1 && startDot === end - 1 && startDot === startPart + 1)) return "";
  return path.slice(startDot, end);
}

export function relative(from, to) {
  from = resolve(from);
  to = resolve(to);
  if (from === to) return "";
  const fromParts = from.split("/").filter(Boolean);
  const toParts = to.split("/").filter(Boolean);
  let i = 0;
  while (i < fromParts.length && i < toParts.length && fromParts[i] === toParts[i]) i++;
  const up = fromParts.length - i;
  const out = [];
  for (let j = 0; j < up; j++) out.push("..");
  return out.concat(toParts.slice(i)).join("/");
}

export function parse(path) {
  const root = isAbsolute(path) ? "/" : "";
  const dir = dirname(path);
  const base = basename(path);
  const ext = extname(base);
  return { root, dir, base, ext, name: ext ? base.slice(0, -ext.length) : base };
}

export const posix = { sep, delimiter: ":", normalize, isAbsolute, join, resolve, dirname, basename, extname, relative, parse };
export { sep };
export const delimiter = ":";
export default { sep, delimiter, normalize, isAbsolute, join, resolve, dirname, basename, extname, relative, parse, posix };
