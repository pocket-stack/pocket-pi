import { Buffer } from "node:buffer";
globalThis.Buffer = Buffer;
globalThis.__nodeBuffer = Buffer;
globalThis.process.nextTick = (fn, ...args) => queueMicrotask(() => fn(...args));
