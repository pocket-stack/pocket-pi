// Bundle the Pocket Pi guest to a single IIFE that QuickJS evaluates.
//
// Run with plain Node (no bun): `npm install && node js/build.mjs`. pi's agent
// core is a **real, unmodified npm dependency** (js/package.json) — sync it with
// `npm update @mariozechner/pi-agent-core` and rebuild. The runtime itself needs
// neither Node nor bun nor esbuild: it embeds the produced bundle via
// include_str! in the crate, and the committed bundle builds fully offline.
//
// The one deliberate substitution: pi's ~5 MB pi-ai (provider SDKs + typebox +
// a 553 KB model table) is aliased to src/pi-ai-stub.js — four stable symbols
// plus our own Rust-backed streamFns — per the runtime's "heavy Node dep →
// native op" rule. pi-agent-core (the loop that gets upstream feature updates)
// is resolved straight from node_modules, unmodified.

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");

if (!existsSync(join(here, "node_modules/@mariozechner/pi-agent-core"))) {
  console.error("pi-agent-core not installed — run `npm install` in js/ first.");
  process.exit(1);
}

const args = [
  "--yes",
  "esbuild@0.24",
  join(here, "src/entry.js"),
  "--bundle",
  "--format=iife",
  "--platform=neutral",
  "--legal-comments=none",
  `--alias:@mariozechner/pi-ai=${join(here, "src/pi-ai-stub.js")}`,
  // Reference the real, npm-installed upstream by its resolved entry (avoids
  // esbuild's platform=neutral exports-map quirk; still the unmodified package
  // that `npm update` syncs).
  `--alias:@mariozechner/pi-agent-core=${join(here, "node_modules/@mariozechner/pi-agent-core/dist/index.js")}`,
  `--outfile=${join(root, "crates/pocket-pi/js/agent.bundle.js")}`,
];

const r = spawnSync("npx", args, { stdio: "inherit" });
process.exit(r.status ?? 1);
