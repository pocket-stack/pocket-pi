globalThis.__piExtResult = null;
globalThis.__piExtError = null;
globalThis.__piExtDone = false;
globalThis.__piLoadExtension = async function(extPath) {
  try {
    const P = globalThis.PiFull;
    if (!P) throw new Error("PiFull not loaded \u2014 run the bundle first");
    const mod = await import(extPath);
    const factory = mod.default;
    if (typeof factory !== "function") throw new Error("extension has no default factory export");
    const runtime = P.createExtensionRuntime();
    const eventBus = P.createEventBus();
    const ext = await P.loadExtensionFromFactory(factory, "/pocket-pi", eventBus, runtime, extPath);
    globalThis.__piExtResult = {
      tools: [...ext.tools.keys()],
      handlers: [...ext.handlers.keys()]
    };
  } catch (e) {
    globalThis.__piExtError = String(e && e.stack || e);
  } finally {
    globalThis.__piExtDone = true;
  }
};
