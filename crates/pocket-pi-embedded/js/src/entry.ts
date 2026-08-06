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

const events: unknown[] = [];
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
  queueMicrotask(() => {
    try {
      const result = JSON.parse(globalThis.host?.modelComplete(JSON.stringify({ model, context })) || "{}") as ModelResult;
      const partial: any = {
        role: "assistant",
        content: [],
        api: model.api,
        provider: model.provider,
        model: model.id,
        usage: emptyUsage(),
        stopReason: result.stopReason || "stop",
        timestamp: Date.now(),
      };
      stream.push({ type: "start", partial });
      if (result.toolCall) {
        const toolCall = {
          type: "toolCall" as const,
          id: result.toolCall.id || `tool_${Date.now()}`,
          name: result.toolCall.name,
          arguments: result.toolCall.arguments || {},
        };
        partial.content = [toolCall];
        partial.stopReason = "toolUse";
        stream.push({ type: "toolcall_start", contentIndex: 0, partial: { ...partial } });
        stream.push({ type: "toolcall_end", contentIndex: 0, toolCall, partial: { ...partial } });
        stream.push({ type: "done", reason: "toolUse", message: { ...partial } });
      } else {
        const text = String(result.text || "");
        partial.content = [{ type: "text", text }];
        stream.push({ type: "text_start", contentIndex: 0, partial: { ...partial } });
        stream.push({ type: "text_delta", contentIndex: 0, delta: text, partial: { ...partial } });
        stream.push({ type: "text_end", contentIndex: 0, content: text, partial: { ...partial } });
        stream.push({ type: "done", reason: partial.stopReason, message: { ...partial } });
      }
    } catch (error) {
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
          errorMessage: String(error),
          timestamp: Date.now(),
        },
      });
    }
  });
  return stream;
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
    execute: async (id: string, args: unknown) => {
      const result = JSON.parse(
        globalThis.host?.tool(id, tool.name, JSON.stringify(args || {})) ||
          JSON.stringify({ text: `tool unavailable: ${tool.name}`, isError: true }),
      );
      if (result.isError) throw new Error(String(result.text || `tool failed: ${tool.name}`));
      return {
        content: [{ type: "text" as const, text: String(result.text || "") }],
        details: result.details,
        terminate: Boolean(result.terminate),
      };
    },
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

function drain(): string {
  return JSON.stringify({
    phase: agent?.state.isStreaming ? "thinking" : agent ? "ready" : "idle",
    messages: agent?.state.messages.length || 0,
    events: events.splice(0, events.length),
  });
}

globalThis.PocketPiEmbedded = { boot, prompt, abort, drain };
