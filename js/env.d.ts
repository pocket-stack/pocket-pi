// Ambient types for Pocket Pi's runtime environment. The glue is a deliberately
// loose Node/Web compatibility layer over QuickJS + the native `host`/`__node`
// ops, so the runtime surface is typed as `any` — this is shim code, not typed
// application logic. Its job is to let the TypeScript build (transpile +
// `tsc --noEmit` gate) run over the sources.

export {};

declare global {
  // Native surface mounted by Rust.
  var host: any;
  var __node: any;

  // Node/Web globals the glue defines on globalThis (and reads back).
  var process: any;
  var Buffer: any;
  var __nodeBuffer: any;
  var global: any;
  var require: any;
  var console: any;
  var performance: any;
  var crypto: any;

  var fetch: any;
  var Headers: any;
  var Request: any;
  var Response: any;
  var URL: any;
  var URLSearchParams: any;
  var ReadableStream: any;
  var TextEncoder: any;
  var TextDecoder: any;
  var Blob: any;
  var File: any;
  var FormData: any;
  var AbortController: any;
  var AbortSignal: any;
  var Event: any;
  var EventTarget: any;
  var CustomEvent: any;
  var MessagePort: any;
  var MessageChannel: any;
  var DOMException: any;

  var atob: any;
  var btoa: any;
  var structuredClone: any;
  var queueMicrotask: any;
  var setTimeout: any;
  var clearTimeout: any;
  var setInterval: any;
  var clearInterval: any;
  var setImmediate: any;
  var clearImmediate: any;

  // Pocket Pi coordination surface (guest entry points + host-poll flags).
  var PocketPi: any;
  var PiFull: any;
  var __builtinExports: any;
  var __cjsRequire: any;
  var __cjsCache: any;
  var __catpiTimers: any;
  var __catpiPump: any;
  var __catpiFetchPump: any;

  // Test-harness globals (driver / probes exchange results with the Rust side).
  var __OPENAI_KEY: any;
  var __piRun: any;
  var __piResult: any;
  var __piError: any;
  var __piDone: any;
  var __piBind: any;
  var __piLog: any;
  var __piLastEvent: any;
  var __piFullLoaded: any;
  var __piLoadExtension: any;
  var __piExtResult: any;
  var __piExtError: any;
  var __piExtDone: any;
  var __piPersist: any;
  var __piPersistResult: any;
  var __piPersistError: any;
}

// The `node:*` builtins are served by Pocket Pi's own loader at runtime; type
// them loosely so cross-builtin imports (`import { EventEmitter } from
// "node:events"`) don't need @types/node.
declare module "node:*" {
  const anything: any;
  export = anything;
}
