// Build-time stub for `undici`. Pocket Pi routes all HTTP through its native,
// proxy-aware streaming fetch (the runtime's one real transport op), so pi's
// undici-backed transport is never used. pi only reaches for undici when an HTTP
// *proxy is configured in its own settings* — which Pocket Pi never does (the
// native hub handles proxying). This stub exists only so `import * as undici`
// loads without dragging in undici's entire web-fetch/websocket/cache stack.
//
// This substitutes a transitive transport dependency, not pi's logic — the same
// "heavy Node transport dep → native" rule the trimmed bundle applies to pi-ai.
// pi's own source is unmodified.

class NoopDispatcher {
  dispatch() { return false; }
  close() { return Promise.resolve(); }
  destroy() { return Promise.resolve(); }
  compose() { return this; }
  on() { return this; }
  once() { return this; }
  off() { return this; }
}
export class Client extends NoopDispatcher {}
export class Pool extends NoopDispatcher {}
export class BalancedPool extends NoopDispatcher {}
export class Agent extends NoopDispatcher {}
export class ProxyAgent extends NoopDispatcher {}
export class EnvHttpProxyAgent extends NoopDispatcher {}
export class MockAgent extends NoopDispatcher {}
export class RetryAgent extends NoopDispatcher {}

let globalDispatcher = new Agent();
export function setGlobalDispatcher(d) { globalDispatcher = d; }
export function getGlobalDispatcher() { return globalDispatcher; }
export function install() {} // pi calls this optionally; keep our globals in place
export function fetch(...args) { return globalThis.fetch(...args); }

export const Headers = globalThis.Headers;
export const Response = globalThis.Response;
export const Request = globalThis.Request;
export const FormData = globalThis.FormData;
export const interceptors = {};
export const errors = {};

export default {
  Client, Pool, BalancedPool, Agent, ProxyAgent, EnvHttpProxyAgent, MockAgent, RetryAgent,
  setGlobalDispatcher, getGlobalDispatcher, install, fetch,
  Headers, Response, Request, FormData, interceptors, errors,
};
