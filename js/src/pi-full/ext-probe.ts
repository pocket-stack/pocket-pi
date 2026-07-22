// Load a real pi extension through Pocket Pi's own module loader (oxc `.ts`
// transpile — NOT jiti), then hand its default-export factory to pi's unmodified
// loadExtensionFromFactory. Proves an unmodified extension registers its tools
// and hooks under Pocket Pi. Results land on globalThis for the Rust side.

globalThis.__piExtResult = null;
globalThis.__piExtError = null;
globalThis.__piExtDone = false;

globalThis.__piLoadExtension = async function (extPath) {
  try {
    const P = globalThis.PiFull as import("./entry").PiFullApi;
    if (!P) throw new Error("PiFull not loaded — run the bundle first");

    // Dynamic import routes through Pocket Pi's NodeResolver/NodeLoader, which
    // transpiles the .ts natively and returns the factory as the default export.
    const mod = await import(extPath);
    const factory = mod.default;
    if (typeof factory !== "function") throw new Error("extension has no default factory export");

    const runtime = P.createExtensionRuntime();
    const eventBus = P.createEventBus();
    const ext = await P.loadExtensionFromFactory(factory, "/pocket-pi", eventBus, runtime, extPath);

    globalThis.__piExtResult = {
      tools: [...ext.tools.keys()],
      handlers: [...ext.handlers.keys()],
    };
  } catch (e) {
    globalThis.__piExtError = String((e && e.stack) || e);
  } finally {
    globalThis.__piExtDone = true;
  }
};
