import * as esbuild from "esbuild";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));

execFileSync(join(here, "node_modules/.bin/tsc"), ["--noEmit", "-p", join(here, "tsconfig.json")], {
  stdio: "inherit",
});

await esbuild.build({
  entryPoints: [join(here, "src/entry.ts")],
  outfile: join(here, "pi-agent.bundle.js"),
  bundle: true,
  format: "iife",
  platform: "neutral",
  target: "es2020",
  legalComments: "eof",
  minifyWhitespace: true,
  lineLimit: 500,
});
