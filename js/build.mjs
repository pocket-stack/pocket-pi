// Bundle the Pocket Pi guest to a single IIFE that QuickJS evaluates.
//
// Run with plain Node (no bun): `node js/build.mjs`. It shells esbuild via npx,
// so the only build-time dependency is a network-reachable npm the first time
// (esbuild is cached afterwards). The runtime itself needs neither Node nor bun
// nor esbuild — it embeds the produced bundle via include_str! in the crate.
//
// Aliases keep the bundle tiny: pi's ~5 MB pi-ai (provider SDKs + typebox +
// a 553 KB model table) is replaced by src/pi-ai-stub.js, leaving just the
// ~43 KB pi-agent-core loop plus our streamFns.

import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");

const args = [
  "--yes",
  "esbuild@0.24",
  join(here, "src/entry.js"),
  "--bundle",
  "--format=iife",
  "--platform=neutral",
  "--legal-comments=none",
  `--alias:@mariozechner/pi-ai=${join(here, "src/pi-ai-stub.js")}`,
  `--alias:@mariozechner/pi-agent-core=${join(here, "vendor/pi-agent-core/dist/index.js")}`,
  `--outfile=${join(root, "crates/pocket-pi/js/agent.bundle.js")}`,
];

const r = spawnSync("npx", args, { stdio: "inherit" });
process.exit(r.status ?? 1);
