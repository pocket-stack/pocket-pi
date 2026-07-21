// Path B driver: stand up an AgentSession from the UNMODIFIED bundled
// pi-coding-agent and run one turn against gpt-5.6 through Pocket Pi's fetch
// (→ __catpiFetchPump → Rust HTTP hub → system proxy). Loaded as a plain script
// after pi-full.bundle.js, so it defines async entry points instead of using
// top-level await. Results land on globalThis for the Rust side to poll.

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

  globalThis.__piResult = "";
  globalThis.__piDone = false;
  globalThis.__piError = null;

  globalThis.__piRun = async function (text) {
    try {
      const key = globalThis.__OPENAI_KEY;
      if (!key) throw new Error("__OPENAI_KEY not set");

      const authBackend = new P.InMemoryAuthStorageBackend();
      const authStorage = P.AuthStorage.fromStorage(authBackend);
      authStorage.setRuntimeApiKey("openai", key);

      const modelRegistry = P.ModelRegistry.inMemory(authStorage);
      // Make the custom model discoverable for auth/selection too.
      try { modelRegistry.getAll().push(MODEL); } catch {}

      const settingsManager = P.SettingsManager.inMemory({});
      const sessionManager = P.SessionManager.inMemory();
      const resourceLoader = new P.DefaultResourceLoader({
        cwd: "/pocket-pi",
        agentDir: "/pocket-pi/.pi",
        settingsManager,
        noExtensions: true,
        noSkills: true,
        noPromptTemplates: true,
        noThemes: true,
        noContextFiles: true,
      });
      if (resourceLoader.reload) await resourceLoader.reload();

      const { session } = await P.createAgentSession({
        model: MODEL,
        authStorage,
        modelRegistry,
        settingsManager,
        sessionManager,
        resourceLoader,
        cwd: "/pocket-pi",
        agentDir: "/pocket-pi/.pi",
        noTools: "all",
        thinkingLevel: "off",
      });

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
            else if (b.type === "reasoning" && typeof b.text === "string") { /* skip */ }
          }
          if (t) globalThis.__piResult = t;
        } else if (typeof content === "string") {
          globalThis.__piResult = content;
        }
      };
      session.subscribe((event) => {
        try {
          if (!event) return;
          globalThis.__piLastEvent = event.type;
          let snip = "";
          try {
            if (event.message) snip = JSON.stringify(event.message).slice(0, 400);
            else if (event.error) snip = String(event.error);
          } catch {}
          globalThis.__piLog.push(event.type + (snip ? " :: " + snip : ""));
          if (event.type === "message_update" || event.type === "message_end") {
            if (event.message && event.message.role !== "user") takeText(event.message);
            if (event.message && event.message.stopReason === "error" && event.message.errorMessage) {
              globalThis.__piError = String(event.message.errorMessage);
            }
          } else if (event.type === "error") {
            globalThis.__piError = String(event.error || event.message || "agent error");
          }
        } catch (e) {
          globalThis.__piLog.push("listener-threw:" + String(e));
        }
      });

      await session.prompt(text);
      globalThis.__piDone = true;
    } catch (e) {
      globalThis.__piError = String((e && e.stack) || e);
      globalThis.__piDone = true;
    }
  };
})();
