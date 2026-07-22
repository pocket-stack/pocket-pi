// Node runtime bootstrap — evaluated once when the Node layer is installed,
// before any guest module loads. Hoists Buffer to a global (many packages assume
// `Buffer` exists without importing it) and wires `process.nextTick` onto the
// microtask queue. Kept as a file (not a Rust string literal) so it stays
// readable and lintable.
import { Buffer } from "node:buffer";
globalThis.Buffer = Buffer;
globalThis.__nodeBuffer = Buffer;
globalThis.process.nextTick = (fn, ...args) => queueMicrotask(() => fn(...args));
