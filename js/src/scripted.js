// A deterministic, offline streamFn for Pocket Pi. Lets the runtime be tested
// (and demoed) without a network or API key: it replays a scripted assistant
// turn — optionally a tool call followed by a final answer — through the same
// pi AssistantMessageEventStream the real Anthropic path uses.
//
// Config shape (cfg.scripted):
//   { steps: [
//       { text: "..." },                                  // plain answer
//       { toolCall: { name, arguments } },                // one tool call
//     ] }
// Steps are consumed one per assistant turn; when tool calls are involved the
// loop drives another turn automatically, so the next step answers.

import { AssistantMessageEventStream } from "@mariozechner/pi-ai";

export function makeScriptedStream(cfg) {
  const steps = (cfg && cfg.steps) || [{ text: "(scripted: no steps configured)" }];
  let cursor = 0;

  return function scriptedStream(model, _context, _options) {
    const stream = new AssistantMessageEventStream();
    const step = steps[Math.min(cursor, steps.length - 1)];
    cursor++;

    const partial = {
      role: "assistant",
      stopReason: "stop",
      content: [],
      api: model.api,
      provider: model.provider,
      model: model.id,
      usage: {
        input: 0,
        output: 0,
        cacheRead: 0,
        cacheWrite: 0,
        totalTokens: 0,
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
      },
      timestamp: Date.now(),
    };

    // Emit on the microtask queue so the caller has attached its consumer,
    // matching the real provider's asynchronous arrival.
    Promise.resolve().then(() => {
      stream.push({ type: "start", partial });
      if (step.toolCall) {
        partial.content[0] = {
          type: "toolCall",
          id: "scripted-" + cursor,
          name: step.toolCall.name,
          arguments: step.toolCall.arguments || {},
          partialJson: JSON.stringify(step.toolCall.arguments || {}),
        };
        stream.push({ type: "toolcall_start", contentIndex: 0, partial });
        const tc = partial.content[0];
        delete tc.partialJson;
        stream.push({ type: "toolcall_end", contentIndex: 0, toolCall: tc, partial });
        partial.stopReason = "toolUse";
      } else {
        partial.content[0] = { type: "text", text: "" };
        stream.push({ type: "text_start", contentIndex: 0, partial });
        const text = step.text || "";
        partial.content[0].text = text;
        stream.push({ type: "text_delta", contentIndex: 0, delta: text, partial });
        stream.push({ type: "text_end", contentIndex: 0, content: text, partial });
      }
      stream.push({ type: "done", reason: partial.stopReason, message: partial });
      stream.end();
    });

    return stream;
  };
}
