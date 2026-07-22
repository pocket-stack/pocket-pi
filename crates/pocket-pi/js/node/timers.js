const setTimeout = globalThis.setTimeout;
const clearTimeout = globalThis.clearTimeout;
const setInterval = globalThis.setInterval;
const clearInterval = globalThis.clearInterval;
const setImmediate = (fn, ...a) => globalThis.setTimeout(fn, 0, ...a);
const clearImmediate = globalThis.clearTimeout;
const promises = { setTimeout: (ms) => new Promise((r) => globalThis.setTimeout(r, ms)) };
var timers_default = { setTimeout, clearTimeout, setInterval, clearInterval, setImmediate, clearImmediate, promises };
export {
  clearImmediate,
  clearInterval,
  clearTimeout,
  timers_default as default,
  promises,
  setImmediate,
  setInterval,
  setTimeout
};
