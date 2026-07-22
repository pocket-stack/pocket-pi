// Pocket Pi host harness. Reimplements the `PocketPi.boot/prompt/abort` surface
// — the API a Rust host (register_tool + boot + prompt + on_event + pump) drives
// — on top of the full, unmodified pi-coding-agent. Native tools bridge through
// `host.tool`; agent events map to the host's compact vocabulary via `host.emit`.
//
// This is loaded once by PiRuntime::new(), right after the full pi bundle.
import type { PiFullApi } from "./entry";

const P = globalThis.PiFull as PiFullApi;

const CWD = "/pocket-pi";
const AGENT_DIR = "/pocket-pi/.pi";
const state: { session: any } = { session: null };

const emit = (o: unknown) => globalThis.host.emit(JSON.stringify(o));

// Build a model descriptor from the boot config for the requested provider.
function buildModel(cfg: any, provider: string): any {
  const base = {
    id: cfg.model,
    name: cfg.model,
    provider,
    reasoning: false,
    input: ["text", "image"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 400000,
    maxTokens: cfg.maxTokens || 4096,
  };
  if (provider === "anthropic") {
    return { ...base, api: "anthropic-messages", baseUrl: "https://api.anthropic.com" };
  }
  return { ...base, api: "openai-responses", baseUrl: "https://api.openai.com/v1" };
}

// A pi extension that exposes each host-registered native tool (name +
// description + JSON-schema params from the boot config) as a pi tool whose
// execute() calls back into native Rust via host.tool.
function nativeToolsExtension(cfg: any) {
  return (pi: any) => {
    for (const t of cfg.tools || []) {
      pi.registerTool({
        name: t.name,
        description: t.description || "",
        parameters: t.parameters || { type: "object", properties: {} },
        execute: async (_id: string, input: any) => {
          const res = JSON.parse(globalThis.host.tool(t.name, JSON.stringify(input || {})));
          const content: any[] = [];
          if (res.text) content.push({ type: "text", text: res.text });
          if (res.image_base64) {
            content.push({ type: "image", data: res.image_base64, mimeType: res.mime_type || "image/jpeg" });
          }
          if (content.length === 0) content.push({ type: "text", text: "" });
          return { content, details: res.terminate ? { terminate: true } : undefined };
        },
      });
    }
  };
}

// Turn a message's content blocks into plain text.
function textOf(msg: any): string {
  const c = msg && msg.content;
  if (typeof c === "string") return c;
  if (Array.isArray(c)) return c.filter((b: any) => b && b.type === "text").map((b: any) => b.text).join("");
  return "";
}

async function boot(configJson: string): Promise<void> {
  globalThis.__ppBooted = false;
  try {
    const cfg = JSON.parse(configJson);
    const provider = cfg.provider || (String(cfg.model || "").startsWith("gpt") ? "openai" : "anthropic");

    const modelRuntime = await P.ModelRuntime.create({ modelsPath: null } as any);
    if (cfg.apiKey) await modelRuntime.setRuntimeApiKey(provider, cfg.apiKey);

    const settingsManager = P.SettingsManager.inMemory({});
    const sessionManager = P.SessionManager.inMemory();
    const resourceLoader = new P.DefaultResourceLoader({
      cwd: CWD,
      agentDir: AGENT_DIR,
      settingsManager,
      noExtensions: true,
      noSkills: true,
      noPromptTemplates: true,
      noThemes: true,
      noContextFiles: true,
      extensionFactories: [nativeToolsExtension(cfg)],
    } as any);
    if (resourceLoader.reload) await resourceLoader.reload();

    const { session } = await P.createAgentSession({
      model: buildModel(cfg, provider),
      modelRuntime,
      settingsManager,
      sessionManager,
      resourceLoader,
      cwd: CWD,
      agentDir: AGENT_DIR,
      thinkingLevel: "off",
      tools: (cfg.tools || []).map((t: any) => t.name),
    } as any);

    // Custom system prompt (base prompt the agent starts each turn with).
    if (cfg.systemPrompt) {
      try {
        (session as any)._baseSystemPrompt = cfg.systemPrompt;
        (session as any).agent.state.systemPrompt = cfg.systemPrompt;
      } catch {}
    }

    // Stream agent events to the host in its compact vocabulary.
    let lastText = "";
    session.subscribe((event: any) => {
      try {
        switch (event.type) {
          case "agent_start":
            lastText = "";
            emit({ kind: "start" });
            break;
          case "tool_execution_start":
            emit({ kind: "tool_start", name: event.toolName });
            break;
          case "message_update":
          case "message_end": {
            const msg = event.message;
            if (msg && msg.role !== "user") {
              const text = textOf(msg);
              if (text && text.length > lastText.length && text.startsWith(lastText)) {
                emit({ kind: "text", delta: text.slice(lastText.length) });
                lastText = text;
              } else if (text && text !== lastText) {
                emit({ kind: "assistant_text", text });
                lastText = text;
              }
              if (msg.stopReason === "error" && msg.errorMessage) {
                emit({ kind: "error", message: msg.errorMessage });
              }
            }
            break;
          }
          case "agent_settled":
            emit({ kind: "end" });
            break;
        }
      } catch {}
    });

    state.session = session;
  } catch (e) {
    emit({ kind: "error", message: String((e as any)?.stack || e) });
  } finally {
    globalThis.__ppBooted = true;
  }
}

async function prompt(text: string): Promise<void> {
  const s = state.session;
  if (!s) {
    emit({ kind: "error", message: "prompt before boot completed" });
    emit({ kind: "end" });
    return;
  }
  try {
    await s.prompt(text);
  } catch (e) {
    emit({ kind: "error", message: String((e as any)?.stack || e) });
    emit({ kind: "end" });
  }
}

function abort(): void {
  try {
    state.session?.abort?.();
  } catch {}
}

(globalThis as any).PocketPi = { boot, prompt, abort };
