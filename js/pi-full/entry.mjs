// Bundle entry for the unmodified pi-coding-agent core (Path B).
import {
  createAgentSession,
  SessionManager,
  SettingsManager,
  ModelRegistry,
  AuthStorage,
  InMemoryAuthStorageBackend,
  DefaultResourceLoader,
  createExtensionRuntime,
  createEventBus,
} from "@mariozechner/pi-coding-agent";
// loadExtensionFromFactory is inside the bundle but not on the top-level barrel
// (the package's exports map only exposes "." and "./hooks"). Reach it by file
// path — esbuild dedupes it to the same bundled module, so pi stays unmodified.
// This is the seam that lets Pocket Pi load extensions through its OWN oxc loader
// instead of jiti (which needs Node internals QuickJS doesn't have).
import { loadExtensionFromFactory } from "../node_modules/@mariozechner/pi-coding-agent/dist/core/extensions/loader.js";

globalThis.PiFull = {
  createAgentSession,
  SessionManager,
  SettingsManager,
  ModelRegistry,
  AuthStorage,
  InMemoryAuthStorageBackend,
  DefaultResourceLoader,
  createExtensionRuntime,
  createEventBus,
  loadExtensionFromFactory,
};
globalThis.__piFullLoaded = typeof createAgentSession === "function";
