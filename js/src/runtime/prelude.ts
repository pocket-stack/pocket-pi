// Pocket Pi prelude — the Web/Node globals pi's agent core expects that classic
// QuickJS does not ship. Evaluated once before the agent bundle. Everything here
// is pure JS on top of QuickJS's ES2023 baseline (Promise, Map, Set, async
// generators, Date, JSON) plus the native `host` namespace mounted by Rust.
//
// Deliberately minimal: byte decoding and HTTPS live in Rust, so no TextDecoder
// or fetch is needed on the JS side — only timers, abort, microtasks, and uuid.

(function () {
  "use strict";

  // Node's `global` alias + setImmediate (many CJS deps assume them).
  if (typeof globalThis.global === "undefined") globalThis.global = globalThis;
  if (typeof globalThis.setImmediate !== "function")
    globalThis.setImmediate = (fn, ...a) => globalThis.setTimeout(fn, 0, ...a);
  if (typeof globalThis.clearImmediate !== "function")
    globalThis.clearImmediate = (id) => globalThis.clearTimeout(id);

  // --- timers, advanced by the host frame pump (globalThis.__catpiTimers) ---
  const timers = new Map();
  let nextTimer = 1;
  globalThis.setTimeout = function (fn, delay, ...args) {
    const id = nextTimer++;
    timers.set(id, { due: Date.now() + (delay || 0), fn, args });
    return id;
  };
  globalThis.clearTimeout = function (id) {
    timers.delete(id);
  };
  globalThis.setInterval = function () {
    throw new Error("setInterval is not supported in Pocket Pi");
  };
  globalThis.clearInterval = function () {};
  globalThis.__catpiTimers = function () {
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

  // --- queueMicrotask (QuickJS drains the job queue after each host frame) ---
  if (typeof globalThis.queueMicrotask !== "function") {
    globalThis.queueMicrotask = function (fn) {
      Promise.resolve().then(fn);
    };
  }

  // --- structuredClone (JSON-safe deep clone is enough for tool arguments) ---
  if (typeof globalThis.structuredClone !== "function") {
    globalThis.structuredClone = function (v) {
      return v === undefined ? undefined : JSON.parse(JSON.stringify(v));
    };
  }

  // --- AbortController / AbortSignal ---
  if (typeof globalThis.AbortController !== "function") {
    class PPAbortSignal {
      constructor() {
        this.aborted = false;
        this.reason = undefined;
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
          } catch {}
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

  // --- crypto.randomUUID (host-backed for real entropy; JS fallback otherwise) ---
  if (typeof globalThis.crypto !== "object" || !globalThis.crypto) globalThis.crypto = {};
  if (typeof globalThis.crypto.randomUUID !== "function") {
    globalThis.crypto.randomUUID = function () {
      if (globalThis.host && host.uuid) return host.uuid();
      // Fallback: not cryptographically strong; fine for tool-call ids.
      return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, function (c) {
        const r = (Date.now() + Math.floor(Math.random() * 1e9)) % 16;
        const v = c === "x" ? r : (r & 0x3) | 0x8;
        return v.toString(16);
      });
    };
  }

  // --- process shim: pi guards most access with typeof process checks ---
  if (typeof globalThis.process !== "object" || !globalThis.process) {
    globalThis.process = { env: {}, platform: "pocket-pi", versions: {} };
  }
})();
