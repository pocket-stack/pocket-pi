(() => {
  const ops = globalThis.net;
  if (!ops || typeof ops.start !== "function" || typeof ops.poll !== "function" ||
      typeof ops.take !== "function" || typeof ops.cancel !== "function") {
    throw new Error("Pocket Pi NET SDK: native net surface is unavailable");
  }
  if (globalThis.fetch) throw new Error("Pocket Pi NET SDK: fetch is already installed");

  const pending = new Map();

  function encode(text) {
    const output = [];
    for (const character of text) {
      const code = character.codePointAt(0);
      if (code < 0x80) output.push(code);
      else if (code < 0x800) output.push(0xc0 | code >> 6, 0x80 | code & 0x3f);
      else if (code < 0x10000) output.push(0xe0 | code >> 12, 0x80 | code >> 6 & 0x3f, 0x80 | code & 0x3f);
      else output.push(0xf0 | code >> 18, 0x80 | code >> 12 & 0x3f, 0x80 | code >> 6 & 0x3f, 0x80 | code & 0x3f);
    }
    return new Uint8Array(output);
  }

  function decode(bytes) {
    let output = "";
    for (let index = 0; index < bytes.length;) {
      const first = bytes[index++];
      if (first < 0x80) {
        output += String.fromCharCode(first);
        continue;
      }
      const length = first < 0xe0 ? 1 : first < 0xf0 ? 2 : first < 0xf8 ? 3 : -1;
      if (length < 0 || index + length > bytes.length) throw new Error("fetch response is not valid UTF-8");
      let code = first & (length === 1 ? 0x1f : length === 2 ? 0x0f : 0x07);
      for (let offset = 0; offset < length; offset += 1) {
        const next = bytes[index++];
        if ((next & 0xc0) !== 0x80) throw new Error("fetch response is not valid UTF-8");
        code = code << 6 | next & 0x3f;
      }
      if (code <= 0xffff) output += String.fromCharCode(code);
      else {
        code -= 0x10000;
        output += String.fromCharCode(0xd800 | code >> 10, 0xdc00 | code & 0x3ff);
      }
    }
    return output;
  }

  function response(event, buffer) {
    const data = new Uint8Array(buffer);
    return Object.freeze({
      status: event.status,
      url: event.url,
      headers: Object.freeze({ ...event.headers }),
      ok: event.status >= 200 && event.status < 300,
      bytes: async () => data.slice(),
      arrayBuffer: async () => data.slice().buffer,
      text: async () => decode(data),
      json: async () => JSON.parse(decode(data)),
    });
  }

  function pump() {
    const line = ops.poll();
    if (line === undefined) return;
    const events = JSON.parse(line);
    if (!Array.isArray(events)) throw new Error("Pocket Pi NET SDK: malformed event batch");
    for (const event of events) {
      const request = pending.get(event?.h);
      if (!request) continue;
      pending.delete(event.h);
      if (event.t === "error") {
        request.reject(new Error(event.message || event.code || "fetch failed"));
        continue;
      }
      if (event.t !== "done" || !Number.isInteger(event.bytes) || event.bytes < 0) {
        ops.cancel(event.h);
        request.reject(new Error("fetch received a malformed response"));
        continue;
      }
      const body = new ArrayBuffer(event.bytes);
      if (ops.take(event.h, body) !== event.bytes) {
        ops.cancel(event.h);
        request.reject(new Error("fetch response transfer failed"));
      } else {
        request.resolve(response(event, body));
      }
    }
  }

  function requestBody(value) {
    if (value === undefined) return new Uint8Array(0);
    if (typeof value === "string") return encode(value);
    if (value instanceof Uint8Array) return value.slice();
    if (value instanceof ArrayBuffer) return new Uint8Array(value.slice(0));
    throw new Error("fetch body must be a string or bytes");
  }

  function fetch(url, options = {}) {
    try {
      const headers = {};
      for (const [name, value] of Object.entries(options.headers ?? {})) headers[name.toLowerCase()] = String(value);
      const body = requestBody(options.body);
      const handle = ops.start(JSON.stringify({
        url: String(url),
        method: String(options.method ?? "GET").toUpperCase(),
        headers,
        timeoutMs: options.timeoutMs ?? 30000,
        maxBytes: options.maxBytes ?? 128 * 1024,
      }), body.buffer);
      if (!Number.isInteger(handle) || handle < 0) return Promise.reject(new Error(ops.lastError() || "fetch refused"));
      return new Promise((resolve, reject) => pending.set(handle, { resolve, reject }));
    } catch (error) {
      return Promise.reject(error);
    }
  }

  PocketPiSystem.registerActionPump(pump);
  Object.defineProperty(globalThis, "fetch", { value: fetch, enumerable: true });
})();
