export const setTimeout = globalThis.setTimeout;
export const clearTimeout = globalThis.clearTimeout;
export const setInterval = globalThis.setInterval;
export const clearInterval = globalThis.clearInterval;
export const setImmediate = (fn, ...a) => globalThis.setTimeout(fn, 0, ...a);
export const clearImmediate = globalThis.clearTimeout;
export const promises = { setTimeout: (ms) => new Promise((r) => globalThis.setTimeout(r, ms)) };
export default { setTimeout, clearTimeout, setInterval, clearInterval, setImmediate, clearImmediate, promises };
