// Bundle entry for the unmodified pi-coding-agent core (Path B).
import {
  createAgentSession,
  SessionManager,
  SettingsManager,
  ModelRegistry,
  AuthStorage,
  InMemoryAuthStorageBackend,
  DefaultResourceLoader,
} from "@mariozechner/pi-coding-agent";

globalThis.PiFull = {
  createAgentSession,
  SessionManager,
  SettingsManager,
  ModelRegistry,
  AuthStorage,
  InMemoryAuthStorageBackend,
  DefaultResourceLoader,
};
globalThis.__piFullLoaded = typeof createAgentSession === "function";
