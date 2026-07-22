// WHATWG Web globals for Pocket Pi — fetch / Response / ReadableStream / Headers
// / URL / atob / btoa / crypto.getRandomValues. Backed by the native HTTP hub
// (raw mode) and driven by the frame pump, so a real npm package that does
// `fetch(...).then(r => r.body.getReader())` works unmodified.

(function () {
  "use strict";
  const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

  // --- Intl (minimal; QuickJS ships none) ---
  if (typeof globalThis.Intl !== "object" || !globalThis.Intl) {
    globalThis.Intl = {
      Segmenter: class {
        constructor() {}
        segment(str) {
          const chars = [...String(str)]; // code-point granularity ≈ grapheme
          return {
            [Symbol.iterator]() {
              let i = 0;
              return {
                next() {
                  return i < chars.length
                    ? { value: { segment: chars[i], index: i++, input: str }, done: false }
                    : { value: undefined, done: true };
                },
              };
            },
          };
        }
      },
      NumberFormat: class { constructor() {} format(n) { return String(n); } formatToParts() { return []; } },
      DateTimeFormat: class { constructor() {} format(d) { return String(d); } formatToParts() { return []; } },
      Collator: class { constructor() {} compare(a, b) { return a < b ? -1 : a > b ? 1 : 0; } },
      getCanonicalLocales: (l) => (Array.isArray(l) ? l : [l]).filter(Boolean),
    };
  }

  // --- Blob / File / FormData (referenced by the OpenAI transport even for
  //     JSON requests; we send JSON bodies, so these just need to exist) ---
  if (typeof globalThis.Blob === "undefined") {
    globalThis.Blob = class Blob {
      constructor(parts = [], opts = {}) {
        this._parts = parts;
        this.type = (opts && opts.type) || "";
        let size = 0;
        for (const p of parts) {
          if (typeof p === "string") size += p.length;
          else if (p && p.byteLength != null) size += p.byteLength;
          else if (p && p.size != null) size += p.size;
        }
        this.size = size;
      }
      async text() {
        return this._parts.map((p) => (typeof p === "string" ? p : "")).join("");
      }
      async arrayBuffer() {
        return new globalThis.TextEncoder().encode(await this.text()).buffer;
      }
      slice() { return new globalThis.Blob(this._parts, { type: this.type }); }
    };
  }
  if (typeof globalThis.File === "undefined") {
    globalThis.File = class File extends globalThis.Blob {
      constructor(parts, name, opts = {}) {
        super(parts, opts);
        this.name = String(name);
        this.lastModified = 0;
      }
    };
  }
  if (typeof globalThis.FormData === "undefined") {
    globalThis.FormData = class FormData {
      constructor() { this._entries = []; }
      append(k, v, filename) { this._entries.push([String(k), v, filename]); }
      set(k, v) {
        this._entries = this._entries.filter((e) => e[0] !== String(k));
        this._entries.push([String(k), v]);
      }
      get(k) { const e = this._entries.find((e) => e[0] === String(k)); return e ? e[1] : null; }
      getAll(k) { return this._entries.filter((e) => e[0] === String(k)).map((e) => e[1]); }
      has(k) { return this._entries.some((e) => e[0] === String(k)); }
      delete(k) { this._entries = this._entries.filter((e) => e[0] !== String(k)); }
      forEach(cb, thisArg) { for (const [k, v] of this._entries) cb.call(thisArg, v, k, this); }
      *entries() { for (const e of this._entries) yield [e[0], e[1]]; }
      *keys() { for (const e of this._entries) yield e[0]; }
      *values() { for (const e of this._entries) yield e[1]; }
      [Symbol.iterator]() { return this.entries(); }
    };
  }

  // --- Web event/message globals (undici's webidl references these as globals
  //     during module init; we don't do real message passing, so they're stubs) ---
  if (typeof globalThis.EventTarget !== "function") {
    globalThis.EventTarget = class EventTarget {
      constructor() { this.__l = {}; }
      addEventListener(t, cb) { (this.__l[t] ||= []).push(cb); }
      removeEventListener(t, cb) { this.__l[t] = (this.__l[t] || []).filter((f) => f !== cb); }
      dispatchEvent(e) { for (const cb of this.__l[e && e.type] || []) { try { cb(e); } catch {} } return true; }
    };
  }
  if (typeof globalThis.Event !== "function") {
    globalThis.Event = class Event {
      constructor(type, init = {}) { this.type = type; this.bubbles = !!init.bubbles; this.defaultPrevented = false; }
      preventDefault() { this.defaultPrevented = true; }
      stopPropagation() {}
      stopImmediatePropagation() {}
    };
  }
  if (typeof globalThis.CustomEvent !== "function") {
    globalThis.CustomEvent = class CustomEvent extends globalThis.Event {
      constructor(type, init = {}) { super(type, init); this.detail = init.detail; }
    };
  }
  if (typeof globalThis.MessagePort !== "function") {
    globalThis.MessagePort = class MessagePort extends globalThis.EventTarget {
      postMessage() {} start() {} close() {}
      on() { return this; } once() { return this; }
      ref() { return this; } unref() { return this; }
    };
  }
  if (typeof globalThis.MessageChannel !== "function") {
    globalThis.MessageChannel = class MessageChannel {
      constructor() { this.port1 = new globalThis.MessagePort(); this.port2 = new globalThis.MessagePort(); }
    };
  }
  if (typeof globalThis.DOMException !== "function") {
    globalThis.DOMException = class DOMException extends Error {
      constructor(message, name) { super(message); this.name = name || "Error"; }
    };
  }

  // --- TextEncoder / TextDecoder (UTF-8) ---
  if (typeof globalThis.TextEncoder !== "function") {
    globalThis.TextEncoder = class TextEncoder {
      get encoding() { return "utf-8"; }
      encode(str) {
        str = String(str);
        const out = [];
        for (let i = 0; i < str.length; i++) {
          let c = str.charCodeAt(i);
          if (c < 0x80) out.push(c);
          else if (c < 0x800) out.push(0xc0 | (c >> 6), 0x80 | (c & 0x3f));
          else if (c >= 0xd800 && c <= 0xdbff) {
            const c2 = str.charCodeAt(++i);
            c = 0x10000 + ((c & 0x3ff) << 10) + (c2 & 0x3ff);
            out.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 0x3f), 0x80 | ((c >> 6) & 0x3f), 0x80 | (c & 0x3f));
          } else out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 0x3f), 0x80 | (c & 0x3f));
        }
        return new Uint8Array(out);
      }
    };
  }
  if (typeof globalThis.TextDecoder !== "function") {
    globalThis.TextDecoder = class TextDecoder {
      constructor(label) { this._enc = label || "utf-8"; }
      get encoding() { return "utf-8"; }
      decode(input) {
        if (!input) return "";
        const bytes = input instanceof Uint8Array ? input : new Uint8Array(input.buffer || input);
        let out = "", i = 0;
        while (i < bytes.length) {
          let c = bytes[i++];
          if (c < 0x80) out += String.fromCharCode(c);
          else if (c >= 0xc0 && c < 0xe0) out += String.fromCharCode(((c & 0x1f) << 6) | (bytes[i++] & 0x3f));
          else if (c >= 0xe0 && c < 0xf0) out += String.fromCharCode(((c & 0xf) << 12) | ((bytes[i++] & 0x3f) << 6) | (bytes[i++] & 0x3f));
          else {
            const cp = ((c & 7) << 18) | ((bytes[i++] & 0x3f) << 12) | ((bytes[i++] & 0x3f) << 6) | (bytes[i++] & 0x3f);
            const off = cp - 0x10000;
            out += String.fromCharCode(0xd800 + (off >> 10), 0xdc00 + (off & 0x3ff));
          }
        }
        return out;
      }
    };
  }

  // --- atob / btoa ---
  if (typeof globalThis.btoa !== "function") {
    globalThis.btoa = function (bin) {
      let out = "";
      for (let i = 0; i < bin.length; i += 3) {
        const a = bin.charCodeAt(i), b = bin.charCodeAt(i + 1), c = bin.charCodeAt(i + 2);
        const n = (a << 16) | ((isNaN(b) ? 0 : b) << 8) | (isNaN(c) ? 0 : c);
        out += B64[(n >> 18) & 63] + B64[(n >> 12) & 63] + (isNaN(b) ? "=" : B64[(n >> 6) & 63]) + (isNaN(c) ? "=" : B64[n & 63]);
      }
      return out;
    };
  }
  if (typeof globalThis.atob !== "function") {
    globalThis.atob = function (b64) {
      b64 = String(b64).replace(/[^A-Za-z0-9+/]/g, "");
      let out = "";
      for (let i = 0; i < b64.length; i += 4) {
        const n = (B64.indexOf(b64[i]) << 18) | (B64.indexOf(b64[i + 1]) << 12) | (B64.indexOf(b64[i + 2] || "A") << 6) | B64.indexOf(b64[i + 3] || "A");
        out += String.fromCharCode((n >> 16) & 255);
        if (b64[i + 2] && b64[i + 2] !== "=") out += String.fromCharCode((n >> 8) & 255);
        if (b64[i + 3] && b64[i + 3] !== "=") out += String.fromCharCode(n & 255);
      }
      return out;
    };
  }
  function b64ToBytes(b64) {
    const bin = globalThis.atob(b64);
    const u = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) u[i] = bin.charCodeAt(i);
    return u;
  }

  // --- crypto.getRandomValues (non-cryptographic fallback; fine for request ids) ---
  if (typeof globalThis.crypto !== "object" || !globalThis.crypto) globalThis.crypto = {};
  if (typeof globalThis.crypto.getRandomValues !== "function") {
    globalThis.crypto.getRandomValues = function (arr) {
      for (let i = 0; i < arr.length; i++) arr[i] = Math.floor(Math.random() * 256);
      return arr;
    };
  }

  // --- Headers ---
  if (typeof globalThis.Headers !== "function") {
    globalThis.Headers = class Headers {
      constructor(init) {
        this._m = new Map();
        if (init) {
          const entries = init instanceof Headers ? init.entries() : Array.isArray(init) ? init : Object.entries(init);
          for (const [k, v] of entries) this.set(k, v);
        }
      }
      set(k, v) { this._m.set(String(k).toLowerCase(), String(v)); }
      append(k, v) { const p = this._m.get(String(k).toLowerCase()); this.set(k, p ? p + ", " + v : v); }
      get(k) { const v = this._m.get(String(k).toLowerCase()); return v === undefined ? null : v; }
      has(k) { return this._m.has(String(k).toLowerCase()); }
      delete(k) { this._m.delete(String(k).toLowerCase()); }
      forEach(fn) { this._m.forEach((v, k) => fn(v, k, this)); }
      entries() { return this._m.entries(); }
      keys() { return this._m.keys(); }
      values() { return this._m.values(); }
      [Symbol.iterator]() { return this._m.entries(); }
    };
  }

  // --- URL / URLSearchParams (minimal but base-aware) ---
  if (typeof globalThis.URLSearchParams !== "function") {
    globalThis.URLSearchParams = class URLSearchParams {
      constructor(init) {
        this._p = [];
        if (typeof init === "string") {
          for (const pair of init.replace(/^\?/, "").split("&")) {
            if (!pair) continue;
            const i = pair.indexOf("=");
            this._p.push(i < 0 ? [decodeURIComponent(pair), ""] : [decodeURIComponent(pair.slice(0, i)), decodeURIComponent(pair.slice(i + 1))]);
          }
        } else if (init) for (const [k, v] of Object.entries(init)) this._p.push([k, String(v)]);
      }
      get(k) { const e = this._p.find((x) => x[0] === k); return e ? e[1] : null; }
      set(k, v) { const e = this._p.find((x) => x[0] === k); if (e) e[1] = String(v); else this._p.push([k, String(v)]); }
      append(k, v) { this._p.push([k, String(v)]); }
      has(k) { return this._p.some((x) => x[0] === k); }
      delete(k) { this._p = this._p.filter((x) => x[0] !== k); }
      forEach(fn) { for (const [k, v] of this._p) fn(v, k, this); }
      toString() { return this._p.map(([k, v]) => encodeURIComponent(k) + "=" + encodeURIComponent(v)).join("&"); }
      [Symbol.iterator]() { return this._p[Symbol.iterator](); }
    };
  }
  if (typeof globalThis.URL !== "function") {
    globalThis.URL = class URL {
      constructor(url, base) {
        let full = String(url);
        if (base && !/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(full)) {
          const b = String(base).replace(/\/+$/, "");
          full = full.startsWith("/") ? originOf(b) + full : b + "/" + full;
        }
        const m = /^([a-zA-Z][a-zA-Z0-9+.-]*:)\/\/([^/?#]*)([^?#]*)(\?[^#]*)?(#.*)?$/.exec(full);
        if (!m) throw new TypeError("Invalid URL: " + full);
        this.protocol = m[1];
        this.host = m[2];
        this.hostname = m[2].split(":")[0];
        this.port = m[2].split(":")[1] || "";
        this.pathname = m[3] || "/";
        this.search = m[4] || "";
        this.hash = m[5] || "";
        this.searchParams = new globalThis.URLSearchParams(this.search);
        this.origin = this.protocol + "//" + this.host;
      }
      get href() { return this.origin + this.pathname + (this.searchParams.toString() ? "?" + this.searchParams.toString() : "") + this.hash; }
      toString() { return this.href; }
    };
    function originOf(u) { const m = /^([a-zA-Z]+:\/\/[^/?#]*)/.exec(u); return m ? m[1] : u; }
  }

  // --- ReadableStream (byte chunks pushed by the fetch pump) ---
  class PPReadableStream {
    constructor() {
      this._queue = [];
      this._closed = false;
      this._error = null;
      this._waiters = [];
    }
    _enqueue(chunk) { this._queue.push(chunk); this._flush(); }
    _close() { this._closed = true; this._flush(); }
    _fail(e) { this._error = e; this._flush(); }
    _flush() {
      while (this._waiters.length) {
        if (this._queue.length) this._waiters.shift().resolve({ value: this._queue.shift(), done: false });
        else if (this._error) this._waiters.shift().reject(this._error);
        else if (this._closed) this._waiters.shift().resolve({ value: undefined, done: true });
        else break;
      }
    }
    getReader() {
      const self = this;
      return {
        read() { return new Promise((resolve, reject) => { self._waiters.push({ resolve, reject }); self._flush(); }); },
        releaseLock() {},
        cancel() { self._closed = true; return Promise.resolve(); },
      };
    }
    [Symbol.asyncIterator]() {
      const reader = this.getReader();
      return { next: () => reader.read(), return: () => { reader.cancel(); return Promise.resolve({ done: true }); }, [Symbol.asyncIterator]() { return this; } };
    }
  }
  globalThis.ReadableStream = globalThis.ReadableStream || PPReadableStream;

  // --- Response ---
  class Response {
    constructor(body, init) {
      init = init || {};
      this.status = init.status ?? 200;
      this.statusText = init.statusText ?? "";
      this.ok = this.status >= 200 && this.status < 300;
      this.headers = init.headers instanceof globalThis.Headers ? init.headers : new globalThis.Headers(init.headers || {});
      this.url = init.url || "";
      this.body = body || null;
      this._bodyUsed = false;
    }
    get bodyUsed() { return this._bodyUsed; }
    async _consume() {
      this._bodyUsed = true;
      if (!this.body) return new Uint8Array(0);
      if (this.body instanceof Uint8Array) return this.body;
      const reader = this.body.getReader();
      const parts = [];
      let total = 0;
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        parts.push(value);
        total += value.length;
      }
      const out = new Uint8Array(total);
      let off = 0;
      for (const p of parts) { out.set(p, off); off += p.length; }
      return out;
    }
    async arrayBuffer() { return (await this._consume()).buffer; }
    async text() { return new TextDecoder().decode(await this._consume()); }
    async json() { return JSON.parse(await this.text()); }
    clone() { return new Response(this.body, { status: this.status, statusText: this.statusText, headers: this.headers, url: this.url }); }
  }
  globalThis.Response = globalThis.Response || Response;

  // --- fetch ---
  const fetchTurns = new Map();

  globalThis.fetch = function (input, init) {
    init = init || {};
    const url = typeof input === "string" ? input : input.url;
    const headers = {};
    if (init.headers) {
      const h = init.headers instanceof globalThis.Headers ? init.headers.entries() : Array.isArray(init.headers) ? init.headers : Object.entries(init.headers);
      for (const [k, v] of h) headers[k] = String(v);
    }
    let body = init.body;
    if (body && typeof body !== "string") {
      if (body instanceof Uint8Array) body = new TextDecoder().decode(body);
      else body = String(body);
    }
    const request = { url, method: init.method || "GET", headers, body, raw: true };

    return new Promise((resolve, reject) => {
      let turnId;
      try {
        turnId = host.http.start(JSON.stringify(request));
      } catch (e) {
        reject(new Error(String(e && e.message ? e.message : e)));
        return;
      }
      const turn = { resolve, reject, stream: null, resolved: false, url };
      fetchTurns.set(turnId, turn);
      if (init.signal) {
        init.signal.addEventListener("abort", () => {
          try { host.http.cancel(turnId); } catch {}
          if (!turn.resolved) reject(new Error("aborted"));
          else if (turn.stream) turn.stream._fail(new Error("aborted"));
          fetchTurns.delete(turnId);
        });
      }
    });
  };

  // Drained once per host frame (added to the pump).
  globalThis.__catpiFetchPump = function () {
    if (fetchTurns.size === 0) return;
    for (const [turnId, turn] of fetchTurns) {
      let out;
      try {
        out = JSON.parse(host.http.drain(turnId));
      } catch (e) {
        if (!turn.resolved) turn.reject(new Error(String(e)));
        else if (turn.stream) turn.stream._fail(new Error(String(e)));
        fetchTurns.delete(turnId);
        continue;
      }
      for (const line of out.lines) {
        let msg;
        try { msg = JSON.parse(line); } catch { continue; }
        if (msg.__meta) {
          turn.stream = new PPReadableStream();
          const resp = new Response(turn.stream, {
            status: msg.__meta.status,
            statusText: msg.__meta.statusText,
            headers: msg.__meta.headers,
            url: turn.url,
          });
          turn.resolved = true;
          turn.resolve(resp);
        } else if (msg.__chunk != null && turn.stream) {
          turn.stream._enqueue(b64ToBytes(msg.__chunk));
        }
      }
      if (out.error) {
        if (!turn.resolved) turn.reject(new Error(out.error));
        else if (turn.stream) turn.stream._fail(new Error(out.error));
        fetchTurns.delete(turnId);
      } else if (out.done) {
        if (turn.stream) turn.stream._close();
        else if (!turn.resolved) turn.reject(new Error("no response"));
        fetchTurns.delete(turnId);
      }
    }
  };
})();
