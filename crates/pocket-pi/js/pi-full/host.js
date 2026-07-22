const P = globalThis.PiFull;
const CWD = "/pocket-pi";
const AGENT_DIR = "/pocket-pi/.pi";
const state = { session: null };
const emit = (o) => globalThis.host.emit(JSON.stringify(o));
function buildModel(cfg, provider) {
  const base = {
    id: cfg.model,
    name: cfg.model,
    provider,
    reasoning: false,
    input: ["text", "image"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 4e5,
    maxTokens: cfg.maxTokens || 4096
  };
  if (provider === "anthropic") {
    return { ...base, api: "anthropic-messages", baseUrl: "https://api.anthropic.com" };
  }
  return { ...base, api: "openai-responses", baseUrl: "https://api.openai.com/v1" };
}
function nativeToolsExtension(cfg) {
  return (pi) => {
    for (const t of cfg.tools || []) {
      pi.registerTool({
        name: t.name,
        description: t.description || "",
        parameters: t.parameters || { type: "object", properties: {} },
        execute: async (_id, input) => {
          const res = JSON.parse(globalThis.host.tool(t.name, JSON.stringify(input || {})));
          const content = [];
          if (res.text) content.push({ type: "text", text: res.text });
          if (res.image_base64) {
            content.push({ type: "image", data: res.image_base64, mimeType: res.mime_type || "image/jpeg" });
          }
          if (content.length === 0) content.push({ type: "text", text: "" });
          return { content, details: res.terminate ? { terminate: true } : void 0 };
        }
      });
    }
  };
}
function textOf(msg) {
  const c = msg && msg.content;
  if (typeof c === "string") return c;
  if (Array.isArray(c)) return c.filter((b) => b && b.type === "text").map((b) => b.text).join("");
  return "";
}
async function boot(configJson) {
  globalThis.__ppBooted = false;
  try {
    const cfg = JSON.parse(configJson);
    const provider = cfg.provider || (String(cfg.model || "").startsWith("gpt") ? "openai" : "anthropic");
    const modelRuntime = await P.ModelRuntime.create({ modelsPath: null });
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
      extensionFactories: [nativeToolsExtension(cfg)]
    });
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
      tools: (cfg.tools || []).map((t) => t.name)
    });
    if (cfg.systemPrompt) {
      try {
        session._baseSystemPrompt = cfg.systemPrompt;
        session.agent.state.systemPrompt = cfg.systemPrompt;
      } catch {
      }
    }
    let lastText = "";
    session.subscribe((event) => {
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
      } catch {
      }
    });
    state.session = session;
  } catch (e) {
    emit({ kind: "error", message: String(e?.stack || e) });
  } finally {
    globalThis.__ppBooted = true;
  }
}
async function prompt(text) {
  const s = state.session;
  if (!s) {
    emit({ kind: "error", message: "prompt before boot completed" });
    emit({ kind: "end" });
    return;
  }
  try {
    await s.prompt(text);
  } catch (e) {
    emit({ kind: "error", message: String(e?.stack || e) });
    emit({ kind: "end" });
  }
}
function abort() {
  try {
    state.session?.abort?.();
  } catch {
  }
}
globalThis.PocketPi = { boot, prompt, abort };
