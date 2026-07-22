import { EventEmitter } from "node:events";
class Readable extends EventEmitter {
  constructor(opts) {
    super();
    this._opts = opts || {};
  }
  push(chunk) {
    if (chunk === null) this.emit("end");
    else this.emit("data", chunk);
    return true;
  }
  pipe(dest) {
    this.on("data", (c) => dest.write && dest.write(c));
    this.on("end", () => dest.end && dest.end());
    return dest;
  }
  read() {
    return null;
  }
  static from(iterable) {
    const r = new Readable();
    queueMicrotask(async () => {
      for await (const c of iterable) r.push(c);
      r.push(null);
    });
    return r;
  }
}
class Writable extends EventEmitter {
  constructor(opts) {
    super();
    this._opts = opts || {};
  }
  write(chunk, _enc, cb) {
    if (this._opts.write) this._opts.write(chunk, _enc, cb || (() => {
    }));
    else if (cb) cb();
    return true;
  }
  end(chunk, _enc, cb) {
    if (chunk) this.write(chunk);
    this.emit("finish");
    if (cb) cb();
  }
}
class Duplex extends Readable {
}
class Transform extends Duplex {
}
class PassThrough extends Transform {
}
var stream_default = { Readable, Writable, Duplex, Transform, PassThrough };
function pipeline(...args) {
  const cb = typeof args[args.length - 1] === "function" ? args.pop() : null;
  queueMicrotask(() => cb && cb(null));
  return args[args.length - 1];
}
function finished(stream, cb) {
  queueMicrotask(() => cb && cb(null));
}
export {
  Duplex,
  PassThrough,
  Readable,
  Transform,
  Writable,
  stream_default as default,
  finished,
  pipeline
};
