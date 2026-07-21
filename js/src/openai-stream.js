// The OpenAI streamFn for Pocket Pi. Builds an OpenAI Chat Completions request
// and maps its SSE deltas (a different shape from Anthropic — `choices[].delta`,
// incremental `tool_calls`, `image_url` content) onto pi's running `partial`.

import { AssistantMessageEventStream } from "@mariozechner/pi-ai";
import { beginTurn, emptyUsage, safeParse } from "./stream-core.js";

function toOpenAIMessages(context) {
  const messages = [];
  if (context.systemPrompt) messages.push({ role: "system", content: context.systemPrompt });
  for (const m of context.messages) {
    if (m.role === "user") {
      messages.push({ role: "user", content: userContent(m.content) });
    } else if (m.role === "assistant") {
      const text = m.content.filter((c) => c.type === "text").map((c) => c.text).join("");
      const toolCalls = m.content
        .filter((c) => c.type === "toolCall")
        .map((c) => ({
          id: c.id,
          type: "function",
          function: { name: c.name, arguments: JSON.stringify(c.arguments ?? {}) },
        }));
      const msg = { role: "assistant", content: text || null };
      if (toolCalls.length) msg.tool_calls = toolCalls;
      messages.push(msg);
    } else if (m.role === "toolResult") {
      // OpenAI tool results are a `tool` message; images can't ride a tool
      // message, so if the result carries one we send the text here and add the
      // image as a following user message.
      const parts = Array.isArray(m.content) ? m.content : [{ type: "text", text: String(m.content) }];
      const text = parts.filter((c) => c.type === "text").map((c) => c.text).join("\n") || "(done)";
      messages.push({ role: "tool", tool_call_id: m.toolCallId, content: text });
      const images = parts.filter((c) => c.type === "image");
      if (images.length) {
        messages.push({
          role: "user",
          content: images.map((c) => ({
            type: "image_url",
            image_url: { url: `data:${c.mimeType || "image/jpeg"};base64,${c.data}` },
          })),
        });
      }
    }
  }
  return messages;
}

function userContent(content) {
  if (typeof content === "string") return content;
  return content.map((c) => {
    if (c.type === "text") return { type: "text", text: c.text };
    if (c.type === "image")
      return {
        type: "image_url",
        image_url: { url: `data:${c.mimeType || "image/png"};base64,${c.data}` },
      };
    return { type: "text", text: JSON.stringify(c) };
  });
}

function toOpenAITools(tools) {
  if (!tools || !tools.length) return undefined;
  return tools.map((t) => ({
    type: "function",
    function: {
      name: t.name,
      description: t.description || "",
      parameters: t.parameters || { type: "object", properties: {} },
    },
  }));
}

export function openaiStream(model, context, options) {
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
  const body = {
    model: model.id,
    messages: toOpenAIMessages(context),
    tools: toOpenAITools(context.tools),
    stream: true,
    max_completion_tokens: options.maxTokens || model.maxTokens || 1024,
  };
  const request = {
    url: (model.baseUrl || "https://api.openai.com") + "/v1/chat/completions",
    apiKey: options.apiKey || model.apiKey || "",
    auth: "bearer",
    body,
  };
  // Per-turn scratch: which pi content index holds the text, and per-tool-call
  // accumulation keyed by the OpenAI tool_call index.
  const scratch = { textIdx: -1, tools: new Map() };
  beginTurn(stream, partial, request, (turn, ev) => applyOpenAIEvent(turn, ev, scratch), options.signal);
  return stream;
}

function applyOpenAIEvent(turn, ev, scratch) {
  const { partial, stream } = turn;
  if (ev.error) {
    partial.stopReason = "error";
    partial.errorMessage = ev.error.message || JSON.stringify(ev.error);
    stream.push({ type: "error", reason: "error", error: partial });
    stream.end();
    return true;
  }
  if (ev.usage) {
    partial.usage.input = ev.usage.prompt_tokens || partial.usage.input;
    partial.usage.output = ev.usage.completion_tokens || partial.usage.output;
  }
  const choice = ev.choices && ev.choices[0];
  if (!choice) return false;

  if (!turn.started) {
    turn.started = true;
    stream.push({ type: "start", partial });
  }

  const d = choice.delta || {};
  if (typeof d.content === "string" && d.content.length) {
    if (scratch.textIdx < 0) {
      scratch.textIdx = partial.content.length;
      partial.content[scratch.textIdx] = { type: "text", text: "" };
      stream.push({ type: "text_start", contentIndex: scratch.textIdx, partial });
    }
    const c = partial.content[scratch.textIdx];
    c.text += d.content;
    stream.push({ type: "text_delta", contentIndex: scratch.textIdx, delta: d.content, partial });
  }

  for (const tc of d.tool_calls || []) {
    let entry = scratch.tools.get(tc.index);
    if (!entry) {
      const piIdx = partial.content.length;
      partial.content[piIdx] = {
        type: "toolCall",
        id: tc.id || `call_${tc.index}`,
        name: (tc.function && tc.function.name) || "",
        arguments: {},
        partialJson: "",
      };
      entry = { piIdx };
      scratch.tools.set(tc.index, entry);
      stream.push({ type: "toolcall_start", contentIndex: piIdx, partial });
    }
    const block = partial.content[entry.piIdx];
    if (tc.function && tc.function.name) block.name = tc.function.name;
    if (tc.function && tc.function.arguments) {
      block.partialJson += tc.function.arguments;
      block.arguments = safeParse(block.partialJson);
      partial.content[entry.piIdx] = { ...block };
      stream.push({ type: "toolcall_delta", contentIndex: entry.piIdx, delta: tc.function.arguments, partial });
    }
  }

  if (choice.finish_reason) {
    // Close open blocks.
    if (scratch.textIdx >= 0) {
      const c = partial.content[scratch.textIdx];
      stream.push({ type: "text_end", contentIndex: scratch.textIdx, content: c.text, partial });
    }
    for (const entry of scratch.tools.values()) {
      const block = partial.content[entry.piIdx];
      delete block.partialJson;
      stream.push({ type: "toolcall_end", contentIndex: entry.piIdx, toolCall: block, partial });
    }
    partial.stopReason = choice.finish_reason === "tool_calls" ? "toolUse" : "stop";
    partial.usage.totalTokens = partial.usage.input + partial.usage.output;
    stream.push({ type: "done", reason: partial.stopReason, message: partial });
    stream.end();
    return true;
  }
  return false;
}
