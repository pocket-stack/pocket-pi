import { Agent } from "../node_modules/@earendil-works/pi-agent-core/dist/agent.js";
import { AssistantMessageEventStream } from "../node_modules/@earendil-works/pi-ai/dist/utils/event-stream.js";

type Config = {
  model?: string;
  provider?: string;
  systemPrompt?: string;
  tools?: Array<{
    name: string;
    label?: string;
    description?: string;
    parameters?: Record<string, unknown>;
  }>;
};

type ModelResult = {
  text?: string;
  stopReason?: "stop" | "length";
  toolCall?: { id?: string; name: string; arguments?: Record<string, unknown> };
};

type HostEvent =
  | { type: "model_delta"; id: number; delta: string }
  | { type: "model_done"; id: number; result: string }
  | { type: "model_error"; id: number; error: string }
  | { type: "tool_done"; id: number; result: string };

type PendingModel = {
  stream: AssistantMessageEventStream;
  model: any;
  partial: any;
  started: boolean;
  textStarted: boolean;
  text: string;
};

const events: unknown[] = [];
const pendingModels = new Map<number, PendingModel>();
const pendingTools = new Map<
  number,
  { resolve: (value: any) => void; reject: (error: Error) => void }
>();
let agent: Agent | null = null;

const emptyUsage = () => ({
  input: 0,
  output: 0,
  cacheRead: 0,
  cacheWrite: 0,
  totalTokens: 0,
  cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
});

function modelFor(config: Config): any {
  const provider = config.provider || "openai";
  return {
    id: config.model || "gpt-5-mini",
    name: config.model || "gpt-5-mini",
    provider,
    api: provider === "anthropic" ? "anthropic-messages" : "openai-responses",
    baseUrl: provider === "anthropic" ? "https://api.anthropic.com" : "https://api.openai.com/v1",
    reasoning: false,
    input: ["text"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 128000,
    maxTokens: 1024,
  };
}

function hostStream(model: any, context: any): AssistantMessageEventStream {
  const stream = new AssistantMessageEventStream();
  const partial: any = {
    role: "assistant",
    content: [],
    api: model.api,
    provider: model.provider,
    model: model.id,
    usage: emptyUsage(),
    stopReason: "stop",
    timestamp: Date.now(),
  };
  try {
    if (!globalThis.host) throw new Error("Pocket Pi Agent host is unavailable");
    const id = globalThis.host.startModel(JSON.stringify({ model, context }));
    pendingModels.set(id, {
      stream,
      model,
      partial,
      started: false,
      textStarted: false,
      text: "",
    });
  } catch (error) {
    pushModelError(stream, model, String(error));
  }
  return stream;
}

function ensureModelStarted(pending: PendingModel): void {
  if (pending.started) return;
  pending.started = true;
  pending.stream.push({ type: "start", partial: { ...pending.partial } });
}

function pushModelDelta(pending: PendingModel, delta: string): void {
  ensureModelStarted(pending);
  if (!pending.textStarted) {
    pending.textStarted = true;
    pending.stream.push({ type: "text_start", contentIndex: 0, partial: { ...pending.partial } });
  }
  pending.text += delta;
  pending.partial.content = [{ type: "text", text: pending.text }];
  pending.stream.push({
    type: "text_delta",
    contentIndex: 0,
    delta,
    partial: { ...pending.partial },
  });
}

function finishModel(pending: PendingModel, result: ModelResult): void {
  ensureModelStarted(pending);
  pending.partial.stopReason = result.stopReason || "stop";
  if (result.toolCall) {
    const toolCall = {
      type: "toolCall" as const,
      id: result.toolCall.id || `tool_${Date.now()}`,
      name: result.toolCall.name,
      arguments: result.toolCall.arguments || {},
    };
    pending.partial.content = [toolCall];
    pending.partial.stopReason = "toolUse";
    pending.stream.push({ type: "toolcall_start", contentIndex: 0, partial: { ...pending.partial } });
    pending.stream.push({ type: "toolcall_end", contentIndex: 0, toolCall, partial: { ...pending.partial } });
    pending.stream.push({ type: "done", reason: "toolUse", message: { ...pending.partial } });
    return;
  }

  const finalText = String(result.text || pending.text);
  if (!pending.textStarted || finalText !== pending.text) {
    const delta = finalText.startsWith(pending.text) ? finalText.slice(pending.text.length) : finalText;
    if (delta) pushModelDelta(pending, delta);
  }
  if (!pending.textStarted) pushModelDelta(pending, "");
  pending.text = finalText;
  pending.partial.content = [{ type: "text", text: finalText }];
  pending.stream.push({ type: "text_end", contentIndex: 0, content: finalText, partial: { ...pending.partial } });
  pending.stream.push({ type: "done", reason: pending.partial.stopReason, message: { ...pending.partial } });
}

function pushModelError(stream: AssistantMessageEventStream, model: any, message: string): void {
  stream.push({
    type: "error",
    reason: "error",
    error: {
      role: "assistant",
      content: [],
      api: model.api,
      provider: model.provider,
      model: model.id,
      usage: emptyUsage(),
      stopReason: "error",
      errorMessage: message,
      timestamp: Date.now(),
    },
  });
}

function boot(configJson: string): void {
  const config = JSON.parse(configJson) as Config;
  const model = modelFor(config);
  const tools = (config.tools || []).map((tool) => ({
    name: tool.name,
    label: tool.label || tool.name,
    description: tool.description || "",
    parameters: tool.parameters || { type: "object", properties: {} },
    executionMode: "sequential" as const,
    execute: (id: string, args: unknown) =>
      new Promise<any>((resolve, reject) => {
        try {
          if (!globalThis.host) throw new Error("Pocket Pi Agent host is unavailable");
          const requestId = globalThis.host.startTool(id, tool.name, JSON.stringify(args || {}));
          pendingTools.set(requestId, { resolve, reject });
        } catch (error) {
          reject(error instanceof Error ? error : new Error(String(error)));
        }
      }),
  }));

  agent = new Agent({
    initialState: {
      systemPrompt: config.systemPrompt || "You are Pocket Pi running on an embedded device.",
      model,
      thinkingLevel: "off",
      tools,
    },
    streamFn: hostStream as any,
    toolExecution: "sequential",
  });
  agent.subscribe((event: any) => {
    const compact: any = { type: event.type };
    if (event.type === "message_update") {
      compact.kind = event.assistantMessageEvent?.type;
      compact.delta = event.assistantMessageEvent?.delta;
    } else if (event.type === "message_end") {
      compact.role = event.message?.role;
      compact.stopReason = event.message?.stopReason;
    } else if (event.type === "tool_execution_start" || event.type === "tool_execution_end") {
      compact.name = event.toolName;
      compact.toolCallId = event.toolCallId;
      compact.isError = Boolean(event.isError);
    }
    events.push(compact);
  });
  events.push({ type: "agent_ready" });
}

function prompt(text: string): void {
  if (!agent) throw new Error("prompt before boot");
  void agent.prompt(text).catch((error) => events.push({ type: "agent_error", message: String(error) }));
}

function abort(): void {
  agent?.abort();
}

function tick(): void {
  const batch = JSON.parse(globalThis.host?.poll() || "[]") as HostEvent[];
  for (const event of batch) {
    if (event.type === "model_delta") {
      const pending = pendingModels.get(event.id);
      if (pending) pushModelDelta(pending, String(event.delta || ""));
    } else if (event.type === "model_done") {
      const pending = pendingModels.get(event.id);
      if (!pending) continue;
      pendingModels.delete(event.id);
      try {
        finishModel(pending, JSON.parse(event.result || "{}") as ModelResult);
      } catch (error) {
        pushModelError(pending.stream, pending.model, String(error));
      }
    } else if (event.type === "model_error") {
      const pending = pendingModels.get(event.id);
      if (!pending) continue;
      pendingModels.delete(event.id);
      pushModelError(pending.stream, pending.model, event.error);
    } else if (event.type === "tool_done") {
      const pending = pendingTools.get(event.id);
      if (!pending) continue;
      pendingTools.delete(event.id);
      try {
        const result = JSON.parse(event.result || "{}");
        if (result.isError) throw new Error(String(result.text || "App tool failed"));
        pending.resolve({
          content: [{ type: "text" as const, text: String(result.text || "") }],
          details: result.details,
          terminate: Boolean(result.terminate),
        });
      } catch (error) {
        pending.reject(error instanceof Error ? error : new Error(String(error)));
      }
    }
  }
}

function drain(): string {
  return JSON.stringify({
    phase: agent?.state.isStreaming ? "thinking" : agent ? "ready" : "idle",
    messages: agent?.state.messages.length || 0,
    events: events.splice(0, events.length),
  });
}

globalThis.PocketPiEmbedded = { boot, prompt, abort, tick, drain };
