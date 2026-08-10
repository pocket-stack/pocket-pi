import { basename, dirname } from "node:path";

const [entry, output, pocketjsRoot] = Bun.argv.slice(2);
if (!entry || !output || !pocketjsRoot) {
  throw new Error("usage: build-agentos-data.ts <entry> <output> <pocketjs-root>");
}

const compiler = await import(`${pocketjsRoot}/framework/compiler/jsx-plugin.ts`);
const result = await Bun.build({
  entrypoints: [entry],
  outdir: dirname(output),
  naming: basename(output),
  target: "browser",
  format: "iife",
  minify: true,
  conditions: ["browser"],
  define: { "process.env.NODE_ENV": '"production"' },
  plugins: [compiler.jsxPlugin("solid", { entry })],
});
if (!result.success) {
  for (const log of result.logs) console.error(log);
  process.exit(1);
}
