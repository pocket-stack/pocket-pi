(function() {
  "use strict";
  if (typeof globalThis.global === "undefined") globalThis.global = globalThis;
  if (typeof globalThis.setImmediate !== "function")
    globalThis.setImmediate = (fn, ...a) => globalThis.setTimeout(fn, 0, ...a);
  if (typeof globalThis.clearImmediate !== "function")
    globalThis.clearImmediate = (id) => globalThis.clearTimeout(id);
  const timers = /* @__PURE__ */ new Map();
  let nextTimer = 1;
  globalThis.setTimeout = function(fn, delay, ...args) {
    const id = nextTimer++;
    timers.set(id, { due: Date.now() + (delay || 0), fn, args });
    return id;
  };
  globalThis.clearTimeout = function(id) {
    timers.delete(id);
  };
  globalThis.setInterval = function() {
    throw new Error("setInterval is not supported in Pocket Pi");
  };
  globalThis.clearInterval = function() {
  };
  globalThis.__catpiTimers = function() {
    if (timers.size === 0) return;
    const now = Date.now();
    for (const [id, t] of timers) {
      if (t.due <= now) {
        timers.delete(id);
        try {
          t.fn(...t.args);
        } catch (e) {
          if (globalThis.host && host.emit)
            host.emit(JSON.stringify({ kind: "error", message: "timer: " + String(e) }));
        }
      }
    }
  };
  if (typeof globalThis.queueMicrotask !== "function") {
    globalThis.queueMicrotask = function(fn) {
      Promise.resolve().then(fn);
    };
  }
  if (typeof globalThis.structuredClone !== "function") {
    globalThis.structuredClone = function(v) {
      return v === void 0 ? void 0 : JSON.parse(JSON.stringify(v));
    };
  }
  if (typeof globalThis.AbortController !== "function") {
    class PPAbortSignal {
      constructor() {
        this.aborted = false;
        this.reason = void 0;
        this._listeners = [];
      }
      addEventListener(type, cb) {
        if (type === "abort") this._listeners.push(cb);
      }
      removeEventListener(type, cb) {
        if (type === "abort") this._listeners = this._listeners.filter((l) => l !== cb);
      }
      _fire() {
        if (this.aborted) return;
        this.aborted = true;
        for (const l of this._listeners.slice()) {
          try {
            l({ type: "abort" });
          } catch {
          }
        }
      }
      throwIfAborted() {
        if (this.aborted) throw this.reason || new Error("Aborted");
      }
    }
    globalThis.AbortSignal = PPAbortSignal;
    globalThis.AbortController = class {
      constructor() {
        this.signal = new PPAbortSignal();
      }
      abort(reason) {
        this.signal.reason = reason || new Error("Aborted");
        this.signal._fire();
      }
    };
  }
  if (typeof globalThis.crypto !== "object" || !globalThis.crypto) globalThis.crypto = {};
  if (typeof globalThis.crypto.randomUUID !== "function") {
    globalThis.crypto.randomUUID = function() {
      if (globalThis.host && host.uuid) return host.uuid();
      return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, function(c) {
        const r = (Date.now() + Math.floor(Math.random() * 1e9)) % 16;
        const v = c === "x" ? r : r & 3 | 8;
        return v.toString(16);
      });
    };
  }
  if (typeof globalThis.process !== "object" || !globalThis.process) {
    globalThis.process = { env: {}, platform: "pocket-pi", versions: {} };
  }
})();
