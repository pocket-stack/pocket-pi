export function lookup(host, _o, cb) { const c = typeof _o === "function" ? _o : cb; c && c(null, "127.0.0.1", 4); }
export function resolve(_h, cb) { cb && cb(null, []); }
export const promises = { lookup: async () => ({ address: "127.0.0.1", family: 4 }), resolve: async () => [] };
export default { lookup, resolve, promises };
