// Minimal stand-in for @earendil-works/pi-ai on the Pocket Pi agent-core path.
//
// pi-agent-core's loop imports exactly three things from pi-ai:
//   { EventStream, streamSimple, validateToolArguments }
// plus proxy.js imports { EventStream, parseStreamingJson }.
//
// With a custom streamFn (see anthropic-stream.js) the Agent never calls
// streamSimple, and Pocket Pi controls the whole dependency graph — so instead
// of bundling pi-ai's ~5 MB of provider SDKs + typebox + a 553 KB model table,
// we reimplement the four symbols the core actually touches. This is the
// "Strategy A" the recon recommended: bare Agent + own Anthropic streamFn.

// --- EventStream: verbatim behavior of pi-ai's generic async-iterable queue ---
export class EventStream {
  constructor(isComplete, extractResult) {
    this.isComplete = isComplete;
    this.extractResult = extractResult;
    this.queue = [];
    this.waiting = [];
    this.done = false;
    this.finalResultPromise = new Promise((resolve) => {
      this.resolveFinalResult = resolve;
    });
  }
  push(event) {
    if (this.done) return;
    if (this.isComplete(event)) {
      this.done = true;
      this.resolveFinalResult(this.extractResult(event));
    }
    const waiter = this.waiting.shift();
    if (waiter) waiter({ value: event, done: false });
    else this.queue.push(event);
  }
  end(result) {
    this.done = true;
    if (result !== undefined) this.resolveFinalResult(result);
    while (this.waiting.length > 0) this.waiting.shift()({ value: undefined, done: true });
  }
  async *[Symbol.asyncIterator]() {
    while (true) {
      if (this.queue.length > 0) {
        yield this.queue.shift();
      } else if (this.done) {
        return;
      } else {
        const result = await new Promise((resolve) => this.waiting.push(resolve));
        if (result.done) return;
        yield result.value;
      }
    }
  }
  result() {
    return this.finalResultPromise;
  }
}

export class AssistantMessageEventStream extends EventStream {
  constructor() {
    super(
      (event) => event.type === "done" || event.type === "error",
      (event) => {
        if (event.type === "done") return event.message;
        if (event.type === "error") return event.error;
        throw new Error("Unexpected event type for final result");
      },
    );
  }
}

// The core only calls streamSimple when no streamFn is supplied. Pocket Pi
// always supplies one, so reaching this is a wiring bug — fail loudly.
export function streamSimple() {
  throw new Error(
    "pi-ai streamSimple is not available in Pocket Pi; pass a streamFn (see anthropic-stream.js)",
  );
}

// The core validates tool arguments against a TypeBox schema. Pocket Pi's tools
// carry plain JSON-schema (or none), so we validate shape leniently: pass the
// prepared arguments through unchanged. Real type coercion isn't needed for the
// PoC — tools own their own input handling.
export function validateToolArguments(_tool, toolCall) {
  return toolCall.arguments ?? {};
}

// Extract the concatenated text of a message's content (array of blocks or a
// bare string), falling back to `fallback` when empty. Used by the harness's
// compaction/editor paths.
export function contentText(content, fallback = "") {
  if (typeof content === "string") return content || fallback;
  if (Array.isArray(content)) {
    const t = content
      .filter((b) => b && b.type === "text" && typeof b.text === "string")
      .map((b) => b.text)
      .join("");
    return t || fallback;
  }
  return fallback;
}

// Retry wrapper around an assistant call. Pocket Pi's PoC path doesn't retry —
// produce once and return (the real policy only matters for compaction).
export async function retryAssistantCall(produce) {
  return await produce();
}

// UUIDv7-ish id. The prelude provides crypto.randomUUID (host-backed entropy).
export function uuidv7() {
  if (globalThis.crypto && globalThis.crypto.randomUUID) return globalThis.crypto.randomUUID();
  if (globalThis.host && globalThis.host.uuid) return globalThis.host.uuid();
  return "xxxxxxxx-xxxx-7xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (c) => {
    const r = Math.floor(Math.random() * 16);
    return (c === "x" ? r : (r & 0x3) | 0x8).toString(16);
  });
}

// Incremental JSON repair for streamed tool-call arguments. Good enough for the
// PoC: try to parse; on failure, close dangling strings/objects/arrays.
export function parseStreamingJson(text) {
  if (!text) return {};
  try {
    return JSON.parse(text);
  } catch {}
  let repaired = text;
  // Close an unterminated string.
  const quotes = (repaired.match(/(?<!\\)"/g) || []).length;
  if (quotes % 2 === 1) repaired += '"';
  // Balance braces/brackets.
  const opens = (repaired.match(/[{[]/g) || []).length;
  const closes = (repaired.match(/[}\]]/g) || []).length;
  for (let i = 0; i < opens - closes; i++) {
    repaired += repaired.lastIndexOf("[") > repaired.lastIndexOf("{") ? "]" : "}";
  }
  try {
    return JSON.parse(repaired);
  } catch {
    return {};
  }
}
