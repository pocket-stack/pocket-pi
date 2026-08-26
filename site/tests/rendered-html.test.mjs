import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const testRoot = dirname(fileURLToPath(import.meta.url));

function documentationSlugs() {
  const docsRoot = resolve(testRoot, "../app/docs");
  const slugs = readdirSync(docsRoot)
    .filter((name) => /^pages-.*\.tsx$/.test(name))
    .flatMap((name) => [
      ...readFileSync(resolve(docsRoot, name), "utf8").matchAll(/slug: "([^"]+)"/g),
    ].map((match) => match[1]));
  assert.equal(new Set(slugs).size, slugs.length, "documentation slugs must be unique");
  return slugs;
}

async function render(route = "/") {
  const pathname = route === "/" ? "/index" : route.replace(/\/$/, "");
  const html = readFileSync(resolve(testRoot, `../dist/client${pathname}.html`), "utf8");
  return new Response(html, { headers: { "content-type": "text/html; charset=utf-8" } });
}

test("renders the PocketPi home page and social metadata", async () => {
  const response = await render("/");
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type") ?? "", /^text\/html\b/i);

  const html = await response.text();
  assert.match(html, /<title>PocketPi · Agent-native runtime for embedded systems<\/title>/i);
  assert.match(html, /The agent-native runtime/);
  assert.match(html, /on embedded devices/);
  assert.match(html, /full Pi Agent core harness on-device/);
  assert.match(html, /pocketpi-system-architecture\.svg/);
  assert.match(html, /aria-label="App equals Data plus Actions plus View"/);
  assert.match(html, /pocketpi-app-architecture\.svg/);
  assert.match(html, /PocketJS is the runtime substrate/);
  assert.match(html, /333\.75 KiB/);
  assert.match(html, /5\.59 MiB/);
  assert.match(html, /Less than 35% used in every measured capacity/);
  assert.match(html, /PSRAM shows the fixed two-framebuffer floor/);
  assert.match(html, /Apps are the unit the agent and human can both understand and act on/);
  assert.match(html, /ESP32-S3-Touch-LCD-4\.3/);
  assert.match(html, /waveshare-esp32-p4-wifi6-touch-lcd-5-back-black\.png/);
  assert.match(html, /waveshare-esp32-s3-touch-lcd-4\.3-back-black\.png/);
  assert.doesNotMatch(html, /Reserved for a physical photograph|Physical device photo/);
  assert.match(html, /Get editable source/);
  assert.match(html, /File Tools revise Actions and View source/);
  assert.match(html, /Move over Checkout, Modify or Review to inspect that moment/);
  assert.match(html, /data-full-page-navigation/);
  assert.match(html, /https:\/\/pi\.pocketlab\.build\/og\.png/);
  assert.doesNotMatch(html, /Pocket Agent OS/i);
  assert.doesNotMatch(html, /One Agent runtime\.\s*Two ESP32 targets/i);
  assert.doesNotMatch(html, /codex-preview|Your site is taking shape|react-loading-skeleton/i);

  const architecture = html.indexOf('data-section="architecture"');
  const substrate = html.indexOf('data-section="substrate"');
  const appDefinition = html.indexOf('data-section="app-definition"');
  const capability = html.indexOf('data-section="capability"');
  const devices = html.indexOf('data-section="devices"');
  assert.ok(architecture > 0, "architecture section must exist");
  assert.ok(architecture < substrate, "architecture must precede the runtime substrate");
  assert.ok(substrate < appDefinition, "runtime substrate must precede the App definition");
  assert.ok(appDefinition < capability, "App definition must precede Agent capability");
  assert.ok(capability < devices, "ready devices must be the final chapter");
  assert.doesNotMatch(html, /esp32-s3-app-iteration\.png/);
});

test("renders every documentation page with a declared page responsibility", async () => {
  const slugs = documentationSlugs();
  assert.equal(slugs.length, 35);

  for (const slug of slugs) {
    const route = slug === "overview" ? "/docs" : `/docs/${slug}`;
    const response = await render(route);
    assert.equal(response.status, 200, route);
    const html = await response.text();
    assert.match(html, /What this page gives you/, route);
    assert.match(html, /<h1>/, route);
    assert.doesNotMatch(html, /Pocket Agent OS/i, route);
  }
});

test("renders representative first-party routes with source-grounded content", async () => {
  const routes = [
    ["/docs", /Agent-native runtime for embedded and dedicated devices/],
    ["/docs/mental-model", /Start with actors, not layers/],
    ["/docs/runtime-flow", /Transaction → revision → View/],
    ["/docs/app-quickstart", /Upload and confirm in the simulator/],
    ["/docs/data-migrations", /migrations\/2\.sql/],
    ["/docs/view-interaction", /Use explicit flow/],
    ["/docs/networking-services", /Credential file for first install/],
    ["/docs/runtime-api", /Private ABI/],
    ["/docs/harnesses", /Research target/],
    ["/docs/current-boundaries", /after PRs #17 through #20/],
    ["/docs/esp32-s3", /ESP32-S3-WROOM-1-N16R8/],
    ["/docs/getting-started", /You do not need a PocketJS checkout to start using PocketPi/i],
    ["/blog", /Making Pocket Pi an Agent-Native App Development Environment/],
    ["/blog/agent-native-app-development-environment", /The development environment is the closed loop/],
    ["/changelog", /Support ordinary App updates/],
  ];

  for (const [route, expected] of routes) {
    const response = await render(route);
    assert.equal(response.status, 200, route);
    assert.match(await response.text(), expected, route);
  }
});

test("renders current build and UART tool contracts without collapsing shell commands", async () => {
  const gettingStarted = await (await render("/docs/getting-started")).text();
  assert.match(gettingStarted, /You do not need a PocketJS checkout to start using PocketPi/i);
  assert.ok(gettingStarted.includes(
    "cargo xtask run esp32-sim \\" + "\n  --backend codex \\" + "\n  --workspace",
  ));

  const p4 = await (await render("/docs/esp32-p4")).text();
  assert.match(p4, /espflash list-ports/);
  assert.match(p4, /espflash board-info --port/);
  assert.match(p4, /It does not regenerate either asset/);
  assert.match(p4, /DeepSeek, the tool reads account/);
  assert.match(p4, /Other providers prompt for their key/);
  assert.ok(p4.includes(
    "espflash flash --baud 921600 --port &quot;$DEVICE_PORT&quot; \\" +
    "\n  --partition-table firmware/esp32-p4/partitions.csv \\" +
    "\n  firmware/esp32-p4/target",
  ));

  const cli = await (await render("/docs/cli-reference")).text();
  assert.match(cli, /Rebuild only the resident Pi Agent JavaScript bundle/);
  assert.match(cli, /Normal simulator and firmware commands do not inspect or modify/);
  assert.match(cli, /leaves DTR and RTS inactive when closing the port/);
  assert.match(cli, /uart-install\.py.*does not reset the board or change model configuration/s);
  assert.match(cli, /Providers: openai, openrouter, anthropic, deepseek/);
  assert.match(cli, /Providers: codex or claude-code/);
  assert.doesNotMatch(cli, /Build assets and run the simulator|Build System assets and ESP32/);

  const s3 = await (await render("/docs/esp32-s3")).text();
  assert.match(s3, /espflash list-ports/);
  assert.match(s3, /committed generated Pi Agent and View SDK assets/);
});

test("escapes every shell continuation in documentation source", () => {
  const docsRoot = resolve(testRoot, "../app/docs");
  for (const name of readdirSync(docsRoot).filter((entry) => /^pages-.*\.tsx$/.test(entry))) {
    const source = readFileSync(resolve(docsRoot, name), "utf8");
    assert.doesNotMatch(source, /(?<!\\)\\$/gm, name);
  }
});

test("renders content routes with or without a trailing slash", async () => {
  for (const route of ["/docs", "/docs/", "/blog", "/blog/", "/changelog", "/changelog/"]) {
    const response = await render(route);
    assert.equal(response.status, 200, route);
  }
});

test("renders every primary navigation destination", async () => {
  for (const route of ["/docs", "/blog", "/changelog"]) {
    const response = await render(route);
    assert.equal(response.status, 200, route);
    const html = await response.text();
    assert.match(html, /data-full-page-navigation/, route);
    assert.match(html, /href="\/docs"/, route);
    assert.match(html, /href="\/blog"/, route);
    assert.match(html, /href="\/changelog"/, route);
  }
});

test("uses page-specific Docs metadata and contains no invented editorial record", async () => {
  const response = await render("/docs/app-quickstart");
  const html = await response.text();
  assert.match(html, /<title>Build your first App · PocketPi Docs<\/title>/i);
  assert.match(html, /property="og:title" content="Build your first App · PocketPi Docs"/i);
  assert.match(html, /name="twitter:title" content="Build your first App · PocketPi Docs"/i);
  assert.doesNotMatch(html, /og\.png/i);

  const blog = await (await render("/blog")).text();
  assert.match(blog, /href="\/blog\/agent-native-app-development-environment"/);
  assert.match(blog, /https:\/\/pocketjs\.dev\/blog\/agent-native-runtime-embedded-systems\//);
  assert.match(blog, /https:\/\/pocketjs\.dev\/blog\/pocket-pi-agent-native-runtime\//);
  assert.match(blog, /https:\/\/pocketjs\.dev\/blog\/pocket-pi-on-esp32-p4\//);
  assert.doesNotMatch(blog, /Current archive|From the next article|Published on PocketJS/i);
  assert.doesNotMatch(blog, /August 20, 2026|The Agent lives on the device/i);

  const article = await (await render("/blog/agent-native-app-development-environment")).text();
  assert.match(article, /<title>Making Pocket Pi an Agent-Native App Development Environment · PocketPi Blog<\/title>/i);
  assert.match(article, /pocketpi-device\/screens\/main\.png/);
  assert.match(article, /pocketpi-device\/screens\/files\.png/);
  assert.match(article, /agent-native-app-development-environment\/update-review\.png/);
  assert.match(article, /physical-before-after\.png/);
  assert.match(article, /minimal closed loop required for an Agent to develop software/);
  assert.match(article, /commit it as one coherent new App version/);
  assert.doesNotMatch(article, /What the demo proves/i);

  const docs = await (await render("/docs")).text();
  assert.match(docs, /Documentation map/);
  assert.match(docs, /35 focused pages/);
  for (const section of ["Start here", "Use the runtime", "Build Apps", "Understand the runtime", "Security", "Reference", "Examples", "Project"]) {
    assert.match(docs, new RegExp(section), section);
  }

  const changelog = await (await render("/changelog/")).text();
  assert.match(changelog, /Shipped runtime history/);
  assert.match(changelog, /Earlier merged history/);
  assert.doesNotMatch(changelog, /UNRELEASED|Identity &amp; site|Identity & site/i);
});
