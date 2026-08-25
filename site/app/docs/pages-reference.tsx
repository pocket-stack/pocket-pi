import type { DocRecord } from "./doc-components";
import { Code, DocLead, Fact, PageGoal, SourceLink } from "./doc-components";

export const REFERENCE_DOCS: DocRecord[] = [
  {
    slug: "manifest",
    title: "App manifest",
    description: "The complete app.json contract for identity, compatibility, versions, capabilities, Tools, schedules, native services and resources.",
    render: () => <>
      <h1>App manifest</h1>
      <DocLead>
        <code>app.json</code> is strict install intent. Unknown top-level fields are rejected, the App
        id anchors storage and routing, and native permissions are reviewed as part of the release.
      </DocLead>
      <PageGoal>
        Exact field meanings, defaults and validation relationships so a developer can write or review
        a manifest without reverse-engineering runtime structs.
      </PageGoal>

      <h2>Minimal manifest</h2>
      <Code>{`{
  "format": 1,
  "frameworkApi": 1,
  "id": "counter",
  "title": "Counter",
  "description": "A durable counter",
  "version": "0.1.0",
  "schemaVersion": 1,
  "capabilities": ["data.sqlite"],
  "resources": {},
  "toolNamespace": "counter",
  "tools": [],
  "schedules": []
}`}</Code>

      <h2>Top-level fields</h2>
      <table>
        <thead><tr><th>Field</th><th>Required/effective default</th><th>Meaning and validation</th></tr></thead>
        <tbody>
          <tr><td><code>format</code></td><td>Must be <code>1</code></td><td>Source package container contract</td></tr>
          <tr><td><code>frameworkApi</code></td><td>Must equal current runtime, now <code>1</code></td><td>Compatibility with platform-owned <code>PocketPi.*</code> Framework</td></tr>
          <tr><td><code>id</code></td><td>Required</td><td>Stable safe component; must match <code>apps/&lt;id&gt;</code>; cannot be <code>pi-agent</code> for ordinary packages</td></tr>
          <tr><td><code>title</code></td><td>Non-empty</td><td>Human-facing review and Apps UI name</td></tr>
          <tr><td><code>description</code></td><td>Required string</td><td>Human/product description</td></tr>
          <tr><td><code>version</code></td><td>Non-empty</td><td>Release metadata shown to people; runtime does not impose SemVer parsing</td></tr>
          <tr><td><code>schemaVersion</code></td><td>Ordinary App: positive integer</td><td>SQLite compatibility, independent from source release version</td></tr>
          <tr><td><code>capabilities</code></td><td><code>[]</code></td><td>Unique values from <code>data.fs</code>, <code>data.sqlite</code>, <code>net.http</code></td></tr>
          <tr><td><code>toolNamespace</code></td><td>App id</td><td>Every public Tool name must start with <code>&lt;namespace&gt;.</code></td></tr>
          <tr><td><code>tools</code></td><td><code>[]</code></td><td>Public Agent Tool definitions plus local <code>action</code> route</td></tr>
          <tr><td><code>schedules</code></td><td><code>[]</code></td><td>Periodic local Action declarations</td></tr>
          <tr><td><code>nativeServices</code></td><td>empty HTTP/MCP lists</td><td>Exact native endpoint, connection and credential policy</td></tr>
          <tr><td><code>providerOperations</code></td><td><code>[]</code></td><td>Unique non-empty native provider operation allowlist, used by MCP Apps</td></tr>
          <tr><td><code>resources</code></td><td><code>{`{}`}</code></td><td>Named manifest-declared JSON files under <code>assets/</code></td></tr>
        </tbody>
      </table>

      <h2>Tool entry</h2>
      <Code>{`{
  "name": "research.search",
  "action": "search",
  "description": "Search and save a bounded local result set.",
  "parameters": {
    "type": "object",
    "properties": {
      "query": { "type": "string", "minLength": 1 }
    },
    "required": ["query"],
    "additionalProperties": false
  }
}`}</Code>
      <ul>
        <li><code>name</code> must use this App&apos;s namespace and be globally unique among installed Apps.</li>
        <li><code>action</code> must be a non-empty local name without a dot.</li>
        <li>The public model definition removes <code>action</code>; it receives name, description and parameters.</li>
        <li>Installation evaluates <code>actions.js</code> and verifies that the routed function exists.</li>
      </ul>

      <h2>Schedule entry</h2>
      <Code>{`{
  "id": "history-cleanup",
  "everyMinutes": 60,
  "action": "cleanup",
  "args": { "maxAgeDays": 7 }
}`}</Code>
      <p>
        <code>action</code> follows the same local-name rule. Runtime cadence is at least one minute.
        <code>args</code> defaults to JSON null if omitted; prefer an explicit object for reviewability.
      </p>

      <h2>HTTP service policy</h2>
      <Code>{`"nativeServices": {
  "http": [{
    "method": "POST",
    "urls": ["https://api.example.com/search"],
    "allowedRequestHeaders": ["accept", "content-type"],
    "credential": {
      "id": "example.api-key",
      "header": "authorization",
      "prefix": "Bearer "
    }
  }]
}`}</Code>
      <p><code>credential</code> may be null/omitted for an endpoint that needs no secret.</p>

      <h2>MCP service policy</h2>
      <Code>{`"nativeServices": {
  "mcp": [{
    "connection": "portfolio",
    "url": "https://provider.example.com/mcp",
    "credential": {
      "id": "portfolio.oauth-token",
      "header": "authorization",
      "prefix": "Bearer "
    }
  }]
},
"providerOperations": ["get_accounts", "get_portfolio"]`}</Code>

      <h2>Resource entry</h2>
      <Code>{`"resources": {
  "toolCatalog": {
    "path": "assets/tool-catalog.json",
    "type": "json"
  }
}`}</Code>
      <p>
        Declared resource paths must exactly equal the files under <code>assets/</code>. Resource names,
        App ids and path components accept ASCII letters, digits, dot, dash and underscore, excluding
        empty, <code>.</code> and <code>..</code> components.
      </p>
      <p>Complete examples: <SourceLink path="apps/exa/app.json">Exa</SourceLink> and <SourceLink path="apps/robinhood/app.json">Robinhood</SourceLink>.</p>
    </>,
  },
  {
    slug: "runtime-api",
    title: "PocketPi API",
    description: "Reference for the public System Framework API available to actions.js and view.js.",
    render: () => <>
      <h1><code>PocketPi</code> API</h1>
      <DocLead>
        The platform-owned System Framework installs one frozen <code>globalThis.PocketPi</code> in
        every App Guest before evaluating App source. Native mounts remain authoritative underneath it.
      </DocLead>
      <PageGoal>
        Exact public functions grouped by Action, Data, View event, resource and native service use,
        plus the line between supported App API and private host ABI.
      </PageGoal>

      <h2>Compatibility</h2>
      <Code>{`PocketPi.frameworkApi // 1`}</Code>
      <p>An ordinary App declares the same value in <code>app.json</code>.</p>

      <h2>Action registration</h2>
      <Code>{`PocketPi.defineActions({
  refresh,
  search,
  cleanup,
});`}</Code>
      <p>
        Call once in <code>actions.js</code>. Every property must be a non-empty name mapped to a function.
        Candidate installation compares the registered names with Tool and schedule routes.
      </p>

      <h2>Action and command events</h2>
      <table>
        <thead><tr><th>API</th><th>Returns</th><th>Use</th></tr></thead>
        <tbody>
          <tr><td><code>PocketPi.action(name, args = {})</code></td><td><code>{`{ type: "action", action, args }`}</code></td><td>Return from View input to request a local Action</td></tr>
          <tr><td><code>PocketPi.navigate(app)</code></td><td><code>apps.open</code> command event</td><td>Return from View input to open another App, normally <code>pi-agent</code></td></tr>
          <tr><td><code>PocketPi.command(name, args = {})</code></td><td><code>{`{ type: "command", command, args }`}</code></td><td>Narrow host command event; native authorization still applies</td></tr>
        </tbody>
      </table>
      <p>
        Ordinary Apps cannot gain System privilege by emitting a command string. Installer, device and
        Agent commands are accepted only from the resident System App where required.
      </p>

      <h2>Data</h2>
      <Code>{`PocketPi.data.query(sql, params)
PocketPi.data.exec(sql)
PocketPi.data.transaction(callback)`}</Code>
      <ul>
        <li><code>query</code> returns rows as plain objects keyed by column name.</li>
        <li><code>exec</code> executes SQL without parameters and returns no domain value.</li>
        <li><code>transaction</code> is available only in writable Action Guests, rolls back on throw and publishes one revision after commit.</li>
        <li>View Guests may query through Projections but do not receive writable database operations.</li>
      </ul>

      <h2>Projections</h2>
      <Code>{`const binding = PocketPi.projection.one(sql, paramsOrFunction, apply)
const binding = PocketPi.projection.many(sql, paramsOrFunction, apply)

binding.refresh()`}</Code>
      <p>
        A binding refreshes immediately when declared. <code>one</code> applies the first row or null;
        <code>many</code> applies all rows. The returned frozen binding exposes <code>refresh()</code>
        for App-controlled pagination in addition to revision-driven refresh.
      </p>

      <h2>Resources</h2>
      <Code>{`const value = PocketPi.resources.get("toolCatalog");`}</Code>
      <p>Unknown names throw. Returned JSON is recursively frozen inside that Guest.</p>

      <h2>Native services and deadline</h2>
      <Code>{`PocketPi.services.call(service, operation, args = {})
PocketPi.actionContext.remainingMs()`}</Code>
      <p>
        <code>services.call</code> is valid only during an admitted Action and throws the native error
        when the operation fails. <code>remainingMs()</code> reports the remainder of the one absolute
        Action deadline and should bound downstream work.
      </p>

      <h2>View registration</h2>
      <Code>{`PocketPi.defineView(definition)`}</Code>
      <p>
        The public View SDK calls this from <code>View.mount()</code>. Ordinary Apps should normally
        mount through <code>View</code> rather than hand-author the lower-level tick/input definition.
      </p>

      <h2>System-only API</h2>
      <Code>{`PocketPi.defineSystem({ update, telemetryVisible })`}</Code>
      <p>
        This is accepted only for App id <code>pi-agent</code>. It binds native <code>SystemFacts</code>
        to the Root View; ordinary Apps use SQLite Projections instead.
      </p>

      <h2>Private ABI</h2>
      <p>
        <code>globalThis.PocketPiSystem</code> is the native-facing ABI for configuring a Guest,
        beginning/polling Actions, refreshing bindings and dispatching input. App source must not call
        it. Its shape may change with runtime internals even while <code>frameworkApi</code> remains stable.
      </p>
      <p>Authoritative source: <SourceLink path="system/framework.js" />.</p>
    </>,
  },
  {
    slug: "view-api",
    title: "View API",
    description: "Reference for View state, primitives, Pi Design components, supported styles, colors and input behavior.",
    render: () => <>
      <h1><code>View</code> API</h1>
      <DocLead>
        The shared View SDK exposes a small retained-UI recipe API over PocketJS. It tracks state reads,
        reconciles compatible nodes, maps styles to native properties and routes press input through
        the host&apos;s hit testing.
      </DocLead>
      <PageGoal>
        The complete exported surface, supported styles and values, component responsibilities and the
        layout/input rules that otherwise appear only in <code>system/view-sdk.js</code>.
      </PageGoal>

      <h2>State and mounting</h2>
      <table>
        <thead><tr><th>API</th><th>Behavior</th></tr></thead>
        <tbody>
          <tr><td><code>View.state(initial)</code></td><td>Returns frozen <code>{`{ get, set, update }`}</code>; tracked reads mark dependent render text/tree dirty</td></tr>
          <tr><td><code>View.mount(render, onDataChanged?)</code></td><td>Mounts exactly one render function and optional file-data refresh callback, then installs tick/data/input hooks</td></tr>
          <tr><td><code>View.measureText(text, style?)</code></td><td>Measures through the baked native font slot</td></tr>
          <tr><td><code>View.viewport</code></td><td>Frozen host viewport with width, height, orientation, scale, layoutWidth and layoutHeight</td></tr>
          <tr><td><code>View.colors</code></td><td>Frozen named ABGR color map</td></tr>
        </tbody>
      </table>

      <h2>Primitive recipes</h2>
      <table>
        <thead><tr><th>Primitive</th><th>Responsibility</th></tr></thead>
        <tbody>
          <tr><td><code>Box(props)</code></td><td>Generic native View node; multiple flow children require explicit direction</td></tr>
          <tr><td><code>Row(props)</code></td><td><code>Box</code> with row direction</td></tr>
          <tr><td><code>Column(props)</code></td><td><code>Box</code> with column direction</td></tr>
          <tr><td><code>Text(props|string|number)</code></td><td>Native text node; <code>text</code> may be a tracked function</td></tr>
          <tr><td><code>Pressable(props)</code></td><td>Box requiring <code>onPress</code>; participates in native hit testing/feedback</td></tr>
        </tbody>
      </table>

      <h2>Pi Design components</h2>
      <table>
        <thead><tr><th>Component</th><th>Key props</th></tr></thead>
        <tbody>
          <tr><td><code>Screen</code></td><td>full viewport; accepts children/style</td></tr>
          <tr><td><code>Card</code></td><td>surface defaults; accepts children/style</td></tr>
          <tr><td><code>Header</code></td><td><code>title</code>, <code>metaTop</code>, <code>metaBottom</code>, <code>onBack</code>, <code>accent</code></td></tr>
          <tr><td><code>PageIntro</code></td><td><code>eyebrow</code>, <code>title</code>, <code>description</code>, <code>tone</code></td></tr>
          <tr><td><code>SectionHeading</code></td><td><code>title</code>, <code>detail</code>, optional <code>action</code> label affordance</td></tr>
          <tr><td><code>ActionButton</code></td><td><code>label</code>, <code>onPress</code>, <code>disabled</code>, <code>tone</code></td></tr>
          <tr><td><code>Badge</code></td><td><code>label</code>, <code>tone</code></td></tr>
          <tr><td><code>EmptyState</code></td><td><code>icon</code>, <code>title</code>, <code>detail</code>, <code>compact</code>, <code>tone</code></td></tr>
          <tr><td><code>MetricCard</code></td><td><code>label</code>, <code>value</code>, <code>tone</code></td></tr>
          <tr><td><code>StatusBar</code></td><td><code>text</code>, <code>tone</code>, <code>dark</code></td></tr>
          <tr><td><code>NavigationBar</code></td><td>Array of label, onPress and active navigation items</td></tr>
          <tr><td><code>ScrollButton</code></td><td><code>direction: &quot;up&quot; | &quot;down&quot;</code>, <code>onPress</code></td></tr>
          <tr><td><code>ScrollRail</code></td><td><code>onUp</code>, <code>onDown</code></td></tr>
          <tr><td><code>Keyboard</code></td><td><code>layer: &quot;lower&quot; | &quot;upper&quot; | &quot;symbols&quot;</code>, <code>onKey</code></td></tr>
          <tr><td><code>Sparkline</code></td><td>Viewport-aware values, labels, tone and empty-state chart recipe</td></tr>
        </tbody>
      </table>

      <h2>Layout styles</h2>
      <p>
        Numeric dimensions are design units scaled exactly once by the SDK. Portrait uses a 720×1280
        reference canvas and landscape uses 800×480. Size properties also accept <code>&quot;full&quot;</code>.
      </p>
      <Code>{`width height minWidth minHeight maxWidth maxHeight
padding paddingX paddingY paddingTop paddingRight paddingBottom paddingLeft
margin marginX marginY marginTop marginRight marginBottom marginLeft
gap direction justify align grow shrink basis wrap
position top right bottom left display overflow zIndex hitPass`}</Code>
      <table>
        <thead><tr><th>Property</th><th>Accepted names</th></tr></thead>
        <tbody>
          <tr><td><code>direction</code></td><td><code>row</code>, <code>column</code></td></tr>
          <tr><td><code>justify</code></td><td><code>start</code>, <code>center</code>, <code>end</code>, <code>between</code>, <code>around</code></td></tr>
          <tr><td><code>align</code></td><td><code>start</code>, <code>center</code>, <code>end</code>, <code>stretch</code></td></tr>
          <tr><td><code>position</code></td><td><code>relative</code>, <code>absolute</code></td></tr>
          <tr><td><code>display</code></td><td><code>flex</code>, <code>none</code></td></tr>
          <tr><td><code>overflow</code></td><td><code>visible</code>, <code>hidden</code></td></tr>
        </tbody>
      </table>

      <h2>Visual, text and transform styles</h2>
      <Code>{`background radius opacity borderColor borderWidth shadow
color fontSize fontWeight textAlign lineHeight tracking
translateX translateY scale rotate scaleX scaleY originX originY
arcStart arcSweep arcWidth`}</Code>
      <p>
        <code>fontWeight</code> is <code>regular</code> or <code>bold</code>.
        <code>fontSize</code> uses <code>body</code>, <code>lg</code>, <code>xl</code>; the
        <code>title</code> slot is available in bold. <code>textAlign</code> is left, center or right.
      </p>

      <h2>Named colors and tones</h2>
      <Code>{`canvas surface shell shellMuted text heading muted subtle border disabled
white accent accentSoft info infoSoft success successSoft
warning warningText warningSoft danger dangerSoft dangerOnDark`}</Code>
      <p>
        <code>Badge</code> accepts neutral, info, success, warning and danger tones. Shared components
        intentionally own common visual semantics; Apps own the conditions that select a tone.
      </p>

      <h2>Input behavior</h2>
      <p>
        Pointer down resolves the nearest pressable ancestor and applies pressed feedback by darkening
        its background or lowering opacity. Pointer up restores the value. Tap calls <code>onPress</code>
        and returns its event to the host. Keep handlers synchronous and return an event; long work
        belongs to an Action.
      </p>
      <p>Authoritative source: <SourceLink path="system/view-sdk.js" />.</p>
    </>,
  },
  {
    slug: "cli-reference",
    title: "CLI reference",
    description: "Current build, package, simulator, provisioning, install, bridge, flash and validation commands.",
    render: () => <>
      <h1>CLI reference</h1>
      <DocLead>
        Repository commands are intentionally centralized in <code>cargo xtask</code> for generated
        System assets and source App packaging. Hardware UART tools each own one narrow operation.
      </DocLead>
      <PageGoal>
        Copyable current commands and option meanings without mixing System builds, ordinary App
        delivery, model provisioning and development-only bridging.
      </PageGoal>

      <h2><code>cargo xtask</code></h2>
      <table>
        <thead><tr><th>Command</th><th>Result</th></tr></thead>
        <tbody>
          <tr><td><code>cargo xtask build pi-agent</code></td><td>Rebuild only the resident Pi Agent JavaScript bundle</td></tr>
          <tr><td><code>cargo xtask build view-sdk</code></td><td>Rebuild only the shared PocketJS View resource pack</td></tr>
          <tr><td><code>cargo xtask package app &lt;id&gt; [credentials.json]</code></td><td>Create <code>target/pocketapps/&lt;id&gt;.pocketapp</code></td></tr>
          <tr><td><code>cargo xtask build esp32-sim</code></td><td>Build the simulator with committed generated System assets</td></tr>
          <tr><td><code>cargo xtask run esp32-sim [args]</code></td><td>Build and run the simulator with committed generated System assets</td></tr>
          <tr><td><code>cargo xtask snapshot esp32-sim</code></td><td>Write deterministic screenshot to <code>artifacts/screenshots/</code></td></tr>
          <tr><td><code>cargo xtask build esp32-p4</code></td><td>Build ESP32-P4 release firmware with committed generated System assets</td></tr>
          <tr><td><code>cargo xtask build esp32-s3</code></td><td>Build ESP32-S3 release firmware with committed generated System assets</td></tr>
        </tbody>
      </table>
      <p>
        Normal simulator and firmware commands do not inspect or modify a neighboring PocketJS
        checkout. <code>POCKETJS_ROOT=/path/to/pocketjs</code> applies only to
        <code>cargo xtask build view-sdk</code>; that command verifies the exact pinned PocketJS
        revision before replacing the generated resource pack.
      </p>

      <h2>Simulator arguments</h2>
      <Code>{`cargo xtask run esp32-sim \\
  --backend codex \\
  --workspace target/esp32-workspace \\
  --app pi-agent`}</Code>
      <p>
        <code>xtask</code> inserts the executable separator itself, so pass simulator flags directly.
        Supported arguments are:
      </p>
      <ul>
        <li><code>--backend codex|openai|openrouter|anthropic|deepseek</code>;</li>
        <li><code>--model &lt;id&gt;</code>;</li>
        <li><code>--workspace &lt;path&gt;</code>;</li>
        <li><code>--viewport 720x1280|800x480|480x800</code> for View SDK and orientation testing;</li>
        <li><code>--app pi-agent|files|apps|settings|keyboard|&lt;installed-id&gt;</code>;</li>
        <li><code>--prompt &lt;text&gt;</code> and <code>--tap x,y</code> for deterministic scenarios;</li>
        <li><code>--screenshot &lt;path&gt;</code> on the simulator binary.</li>
      </ul>
      <Fact>
        For the normal <code>xtask</code> form use
        <code>cargo xtask run esp32-sim --backend codex</code>. Do not add a second separator unless
        invoking Cargo directly.
      </Fact>

      <h2>UART helper boundary</h2>
      <p>
        The three Python commands share one raw 115200-baud POSIX UART layer in
        <code>tools/uart_io.py</code>. It leaves DTR and RTS inactive when closing the port. Provisioning
        and the development bridge perform one explicit reset to enter their boot exchange;
        <code>uart-install.py</code> does not reset the board or change model configuration and only
        transfers one package to the on-device review flow.
      </p>

      <h2>Provision a physical board</h2>
      <Code>{`python3 tools/uart-provision.py "$DEVICE_PORT" \\
  --provider deepseek \\
  --thinking-level high \\
  --provision-wifi`}</Code>
      <p>
        Providers: openai, openrouter, anthropic, deepseek. <code>--model</code> overrides the provider
        default. Thinking level is high or xhigh. The command resets once to enter provisioning and
        waits for native storage confirmation. DeepSeek alone can read account
        <code>deepseek-api-key</code> from macOS Keychain service <code>Pocket Pi Credentials</code>;
        otherwise the tool prompts without echo.
      </p>

      <h2>Upload an App over UART</h2>
      <Code>{`python3 tools/uart-install.py "$DEVICE_PORT" \\
  target/pocketapps/exa.pocketapp`}</Code>
      <p>This transfers one package to a running device and waits for the upload acknowledgement; confirmation remains on-device.</p>

      <h2>Development-only model bridge</h2>
      <Code>{`python3 tools/uart-model-bridge.py "$DEVICE_PORT" \\
  --provider codex \\
  --thinking-level high \\
  --prompt "List your workspace." \\
  --prompt-delay-seconds 3`}</Code>
      <p>Providers: codex or claude-code. Prompt delay accepts 0 to 120 seconds.</p>

      <h2>Flash and monitor</h2>
      <Code>{`espflash list-ports
export DEVICE_PORT=/dev/cu.usbmodem...
espflash board-info --port "$DEVICE_PORT"

espflash flash --baud 921600 --port "$DEVICE_PORT" \\
  --partition-table firmware/esp32-p4/partitions.csv \\
  firmware/esp32-p4/target/riscv32imafc-esp-espidf/release/pocket-pi-p4

espflash monitor --port "$DEVICE_PORT"
espflash reset --port "$DEVICE_PORT" --non-interactive`}</Code>

      <h2>Validation</h2>
      <Code>{`cargo test --workspace
bun test apps/pi-agent/text.test.js
cargo clippy --workspace --all-targets -- -D warnings`}</Code>
    </>,
  },
  {
    slug: "limits",
    title: "Limits and compatibility",
    description: "Current public package/runtime limits and implementation bounds that App and host developers must design around.",
    render: () => <>
      <h1>Limits and compatibility</h1>
      <DocLead>
        PocketPi uses explicit bounds because the reference target is a constrained device.
        Some values are public App contracts; others describe the current implementation and may move
        with measured hardware evidence.
      </DocLead>
      <PageGoal>
        One lookup table for package size, resources, credentials, Action timing, queues, Guests,
        network bodies, viewport and compatibility versions, with contract and implementation values separated.
      </PageGoal>

      <h2>Public App/package contract</h2>
      <table>
        <thead><tr><th>Limit</th><th>Current value</th><th>Behavior</th></tr></thead>
        <tbody>
          <tr><td>Package format</td><td><code>1</code></td><td>Other formats rejected</td></tr>
          <tr><td>Framework API</td><td><code>1</code></td><td>Must equal installed System Framework</td></tr>
          <tr><td><code>.pocketapp</code> size</td><td>2 MiB</td><td>Rejected at ingress/staging when exceeded</td></tr>
          <tr><td>One JSON resource</td><td>256 KiB</td><td>Rejected before candidate evaluation</td></tr>
          <tr><td>All JSON resources</td><td>512 KiB</td><td>Sum of declared files</td></tr>
          <tr><td>Asset archive path</td><td>100 bytes maximum</td><td>Safe <code>assets/</code> components only</td></tr>
          <tr><td>Credential count</td><td>16 maximum</td><td>First-install transport file only</td></tr>
          <tr><td>One credential value</td><td>4096 bytes maximum; non-empty</td><td>Stored natively after validation</td></tr>
          <tr><td>Schema version</td><td>1 to signed 32-bit maximum</td><td>Forward-only updates</td></tr>
          <tr><td>Supported capability names</td><td>3</td><td><code>data.fs</code>, <code>data.sqlite</code>, <code>net.http</code></td></tr>
        </tbody>
      </table>

      <h2>Current runtime bounds</h2>
      <table>
        <thead><tr><th>Bound</th><th>Current value</th><th>Design consequence</th></tr></thead>
        <tbody>
          <tr><td>Ordinary View Guest cache</td><td>3-entry LRU</td><td>View heap is evictable</td></tr>
          <tr><td>Ordinary Action Guest cache</td><td>3-entry LRU</td><td>Action initialization must be repeatable</td></tr>
          <tr><td>Resident System Guests</td><td>1</td><td>Pi Agent stays outside ordinary LRUs</td></tr>
          <tr><td>Maximum simultaneous Guests</td><td>7</td><td>1 + 3 View + 3 Action</td></tr>
          <tr><td>Ordinary Action execution</td><td>one at a time</td><td>No assumed App concurrency</td></tr>
          <tr><td>Action admission queue</td><td>8</td><td>Do not use Actions as an unbounded job system</td></tr>
          <tr><td>Tool Action deadline</td><td>80 seconds absolute</td><td>Queueing, JS and native transport share it</td></tr>
          <tr><td>App-local View FS mount quota</td><td>2 MiB</td><td>Prefer SQLite for structured durable state</td></tr>
          <tr><td>Logical viewport</td><td>720×1280</td><td>All View coordinates and layout use this space</td></tr>
        </tbody>
      </table>

      <h2>HTTP body bounds</h2>
      <p>
        App <code>fetch()</code> defaults to a 30-second request timeout and 128 KiB
        <code>maxBytes</code>, but the request is also capped by the remaining App Action deadline.
        Set both values explicitly for the operation. The ESP32 MCP host currently bounds one response
        at 160 KiB; that is a host implementation limit rather than a generic App HTTP promise.
      </p>

      <h2>Source compatibility</h2>
      <ul>
        <li>One classic-script <code>actions.js</code> and one <code>view.js</code>; no imports.</li>
        <li>No TypeScript/TSX/JSX transform in ordinary App packaging or on device.</li>
        <li>JSON resources only; no arbitrary binary resource contract.</li>
        <li>No dependency solver, package graph or App plugin loader.</li>
        <li>System App/Harness and System Framework update with firmware today.</li>
      </ul>
      <Fact>
        Treat public format/API/size validation as compatibility constraints. Treat cache counts,
        queues and worker sizes as current implementation bounds that should change only with profiling
        and physical-target evidence.
      </Fact>
    </>,
  },
];
