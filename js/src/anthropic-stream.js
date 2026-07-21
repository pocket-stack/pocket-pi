// The Anthropic streamFn for Pocket Pi.
//
// pi-agent-core's loop calls `streamFn(model, context, options)` and consumes
// the returned async-iterable of pi `AssistantMessageEvent`s. We build the
// Anthropic Messages request here, hand it to the native HTTP op (which does
// the blocking HTTPS + SSE read on a background thread), and each frame drain
// the raw `data:` lines it collected — mapping Anthropic SSE deltas onto pi's
// running `partial` message exactly the way pi-ai's own provider does.
//
// The native surface (see native ops in Rust):
//   host.http.start(requestJson) -> turnId : begins the request on a thread
//   host.http.drain(turnId) -> { lines:[string], done:bool, error:string|null }
//   host.http.cancel(turnId)
//
// Nothing here blocks. `globalThis.__catpiPump()` (called once per host frame)
// walks the active turns, drains their mailboxes, and pushes pi events into the
// AssistantMessageEventStream. LLM latency (seconds) dwarfs the frame period,
// so a 2 Hz headless pump is plenty — this is the whole point of borrowing
// PocketJS's frame scheduler for an agent runtime.

import { AssistantMessageEventStream } from "@mariozechner/pi-ai";

const activeTurns = new Map();

function emptyUsage() {
  return {
    input: 0,
    output: 0,
    cacheRead: 0,
    cacheWrite: 0,
    totalTokens: 0,
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
  };
}

// pi message content ← Anthropic content-block shapes.
function toAnthropicMessages(context) {
  const messages = [];
  for (const m of context.messages) {
    if (m.role === "user") {
      messages.push({ role: "user", content: normalizeUserContent(m.content) });
    } else if (m.role === "assistant") {
      const content = [];
      for (const c of m.content) {
        if (c.type === "text") content.push({ type: "text", text: c.text });
        else if (c.type === "toolCall")
          content.push({ type: "tool_use", id: c.id, name: c.name, input: c.arguments ?? {} });
      }
      if (content.length) messages.push({ role: "assistant", content });
    } else if (m.role === "toolResult") {
      messages.push({
        role: "user",
        content: [
          {
            type: "tool_result",
            tool_use_id: m.toolCallId,
            content: normalizeToolResultContent(m.content),
            is_error: !!m.isError,
          },
        ],
      });
    }
  }
  return messages;
}

function normalizeUserContent(content) {
  if (typeof content === "string") return [{ type: "text", text: content }];
  return content.map((c) => {
    if (c.type === "text") return { type: "text", text: c.text };
    if (c.type === "image")
      return {
        type: "image",
        source: { type: "base64", media_type: c.mimeType || "image/png", data: c.data },
      };
    return c;
  });
}

function normalizeToolResultContent(content) {
  if (typeof content === "string") return [{ type: "text", text: content }];
  return content.map((c) => {
    if (c.type === "text") return { type: "text", text: c.text };
    if (c.type === "image")
      return {
        type: "image",
        source: { type: "base64", media_type: c.mimeType || "image/png", data: c.data },
      };
    return { type: "text", text: JSON.stringify(c) };
  });
}

function toAnthropicTools(tools) {
  if (!tools || !tools.length) return undefined;
  return tools.map((t) => ({
    name: t.name,
    description: t.description || "",
    input_schema: t.parameters || { type: "object", properties: {} },
  }));
}

export function anthropicStream(model, context, options) {
  const stream = new AssistantMessageEventStream();

  const partial = {
    role: "assistant",
    stopReason: "stop",
    content: [],
    api: model.api,
    provider: model.provider,
    model: model.id,
    usage: emptyUsage(),
    timestamp: Date.now(),
  };

  const request = {
    url: (model.baseUrl || "https://api.anthropic.com") + "/v1/messages",
    apiKey: options.apiKey || model.apiKey || "",
    body: {
      model: model.id,
      max_tokens: options.maxTokens || model.maxTokens || 4096,
      system: context.systemPrompt || undefined,
      messages: toAnthropicMessages(context),
      tools: toAnthropicTools(context.tools),
      stream: true,
    },
  };

  let turnId;
  try {
    turnId = host.http.start(JSON.stringify(request));
  } catch (e) {
    partial.stopReason = "error";
    partial.errorMessage = String(e && e.message ? e.message : e);
    stream.push({ type: "error", reason: "error", error: partial });
    stream.end();
    return stream;
  }

  const state = { started: false };

  activeTurns.set(turnId, {
    stream,
    partial,
    state,
    onAbort: () => {
      try {
        host.http.cancel(turnId);
      } catch {}
      partial.stopReason = "aborted";
      stream.push({ type: "error", reason: "aborted", error: partial });
      stream.end();
      activeTurns.delete(turnId);
    },
  });

  if (options.signal) {
    if (options.signal.aborted) {
      activeTurns.get(turnId).onAbort();
    } else {
      options.signal.addEventListener("abort", () => {
        const t = activeTurns.get(turnId);
        if (t) t.onAbort();
      });
    }
  }

  return stream;
}

// Map one decoded Anthropic SSE `data:` JSON onto the running partial message,
// pushing the corresponding pi event. Returns true when the turn is finished.
function applyAnthropicEvent(turn, ev) {
  const { partial, stream, state } = turn;
  switch (ev.type) {
    case "message_start": {
      if (ev.message && ev.message.usage) {
        partial.usage.input = ev.message.usage.input_tokens || 0;
      }
      if (!state.started) {
        state.started = true;
        stream.push({ type: "start", partial });
      }
      return false;
    }
    case "content_block_start": {
      const i = ev.index;
      const b = ev.content_block;
      if (b.type === "text") {
        partial.content[i] = { type: "text", text: "" };
        stream.push({ type: "text_start", contentIndex: i, partial });
      } else if (b.type === "thinking") {
        partial.content[i] = { type: "thinking", thinking: "" };
        stream.push({ type: "thinking_start", contentIndex: i, partial });
      } else if (b.type === "tool_use") {
        partial.content[i] = {
          type: "toolCall",
          id: b.id,
          name: b.name,
          arguments: {},
          partialJson: "",
        };
        stream.push({ type: "toolcall_start", contentIndex: i, partial });
      }
      return false;
    }
    case "content_block_delta": {
      const i = ev.index;
      const d = ev.delta;
      const c = partial.content[i];
      if (d.type === "text_delta" && c && c.type === "text") {
        c.text += d.text;
        stream.push({ type: "text_delta", contentIndex: i, delta: d.text, partial });
      } else if (d.type === "thinking_delta" && c && c.type === "thinking") {
        c.thinking += d.thinking;
        stream.push({ type: "thinking_delta", contentIndex: i, delta: d.thinking, partial });
      } else if (d.type === "input_json_delta" && c && c.type === "toolCall") {
        c.partialJson += d.partial_json;
        c.arguments = safeParse(c.partialJson);
        partial.content[i] = { ...c };
        stream.push({ type: "toolcall_delta", contentIndex: i, delta: d.partial_json, partial });
      }
      return false;
    }
    case "content_block_stop": {
      const i = ev.index;
      const c = partial.content[i];
      if (c && c.type === "text") stream.push({ type: "text_end", contentIndex: i, content: c.text, partial });
      else if (c && c.type === "thinking")
        stream.push({ type: "thinking_end", contentIndex: i, content: c.thinking, partial });
      else if (c && c.type === "toolCall") {
        delete c.partialJson;
        stream.push({ type: "toolcall_end", contentIndex: i, toolCall: c, partial });
      }
      return false;
    }
    case "message_delta": {
      if (ev.usage) partial.usage.output = ev.usage.output_tokens || partial.usage.output;
      if (ev.delta && ev.delta.stop_reason) {
        partial.stopReason = ev.delta.stop_reason === "tool_use" ? "toolUse" : "stop";
      }
      return false;
    }
    case "message_stop": {
      partial.usage.totalTokens = partial.usage.input + partial.usage.output;
      stream.push({ type: "done", reason: partial.stopReason, message: partial });
      stream.end();
      return true;
    }
    case "error": {
      partial.stopReason = "error";
      partial.errorMessage = ev.error ? ev.error.message || String(ev.error) : "stream error";
      stream.push({ type: "error", reason: "error", error: partial });
      stream.end();
      return true;
    }
    default:
      return false;
  }
}

function safeParse(text) {
  try {
    return JSON.parse(text);
  } catch {
    return {};
  }
}

// Called once per host frame. Drains every active turn's HTTP mailbox and
// advances its event stream. Kept deliberately allocation-light on the hot path
// when there are no active turns.
globalThis.__catpiPump = function () {
  if (activeTurns.size === 0) return;
  for (const [turnId, turn] of activeTurns) {
    let out;
    try {
      out = JSON.parse(host.http.drain(turnId));
    } catch (e) {
      turn.partial.stopReason = "error";
      turn.partial.errorMessage = String(e);
      turn.stream.push({ type: "error", reason: "error", error: turn.partial });
      turn.stream.end();
      activeTurns.delete(turnId);
      continue;
    }
    let finished = false;
    for (const line of out.lines) {
      let ev;
      try {
        ev = JSON.parse(line);
      } catch {
        continue;
      }
      if (applyAnthropicEvent(turn, ev)) {
        finished = true;
        break;
      }
    }
    if (out.error && !finished) {
      turn.partial.stopReason = "error";
      turn.partial.errorMessage = out.error;
      turn.stream.push({ type: "error", reason: "error", error: turn.partial });
      turn.stream.end();
      finished = true;
    } else if (out.done && !finished && !turn.stream.done) {
      // Connection closed without message_stop — treat as protocol error.
      turn.partial.stopReason = "error";
      turn.partial.errorMessage = "stream ended without message_stop";
      turn.stream.push({ type: "error", reason: "error", error: turn.partial });
      turn.stream.end();
      finished = true;
    }
    if (finished) activeTurns.delete(turnId);
  }
};
