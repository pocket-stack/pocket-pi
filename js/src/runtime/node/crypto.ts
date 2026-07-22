// node:crypto — the subset pi touches. Digests use a fast non-cryptographic hash
// (FNV-1a) — fine for cache keys / ids; NOT for security.
function fnv(str) {
  let h = 0xcbf29ce484222325n;
  const bytes = globalThis.Buffer ? globalThis.Buffer.from(str) : new TextEncoder().encode(str);
  for (const b of bytes) { h ^= BigInt(b); h = (h * 0x100000001b3n) & 0xffffffffffffffffn; }
  return h;
}
export function randomBytes(n) {
  const b = globalThis.Buffer ? globalThis.Buffer.alloc(n) : new Uint8Array(n);
  for (let i = 0; i < n; i++) b[i] = Math.floor(Math.random() * 256);
  return b;
}
export function randomUUID() { return globalThis.crypto.randomUUID(); }
export function randomFillSync(buf) { for (let i = 0; i < buf.length; i++) buf[i] = Math.floor(Math.random() * 256); return buf; }
export function createHash(_algo) {
  let data = "";
  return {
    update(chunk) { data += typeof chunk === "string" ? chunk : globalThis.Buffer.from(chunk).toString(); return this; },
    digest(enc) {
      // 256-bit-ish by concatenating four FNV rounds with salts.
      let hex = "";
      for (let i = 0; i < 4; i++) hex += fnv(i + ":" + data).toString(16).padStart(16, "0");
      if (enc === "hex") return hex;
      const bytes = globalThis.Buffer.from(hex, "hex");
      return enc ? bytes.toString(enc) : bytes;
    },
  };
}
export function createHmac(algo, key) { const h = createHash(algo); h.update(String(key) + ":"); return h; }
export function getHashes() { return ["sha1", "sha256", "sha384", "sha512", "md5"]; }
export function getCiphers() { return []; }
export function timingSafeEqual(a, b) {
  if (!a || !b || a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i];
  return diff === 0;
}
export const constants = {};
export const webcrypto = globalThis.crypto;
export function getRandomValues(a) { return globalThis.crypto.getRandomValues(a); }
export default { randomBytes, randomUUID, randomFillSync, createHash, createHmac, getHashes, getCiphers, timingSafeEqual, constants, webcrypto, getRandomValues };
