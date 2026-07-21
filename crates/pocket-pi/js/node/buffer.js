// node:buffer — a functional Buffer subset over Uint8Array. Covers the encodings
// pi + its deps actually touch (utf8, base64, hex, latin1) and the common ops.
const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

function utf8ToBytes(str) {
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
  return out;
}
function bytesToUtf8(bytes) {
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
function toBase64(bytes) {
  let out = "";
  for (let i = 0; i < bytes.length; i += 3) {
    const b0 = bytes[i], b1 = bytes[i + 1], b2 = bytes[i + 2];
    const n = (b0 << 16) | ((b1 || 0) << 8) | (b2 || 0);
    out += B64[(n >> 18) & 63] + B64[(n >> 12) & 63] + (i + 1 < bytes.length ? B64[(n >> 6) & 63] : "=") + (i + 2 < bytes.length ? B64[n & 63] : "=");
  }
  return out;
}
function fromBase64(str) {
  str = str.replace(/[^A-Za-z0-9+/]/g, "");
  const out = [];
  for (let i = 0; i < str.length; i += 4) {
    const n = (B64.indexOf(str[i]) << 18) | (B64.indexOf(str[i + 1]) << 12) | (B64.indexOf(str[i + 2] || "A") << 6) | B64.indexOf(str[i + 3] || "A");
    out.push((n >> 16) & 255);
    if (str[i + 2] && str[i + 2] !== "=") out.push((n >> 8) & 255);
    if (str[i + 3] && str[i + 3] !== "=") out.push(n & 255);
  }
  return out;
}

export class Buffer extends Uint8Array {
  static from(value, encoding) {
    if (typeof value === "string") {
      if (encoding === "base64") return new Buffer(fromBase64(value));
      if (encoding === "hex") {
        const out = [];
        for (let i = 0; i < value.length; i += 2) out.push(parseInt(value.substr(i, 2), 16));
        return new Buffer(out);
      }
      if (encoding === "latin1" || encoding === "binary") {
        const out = new Buffer(value.length);
        for (let i = 0; i < value.length; i++) out[i] = value.charCodeAt(i) & 255;
        return out;
      }
      return new Buffer(utf8ToBytes(value));
    }
    if (value instanceof Uint8Array || Array.isArray(value)) return new Buffer(value);
    if (value instanceof ArrayBuffer) return new Buffer(new Uint8Array(value));
    return new Buffer(0);
  }
  static alloc(size, fill) {
    const b = new Buffer(size);
    if (fill != null) b.fill(typeof fill === "string" ? fill.charCodeAt(0) : fill);
    return b;
  }
  static allocUnsafe(size) { return new Buffer(size); }
  static isBuffer(v) { return v instanceof Buffer; }
  static concat(list, length) {
    let total = length ?? list.reduce((n, b) => n + b.length, 0);
    const out = new Buffer(total);
    let off = 0;
    for (const b of list) { out.set(b.subarray(0, Math.min(b.length, total - off)), off); off += b.length; if (off >= total) break; }
    return out;
  }
  static byteLength(str, encoding) {
    return Buffer.from(str, encoding).length;
  }
  toString(encoding, start, end) {
    const view = this.subarray(start || 0, end == null ? this.length : end);
    if (encoding === "base64") return toBase64(view);
    if (encoding === "hex") return Array.from(view, (b) => b.toString(16).padStart(2, "0")).join("");
    if (encoding === "latin1" || encoding === "binary") return Array.from(view, (b) => String.fromCharCode(b)).join("");
    return bytesToUtf8(view);
  }
  toJSON() { return { type: "Buffer", data: Array.from(this) }; }
  equals(other) { return this.length === other.length && this.every((v, i) => v === other[i]); }
  write(str, offset = 0, length, encoding) {
    const src = Buffer.from(str, typeof length === "string" ? length : encoding);
    const n = Math.min(src.length, this.length - offset, typeof length === "number" ? length : Infinity);
    this.set(src.subarray(0, n), offset);
    return n;
  }
  slice(a, b) { return new Buffer(this.subarray(a, b)); }
}

export const SlowBuffer = Buffer;
export const constants = { MAX_LENGTH: 0x7fffffff, MAX_STRING_LENGTH: 0x1fffffff };
export default { Buffer, SlowBuffer, constants };
