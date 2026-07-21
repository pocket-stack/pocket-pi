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
    reasoningEffort: cfg.reasoningEffort, // openai reasoning models; undefined = auto
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
        if (m.stopReason === "error" || m.errorMessage) {
          out = { kind: "error", message: m.errorMessage || "stream error" };
        } else {
          const text = (m.content || [])
            .filter((c) => c.type === "text")
            .map((c) => c.text)
            .join("");
          if (text) out = { kind: "assistant_text", text };
        }
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
    // Optional self-extension: give the agent a tool to write its own tools as
    // TypeScript plugins, compiled and loaded live (the runtime analogue of a
    // developer dropping a .ts extension into pi).
    if (cfg.selfExtend) {
      agent.state.tools = [...agent.state.tools, definePluginTool()];
    }

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

  /// Load an agent-authored TypeScript plugin at runtime. `tsSource` is
  /// transpiled natively (host.transpile → oxc) and its default export — a
  /// factory `(api) => void` — is invoked with a small plugin API that can
  /// register tools and subscribe to events on the running agent.
  loadPlugin(name, tsSource) {
    if (!agent) throw new Error("PocketPi.boot() first");
    const js = host.transpile(name, tsSource);
    // Turn the ES module's `export default <expr>` into a returned value so we
    // can evaluate a single-file plugin without a module loader. Value imports
    // aren't supported (type-only imports are stripped by the transpiler).
    const body = js.replace(/export\s+default\s+/, "return ");
    let factory;
    try {
      factory = new Function(body)();
    } catch (e) {
      throw new Error(`plugin '${name}' failed to evaluate: ${e && e.message ? e.message : e}`);
    }
    if (typeof factory !== "function") {
      throw new Error(`plugin '${name}' must 'export default' a function (api) => void`);
    }
    factory(makePluginApi(name));
    host.emit(JSON.stringify({ kind: "plugin_loaded", name }));
    return true;
  },
};

// The lean plugin API (Pocket Pi "Option A"). A plugin can add tools to the
// running agent and observe its events. Tool `execute` runs in the QuickJS
// realm and may reach native capabilities through `api.callHostTool`.
function makePluginApi(name) {
  return {
    addTool(def) {
      if (!def || !def.name || typeof def.execute !== "function") {
        throw new Error(`plugin '${name}': addTool needs { name, execute }`);
      }
      const tool = {
        name: def.name,
        label: def.name,
        description: def.description || "",
        parameters: def.parameters || { type: "object", properties: {} },
        execute: async (toolCallId, args) => {
          const r = await def.execute(args ?? {}, { toolCallId });
          return normalizePluginResult(r);
        },
      };
      agent.state.tools = [...agent.state.tools, tool];
    },
    onEvent(fn) {
      if (agent) agent.subscribe(fn);
    },
    callHostTool(hostName, args) {
      return host.tool(hostName, JSON.stringify(args ?? {}));
    },
    log(msg) {
      host.emit(JSON.stringify({ kind: "plugin_log", name, message: String(msg) }));
    },
  };
}

// The self-extension tool: the agent writes a TypeScript plugin, it's compiled
// and loaded live, and the tools it registers are callable on the next turn.
function definePluginTool() {
  return {
    name: "define_plugin",
    label: "define_plugin",
    description:
      "Give yourself a new tool by writing a TypeScript plugin. The source must " +
      "`export default (api) => { api.addTool({ name, description, parameters, execute }) }`, " +
      "where execute(args) returns a string or { content }. It is compiled and " +
      "loaded immediately — call the new tool on your next turn.",
    parameters: {
      type: "object",
      properties: {
        name: { type: "string", description: "A short id for the plugin." },
        typescript: { type: "string", description: "The plugin's TypeScript source." },
      },
      required: ["name", "typescript"],
    },
    execute: async (_id, args) => {
      try {
        globalThis.PocketPi.loadPlugin(args.name, args.typescript);
        return { content: [{ type: "text", text: `Plugin '${args.name}' compiled and loaded.` }], details: {} };
      } catch (e) {
        return {
          content: [{ type: "text", text: `Plugin failed: ${e && e.message ? e.message : e}` }],
          details: {},
        };
      }
    },
  };
}

function normalizePluginResult(r) {
  if (r == null) return { content: [{ type: "text", text: "" }], details: {} };
  if (typeof r === "string") return { content: [{ type: "text", text: r }], details: {} };
  if (Array.isArray(r.content)) return { content: r.content, details: r.details ?? {} };
  if (r.text != null) return { content: [{ type: "text", text: String(r.text) }], details: {} };
  return { content: [{ type: "text", text: JSON.stringify(r) }], details: {} };
}
