(function() {
  const P = globalThis.PiFull;
  if (!P) throw new Error("PiFull not loaded \u2014 run the bundle first");
  const MODEL = {
    id: "gpt-5.6",
    name: "GPT-5.6",
    api: "openai-responses",
    provider: "openai",
    baseUrl: "https://api.openai.com/v1",
    reasoning: false,
    input: ["text"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 4e5,
    maxTokens: 8192
  };
  const CWD = "/pocket-pi";
  const AGENT_DIR = "/pocket-pi/.pi";
  globalThis.__piResult = "";
  globalThis.__piError = null;
  globalThis.__piDone = false;
  globalThis.__piBind = null;
  globalThis.__piLog = [];
  const takeText = (msg) => {
    if (!msg) return;
    const content = msg.content;
    if (Array.isArray(content)) {
      let t = "";
      for (const b of content) {
        if (!b) continue;
        if (b.type === "text" && typeof b.text === "string") t += b.text;
        else if (b.type === "error") t += "[ERROR] " + String(b.error || b.text || "");
      }
      if (t) globalThis.__piResult = t;
    } else if (typeof content === "string") {
      globalThis.__piResult = content;
    }
  };
  globalThis.__piRun = async function(opts) {
    opts = typeof opts === "string" ? opts.trim().startsWith("{") ? JSON.parse(opts) : { prompt: opts } : opts || {};
    try {
      const modelRuntime = await P.ModelRuntime.create({ modelsPath: null });
      if (globalThis.__OPENAI_KEY) await modelRuntime.setRuntimeApiKey("openai", globalThis.__OPENAI_KEY);
      const settingsManager = P.SettingsManager.inMemory({});
      const sessionManager = opts.sessionDir ? opts.resume ? P.SessionManager.continueRecent(CWD, opts.sessionDir) : P.SessionManager.create(CWD, opts.sessionDir) : P.SessionManager.inMemory();
      const extensionFactories = [];
      if (opts.extensionPath) {
        const mod = await import(opts.extensionPath);
        if (typeof mod.default === "function") extensionFactories.push(mod.default);
        else throw new Error("extension has no default factory export: " + opts.extensionPath);
      }
      const resourceLoader = new P.DefaultResourceLoader({
        cwd: CWD,
        agentDir: AGENT_DIR,
        settingsManager,
        noExtensions: true,
        noSkills: true,
        noPromptTemplates: true,
        noThemes: true,
        noContextFiles: true,
        extensionFactories
      });
      if (resourceLoader.reload) await resourceLoader.reload();
      const sessionOpts = {
        model: MODEL,
        modelRuntime,
        settingsManager,
        sessionManager,
        resourceLoader,
        cwd: CWD,
        agentDir: AGENT_DIR,
        thinkingLevel: "off"
      };
      if (opts.tools) sessionOpts.tools = opts.tools;
      else sessionOpts.noTools = "all";
      const { session } = await P.createAgentSession(sessionOpts);
      try {
        const runner = session._extensionRunner;
        const regTools = runner && runner.getAllRegisteredTools ? runner.getAllRegisteredTools() : [];
        globalThis.__piBind = {
          hasAgentStart: !!(runner && runner.hasHandlers && runner.hasHandlers("agent_start")),
          registeredTools: regTools.map((t) => t && (t.name || t.definition && t.definition.name)).filter(Boolean)
        };
      } catch (e) {
        globalThis.__piBind = { error: String(e) };
      }
      session.subscribe((event) => {
        try {
          if (!event) return;
          globalThis.__piLastEvent = event.type;
          if (event.type === "message_update" || event.type === "message_end") {
            if (event.message && event.message.role !== "user") takeText(event.message);
            if (event.message && event.message.stopReason === "error" && event.message.errorMessage) {
              globalThis.__piError = String(event.message.errorMessage);
            }
          } else if (event.type === "error") {
            globalThis.__piError = String(event.error || event.message || "agent error");
          }
        } catch {
        }
      });
      if (opts.prompt) await session.prompt(opts.prompt);
      globalThis.__piDone = true;
    } catch (e) {
      globalThis.__piError = String(e && e.stack || e);
      globalThis.__piDone = true;
    }
  };
})();
