// Bundle entry for the unmodified pi-coding-agent core.
import {
  createAgentSession,
  SessionManager,
  SettingsManager,
  ModelRegistry,
  DefaultResourceLoader,
  createExtensionRuntime,
  createEventBus,
} from "@earendil-works/pi-coding-agent";
// A few symbols aren't on the top-level barrel (the package's exports map only
// exposes "." and "./hooks"). Reach them by file path — esbuild dedupes to the
// same bundled module, so pi stays unmodified. This is what lets Pocket Pi wire
// model/auth/extensions headlessly.
import { ModelRuntime } from "../../node_modules/@earendil-works/pi-coding-agent/dist/core/model-runtime.js";
import {
  AuthStorage,
  InMemoryAuthStorageBackend,
} from "../../node_modules/@earendil-works/pi-coding-agent/dist/core/auth-storage.js";
import { loadExtensionFromFactory } from "../../node_modules/@earendil-works/pi-coding-agent/dist/core/extensions/loader.js";

globalThis.PiFull = {
  createAgentSession,
  SessionManager,
  SettingsManager,
  ModelRegistry,
  ModelRuntime,
  AuthStorage,
  InMemoryAuthStorageBackend,
  DefaultResourceLoader,
  createExtensionRuntime,
  createEventBus,
  loadExtensionFromFactory,
};
globalThis.__piFullLoaded = typeof createAgentSession === "function";
