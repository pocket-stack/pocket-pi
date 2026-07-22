class EventEmitter {
  constructor() {
    this._events = /* @__PURE__ */ new Map();
    this._maxListeners = 10;
  }
  setMaxListeners(n) {
    this._maxListeners = n;
    return this;
  }
  getMaxListeners() {
    return this._maxListeners;
  }
  on(type, fn) {
    let arr = this._events.get(type);
    if (!arr) {
      arr = [];
      this._events.set(type, arr);
    }
    arr.push(fn);
    return this;
  }
  addListener(type, fn) {
    return this.on(type, fn);
  }
  once(type, fn) {
    const wrap = (...args) => {
      this.off(type, wrap);
      fn(...args);
    };
    wrap.listener = fn;
    return this.on(type, wrap);
  }
  prependListener(type, fn) {
    let arr = this._events.get(type);
    if (!arr) {
      arr = [];
      this._events.set(type, arr);
    }
    arr.unshift(fn);
    return this;
  }
  off(type, fn) {
    const arr = this._events.get(type);
    if (arr) {
      const i = arr.findIndex((f) => f === fn || f.listener === fn);
      if (i !== -1) arr.splice(i, 1);
    }
    return this;
  }
  removeListener(type, fn) {
    return this.off(type, fn);
  }
  removeAllListeners(type) {
    if (type === void 0) this._events.clear();
    else this._events.delete(type);
    return this;
  }
  emit(type, ...args) {
    const arr = this._events.get(type);
    if (!arr || arr.length === 0) {
      if (type === "error") throw args[0] instanceof Error ? args[0] : new Error("Unhandled error");
      return false;
    }
    for (const fn of arr.slice()) fn.apply(this, args);
    return true;
  }
  listeners(type) {
    return (this._events.get(type) || []).slice();
  }
  listenerCount(type) {
    return (this._events.get(type) || []).length;
  }
  eventNames() {
    return [...this._events.keys()];
  }
}
const once = (emitter, name) => new Promise((resolve, reject) => {
  emitter.once(name, (...args) => resolve(args));
  emitter.once("error", reject);
});
var events_default = EventEmitter;
EventEmitter.EventEmitter = EventEmitter;
EventEmitter.once = once;
EventEmitter.defaultMaxListeners = 10;
export {
  EventEmitter,
  events_default as default,
  once
};
