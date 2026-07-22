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

const PiFull = {
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
globalThis.PiFull = PiFull;
globalThis.__piFullLoaded = typeof createAgentSession === "function";

// The exact shape of globalThis.PiFull, carrying pi's REAL types. The harness
// (loaded as a separate script after this bundle) casts globalThis.PiFull to
// this, so its session/model/auth calls are typechecked against pi's actual API
// — a pi bump that changes those signatures fails `tsc`, not just a runtime test.
export type PiFullApi = typeof PiFull;
