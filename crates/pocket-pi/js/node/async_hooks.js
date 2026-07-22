class AsyncLocalStorage {
  run(_store, cb, ...args) {
    return cb(...args);
  }
  getStore() {
    return this._store;
  }
  enterWith(store) {
    this._store = store;
  }
  exit(cb, ...args) {
    return cb(...args);
  }
  disable() {
  }
}
class AsyncResource {
  constructor(type, opts) {
    this.type = type;
    this._opts = opts;
  }
  runInAsyncScope(fn, thisArg, ...args) {
    return fn.apply(thisArg, args);
  }
  emitDestroy() {
    return this;
  }
  asyncId() {
    return 0;
  }
  triggerAsyncId() {
    return 0;
  }
  bind(fn) {
    return fn;
  }
  static bind(fn) {
    return fn;
  }
}
function createHook() {
  return { enable() {
    return this;
  }, disable() {
    return this;
  } };
}
function executionAsyncId() {
  return 0;
}
function triggerAsyncId() {
  return 0;
}
function executionAsyncResource() {
  return {};
}
var async_hooks_default = { AsyncLocalStorage, AsyncResource, createHook, executionAsyncId, triggerAsyncId, executionAsyncResource };
export {
  AsyncLocalStorage,
  AsyncResource,
  createHook,
  async_hooks_default as default,
  executionAsyncId,
  executionAsyncResource,
  triggerAsyncId
};
