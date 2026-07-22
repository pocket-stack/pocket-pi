const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
function utf8ToBytes(str) {
  const out = [];
  for (let i = 0; i < str.length; i++) {
    let c = str.charCodeAt(i);
    if (c < 128) out.push(c);
    else if (c < 2048) out.push(192 | c >> 6, 128 | c & 63);
    else if (c >= 55296 && c <= 56319) {
      const c2 = str.charCodeAt(++i);
      c = 65536 + ((c & 1023) << 10) + (c2 & 1023);
      out.push(240 | c >> 18, 128 | c >> 12 & 63, 128 | c >> 6 & 63, 128 | c & 63);
    } else out.push(224 | c >> 12, 128 | c >> 6 & 63, 128 | c & 63);
  }
  return out;
}
function bytesToUtf8(bytes) {
  let out = "", i = 0;
  while (i < bytes.length) {
    let c = bytes[i++];
    if (c < 128) out += String.fromCharCode(c);
    else if (c >= 192 && c < 224) out += String.fromCharCode((c & 31) << 6 | bytes[i++] & 63);
    else if (c >= 224 && c < 240) out += String.fromCharCode((c & 15) << 12 | (bytes[i++] & 63) << 6 | bytes[i++] & 63);
    else {
      const cp = (c & 7) << 18 | (bytes[i++] & 63) << 12 | (bytes[i++] & 63) << 6 | bytes[i++] & 63;
      const off = cp - 65536;
      out += String.fromCharCode(55296 + (off >> 10), 56320 + (off & 1023));
    }
  }
  return out;
}
function toBase64(bytes) {
  let out = "";
  for (let i = 0; i < bytes.length; i += 3) {
    const b0 = bytes[i], b1 = bytes[i + 1], b2 = bytes[i + 2];
    const n = b0 << 16 | (b1 || 0) << 8 | (b2 || 0);
    out += B64[n >> 18 & 63] + B64[n >> 12 & 63] + (i + 1 < bytes.length ? B64[n >> 6 & 63] : "=") + (i + 2 < bytes.length ? B64[n & 63] : "=");
  }
  return out;
}
function fromBase64(str) {
  str = str.replace(/[^A-Za-z0-9+/]/g, "");
  const out = [];
  for (let i = 0; i < str.length; i += 4) {
    const n = B64.indexOf(str[i]) << 18 | B64.indexOf(str[i + 1]) << 12 | B64.indexOf(str[i + 2] || "A") << 6 | B64.indexOf(str[i + 3] || "A");
    out.push(n >> 16 & 255);
    if (str[i + 2] && str[i + 2] !== "=") out.push(n >> 8 & 255);
    if (str[i + 3] && str[i + 3] !== "=") out.push(n & 255);
  }
  return out;
}
class Buffer extends Uint8Array {
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
  static allocUnsafe(size) {
    return new Buffer(size);
  }
  static isBuffer(v) {
    return v instanceof Buffer;
  }
  static concat(list, length) {
    let total = length ?? list.reduce((n, b) => n + b.length, 0);
    const out = new Buffer(total);
    let off = 0;
    for (const b of list) {
      out.set(b.subarray(0, Math.min(b.length, total - off)), off);
      off += b.length;
      if (off >= total) break;
    }
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
  toJSON() {
    return { type: "Buffer", data: Array.from(this) };
  }
  equals(other) {
    return this.length === other.length && this.every((v, i) => v === other[i]);
  }
  write(str, offset = 0, length, encoding) {
    const src = Buffer.from(str, typeof length === "string" ? length : encoding);
    const n = Math.min(src.length, this.length - offset, typeof length === "number" ? length : Infinity);
    this.set(src.subarray(0, n), offset);
    return n;
  }
  slice(a, b) {
    return new Buffer(this.subarray(a, b));
  }
}
const SlowBuffer = Buffer;
const constants = { MAX_LENGTH: 2147483647, MAX_STRING_LENGTH: 536870911 };
var buffer_default = { Buffer, SlowBuffer, constants };
export {
  Buffer,
  SlowBuffer,
  constants,
  buffer_default as default
};
