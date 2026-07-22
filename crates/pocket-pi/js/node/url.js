// node:url
export const URL = globalThis.URL;
export const URLSearchParams = globalThis.URLSearchParams;
export function fileURLToPath(url) {
  let s = typeof url === "string" ? url : url.href;
  if (s.startsWith("file://")) s = s.slice(7);
  return decodeURIComponent(s);
}
export function pathToFileURL(path) {
  return new globalThis.URL("file://" + encodeURI(path));
}
// Legacy url.parse/format/resolve.
export function parse(str) {
  try {
    const u = new globalThis.URL(str);
    return { href: u.href, protocol: u.protocol, host: u.host, hostname: u.hostname, port: u.port, pathname: u.pathname, search: u.search, hash: u.hash, query: u.search.replace(/^\?/, "") };
  } catch {
    return { href: str, pathname: str, protocol: null, host: null, hostname: null, port: "", search: "", hash: "", query: "" };
  }
}
export function format(obj) {
  if (typeof obj === "string") return obj;
  if (obj && typeof obj.href === "string" && obj.protocol) return obj.href;
  const proto = obj.protocol ? (obj.protocol.endsWith(":") ? obj.protocol : obj.protocol + ":") : "";
  const host = obj.host || (obj.hostname ? obj.hostname + (obj.port ? ":" + obj.port : "") : "");
  const search = obj.search || (obj.query ? "?" + (typeof obj.query === "string" ? obj.query : new globalThis.URLSearchParams(obj.query).toString()) : "");
  return (proto ? proto + "//" : "") + host + (obj.pathname || "") + search + (obj.hash || "");
}
export function resolve(from, to) {
  try { return new globalThis.URL(to, from).href; } catch { return to; }
}
export const Url = globalThis.URL;
export function domainToASCII(d) { return d; }
export function domainToUnicode(d) { return d; }
export default { URL, URLSearchParams, fileURLToPath, pathToFileURL, parse, format, resolve, Url, domainToASCII, domainToUnicode };
