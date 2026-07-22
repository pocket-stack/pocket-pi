function lookup(host, _o, cb) {
  const c = typeof _o === "function" ? _o : cb;
  c && c(null, "127.0.0.1", 4);
}
function resolve(_h, cb) {
  cb && cb(null, []);
}
const promises = { lookup: async () => ({ address: "127.0.0.1", family: 4 }), resolve: async () => [] };
var dns_default = { lookup, resolve, promises };
export {
  dns_default as default,
  lookup,
  promises,
  resolve
};
