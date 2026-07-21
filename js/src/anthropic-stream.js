// The Anthropic streamFn for Pocket Pi. Builds an Anthropic Messages request,
// hands it to the shared streaming core, and maps Anthropic SSE deltas onto pi's
// running `partial` message (the same shape pi-ai's own provider produces).

import { AssistantMessageEventStream } from "@mariozechner/pi-ai";
import { beginTurn, emptyUsage, safeParse } from "./stream-core.js";

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
            content: normalizeBlocks(m.content),
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
  return normalizeBlocks(content);
}

function normalizeBlocks(content) {
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
    auth: "x-api-key",
    body: {
      model: model.id,
      max_tokens: options.maxTokens || model.maxTokens || 4096,
      system: context.systemPrompt || undefined,
      messages: toAnthropicMessages(context),
      tools: toAnthropicTools(context.tools),
      stream: true,
    },
  };
  beginTurn(stream, partial, request, applyAnthropicEvent, options.signal);
  return stream;
}

// Map one Anthropic SSE event onto the running partial. Returns true when done.
function applyAnthropicEvent(turn, ev) {
  const { partial, stream } = turn;
  switch (ev.type) {
    case "message_start":
      if (ev.message && ev.message.usage) partial.usage.input = ev.message.usage.input_tokens || 0;
      if (!turn.started) {
        turn.started = true;
        stream.push({ type: "start", partial });
      }
      return false;
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
        partial.content[i] = { type: "toolCall", id: b.id, name: b.name, arguments: {}, partialJson: "" };
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
    case "message_delta":
      if (ev.usage) partial.usage.output = ev.usage.output_tokens || partial.usage.output;
      if (ev.delta && ev.delta.stop_reason)
        partial.stopReason = ev.delta.stop_reason === "tool_use" ? "toolUse" : "stop";
      return false;
    case "message_stop":
      partial.usage.totalTokens = partial.usage.input + partial.usage.output;
      stream.push({ type: "done", reason: partial.stopReason, message: partial });
      stream.end();
      return true;
    case "error":
      partial.stopReason = "error";
      partial.errorMessage = ev.error ? ev.error.message || String(ev.error) : "stream error";
      stream.push({ type: "error", reason: "error", error: partial });
      stream.end();
      return true;
    default:
      return false;
  }
}
