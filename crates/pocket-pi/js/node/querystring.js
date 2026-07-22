function parse(str) {
  const o = {};
  for (const p of String(str).split("&")) {
    if (!p) continue;
    const i = p.indexOf("=");
    const k = decodeURIComponent(i < 0 ? p : p.slice(0, i));
    const v = i < 0 ? "" : decodeURIComponent(p.slice(i + 1));
    o[k] = v;
  }
  return o;
}
function stringify(obj) {
  return Object.entries(obj || {}).map(([k, v]) => encodeURIComponent(k) + "=" + encodeURIComponent(v)).join("&");
}
const decode = parse, encode = stringify;
var querystring_default = { parse, stringify, decode, encode };
export {
  decode,
  querystring_default as default,
  encode,
  parse,
  stringify
};
