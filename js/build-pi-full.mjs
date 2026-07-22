// Bundle the UNMODIFIED, full pi-coding-agent into a single ES module that
// QuickJS evaluates as one unit — sidestepping QuickJS's multi-module linker
// crash on pi's ~500-module circular import graph. Unlike build.mjs (which stubs
// pi-ai), this keeps *every* dependency real: a minimal QuickJS runtime that runs
// pi with zero source edits, synced via `npm update`.
//
// Run with plain Node: `npm install && node js/build-pi-full.mjs`. Only node:*
// builtins stay external — Pocket Pi supplies them at runtime. Emits two
// artifacts (both git-ignored):
//   crates/pocket-pi/js/pi-full.bundle.js      minified ESM (~6.5 MB) — path-loaded by tests
//   crates/pocket-pi/js/pi-full.bundle.js.gz   gzip of the above (~1.6 MB) — embedded in the binary

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const outFile = join(root, "crates/pocket-pi/js/pi-full.bundle.js");

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
  // platform=node marks node:* builtins external; Pocket Pi supplies them.
  "--platform=node",
  "--legal-comments=none",
  // Shrink the bundle with WHITESPACE minification only. --minify-identifiers
  // (renaming) and --minify-syntax (AST rewrites) both produce output the
  // embedded QuickJS parser rejects, so we skip them; gzip reclaims most of the
  // difference anyway. --line-limit wraps the long lines QuickJS chokes on.
  "--minify-whitespace",
  "--line-limit=500",
  // undici is pi's HTTP-proxy transport; Pocket Pi replaces it with the native
  // streaming fetch, so alias it to a stub (never used at runtime, keeps its huge
  // web-fetch stack out of the bundle). pi's own source stays unmodified.
  `--alias:undici=${join(here, "pi-full/undici-stub.js")}`,
  `--outfile=${outFile}`,
];

const r = spawnSync("npx", args, { stdio: "inherit" });
if (r.status !== 0) process.exit(r.status ?? 1);

// Compress for the embedded (self-contained binary) distribution path.
const src = readFileSync(outFile);
const gz = gzipSync(src, { level: 9 });
writeFileSync(`${outFile}.gz`, gz);
const mb = (n) => (n / 1048576).toFixed(1);
console.log(`bundle: ${mb(src.length)} MB minified → ${mb(gz.length)} MB gzip (${outFile}.gz)`);
