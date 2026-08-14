import { Agent } from "../node_modules/@earendil-works/pi-agent-core/dist/agent.js";
import { AssistantMessageEventStream } from "../node_modules/@earendil-works/pi-ai/dist/utils/event-stream.js";

type Config = {
  model?: string;
  provider?: string;
  thinkingLevel?: "high" | "xhigh";
  systemPrompt?: string;
  tools?: Array<{
    name: string;
    label?: string;
    description?: string;
    parameters?: Record<string, unknown>;
  }>;
};

type Usage = ReturnType<typeof emptyUsage>;

type ModelResult = {
  thinking: string;
  thinkingSignature?: string;
  text: string;
  toolCalls: Array<{
    id: string;
    name: string;
    arguments: Record<string, unknown>;
  }>;
  usage: Partial<Usage>;
  stopReason: "stop" | "length" | "toolUse";
};

type HostEvent =
  | { type: "model_progress"; id: number; thinkingDelta: string; textDelta: string }
  | { type: "model_done"; id: number; result: string }
  | { type: "model_error"; id: number; error: string }
  | { type: "tool_done"; id: number; result: string };

type PendingModel = {
  stream: AssistantMessageEventStream;
  model: any;
  partial: any;
  started: boolean;
  thinkingStarted: boolean;
  textStarted: boolean;
  thinking: string;
  text: string;
};

const events: unknown[] = [];
const pendingModels = new Map<number, PendingModel>();
const pendingTools = new Map<
  number,
  { resolve: (value: any) => void; reject: (error: Error) => void }
>();
let agent: Agent | null = null;

function toolsFor(definitions: NonNullable<Config["tools"]>): any[] {
  return definitions.map((tool) => ({
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
}

const emptyUsage = () => ({
  input: 0,
  output: 0,
  cacheRead: 0,
  cacheWrite: 0,
  reasoning: 0,
  totalTokens: 0,
  cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
});

function modelFor(config: Config): any {
  const provider = config.provider || "openai";
  const deepseek = provider === "deepseek";
  const anthropic = provider === "anthropic";
  const defaultModel = deepseek ? "deepseek-v4-flash" : "gpt-5-mini";
  return {
    id: config.model || defaultModel,
    name: config.model || defaultModel,
    provider,
    api: anthropic ? "anthropic-messages" : "openai-completions",
    baseUrl: deepseek
      ? "https://api.deepseek.com"
      : anthropic
        ? "https://api.anthropic.com"
        : "https://api.openai.com/v1",
    reasoning: deepseek,
    thinkingLevelMap: deepseek
      ? { off: null, minimal: null, low: null, medium: null, high: "high", xhigh: "max", max: "max" }
      : undefined,
    input: ["text"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: deepseek ? 1_000_000 : 128_000,
    maxTokens: deepseek ? 384_000 : 16_384,
  };
}

function hostStream(model: any, context: any, options: any = {}): AssistantMessageEventStream {
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
    const id = globalThis.host.startModel(JSON.stringify({ model, context, options }));
    pendingModels.set(id, {
      stream,
      model,
      partial,
      started: false,
      thinkingStarted: false,
      textStarted: false,
      thinking: "",
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

function syncContent(pending: PendingModel): void {
  const content: any[] = [];
  if (pending.thinkingStarted) {
    content.push({
      type: "thinking",
      thinking: pending.thinking,
      thinkingSignature: "reasoning_content",
    });
  }
  if (pending.textStarted) content.push({ type: "text", text: pending.text });
  pending.partial.content = content;
}

function pushThinkingDelta(pending: PendingModel, delta: string): void {
  if (!delta) return;
  if (pending.textStarted) throw new Error("thinking delta arrived after text output started");
  ensureModelStarted(pending);
  if (!pending.thinkingStarted) {
    pending.thinkingStarted = true;
    syncContent(pending);
    pending.stream.push({ type: "thinking_start", contentIndex: 0, partial: { ...pending.partial } });
  }
  pending.thinking += delta;
  syncContent(pending);
  pending.stream.push({
    type: "thinking_delta",
    contentIndex: 0,
    delta,
    partial: { ...pending.partial },
  });
}

function textIndex(pending: PendingModel): number {
  return pending.thinkingStarted ? 1 : 0;
}

function pushTextDelta(pending: PendingModel, delta: string): void {
  if (!delta) return;
  ensureModelStarted(pending);
  if (!pending.textStarted) {
    pending.textStarted = true;
    syncContent(pending);
    pending.stream.push({
      type: "text_start",
      contentIndex: textIndex(pending),
      partial: { ...pending.partial },
    });
  }
  pending.text += delta;
  syncContent(pending);
  pending.stream.push({
    type: "text_delta",
    contentIndex: textIndex(pending),
    delta,
    partial: { ...pending.partial },
  });
}

function appendFinalDelta(
  current: string,
  complete: string,
  append: (delta: string) => void,
  label: string,
): void {
  if (current === complete) return;
  if (!complete.startsWith(current)) throw new Error(`${label} stream does not match final result`);
  append(complete.slice(current.length));
}

function finishModel(pending: PendingModel, result: ModelResult): void {
  ensureModelStarted(pending);
  if (typeof result.thinking !== "string" || typeof result.text !== "string") {
    throw new Error("model result is missing thinking or text");
  }
  if (!Array.isArray(result.toolCalls)) throw new Error("model result is missing toolCalls");
  if (!result.usage || typeof result.usage !== "object") {
    throw new Error("model result is missing usage");
  }
  if (result.thinking && typeof result.thinkingSignature !== "string") {
    throw new Error("thinking result is missing thinkingSignature");
  }
  if (!(["stop", "length", "toolUse"] as const).includes(result.stopReason)) {
    throw new Error("model result has an invalid stopReason");
  }
  appendFinalDelta(
    pending.thinking,
    result.thinking,
    (delta) => pushThinkingDelta(pending, delta),
    "thinking",
  );
  appendFinalDelta(
    pending.text,
    result.text,
    (delta) => pushTextDelta(pending, delta),
    "text",
  );

  pending.partial.usage = { ...emptyUsage(), ...result.usage };
  if (pending.thinkingStarted) {
    const thinking = pending.partial.content[0];
    thinking.thinkingSignature = result.thinkingSignature;
    pending.stream.push({
      type: "thinking_end",
      contentIndex: 0,
      content: pending.thinking,
      partial: { ...pending.partial },
    });
  }
  if (pending.textStarted) {
    pending.stream.push({
      type: "text_end",
      contentIndex: textIndex(pending),
      content: pending.text,
      partial: { ...pending.partial },
    });
  }

  if (result.toolCalls.length > 0 && result.stopReason !== "toolUse") {
    throw new Error("tool calls require toolUse stopReason");
  }
  if (result.toolCalls.length === 0 && result.stopReason === "toolUse") {
    throw new Error("toolUse stopReason requires tool calls");
  }
  for (const call of result.toolCalls) {
    if (
      !call.id ||
      !call.name ||
      !call.arguments ||
      typeof call.arguments !== "object" ||
      Array.isArray(call.arguments)
    ) {
      throw new Error("model result contains an invalid tool call");
    }
    const toolCall = {
      type: "toolCall" as const,
      id: call.id,
      name: call.name,
      arguments: call.arguments,
    };
    const contentIndex = pending.partial.content.length;
    pending.partial.content = [...pending.partial.content, toolCall];
    pending.stream.push({ type: "toolcall_start", contentIndex, partial: { ...pending.partial } });
    pending.stream.push({
      type: "toolcall_end",
      contentIndex,
      toolCall,
      partial: { ...pending.partial },
    });
  }
  if (pending.partial.content.length === 0) throw new Error("model result contains no decision");

  pending.partial.stopReason = result.stopReason;
  pending.stream.push({ type: "done", reason: result.stopReason, message: { ...pending.partial } });
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
  const tools = toolsFor(config.tools || []);

  agent = new Agent({
    initialState: {
      systemPrompt: config.systemPrompt || "You are Pocket Pi running on an embedded device.",
      model,
      thinkingLevel: model.reasoning ? config.thinkingLevel || "high" : "off",
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

function tick(): void {
  const batch = JSON.parse(globalThis.host?.poll() || "[]") as HostEvent[];
  for (const event of batch) {
    if (event.type === "model_progress") {
      const pending = pendingModels.get(event.id);
      if (!pending) continue;
      pushThinkingDelta(pending, event.thinkingDelta);
      pushTextDelta(pending, event.textDelta);
    } else if (event.type === "model_done") {
      const pending = pendingModels.get(event.id);
      if (!pending) continue;
      pendingModels.delete(event.id);
      try {
        finishModel(pending, JSON.parse(event.result) as ModelResult);
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
        const result = JSON.parse(event.result);
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

function replaceTools(definitionsJson: string): void {
  if (!agent) throw new Error("replaceTools before boot");
  if (agent.state.isStreaming || pendingModels.size || pendingTools.size) {
    throw new Error("replaceTools while Agent is busy");
  }
  agent.state.tools = toolsFor(JSON.parse(definitionsJson));
}

globalThis.PocketPiEmbedded = { boot, prompt, tick, drain, replaceTools };
