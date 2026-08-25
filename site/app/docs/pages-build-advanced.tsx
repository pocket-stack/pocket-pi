import type { DocRecord } from "./doc-components";
import { Code, DocLead, Fact, PageGoal, SourceLink } from "./doc-components";

export const BUILD_ADVANCED_DOCS: DocRecord[] = [
  {
    slug: "networking-services",
    title: "Networking and native services",
    description: "Call HTTP and MCP providers without exposing credentials or moving domain policy into native code.",
    render: () => <>
      <h1>Networking and native services</h1>
      <DocLead>
        App JavaScript owns provider mapping and domain semantics. The native host owns credential
        application, TLS, exact endpoint/operation policy, response bounds and the Action deadline.
      </DocLead>
      <PageGoal>
        A credential-safe HTTP or MCP integration with explicit manifest policy, bounded response
        handling, Action-owned normalization and no secret values in App source, SQLite or Agent context.
      </PageGoal>

      <h2>HTTP: declare capability and exact policy</h2>
      <Code>{`{
  "capabilities": ["data.sqlite", "net.http"],
  "nativeServices": {
    "http": [
      {
        "method": "POST",
        "urls": [
          "https://api.example.com/search",
          "https://api.example.com/contents"
        ],
        "allowedRequestHeaders": ["accept", "content-type"],
        "credential": {
          "id": "example.api-key",
          "header": "authorization",
          "prefix": "Bearer "
        }
      }
    ]
  }
}`}</Code>
      <p>
        The App declares where it may connect, which method it may use, which request headers its
        JavaScript may set and which native credential binding should be applied. The secret value is
        supplied only during first-install transport and is stored by the host.
      </p>

      <h2>Call with the installed <code>fetch()</code></h2>
      <Code>{`async function post(path, value) {
  const response = await fetch(` + "`" + `https://api.example.com/${"${path}"}` + "`" + `, {
    method: "POST",
    headers: {
      accept: "application/json",
      "content-type": "application/json",
    },
    body: JSON.stringify(value),
    timeoutMs: PocketPi.actionContext.remainingMs(),
    maxBytes: 96 * 1024,
  });

  const body = await response.json();
  if (!response.ok) {
    throw new Error(` + "`" + `Provider HTTP ${"${response.status}"}: ${"${JSON.stringify(body)}"}` + "`" + `);
  }
  return body;
}`}</Code>
      <p>
        The response provides <code>status</code>, <code>url</code>, frozen headers, <code>ok</code>,
        and async <code>bytes()</code>, <code>arrayBuffer()</code>, <code>text()</code> and
        <code>json()</code>. Always set a product-appropriate <code>maxBytes</code> rather than relying
        on the default 128 KiB.
      </p>

      <h2>MCP: keep the connection native and the Tool policy App-owned</h2>
      <Code>{`{
  "nativeServices": {
    "mcp": [
      {
        "connection": "portfolio",
        "url": "https://provider.example.com/mcp",
        "credential": {
          "id": "portfolio.oauth-token",
          "header": "authorization",
          "prefix": "Bearer "
        }
      }
    ]
  },
  "providerOperations": ["get_accounts", "get_portfolio"]
}`}</Code>
      <Code>{`const value = PocketPi.services.call("mcp.client", "callTool", {
  connection: "portfolio",
  name: "get_portfolio",
  arguments: { account_number: account },
  retryable: true,
});`}</Code>
      <p>
        <code>providerOperations</code> is the native allowlist. App source still owns which public
        Tools exist, how arguments are validated, which upstream operation is selected, how the
        response maps into product Data, and what the Agent receives.
      </p>

      <h2>Batch only when the product needs it</h2>
      <Code>{`const value = PocketPi.services.call("mcp.client", "callTools", {
  connection: "portfolio",
  calls: [
    { name: "get_accounts", arguments: {} },
    { name: "get_portfolio", arguments: { account_number: account } },
  ],
  retryable: true,
});`}</Code>
      <p>
        A batch shares the same Action deadline and native connection policy. It does not turn native
        code into the product workflow; JavaScript still decides the calls and consumes the results.
      </p>

      <h2>Credential file for first install</h2>
      <Code>{`{
  "example.api-key": "secret value supplied out of band"
}`}</Code>
      <Code>{`cargo xtask package app example path/to/credentials.json`}</Code>
      <p>
        The credential ids must exactly match the manifest. The Installer removes the file from the
        staged source and stores values natively. Update packages omit credentials and may not change
        native permission policy.
      </p>

      <h2>Rules that keep the boundary honest</h2>
      <ul>
        <li>Never put a secret in <code>app.json</code>, an asset, SQLite, Tool arguments or Agent workspace.</li>
        <li>Use exact URLs and operations; do not declare a broad proxy and rebuild authorization in JavaScript.</li>
        <li>Validate domain arguments before crossing the native service boundary.</li>
        <li>Return useful live results to the Agent; persist only bounded View/product state.</li>
        <li>Treat ambiguous real-world side effects as unknown, not as safe to retry with a new id.</li>
      </ul>
    </>,
  },
  {
    slug: "resources",
    title: "App resources",
    description: "Ship frozen JSON catalogs and other declared source data without creating an App module system.",
    render: () => <>
      <h1>App resources</h1>
      <DocLead>
        Resources are immutable JSON values packaged with an App release. They are useful for large
        Tool catalogs, mapping tables and reviewed source data that should not be embedded as executable
        JavaScript.
      </DocLead>
      <PageGoal>
        A correctly declared resource tree, exact package validation, runtime access and a clear line
        between frozen source data, mutable SQLite Data and credentials.
      </PageGoal>

      <h2>Declare every resource</h2>
      <Code>{`{
  "resources": {
    "toolCatalog": {
      "path": "assets/tool-catalog.json",
      "type": "json"
    }
  }
}`}</Code>
      <p>
        Resource names are safe single path components. Paths must remain under <code>assets/</code>,
        use safe components and match an actual regular file.
      </p>

      <h2>Read the frozen value</h2>
      <Code>{`const toolCatalog = PocketPi.resources.get("toolCatalog");

function findTool(name) {
  return toolCatalog.tools.find((tool) => tool.name === name) ?? null;
}`}</Code>
      <p>
        The framework deep-freezes parsed resource values before exposing them. Both the Action and
        View Guest receive the same release data, but they do not share a JavaScript object or heap.
      </p>

      <h2>Validation rules</h2>
      <ul>
        <li>Only <code>type: &quot;json&quot;</code> is supported.</li>
        <li>Files present under <code>assets/</code> must exactly equal the paths declared by <code>resources</code>.</li>
        <li>One JSON resource is limited to 256 KiB.</li>
        <li>All resources together are limited to 512 KiB.</li>
        <li>The entire <code>.pocketapp</code>, including resources, is limited to 2 MiB.</li>
        <li>Invalid JSON rejects the candidate before activation.</li>
      </ul>

      <h2>Choose the right storage</h2>
      <table>
        <thead><tr><th>Data</th><th>Put it in</th></tr></thead>
        <tbody>
          <tr><td>Reviewed catalog/versioned mapping shipped with source</td><td><code>assets/*.json</code></td></tr>
          <tr><td>Mutable product state, history or cached View projection</td><td>App-owned SQLite</td></tr>
          <tr><td>Raw provider credential</td><td>Native credential store</td></tr>
          <tr><td>Executable behavior</td><td><code>actions.js</code> or <code>view.js</code></td></tr>
          <tr><td>Agent-authored memory</td><td>Pi Agent <code>/workspace</code></td></tr>
        </tbody>
      </table>
      <p>
        Robinhood uses a resource for its checked-in provider Tool catalog; see
        <SourceLink path="apps/robinhood/assets/tool-catalog.json"> the catalog source</SourceLink>.
      </p>
    </>,
  },
  {
    slug: "schedules",
    title: "Schedules",
    description: "Choose between Agent wake schedules and deterministic App schedules, and understand headless execution.",
    render: () => <>
      <h1>Schedules</h1>
      <DocLead>
        PocketPi has two scheduling models because reasoning work and deterministic App work
        have different owners. Use an Agent wake to ask the resident Agent to think; use an App schedule
        to run one named Action without a model turn.
      </DocLead>
      <PageGoal>
        The right schedule type, a valid manifest declaration, headless Action behavior, persistence
        semantics and the conditions under which scheduled work is admitted or delayed.
      </PageGoal>

      <h2>Choose by owner</h2>
      <table>
        <thead><tr><th></th><th>Agent wake</th><th>App schedule</th></tr></thead>
        <tbody>
          <tr><td>Declared by</td><td>Agent through <code>schedule.*</code> Tools</td><td>App release in <code>app.json</code></td></tr>
          <tr><td>Persistent state</td><td>Pi Agent schedule store</td><td>App-local schedule state</td></tr>
          <tr><td>Runs</td><td>A prompt through the resident Harness</td><td>One local Action with fixed args</td></tr>
          <tr><td>Model required</td><td>Yes</td><td>No</td></tr>
          <tr><td>Example</td><td>“Every morning, review research and decide what needs attention.”</td><td>“Refresh portfolio data every five minutes.”</td></tr>
        </tbody>
      </table>

      <h2>Declare an App schedule</h2>
      <Code>{`"schedules": [
  {
    "id": "portfolio-refresh",
    "everyMinutes": 5,
    "action": "refreshPortfolio",
    "args": {}
  }
]`}</Code>
      <p>
        The Action name is local and unqualified. Installation evaluates <code>actions.js</code> and
        rejects the App if the Action is missing. The runtime clamps intervals to at least 60 seconds.
      </p>

      <h2>Headless execution</h2>
      <ol>
        <li>The scheduler claims a due App declaration.</li>
        <li>The shared Action runner loads or reuses that App&apos;s Action Guest.</li>
        <li>The Action executes with <code>source: &quot;schedule&quot;</code>.</li>
        <li>Any successful transaction commits SQLite and increments the App revision.</li>
        <li>The schedule records success only after the Action completes.</li>
        <li>A closed View remains unloaded; it projects current data when opened later.</li>
      </ol>

      <h2>Admission and busy behavior</h2>
      <p>
        v1 executes one ordinary Action at a time. Scheduled Actions share that bounded runner rather
        than creating background concurrency. Hosts poll schedules only when install/uninstall and
        other exclusive product work allow admission. This keeps behavior deterministic on constrained
        hardware.
      </p>

      <h2>Update and uninstall behavior</h2>
      <ul>
        <li>An App update replaces its schedule declarations with the candidate release after successful activation.</li>
        <li>Existing App schedule state is reconciled to the current declaration.</li>
        <li>Uninstall removes all schedules and cursors owned by that App.</li>
        <li>Removing an App Tool does not affect Pi Agent wake schedules, but a future wake may no longer find that Tool.</li>
      </ul>
    </>,
  },
  {
    slug: "package-update",
    title: "Package and update",
    description: "Build the .pocketapp artifact and understand review, rehearsal, activation and update invariants.",
    render: () => <>
      <h1>Package and update</h1>
      <DocLead>
        Packaging is intentionally mechanical; installation is intentionally strict. The packager
        gathers the declared source. The Installer is the authority that validates, reviews, rehearses
        and activates it.
      </DocLead>
      <PageGoal>
        Reproducible first-install and update artifacts, correct version/schema choices, and a precise
        understanding of what can change without losing state or native permissions.
      </PageGoal>

      <h2>Package commands</h2>
      <Code>{`# First install without credentials
cargo xtask package app counter

# First install with every declared credential
cargo xtask package app exa path/to/exa-credentials.json

# Update: never carry credentials
cargo xtask package app exa`}</Code>

      <h2>Inspect the artifact</h2>
      <Code>{`tar -tf target/pocketapps/exa.pocketapp`}</Code>
      <p>
        Expect only the four required source files, declared assets, valid migrations and optionally
        <code>credentials.json</code> for first install. The output file is permission-restricted on Unix.
      </p>

      <h2>Version decisions</h2>
      <table>
        <thead><tr><th>You changed…</th><th><code>version</code></th><th><code>schemaVersion</code></th><th>Migration</th></tr></thead>
        <tbody>
          <tr><td>View copy or layout</td><td>Advance</td><td>Keep</td><td>No</td></tr>
          <tr><td>Action validation/provider mapping</td><td>Advance</td><td>Keep</td><td>No</td></tr>
          <tr><td>SQLite table/index shape</td><td>Advance</td><td>Advance</td><td>Every forward step</td></tr>
          <tr><td>Credential id, endpoint or native permission</td><td>New release is not eligible for ordinary update</td><td>As needed</td><td>Install contract must be reconsidered</td></tr>
        </tbody>
      </table>

      <h2>Fresh activation</h2>
      <ol>
        <li>Stage and validate the package.</li>
        <li>Show one product review and wait for human confirmation.</li>
        <li>Initialize a new database from <code>schema.sql</code>.</li>
        <li>Evaluate Actions and View against the candidate App boundary.</li>
        <li>Store credentials natively and remove their transport file.</li>
        <li>Move the release into place and register Tools/schedules.</li>
      </ol>

      <h2>Update activation</h2>
      <ol>
        <li>Reject credentials, native permission changes, schema downgrade or missing steps.</li>
        <li>Wait for a quiescent App service boundary.</li>
        <li>Copy SQLite and rehearse migrations plus candidate Actions/View on the copy.</li>
        <li>Apply migrations in one live transaction.</li>
        <li>Swap the single source release and replace Tools, schedules and cached Guests.</li>
        <li>Remove temporary old source after the new App is active.</li>
      </ol>

      <h2>State that is preserved</h2>
      <ul>
        <li>App SQLite rows and App data files;</li>
        <li>native credential values already installed;</li>
        <li>the stable App id and private data root;</li>
        <li>schedule state that remains compatible with the new declarations.</li>
      </ul>

      <h2>State that is replaced</h2>
      <ul>
        <li><code>app.json</code>, <code>actions.js</code>, <code>view.js</code> and declared resources;</li>
        <li>public Tool routes and schedule declarations;</li>
        <li>cached ordinary View and Action Guests.</li>
      </ul>
      <Fact>
        The runtime retains one active ordinary App release. There is recovery for an approved
        interrupted update, but no release history, automatic rollback or downgrade path.
      </Fact>
    </>,
  },
  {
    slug: "testing-debugging",
    title: "Testing and debugging",
    description: "Validate source Apps through package, install, simulator, restart, update, failure and physical-device tiers.",
    render: () => <>
      <h1>Testing and debugging</h1>
      <DocLead>
        A successful JavaScript evaluation is only the first evidence tier. An App must also prove
        routing, SQLite ownership, Projection refresh, restart restoration and update safety. When a
        path depends on hardware or live transport, also verify fresh physical behavior.
      </DocLead>
      <PageGoal>
        A repeatable test matrix, commands for the current repository, common failure localization and
        evidence language that does not confuse a simulator pass with physical acceptance.
      </PageGoal>

      <h2>Use a fresh simulator workspace for clean-install tests</h2>
      <Code>{`cargo xtask run esp32-sim \\
  --backend codex \\
  --workspace target/app-test-workspace`}</Code>
      <p>
        A separate path gives you a genuinely fresh App catalog without destroying another development
        workspace. Keep a second persistent path for update/migration testing.
      </p>

      <h2>Minimum App test matrix</h2>
      <table>
        <thead><tr><th>Case</th><th>What to verify</th></tr></thead>
        <tbody>
          <tr><td>Package</td><td>Only expected files; credential ids and resources match manifest</td></tr>
          <tr><td>Fresh install</td><td>Review → confirmation → Tool registration → App opens</td></tr>
          <tr><td>UI Action</td><td>Tap routes to Action; transaction commits; visible Projection refreshes</td></tr>
          <tr><td>Agent Tool</td><td>Agent sees namespaced Tool and receives the completed result</td></tr>
          <tr><td>Headless schedule</td><td>Action runs with View closed; data appears when reopened</td></tr>
          <tr><td>Restart</td><td>Installed source, SQLite and schedules restore; transient Guests do not matter</td></tr>
          <tr><td>Code-only update</td><td>Source changes; schema/data and credentials remain</td></tr>
          <tr><td>Schema update</td><td>Every migration runs; existing rows survive</td></tr>
          <tr><td>Rejected update</td><td>Downgrade/missing/failing migration leaves installed release and data intact</td></tr>
          <tr><td>Uninstall/reinstall</td><td>All App-owned state disappears, then fresh install starts clean</td></tr>
        </tbody>
      </table>

      <h2>Repository checks</h2>
      <Code>{`cargo test --workspace
bun test apps/pi-agent/text.test.js
cargo clippy --workspace --all-targets -- -D warnings
cargo xtask build esp32-sim
cargo xtask build esp32-p4`}</Code>
      <p>
        These cover Rust runtime contracts, Pi Agent text helpers, linting and both host builds. They
        do not automatically prove a specific App&apos;s real provider behavior or physical UI.
      </p>

      <h2>Inspect a package before upload</h2>
      <Code>{`tar -tf target/pocketapps/example.pocketapp
tar -xOf target/pocketapps/example.pocketapp app.json`}</Code>
      <p>
        Check that updates omit <code>credentials.json</code>, resource paths match declarations and
        every intended migration file is present.
      </p>

      <h2>Localize by lifecycle stage</h2>
      <table>
        <thead><tr><th>Failure appears…</th><th>Inspect first</th></tr></thead>
        <tbody>
          <tr><td>During packaging</td><td>Directory id, manifest identity, credential ids, asset/migration filenames</td></tr>
          <tr><td>Before review</td><td>Archive shape, size, Framework API, capabilities and manifest policy</td></tr>
          <tr><td>During candidate validation</td><td><code>schema.sql</code>, missing Action names, View evaluation and Projection SQL</td></tr>
          <tr><td>When Action runs</td><td>argument validation, remaining deadline, native policy, provider status and thrown error</td></tr>
          <tr><td>After commit but View is stale</td><td>transaction usage, bounded Projection registration, foreground App and revision path</td></tr>
          <tr><td>Only after restart</td><td>workspace path, installed release, SQLite file, update recovery and schedule store</td></tr>
          <tr><td>Only on the board</td><td>PSRAM, LittleFS, Wi-Fi/NVS, TLS, response bounds, touch/display and UART state</td></tr>
        </tbody>
      </table>

      <h2>Evidence tiers</h2>
      <Code>{`source inspection
  < unit / contract tests
  < simulator end-to-end
  < ESP32 cross-build
  < physical boot and UI
  < fresh physical provider end-to-end`}</Code>
      <p>
        Report the highest tier actually observed and name what remains. A green simulator does not
        prove Wi-Fi association; a boot does not prove a fresh provider response; an older physical
        result does not automatically validate a later Source App refactor.
      </p>
    </>,
  },
];
