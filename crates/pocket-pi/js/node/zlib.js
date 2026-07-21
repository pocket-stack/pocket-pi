const nope = (n) => () => { throw new Error("zlib." + n + " not supported"); };
export const gzip = nope("gzip"), gunzip = nope("gunzip"), deflate = nope("deflate"), inflate = nope("inflate");
export const gzipSync = nope("gzipSync"), gunzipSync = nope("gunzipSync"), deflateSync = nope("deflateSync"), inflateSync = nope("inflateSync"), brotliCompressSync = nope("brotliCompressSync"), brotliDecompressSync = nope("brotliDecompressSync");
export const constants = {};
export function createGzip() { throw new Error("zlib streams not supported"); }
export default { gzip, gunzip, deflate, inflate, gzipSync, gunzipSync, deflateSync, inflateSync, brotliCompressSync, brotliDecompressSync, constants, createGzip };
