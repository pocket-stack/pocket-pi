// Unified build for all of Pocket Pi's JavaScript guest layer.
//
// Sources are TypeScript under js/src/; this emits the artifacts the Rust crate
// embeds. Run with plain Node: `npm install && node js/build.mjs`.
//
//   src/runtime/**       →  crates/pocket-pi/js/**            (per-file transpile; embedded via include_str!)
//   src/pi-full/{driver,ext-probe,persist-probe}
//                        →  crates/pocket-pi/js/pi-full/**    (test harness scripts)
//   src/trimmed/entry    →  crates/pocket-pi/js/agent.bundle.js       (trimmed core, pi-ai stubbed)
//   src/pi-full/entry    →  crates/pocket-pi/js/pi-full.bundle.js(.gz) (full unmodified pi)
//
// pi is a real, unmodified npm dependency (package.json) — sync with `npm update`
// and rerun this. The emitted runtime .js and agent.bundle.js are committed so
// `cargo` builds Rust-only; the full pi bundle is git-ignored (built on demand).

import * as esbuild from "esbuild";
import { execFileSync } from "node:child_process";
import { gzipSync } from "node:zlib";
import { readFileSync, writeFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, relative } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const outJs = join(root, "crates/pocket-pi/js");
const nm = join(here, "node_modules");

if (!existsSync(join(nm, "@earendil-works/pi-coding-agent"))) {
  console.error("pi packages not installed — run `npm install` in js/ first.");
  process.exit(1);
}

const mb = (n) => (n / 1048576).toFixed(1);
function walk(dir) {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    return statSync(p).isDirectory() ? walk(p) : [p];
  });
}

// 1. Typecheck gate over Pocket Pi's own orchestration code (see tsconfig.json).
console.log("• typecheck (tsc --noEmit)");
execFileSync(join(nm, ".bin/tsc"), ["--noEmit", "-p", join(here, "tsconfig.json")], { stdio: "inherit" });

// 2. Runtime glue: per-file transpile (type-strip), structure preserved. These
//    are loaded individually as modules by the runtime, so no bundling.
const runtimeSrc = join(here, "src/runtime");
console.log("• runtime glue → crates/pocket-pi/js");
await esbuild.build({
  entryPoints: walk(runtimeSrc).filter((f) => f.endsWith(".ts")),
  outdir: outJs,
  outbase: runtimeSrc,
  bundle: false,
  format: "esm",
  platform: "neutral",
  logLevel: "warning",
});

// 3. The host harness (PocketPi on full pi) + test-harness scripts, eval'd as
//    plain scripts by the runtime / the Rust tests.
console.log("• harness scripts → crates/pocket-pi/js/pi-full");
await esbuild.build({
  entryPoints: ["host", "driver", "ext-probe", "persist-probe"].map((n) => join(here, `src/pi-full/${n}.ts`)),
  outdir: join(outJs, "pi-full"),
  bundle: false,
  format: "esm",
  platform: "neutral",
  logLevel: "warning",
});

// 4. Full, unmodified pi-coding-agent bundle. Whitespace-minify only (identifier
//    and syntax minification emit tokens the embedded QuickJS parser rejects);
//    line-limit wraps the long lines QuickJS chokes on; undici → stub.
console.log("• full pi bundle → pi-full.bundle.js(.gz)");
const fullOut = join(outJs, "pi-full.bundle.js");
await esbuild.build({
  entryPoints: [join(here, "src/pi-full/entry.ts")],
  outfile: fullOut,
  bundle: true,
  format: "esm",
  platform: "node",
  legalComments: "none",
  minifyWhitespace: true,
  lineLimit: 500,
  alias: { undici: join(here, "src/pi-full/undici-stub.ts") },
  logLevel: "warning",
});
const src = readFileSync(fullOut);
const gz = gzipSync(src, { level: 9 });
writeFileSync(`${fullOut}.gz`, gz);

console.log(`\n✓ built. full pi: ${mb(src.length)} MB minified → ${mb(gz.length)} MB gzip`);
