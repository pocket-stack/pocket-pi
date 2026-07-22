const g = globalThis.console || {};
const out = (...a) => {
  try {
    (g.log || (() => {
    }))(...a);
  } catch {
  }
};
const err = (...a) => {
  try {
    (g.error || g.log || (() => {
    }))(...a);
  } catch {
  }
};
class Console {
  constructor(_stdout, _stderr) {
  }
  log(...a) {
    out(...a);
  }
  info(...a) {
    out(...a);
  }
  debug(...a) {
    out(...a);
  }
  dir(...a) {
    out(...a);
  }
  warn(...a) {
    err(...a);
  }
  error(...a) {
    err(...a);
  }
  trace(...a) {
    err(...a);
  }
  table(...a) {
    out(...a);
  }
  group(...a) {
    out(...a);
  }
  groupCollapsed(...a) {
    out(...a);
  }
  groupEnd() {
  }
  assert(cond, ...a) {
    if (!cond) err("Assertion failed:", ...a);
  }
  count() {
  }
  countReset() {
  }
  time() {
  }
  timeEnd() {
  }
  timeLog() {
  }
  clear() {
  }
}
const instance = new Console();
const log = instance.log;
const info = instance.info;
const warn = instance.warn;
const error = instance.error;
const debug = instance.debug;
var console_default = instance;
export {
  Console,
  debug,
  console_default as default,
  error,
  info,
  log,
  warn
};
