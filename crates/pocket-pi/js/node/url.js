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
export default { URL, URLSearchParams, fileURLToPath, pathToFileURL };
