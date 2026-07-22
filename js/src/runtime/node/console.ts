// node:console — wraps QuickJS's global console. The `Console` class lets code
// construct its own logger (undici's mock formatter does `new Console(...)`).
const g = globalThis.console || {};
const out = (...a) => { try { (g.log || (() => {}))(...a); } catch {} };
const err = (...a) => { try { (g.error || g.log || (() => {}))(...a); } catch {} };

export class Console {
  constructor(_stdout, _stderr) {}
  log(...a) { out(...a); }
  info(...a) { out(...a); }
  debug(...a) { out(...a); }
  dir(...a) { out(...a); }
  warn(...a) { err(...a); }
  error(...a) { err(...a); }
  trace(...a) { err(...a); }
  table(...a) { out(...a); }
  group(...a) { out(...a); }
  groupCollapsed(...a) { out(...a); }
  groupEnd() {}
  assert(cond, ...a) { if (!cond) err("Assertion failed:", ...a); }
  count() {}
  countReset() {}
  time() {}
  timeEnd() {}
  timeLog() {}
  clear() {}
}

const instance = new Console();
export const log = instance.log;
export const info = instance.info;
export const warn = instance.warn;
export const error = instance.error;
export const debug = instance.debug;
export default instance;
