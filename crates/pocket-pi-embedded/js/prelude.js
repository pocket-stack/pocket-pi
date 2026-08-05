(() => {
  if (typeof globalThis.queueMicrotask !== "function") {
    globalThis.queueMicrotask = (fn) => Promise.resolve().then(fn);
  }
  if (typeof globalThis.structuredClone !== "function") {
    globalThis.structuredClone = (value) =>
      value === undefined ? undefined : JSON.parse(JSON.stringify(value));
  }
  if (typeof globalThis.performance !== "object") {
    globalThis.performance = { now: () => Date.now() };
  }
  if (typeof globalThis.AbortController !== "function") {
    class Signal {
      constructor() {
        this.aborted = false;
        this.reason = undefined;
        this.listeners = [];
      }
      addEventListener(type, listener) {
        if (type === "abort") this.listeners.push(listener);
      }
      removeEventListener(type, listener) {
        if (type === "abort") this.listeners = this.listeners.filter((item) => item !== listener);
      }
    }
    globalThis.AbortController = class {
      constructor() { this.signal = new Signal(); }
      abort(reason) {
        if (this.signal.aborted) return;
        this.signal.aborted = true;
        this.signal.reason = reason;
        for (const listener of this.signal.listeners.slice()) listener();
      }
    };
  }
  if (typeof globalThis.TextEncoder !== "function") {
    globalThis.TextEncoder = class {
      encode(input = "") {
        const text = unescape(encodeURIComponent(String(input)));
        const bytes = new Uint8Array(text.length);
        for (let i = 0; i < text.length; i += 1) bytes[i] = text.charCodeAt(i);
        return bytes;
      }
    };
  }
  if (typeof globalThis.TextDecoder !== "function") {
    globalThis.TextDecoder = class {
      decode(input) {
        if (!input) return "";
        const bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
        let binary = "";
        for (let i = 0; i < bytes.length; i += 1) binary += String.fromCharCode(bytes[i]);
        return decodeURIComponent(escape(binary));
      }
    };
  }
  if (typeof globalThis.URL !== "function") {
    globalThis.URL = class {
      constructor(input, base = "") {
        const value = String(input);
        const root = String(base).replace(/[^/]*$/, "");
        this.href = /^[a-z][a-z0-9+.-]*:/i.test(value) ? value : root + value;
      }
      toString() { return this.href; }
      toJSON() { return this.href; }
    };
  }
})();
