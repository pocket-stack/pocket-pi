// node:async_hooks — single-threaded, synchronous stubs. AsyncLocalStorage runs
// callbacks inline; AsyncResource is a real (no-op) base class so libraries that
// `class X extends AsyncResource {}` (e.g. undici) load and construct.
export class AsyncLocalStorage {
  run(_store, cb, ...args) { return cb(...args); }
  getStore() { return this._store; }
  enterWith(store) { this._store = store; }
  exit(cb, ...args) { return cb(...args); }
  disable() {}
}
export class AsyncResource {
  constructor(type, opts) { this.type = type; this._opts = opts; }
  runInAsyncScope(fn, thisArg, ...args) { return fn.apply(thisArg, args); }
  emitDestroy() { return this; }
  asyncId() { return 0; }
  triggerAsyncId() { return 0; }
  bind(fn) { return fn; }
  static bind(fn) { return fn; }
}
export function createHook() { return { enable() { return this; }, disable() { return this; } }; }
export function executionAsyncId() { return 0; }
export function triggerAsyncId() { return 0; }
export function executionAsyncResource() { return {}; }
export default { AsyncLocalStorage, AsyncResource, createHook, executionAsyncId, triggerAsyncId, executionAsyncResource };
