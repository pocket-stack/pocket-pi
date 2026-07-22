// node:util — the commonly-used subset.
export function inherits(ctor, superCtor) {
  ctor.super_ = superCtor;
  ctor.prototype = Object.create(superCtor.prototype, {
    constructor: { value: ctor, enumerable: false, writable: true, configurable: true },
  });
}

export function format(fmt, ...args) {
  if (typeof fmt !== "string") return [fmt, ...args].map(inspect).join(" ");
  let i = 0;
  let out = fmt.replace(/%[sdifjoO%]/g, (m) => {
    if (m === "%%") return "%";
    if (i >= args.length) return m;
    const a = args[i++];
    switch (m) {
      case "%s": return String(a);
      case "%d": case "%i": return String(parseInt(a, 10));
      case "%f": return String(parseFloat(a));
      case "%j": try { return JSON.stringify(a); } catch { return "[Circular]"; }
      default: return inspect(a);
    }
  });
  for (; i < args.length; i++) out += " " + (typeof args[i] === "string" ? args[i] : inspect(args[i]));
  return out;
}

export function inspect(obj) {
  if (typeof obj === "string") return obj;
  try { return JSON.stringify(obj); } catch { return String(obj); }
}
inspect.custom = Symbol.for("nodejs.util.inspect.custom");

export function promisify(fn) {
  return function (...args) {
    return new Promise((resolve, reject) => {
      fn.call(this, ...args, (err, ...rest) => (err ? reject(err) : resolve(rest.length > 1 ? rest : rest[0])));
    });
  };
}

export function callbackify(fn) {
  return function (...args) {
    const cb = args.pop();
    fn.apply(this, args).then((v) => cb(null, v), (e) => cb(e));
  };
}

export function deprecate(fn) { return fn; }
// debuglog(section) → a logger; enabled only if NODE_DEBUG lists the section.
export function debuglog(section, cb) {
  const env = (globalThis.process && globalThis.process.env && globalThis.process.env.NODE_DEBUG) || "";
  const on = env.split(/[\s,]+/).includes(section);
  const fn = on ? (...args) => { try { console.error(`${section}:`, format(...args)); } catch {} } : () => {};
  if (typeof cb === "function") cb(fn);
  return fn;
}
export const debug = debuglog;
export function inspect2() {}
export const isDeepStrictEqual = (a, b) => { try { return JSON.stringify(a) === JSON.stringify(b); } catch { return a === b; } };
export function stripVTControlCharacters(s) { return String(s).replace(/\x1b\[[0-9;]*m/g, ""); }
export const _extend = Object.assign;

export const types = {
  isPromise: (v) => v && typeof v.then === "function",
  isDate: (v) => v instanceof Date,
  isRegExp: (v) => v instanceof RegExp,
  isArrayBuffer: (v) => v instanceof ArrayBuffer,
  isTypedArray: (v) => ArrayBuffer.isView(v) && !(v instanceof DataView),
  isAsyncFunction: (v) => v && v.constructor && v.constructor.name === "AsyncFunction",
};

export const TextEncoder = globalThis.TextEncoder;
export const TextDecoder = globalThis.TextDecoder;

export default { inherits, format, inspect, promisify, callbackify, deprecate, debuglog, debug, isDeepStrictEqual, stripVTControlCharacters, _extend, types, TextEncoder, TextDecoder };
