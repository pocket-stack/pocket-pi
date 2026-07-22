function fnv(str) {
  let h = 0xcbf29ce484222325n;
  const bytes = globalThis.Buffer ? globalThis.Buffer.from(str) : new TextEncoder().encode(str);
  for (const b of bytes) {
    h ^= BigInt(b);
    h = h * 0x100000001b3n & 0xffffffffffffffffn;
  }
  return h;
}
function randomBytes(n) {
  const b = globalThis.Buffer ? globalThis.Buffer.alloc(n) : new Uint8Array(n);
  for (let i = 0; i < n; i++) b[i] = Math.floor(Math.random() * 256);
  return b;
}
function randomUUID() {
  return globalThis.crypto.randomUUID();
}
function randomFillSync(buf) {
  for (let i = 0; i < buf.length; i++) buf[i] = Math.floor(Math.random() * 256);
  return buf;
}
function createHash(_algo) {
  let data = "";
  return {
    update(chunk) {
      data += typeof chunk === "string" ? chunk : globalThis.Buffer.from(chunk).toString();
      return this;
    },
    digest(enc) {
      let hex = "";
      for (let i = 0; i < 4; i++) hex += fnv(i + ":" + data).toString(16).padStart(16, "0");
      if (enc === "hex") return hex;
      const bytes = globalThis.Buffer.from(hex, "hex");
      return enc ? bytes.toString(enc) : bytes;
    }
  };
}
function createHmac(algo, key) {
  const h = createHash(algo);
  h.update(String(key) + ":");
  return h;
}
function getHashes() {
  return ["sha1", "sha256", "sha384", "sha512", "md5"];
}
function getCiphers() {
  return [];
}
function timingSafeEqual(a, b) {
  if (!a || !b || a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i];
  return diff === 0;
}
const constants = {};
const webcrypto = globalThis.crypto;
function getRandomValues(a) {
  return globalThis.crypto.getRandomValues(a);
}
var crypto_default = { randomBytes, randomUUID, randomFillSync, createHash, createHmac, getHashes, getCiphers, timingSafeEqual, constants, webcrypto, getRandomValues };
export {
  constants,
  createHash,
  createHmac,
  crypto_default as default,
  getCiphers,
  getHashes,
  getRandomValues,
  randomBytes,
  randomFillSync,
  randomUUID,
  timingSafeEqual,
  webcrypto
};
