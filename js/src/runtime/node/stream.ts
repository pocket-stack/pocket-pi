// node:stream — minimal Readable/Writable/Transform/PassThrough over EventEmitter.
import { EventEmitter } from "node:events";
export class Readable extends EventEmitter {
  constructor(opts) { super(); this._opts = opts || {}; }
  push(chunk) { if (chunk === null) this.emit("end"); else this.emit("data", chunk); return true; }
  pipe(dest) { this.on("data", (c) => dest.write && dest.write(c)); this.on("end", () => dest.end && dest.end()); return dest; }
  read() { return null; }
  static from(iterable) { const r = new Readable(); queueMicrotask(async () => { for await (const c of iterable) r.push(c); r.push(null); }); return r; }
}
export class Writable extends EventEmitter {
  constructor(opts) { super(); this._opts = opts || {}; }
  write(chunk, _enc, cb) { if (this._opts.write) this._opts.write(chunk, _enc, cb || (() => {})); else if (cb) cb(); return true; }
  end(chunk, _enc, cb) { if (chunk) this.write(chunk); this.emit("finish"); if (cb) cb(); }
}
export class Duplex extends Readable {}
export class Transform extends Duplex {}
export class PassThrough extends Transform {}
export default { Readable, Writable, Duplex, Transform, PassThrough };
export function pipeline(...args) {
  const cb = typeof args[args.length - 1] === "function" ? args.pop() : null;
  queueMicrotask(() => cb && cb(null));
  return args[args.length - 1];
}
export function finished(stream, cb) { queueMicrotask(() => cb && cb(null)); }
