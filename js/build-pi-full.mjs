// Path B: bundle the UNMODIFIED, full pi-coding-agent into a single ES module
// that QuickJS evaluates as one unit — sidestepping QuickJS's multi-module
// linker crash on pi's ~500-module circular import graph. Unlike build.mjs
// (which stubs pi-ai), this keeps *every* dependency real: the goal is a minimal
// QuickJS runtime that runs pi with zero source edits, synced via `npm update`.
//
// Run with plain Node: `npm install && node js/build-pi-full.mjs`. Only node:*
// builtins stay external — Pocket Pi supplies them at runtime (crates/pocket-pi/
// js/node/*.js + web-globals.js). The produced bundle is large (~13 MB) and is
// git-ignored; the Path B integration tests skip gracefully when it is absent.

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");

if (!existsSync(join(here, "node_modules/@earendil-works/pi-coding-agent"))) {
  console.error("pi-coding-agent not installed — run `npm install` in js/ first.");
  process.exit(1);
}

const args = [
  "--yes",
  "esbuild@0.24",
  join(here, "pi-full/entry.mjs"),
  "--bundle",
  "--format=esm",
  // platform=node marks node:* builtins external; Pocket Pi provides them.
  "--platform=node",
  "--legal-comments=none",
  // undici is pi's HTTP-proxy transport; Pocket Pi replaces it with the native
  // streaming fetch, so alias it to a stub (never used at runtime, keeps its
  // huge web-fetch stack out of the bundle). pi's own source stays unmodified.
  `--alias:undici=${join(here, "pi-full/undici-stub.js")}`,
  `--outfile=${join(root, "crates/pocket-pi/js/pi-full.bundle.js")}`,
];

const r = spawnSync("npx", args, { stdio: "inherit" });
process.exit(r.status ?? 1);
