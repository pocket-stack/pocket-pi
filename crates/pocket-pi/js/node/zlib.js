const nope = (n) => () => {
  throw new Error("zlib." + n + " not supported");
};
const gzip = nope("gzip"), gunzip = nope("gunzip"), deflate = nope("deflate"), inflate = nope("inflate");
const gzipSync = nope("gzipSync"), gunzipSync = nope("gunzipSync"), deflateSync = nope("deflateSync"), inflateSync = nope("inflateSync"), brotliCompressSync = nope("brotliCompressSync"), brotliDecompressSync = nope("brotliDecompressSync");
const constants = {};
function createGzip() {
  throw new Error("zlib streams not supported");
}
var zlib_default = { gzip, gunzip, deflate, inflate, gzipSync, gunzipSync, deflateSync, inflateSync, brotliCompressSync, brotliDecompressSync, constants, createGzip };
export {
  brotliCompressSync,
  brotliDecompressSync,
  constants,
  createGzip,
  zlib_default as default,
  deflate,
  deflateSync,
  gunzip,
  gunzipSync,
  gzip,
  gzipSync,
  inflate,
  inflateSync
};
