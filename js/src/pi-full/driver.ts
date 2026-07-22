// Path B session harness: stand up an AgentSession from the UNMODIFIED bundled
// pi-coding-agent and (optionally) run one turn, load an extension, enable tools,
// or persist/resume — all through Pocket Pi's runtime (fetch → __catpiFetchPump →
// Rust HTTP hub → system proxy; our oxc loader for extension .ts; our fs for the
// session store). Loaded as a plain script after pi-full.bundle.js. Results land
// on globalThis for the Rust side to poll.
//
// __piRun(opts) — opts (object or JSON string):
//   prompt?:        string   — send one turn (omit to just build the session)
//   extensionPath?: string   — absolute path to a .ts/.js extension (our loader)
//   tools?:         string[] — active tool names (omit → noTools:"all")
//   sessionDir?:    string   — persist the session here (SessionManager.create)
//   resume?:        bool     — resume the most recent session in sessionDir

(function () {
  const P = globalThis.PiFull;
  if (!P) throw new Error("PiFull not loaded — run the bundle first");

  const MODEL = {
    id: "gpt-5.6",
    name: "GPT-5.6",
    api: "openai-responses",
    provider: "openai",
    baseUrl: "https://api.openai.com/v1",
    reasoning: false,
    input: ["text"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 400000,
    maxTokens: 8192,
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

  globalThis.__piRun = async function (opts) {
    opts = typeof opts === "string" ? (opts.trim().startsWith("{") ? JSON.parse(opts) : { prompt: opts }) : (opts || {});
    try {
      // 0.81: model + auth live on a ModelRuntime (in-memory, offline). The
      // custom gpt-5.6 model is passed straight to createAgentSession; auth is a
      // runtime API-key override for the "openai" provider.
      const modelRuntime = await P.ModelRuntime.create({ modelsPath: null });
      if (globalThis.__OPENAI_KEY) await modelRuntime.setRuntimeApiKey("openai", globalThis.__OPENAI_KEY);

      const settingsManager = P.SettingsManager.inMemory({});
      const sessionManager = opts.sessionDir
        ? (opts.resume ? P.SessionManager.continueRecent(CWD, opts.sessionDir) : P.SessionManager.create(CWD, opts.sessionDir))
        : P.SessionManager.inMemory();

      // Load an extension through OUR loader (oxc .ts transpile), not jiti, and
      // inject its factory — DefaultResourceLoader wires it via loadExtensionFromFactory.
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
        extensionFactories,
      });
      if (resourceLoader.reload) await resourceLoader.reload();

      const sessionOpts: Record<string, any> = {
        model: MODEL,
        modelRuntime,
        settingsManager,
        sessionManager,
        resourceLoader,
        cwd: CWD,
        agentDir: AGENT_DIR,
        thinkingLevel: "off",
      };
      if (opts.tools) sessionOpts.tools = opts.tools;
      else sessionOpts.noTools = "all";

      const { session } = await P.createAgentSession(sessionOpts);

      // Report extension binding into the live session (offline-observable).
      try {
        const runner = session._extensionRunner;
        const regTools = runner && runner.getAllRegisteredTools ? runner.getAllRegisteredTools() : [];
        globalThis.__piBind = {
          hasAgentStart: !!(runner && runner.hasHandlers && runner.hasHandlers("agent_start")),
          registeredTools: regTools.map((t) => t && (t.name || (t.definition && t.definition.name))).filter(Boolean),
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
        } catch {}
      });

      if (opts.prompt) await session.prompt(opts.prompt);
      globalThis.__piDone = true;
    } catch (e) {
      globalThis.__piError = String((e && e.stack) || e);
      globalThis.__piDone = true;
    }
  };
})();
