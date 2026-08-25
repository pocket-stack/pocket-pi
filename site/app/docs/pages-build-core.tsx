import type { DocRecord } from "./doc-components";
import { Code, DocLead, Fact, PageGoal, SourceLink } from "./doc-components";

export const BUILD_CORE_DOCS: DocRecord[] = [
  {
    slug: "app-guide",
    title: "App developer guide",
    description: "The complete ordinary App development loop and the responsibility of every source file.",
    render: () => <>
      <h1>App developer guide</h1>
      <DocLead>
        An ordinary PocketPi App is a small source package, not a firmware module. You write
        raw JavaScript and SQL, package it into one <code>.pocketapp</code>, install it through the
        product review flow, and iterate without rebuilding PocketJS or flashing the device.
      </DocLead>
      <PageGoal>
        A map of the whole App workflow before the detailed guides: define product state, implement
        shared Actions, build a fixed View, declare capabilities, test in the simulator, package and update.
      </PageGoal>

      <h2>The developer loop</h2>
      <Code>{`Define the App boundary
        ↓
Design SQLite as durable product truth
        ↓
Implement actor-neutral Actions
        ↓
Project bounded Data into a fixed View
        ↓
Package → upload → review → install
        ↓
Exercise through Agent Tool + UI + schedule
        ↓
Change source → package update → preserve Data`}</Code>

      <h2>One responsibility per file</h2>
      <table>
        <thead><tr><th>File</th><th>Owns</th><th>Must not own</th></tr></thead>
        <tbody>
          <tr><td><code>app.json</code></td><td>Identity, versions, capabilities, Tools, schedules, service policy and resources</td><td>Executable product behavior or secret values</td></tr>
          <tr><td><code>schema.sql</code></td><td>Final SQLite shape for a fresh install</td><td>Historical migration sequence or runtime transaction control</td></tr>
          <tr><td><code>actions.js</code></td><td>Validation, native service calls, normalization, SQLite writes and domain results</td><td>Foreground layout or actor-specific duplicate logic</td></tr>
          <tr><td><code>view.js</code></td><td>Bounded Projections, presentation state, interaction and fixed UI</td><td>Provider calls, credentials or direct SQLite writes</td></tr>
          <tr><td><code>assets/*.json</code></td><td>Manifest-declared frozen source data</td><td>Secrets, executable modules or undeclared files</td></tr>
          <tr><td><code>migrations/N.sql</code></td><td>One forward SQLite step from N−1 to N</td><td><code>BEGIN</code>, <code>COMMIT</code> or <code>PRAGMA user_version</code></td></tr>
        </tbody>
      </table>

      <h2>Design from product state outward</h2>
      <p>
        Start by asking what a person should still see after the device restarts. Those facts belong
        in App-owned SQLite. Next, list every way that state changes. Each change becomes a named
        Action. Expose only the Actions the Agent needs as public Tools. Finally, bind bounded queries
        to a fixed View.
      </p>
      <p>
        This order prevents two common mistakes: turning the View into the business runtime, and
        treating the Agent as a workflow interpreter for deterministic product behavior.
      </p>

      <h2>Choose the right trigger</h2>
      <table>
        <thead><tr><th>Need</th><th>Use</th><th>Why</th></tr></thead>
        <tbody>
          <tr><td>The Agent may decide whether/when to act</td><td>Public Tool → Action</td><td>Reasoning selects a deterministic capability</td></tr>
          <tr><td>A person directly requests behavior</td><td>UI event → same Action</td><td>No duplicate business path</td></tr>
          <tr><td>Periodic deterministic work</td><td>App schedule → same Action</td><td>No model turn or foreground View required</td></tr>
          <tr><td>Reasoned follow-up across Apps</td><td>Agent wake schedule</td><td>Runs a prompt through the resident Agent loop</td></tr>
        </tbody>
      </table>

      <h2>Development sequence</h2>
      <ol>
        <li>Complete <a href="/docs/app-quickstart">Build your first App</a> without adding networking.</li>
        <li>Read <a href="/docs/app-files">App source and package</a> before changing file layout.</li>
        <li>Model durable state with <a href="/docs/data-migrations">Data and migrations</a>.</li>
        <li>Define behavior through <a href="/docs/actions-tools">Actions and Tools</a>.</li>
        <li>Build the human surface with <a href="/docs/view-interaction">View and interaction</a>.</li>
        <li>Add external access only through <a href="/docs/networking-services">Networking and native services</a>.</li>
        <li>Exercise install, restart, update and failure paths in <a href="/docs/testing-debugging">Testing and debugging</a>.</li>
      </ol>
      <Fact>
        Ordinary App packaging has no Bun, TypeScript, JSX or PocketJS compile step today. The
        device evaluates <code>actions.js</code> and <code>view.js</code> as source inside bounded Guests.
      </Fact>
    </>,
  },
  {
    slug: "app-quickstart",
    title: "Build your first App",
    description: "Create, install and exercise a complete Counter App through both UI and Agent Tool paths.",
    render: () => <>
      <h1>Build your first App</h1>
      <DocLead>
        You will build a durable Counter with four source files. The button and the Agent Tool call
        the same Action, SQLite survives restart, and the View refreshes from a Projection after commit.
      </DocLead>
      <PageGoal>
        A complete App installed into the simulator, including source, package,
        review, UI exercise, Agent Tool exercise and persistence check.
      </PageGoal>

      <h2>Before you start</h2>
      <p>
        Keep the simulator running with a persistent workspace as described in
        <a href="/docs/getting-started"> Getting started</a>. Work from the PocketPi repository root.
      </p>

      <h2>1. Create the source directory</h2>
      <Code>{`apps/counter/
├── app.json
├── schema.sql
├── actions.js
└── view.js`}</Code>

      <h2>2. Declare the App and its public Tool</h2>
      <p>Create <code>apps/counter/app.json</code>:</p>
      <Code>{`{
  "format": 1,
  "frameworkApi": 1,
  "id": "counter",
  "title": "Counter",
  "description": "A durable counter shared by a person and the Agent",
  "version": "0.1.0",
  "schemaVersion": 1,
  "capabilities": ["data.sqlite"],
  "resources": {},
  "toolNamespace": "counter",
  "tools": [
    {
      "name": "counter.increment",
      "action": "increment",
      "description": "Increment the durable counter by 1 to 10.",
      "parameters": {
        "type": "object",
        "properties": {
          "by": { "type": "integer", "minimum": 1, "maximum": 10 }
        },
        "additionalProperties": false
      }
    }
  ],
  "schedules": []
}`}</Code>
      <p>
        <code>counter.increment</code> is the Agent-visible Tool name. <code>increment</code> is the
        local Action function. The native router maps one to the other after installation.
      </p>

      <h2>3. Create the durable Data</h2>
      <p>Create <code>apps/counter/schema.sql</code>:</p>
      <Code>{`CREATE TABLE counter (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  value INTEGER NOT NULL
);

INSERT INTO counter(id, value) VALUES(1, 0);`}</Code>
      <p>
        A fresh install executes this final schema. The one-row check makes the invariant explicit
        instead of relying on Action code to avoid duplicate counters.
      </p>

      <h2>4. Implement the shared Action</h2>
      <p>Create <code>apps/counter/actions.js</code>:</p>
      <Code>{`function increment(args) {
  const by = Number(args?.by ?? 1);
  if (!Number.isInteger(by) || by < 1 || by > 10) {
    throw new Error("by must be an integer from 1 to 10");
  }

  PocketPi.data.transaction(() => {
    PocketPi.data.query(
      "UPDATE counter SET value = value + ? WHERE id = 1",
      [by],
    );
  });

  const [row] = PocketPi.data.query(
    "SELECT value FROM counter WHERE id = 1",
    [],
  );
  return { value: row.value, incrementedBy: by };
}

PocketPi.defineActions({ increment });`}</Code>
      <p>
        The transaction owns mutation and publishes one revision only after commit. Returning the
        current value gives the Agent an immediate domain result; the View still refreshes through
        SQLite rather than through this return value.
      </p>

      <h2>5. Project the Data into a fixed View</h2>
      <p>Create <code>apps/counter/view.js</code>:</p>
      <Code>{`const model = View.state({ value: 0, status: "READY" });

PocketPi.projection.one(
  "SELECT value FROM counter WHERE id = 1",
  {},
  (row) => model.update({ value: row?.value ?? 0 }),
);

function render() {
  const state = model.get();
  return View.Screen({ children: [
    View.Header({
      title: "COUNTER",
      metaTop: "POCKET APP",
      metaBottom: "LOCAL SQLITE",
      onBack: () => PocketPi.navigate("pi-agent"),
    }),
    View.Column({
      style: { grow: 1, padding: 24, gap: 20 },
      children: [
        View.MetricCard({
          label: "DURABLE VALUE",
          value: () => String(model.get().value),
        }),
        View.Box({
          style: { height: 84 },
          children: View.ActionButton({
            label: "+1",
            onPress: () => PocketPi.action("increment", { by: 1 }),
          }),
        }),
      ],
    }),
    View.Box({
      style: { height: 96 },
      children: View.StatusBar({ text: state.status, dark: true }),
    }),
  ] });
}

View.mount(render);`}</Code>
      <p>
        The button returns an Action event. It does not call the function directly and does not write
        SQLite from the View Guest. Native routing sends the request to the App&apos;s Action Guest.
      </p>

      <h2>6. Package the source release</h2>
      <Code>{`cargo xtask package app counter`}</Code>
      <p>The package is written to <code>target/pocketapps/counter.pocketapp</code>.</p>

      <h2>7. Upload and confirm in the simulator</h2>
      <Code>{`curl --fail-with-body \\
  --data-binary @target/pocketapps/counter.pocketapp \\
  http://127.0.0.1:8080/install`}</Code>
      <ol>
        <li>The simulator switches to the shared App review screen.</li>
        <li>Confirm <strong>INSTALL</strong>.</li>
        <li>Open <strong>Apps</strong> and choose <strong>Counter</strong>.</li>
        <li>Tap <strong>+1</strong>; the value should refresh after the Action commits.</li>
      </ol>

      <h2>8. Exercise the same Action through the Agent</h2>
      <p>
        Return to Pi Agent and ask: <em>“Use counter.increment to add 3.”</em> The installed Tool
        routes to <code>increment</code>, commits the same database and refreshes the Counter View the
        next time it is foregrounded.
      </p>

      <h2>9. Verify persistence</h2>
      <p>
        Stop and restart the simulator with the same <code>--workspace</code>. Counter remains installed
        and the value remains in its SQLite database even though its previous QuickJS Guests are gone.
      </p>

      <h2>What you just proved</h2>
      <ul>
        <li>one product behavior is shared by UI and Agent;</li>
        <li>the App, not firmware, owns the schema and Action;</li>
        <li>the View is fixed source and reads through a bounded Projection;</li>
        <li>durable state survives Guest eviction and restart;</li>
        <li>ordinary App installation does not require a firmware rebuild.</li>
      </ul>
    </>,
  },
  {
    slug: "app-files",
    title: "App source and package",
    description: "The accepted source tree, package contents, execution order and ownership of generated or native files.",
    render: () => <>
      <h1>App source and package</h1>
      <DocLead>
        The Source App contract is deliberately small: one manifest, one final schema, one Actions
        entrypoint, one View entrypoint, optional JSON resources and conventional forward migrations.
      </DocLead>
      <PageGoal>
        Exact file placement, what the packager accepts, what the Installer adds, and which familiar
        JavaScript project conventions are intentionally absent today.
      </PageGoal>

      <h2>Source tree</h2>
      <Code>{`apps/<id>/
├── app.json                 required
├── schema.sql               required
├── actions.js               required
├── view.js                  required
├── assets/                  optional, JSON only
│   └── catalog.json
└── migrations/              optional
    ├── 2.sql
    └── 3.sql`}</Code>

      <h2>Package contents</h2>
      <Code>{`app.json
schema.sql
actions.js
view.js
assets/...                  only files declared by app.json
migrations/N.sql
credentials.json           optional first-install transport input`}</Code>
      <p>
        A <code>.pocketapp</code> is an uncompressed tar container capped at 2 MiB. The Installer
        accepts regular files only, rejects duplicate or unexpected paths, and strips
        <code>credentials.json</code> before activation.
      </p>

      <h2>Execution order on a fresh install</h2>
      <ol>
        <li>Parse and validate <code>app.json</code>.</li>
        <li>Create the App data root and initialize SQLite with <code>schema.sql</code>.</li>
        <li>Evaluate the platform-owned System Framework inside a candidate Action Guest.</li>
        <li>Evaluate <code>actions.js</code> and verify every declared Tool/schedule Action exists.</li>
        <li>Evaluate the shared View SDK and <code>view.js</code> through a read-only database mount.</li>
        <li>Store credentials natively, move source to <code>apps/&lt;id&gt;/release</code>, then register Tools and schedules.</li>
      </ol>

      <h2>What the runtime supplies</h2>
      <p>Your package does not carry:</p>
      <ul>
        <li><code>system/framework.js</code>, which installs the <code>PocketPi.*</code> API;</li>
        <li><code>system/view-sdk.js</code> and its packed font/resources;</li>
        <li><code>plan.json</code>, native capabilities or platform modules;</li>
        <li>a QuickJS engine or PocketJS runtime binary.</li>
      </ul>
      <p>
        Those belong to PocketPi and firmware. <code>frameworkApi</code> is the compatibility
        check between an App release and that platform-owned layer.
      </p>

      <h2>What is not supported</h2>
      <ul>
        <li>multi-file ES module imports for executable App source;</li>
        <li><code>package.json</code>, npm dependencies or a dependency graph;</li>
        <li>on-device TypeScript, TSX or JSX transformation;</li>
        <li>arbitrary binary assets or executable code under <code>assets/</code>;</li>
        <li>App-supplied native modules or a second JavaScript runtime.</li>
      </ul>
      <Fact>
        Keep the current contract small until a concrete App cannot be expressed cleanly with one
        Actions entrypoint and one View entrypoint. Do not recreate a package ecosystem inside each App.
      </Fact>
    </>,
  },
  {
    slug: "data-migrations",
    title: "Data and migrations",
    description: "Design App-owned SQLite state, commit through Actions, bind bounded reads and evolve schemas safely.",
    render: () => <>
      <h1>Data and migrations</h1>
      <DocLead>
        SQLite is the durable product truth for an ordinary App. One native database owner is shared
        by isolated Action and View Guests, so data survives Guest eviction without creating competing
        embedded SQLite connections.
      </DocLead>
      <PageGoal>
        A correct schema, write/read boundaries, revision behavior, bounded Projection queries and a
        safe forward-migration sequence for updates.
      </PageGoal>

      <h2>Fresh install uses the final schema</h2>
      <p>
        <code>schema.sql</code> describes the complete current shape, not a historical sequence. A
        new installation executes it once and sets the runtime-owned <code>user_version</code> to
        <code>schemaVersion</code> after successful validation.
      </p>
      <Code>{`CREATE TABLE events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  kind TEXT NOT NULL,
  detail TEXT,
  created_at INTEGER NOT NULL
);

CREATE INDEX events_created_at ON events(created_at);`}</Code>

      <h2>Writes belong to Actions</h2>
      <Code>{`PocketPi.data.transaction(() => {
  PocketPi.data.query(
    "INSERT INTO events(kind, detail, created_at) VALUES(?, ?, ?)",
    ["refresh", "completed", Math.floor(Date.now() / 1000)],
  );
});`}</Code>
      <p>
        <code>transaction()</code> owns <code>BEGIN IMMEDIATE</code>, <code>COMMIT</code> and
        <code>ROLLBACK</code>. It calls the native App commit hook only after SQLite commits. A thrown
        error rolls back and does not increment the App revision.
      </p>

      <h2>Views read through bounded Projections</h2>
      <Code>{`const history = PocketPi.projection.many(
  ` + "`" + `SELECT id, kind, detail, created_at
   FROM events
   ORDER BY id DESC
   LIMIT $limit` + "`" + `,
  () => ({ "$limit": 20 }),
  (rows) => model.update({ events: rows }),
);`}</Code>
      <p>
        A Projection runs when registered and again when the foreground View sees a newer App
        revision. It should return only the rows and columns the current View needs. Closed Views do
        not poll, and a normal frame with no revision change performs zero SQLite queries.
      </p>

      <h2>Revision is invalidation, not reactive data</h2>
      <p>
        The revision is a monotonic in-memory counter. It does not contain changed rows and it is not
        a SQLite watch event. Multiple successful commits before the next visible frame coalesce into
        one Projection refresh. If another commit races that refresh, the newer revision remains stale
        and is picked up on the following frame.
      </p>

      <h2>Advance <code>schemaVersion</code> only for shape changes</h2>
      <p>
        <code>version</code> identifies the source release shown to people. <code>schemaVersion</code>
        identifies SQLite compatibility. A code/View-only update changes <code>version</code> but keeps
        the same <code>schemaVersion</code>.
      </p>
      <Code>{`// app.json
"version": "1.2.0",
"schemaVersion": 2`}</Code>

      <h2>Add one file per forward step</h2>
      <Code>{`-- migrations/2.sql
ALTER TABLE events ADD COLUMN source TEXT;
CREATE INDEX events_source ON events(source);`}</Code>
      <ul>
        <li><code>migrations/2.sql</code> moves schema 1 to 2.</li>
        <li><code>migrations/3.sql</code> moves schema 2 to 3.</li>
        <li>An update from 1 to 3 must contain both steps.</li>
        <li>Downgrades and missing intermediate steps are rejected before live mutation.</li>
        <li>Do not include transaction control or set <code>PRAGMA user_version</code>.</li>
      </ul>

      <h2>Update rehearsal and recovery</h2>
      <p>
        The runtime copies the quiescent database, applies every candidate migration, then evaluates
        candidate Actions and View against that copy. Only after rehearsal succeeds does it run the
        same steps in one live transaction and swap source. If power is lost after physical approval,
        <code>.update/release</code> is the recovery signal and boot completes the interrupted update.
      </p>
      <Fact>
        App data has one owner and one mutation path. Do not open an independent SQLite connection,
        write from <code>view.js</code>, or keep the only copy of durable state in <code>View.state</code>.
      </Fact>
    </>,
  },
  {
    slug: "actions-tools",
    title: "Actions and Tools",
    description: "Define actor-neutral behavior once, expose selected Actions to the Agent and handle deadlines and errors correctly.",
    render: () => <>
      <h1>Actions and Tools</h1>
      <DocLead>
        Actions are the App&apos;s behavior boundary. A public Agent Tool, a UI event and an App
        schedule are three sources of the same request, not three implementations of the product logic.
      </DocLead>
      <PageGoal>
        Correct Action definition and validation, public Tool routing, async native service work,
        the single execution budget, serial execution and useful error/result behavior.
      </PageGoal>

      <h2>Define local Actions</h2>
      <Code>{`async function refresh(args, context) {
  const account = String(args.account ?? "").trim();
  if (!account) throw new Error("account is required");

  const value = await loadProviderState(account);
  PocketPi.data.transaction(() => {
    saveProviderState(value);
  });
  return { account, refreshed: true, source: context.source };
}

PocketPi.defineActions({ refresh });`}</Code>
      <p>
        The optional second argument contains frozen request context such as
        <code>{`{ source: "tool" | "ui" | "schedule" }`}</code>. Use it for diagnostics or narrowly
        justified behavior, not to fork the App into separate business systems.
      </p>

      <h2>Expose selected Actions as Tools</h2>
      <Code>{`{
  "name": "portfolio.refresh",
  "action": "refresh",
  "description": "Refresh and persist the selected portfolio.",
  "parameters": {
    "type": "object",
    "properties": {
      "account": { "type": "string", "minLength": 1 }
    },
    "required": ["account"],
    "additionalProperties": false
  }
}`}</Code>
      <p>
        The Tool&apos;s <code>name</code> is global and namespaced. Its <code>action</code> is local and
        must not contain a dot. Installation verifies that every Tool route resolves to an Action
        registered by <code>actions.js</code>.
      </p>

      <h2>Request an Action from the View</h2>
      <Code>{`View.ActionButton({
  label: "REFRESH",
  onPress: () => PocketPi.action("refresh", { account: selectedAccount }),
})`}</Code>
      <p>
        Returning this event transfers the request to native routing. Do not import
        <code>actions.js</code> into the View or mutate SQLite in the pointer callback.
      </p>

      <h2>Execution envelope and ordering</h2>
      <Code>{`{"action":"refresh","args":{"account":"..."},"source":"tool|ui|schedule"}`}</Code>
      <p>
        One bounded Action queue and one Action runner serve ordinary Apps. Only one Action executes
        at a time in v1. The Tool call receives one absolute 80-second budget that includes queueing,
        JavaScript and native transport. Use the remaining value for downstream requests:
      </p>
      <Code>{`const response = await fetch(url, {
  timeoutMs: PocketPi.actionContext.remainingMs(),
  maxBytes: 96 * 1024,
});`}</Code>

      <h2>Return domain results</h2>
      <p>
        Return JSON-serializable values that tell the caller what completed. The framework converts
        a successful value into Tool result text. Throw an <code>Error</code> for failure; the pending
        Tool call receives an error result instead of a false empty success.
      </p>
      <Code>{`if (!response.ok) {
  throw new Error(` + "`" + `Provider HTTP ${"${response.status}"}` + "`" + `);
}

return { refreshedAt, rows: normalizedRows.length };`}</Code>

      <h2>Do not persist everything</h2>
      <p>
        A provider response should be returned to the Agent when useful. Persist only normalized state
        consumed by the App&apos;s durable behavior or fixed View. Raw-response caches, generic Tool logs
        and duplicate details increase memory/storage pressure without strengthening the product model.
      </p>
    </>,
  },
  {
    slug: "view-interaction",
    title: "View and interaction",
    description: "Build a fixed PocketJS View with bounded state, explicit flow, Projections and Action events.",
    render: () => <>
      <h1>View and interaction</h1>
      <DocLead>
        A View is fixed JavaScript source shipped by the App release. It projects durable Data into a
        retained PocketJS node tree and turns human input into Action or narrow navigation events.
      </DocLead>
      <PageGoal>
        A correct View lifecycle, presentation state, bounded Projection, explicit layout, shared Pi
        Design components and input behavior that stays out of the business/data path.
      </PageGoal>

      <h2>Keep presentation state small</h2>
      <Code>{`const model = View.state({
  items: [],
  offset: 0,
  status: "READY",
});`}</Code>
      <p>
        Use View state for the current screen, selection, pagination cursor, loading label and other
        ephemeral presentation choices. Anything that must survive Guest eviction or restart belongs
        in App Data.
      </p>

      <h2>Bind durable state through a Projection</h2>
      <Code>{`const itemsProjection = PocketPi.projection.many(
  "SELECT id, title FROM items ORDER BY id DESC LIMIT $limit",
  () => ({ "$limit": 20 }),
  (rows) => model.update({ items: rows }),
);`}</Code>
      <p>
        <code>projection.one</code> applies one row or <code>null</code>.
        <code>projection.many</code> applies an array. Keep SQL bounded with limits and only select
        columns needed by the current surface.
      </p>

      <h2>Mount one render function</h2>
      <Code>{`function render() {
  const state = model.get();
  return View.Screen({ children: [
    View.Header({
      title: "ITEMS",
      onBack: () => PocketPi.navigate("pi-agent"),
    }),
    View.Column({
      style: { grow: 1, padding: 24, gap: 12 },
      children: state.items.map((item) => View.Card({
        style: { padding: 20 },
        children: View.Text({ text: item.title }),
      })),
    }),
  ] });
}

View.mount(render);`}</Code>
      <p>
        Reading a state value during render tracks it. Updating that state marks the View dirty, and
        the next View tick reconciles changed node properties/text while retaining compatible nodes.
      </p>

      <h2>Use explicit flow</h2>
      <p>
        A container with multiple normal-flow children must use <code>View.Row</code>,
        <code>View.Column</code> or an explicit <code>direction</code>. This avoids a hidden layout
        default. Use absolute positioning only for a deliberate overlay.
      </p>
      <Code>{`View.Row({
  style: { height: 84, gap: 12, align: "center" },
  children: [left, right],
})`}</Code>

      <h2>Read the host viewport, not the board name</h2>
      <Code>{`View.viewport
// { width, height, orientation, scale, layoutWidth, layoutHeight }

const LANDSCAPE = View.viewport.orientation === "landscape";

const content = LANDSCAPE
  ? View.Row({ style: { grow: 1, gap: 12 }, children: [primary, aside] })
  : View.Column({ style: { grow: 1, gap: 24 }, children: [primary, aside] });`}</Code>
      <p>
        The P4 host reports 720×1280. The S3 host rotates its 800×480 physical panel and reports
        480×800. The simulator can exercise 720×1280, 800×480 and 480×800. The View SDK chooses a
        720×1280 portrait or 800×480 landscape reference canvas and derives one continuous geometry
        scale. App numeric style values are design units and must not be multiplied by
        <code>View.viewport.scale</code> again.
      </p>
      <p>
        Branch on orientation only when the composition itself should change, such as a portrait
        stack becoming landscape columns. Use <code>scale</code> only to reduce bounded repeated
        content. Do not branch on ESP32-P4, ESP32-S3 or a board profile.
      </p>

      <h2>Send events, not side effects</h2>
      <Code>{`View.Pressable({
  onPress: () => PocketPi.action("select", { id: item.id }),
  children: View.Text({ text: item.title }),
})`}</Code>
      <p>
        <code>View.Pressable</code> participates in native hit testing and pressed feedback. A handler
        returns an Action/navigation event; it should not wait for HTTP, call a provider, block the
        frame or write business state directly.
      </p>

      <h2>Shared Pi Design components</h2>
      <table>
        <thead><tr><th>Component</th><th>Use</th></tr></thead>
        <tbody>
          <tr><td><code>Header</code>, <code>PageIntro</code>, <code>SectionHeading</code></td><td>Stable product hierarchy</td></tr>
          <tr><td><code>ActionButton</code>, <code>Pressable</code></td><td>Direct human intents</td></tr>
          <tr><td><code>Card</code>, <code>Badge</code>, <code>MetricCard</code>, <code>EmptyState</code></td><td>Common content/status surfaces</td></tr>
          <tr><td><code>StatusBar</code>, <code>ScrollButton</code>, <code>ScrollRail</code></td><td>Bounded status and paging</td></tr>
          <tr><td><code>Keyboard</code></td><td>Shared on-device key layout; App still owns input meaning</td></tr>
        </tbody>
      </table>
      <p>
        Keep domain components such as portfolio charts, search-history rows and account selectors inside the
        App until their semantics are genuinely reusable. Inventory: <SourceLink path="docs/pocket-pi-design-system.md" />.
      </p>

      <h2>Design for the device</h2>
      <ul>
        <li>The logical viewport is host-provided: currently 720×1280 on P4 and 480×800 on S3 after rotation.</li>
        <li><code>Pressable</code> preserves at least a 40×40 physical-pixel hit target; <code>ActionButton</code> preserves at least 48 physical pixels of height.</li>
        <li>Use bounded visible rows and explicit UP/DN paging rather than unbounded node lists.</li>
        <li>Use the shared baked font slots; there is no runtime font loader.</li>
        <li>Test pointer-down feedback, release and action routing in the simulator and physical touch.</li>
        <li>Do not show unstable CPU/PSRAM/FPS telemetry as product UI.</li>
      </ul>
    </>,
  },
];
