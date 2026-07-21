// Pocket Pi guest entry — the JS half of the runtime.
//
// Boots pi's embeddable Agent core inside QuickJS, wires it to the Anthropic
// streamFn (native HTTP mailbox) or an offline scripted assistant, and exposes
// a compact host-facing API on `globalThis.PocketPi`. Tool execution is routed
// back to native Rust so the guest realm stays capability-free (no fs, no net
// except through the vetted http op).

import { Agent } from "@mariozechner/pi-agent-core";
import { anthropicStream } from "./anthropic-stream.js";
import { openaiStream } from "./openai-stream.js";
import { makeScriptedStream } from "./scripted.js";
import "./stream-core.js"; // installs globalThis.__catpiPump

// Provider is chosen explicitly (cfg.provider) or inferred from the model id.
function resolveProvider(cfg) {
  if (cfg.provider) return cfg.provider;
  const id = (cfg.model || "").toLowerCase();
  if (id.startsWith("gpt") || id.startsWith("o1") || id.startsWith("o3") || id.startsWith("o4"))
    return "openai";
  return "anthropic";
}

function buildModel(cfg, provider) {
  const openai = provider === "openai";
  return {
    id: cfg.model || (openai ? "gpt-4o" : "claude-opus-4-8"),
    name: cfg.model || (openai ? "gpt-4o" : "claude-opus-4-8"),
    api: openai ? "openai-completions" : "anthropic-messages",
    provider,
    baseUrl: cfg.baseUrl || (openai ? "https://api.openai.com" : "https://api.anthropic.com"),
    apiKey: cfg.apiKey || "",
    reasoning: false,
    input: ["text", "image"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: cfg.contextWindow || 128000,
    maxTokens: cfg.maxTokens || 1024,
  };
}

// Turn a host tool declaration into a pi AgentTool whose execute() round-trips
// through native Rust: the guest never runs the real side effect itself.
function hostTool(decl) {
  return {
    name: decl.name,
    label: decl.name,
    description: decl.description || "",
    parameters: decl.parameters || { type: "object", properties: {} },
    execute: async (_toolCallId, params) => {
      const resultJson = host.tool(decl.name, JSON.stringify(params ?? {}));
      let result;
      try {
        result = JSON.parse(resultJson);
      } catch {
        result = { text: String(resultJson) };
      }
      const content = [];
      if (result.text != null) content.push({ type: "text", text: String(result.text) });
      if (result.image) content.push({ type: "image", data: result.image, mimeType: result.mimeType || "image/jpeg" });
      if (!content.length) content.push({ type: "text", text: "" });
      return { content, details: result.details ?? {}, terminate: !!result.terminate };
    },
  };
}

let agent = null;

function forward(event) {
  // Compress pi's fine-grained event stream into a small host vocabulary and
  // hand each one to native Rust as a JSON line.
  let out = null;
  switch (event.type) {
    case "agent_start":
      out = { kind: "start" };
      break;
    case "message_update": {
      const e = event.assistantMessageEvent;
      if (e && e.type === "text_delta") out = { kind: "text", delta: e.delta };
      else if (e && e.type === "thinking_delta") out = { kind: "thinking", delta: e.delta };
      break;
    }
    case "message_end": {
      const m = event.message;
      if (m && m.role === "assistant") {
        const text = (m.content || [])
          .filter((c) => c.type === "text")
          .map((c) => c.text)
          .join("");
        if (text) out = { kind: "assistant_text", text };
      }
      break;
    }
    case "tool_execution_start":
      out = { kind: "tool_start", name: event.toolName, args: event.args };
      break;
    case "tool_execution_end":
      out = { kind: "tool_end", name: event.toolName, isError: !!event.isError };
      break;
    case "agent_end":
      out = { kind: "end" };
      break;
  }
  if (out) host.emit(JSON.stringify(out));
}

globalThis.PocketPi = {
  boot(configJson) {
    const cfg = typeof configJson === "string" ? JSON.parse(configJson) : configJson;
    const provider = resolveProvider(cfg);
    const model = buildModel(cfg, provider);
    const tools = (cfg.tools || []).map(hostTool);
    const streamFn = cfg.scripted
      ? makeScriptedStream(cfg.scripted)
      : provider === "openai"
        ? openaiStream
        : anthropicStream;
    agent = new Agent({
      initialState: {
        systemPrompt: cfg.systemPrompt || "",
        model,
        thinkingLevel: "off",
        tools,
      },
      streamFn,
      getApiKey: () => model.apiKey,
      apiKey: model.apiKey,
    });
    agent.subscribe(forward);
    host.emit(JSON.stringify({ kind: "booted" }));
    return true;
  },

  prompt(text) {
    if (!agent) throw new Error("PocketPi.boot() first");
    // Fire and forget — events flow through forward(); errors surface as an event.
    agent.prompt(text).catch((e) => {
      host.emit(JSON.stringify({ kind: "error", message: String(e && e.message ? e.message : e) }));
    });
    return true;
  },

  abort() {
    if (agent) agent.abort();
  },

  busy() {
    return !!(agent && agent.state && agent.state.isStreaming);
  },
};
