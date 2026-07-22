class Channel {
  constructor(name) {
    this.name = name;
    this._subs = [];
  }
  get hasSubscribers() {
    return this._subs.length > 0;
  }
  publish(msg) {
    for (const s of this._subs.slice()) {
      try {
        s(msg, this.name);
      } catch {
      }
    }
  }
  subscribe(fn) {
    this._subs.push(fn);
  }
  unsubscribe(fn) {
    this._subs = this._subs.filter((s) => s !== fn);
    return true;
  }
}
const registry = /* @__PURE__ */ new Map();
function channel(name) {
  let c = registry.get(name);
  if (!c) {
    c = new Channel(name);
    registry.set(name, c);
  }
  return c;
}
function hasSubscribers(name) {
  const c = registry.get(name);
  return !!c && c.hasSubscribers;
}
function subscribe(name, fn) {
  channel(name).subscribe(fn);
}
function unsubscribe(name, fn) {
  return channel(name).unsubscribe(fn);
}
function tracingChannel(nameOrChannels) {
  const base = typeof nameOrChannels === "string" ? nameOrChannels : "";
  const mk = (suffix) => channel(base ? `tracing:${base}:${suffix}` : suffix);
  return {
    start: mk("start"),
    end: mk("end"),
    asyncStart: mk("asyncStart"),
    asyncEnd: mk("asyncEnd"),
    error: mk("error"),
    traceSync(fn, ctx, thisArg, ...a) {
      return fn.apply(thisArg, a);
    },
    tracePromise(fn, ctx, thisArg, ...a) {
      return fn.apply(thisArg, a);
    },
    traceCallback(fn, pos, ctx, thisArg, ...a) {
      return fn.apply(thisArg, a);
    }
  };
}
var diagnostics_channel_default = { channel, hasSubscribers, subscribe, unsubscribe, tracingChannel, Channel };
export {
  channel,
  diagnostics_channel_default as default,
  hasSubscribers,
  subscribe,
  tracingChannel,
  unsubscribe
};
