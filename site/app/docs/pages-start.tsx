import type { DocRecord } from "./doc-components";
import { Code, DocLead, Fact, PageGoal, SourceLink, Status } from "./doc-components";

export const START_DOCS: DocRecord[] = [
  {
    slug: "overview",
    title: "Overview",
    description: "A positive definition of PocketPi, the product mental model, and the right path through the documentation.",
    render: () => <>
      <h2 id="product-overview">Product overview</h2>
      <DocLead>
        PocketPi is an <strong>Agent-native runtime for embedded and dedicated devices</strong>.
        It keeps an Agent resident on the product and gives it a durable workspace, native
        capabilities, schedules, and installable Apps with local Data, shared Actions and fixed
        human-facing Views.
      </DocLead>
      <PageGoal>
        A first-principles definition of the product, one end-to-end execution story, and a reading
        path based on whether you want to try the runtime, build an App, port a host, or understand
        the architecture.
      </PageGoal>

      <h2>The shortest useful definition</h2>
      <Code>{`PocketPi = resident Agent + durable workspace + native capabilities + Apps

App = Data + Actions + View`}</Code>
      <p>
        The Agent is not launched for one request and then discarded. It shares the device
        lifecycle, owns its top-level <code>/workspace</code>, and can reason across the capabilities
        that are currently installed. An App is not just a Tool and not just a screen: it combines
        durable product state, deterministic behavior, and a fixed interface for people.
      </p>

      <h2>One concrete device story</h2>
      <ol>
        <li>The device boots and restores the resident Pi Agent and its <code>/workspace</code>.</li>
        <li>The Agent sees native Tools plus the public Tools of installed Apps.</li>
        <li>A person asks the Agent to refresh a portfolio, or taps the App&apos;s refresh button.</li>
        <li>Both requests route to the same named App Action.</li>
        <li>The Action calls an allowed native service and commits normalized data to App-owned SQLite.</li>
        <li>The successful transaction increments one App revision.</li>
        <li>If the App View is visible, its bounded Projection re-queries SQLite and the fixed View refreshes.</li>
        <li>If the View is closed, the data still commits; the View projects the latest state when opened later.</li>
      </ol>
      <Code>{`Agent Tool ─┐
UI event   ─┼─→ Action → native capability → SQLite → revision → Projection → View
Schedule   ─┘`}</Code>

      <h2>The four things to keep separate</h2>
      <table>
        <thead><tr><th>Part</th><th>Responsibility</th><th>Durable state</th></tr></thead>
        <tbody>
          <tr><td><strong>Pi Agent</strong></td><td>Resident reasoning, cross-App coordination and workspace Tools</td><td><code>/workspace</code></td></tr>
          <tr><td><strong>Ordinary App</strong></td><td>Domain Tools, Actions, schedules, local Data and fixed View</td><td>Its private SQLite/files</td></tr>
          <tr><td><strong>PocketJS</strong></td><td>Bounded JavaScript Guests, UI tree, layout and rendering contracts</td><td>None by itself</td></tr>
          <tr><td><strong>Native Host</strong></td><td>Hardware, transport, credentials, enforcement and lifecycle</td><td>NVS/LittleFS or host equivalents</td></tr>
        </tbody>
      </table>

      <h2>PocketPi is the product name</h2>
      <p>
        The precise technical category is <strong>Agent-native runtime</strong>. PocketPi is broader
        than the embedded Pi Agent loop: it owns the Agent/App lifecycle, capability boundary,
        schedules, workspace, foreground selection and recovery model. Pi Agent remains the current
        resident Harness; a replaceable Harness boundary is a staged target, not a shipped claim.
      </p>
      <Fact>
        Product name: <strong>PocketPi</strong>. Technical category: <strong>an Agent-native
        runtime for embedded devices</strong>.
      </Fact>

      <h2>Choose your path</h2>
      <table>
        <thead><tr><th>Your goal</th><th>Start with</th><th>You will finish with</th></tr></thead>
        <tbody>
          <tr><td>See the product running</td><td><a href="/docs/getting-started">Getting started</a></td><td>A persistent simulator workspace and resident Agent</td></tr>
          <tr><td>Build an App</td><td><a href="/docs/app-guide">App developer guide</a></td><td>A packaged App installed through the real review path</td></tr>
          <tr><td>Operate physical hardware</td><td><a href="/docs/esp32-p4">ESP32-P4</a> or <a href="/docs/esp32-s3">ESP32-S3</a></td><td>A provisioned standalone target and an explicit validation boundary</td></tr>
          <tr><td>Understand internals</td><td><a href="/docs/runtime-flow">Runtime flow</a></td><td>Ownership and lifecycle traced from actor to display</td></tr>
          <tr><td>Integrate a different Harness</td><td><a href="/docs/harnesses">Harness boundary</a></td><td>A clear view of current Pi coupling and the planned replaceable seam</td></tr>
        </tbody>
      </table>

      <h2>Current product surface</h2>
      <p>
        ESP32-P4 is the reference target, and ESP32-S3 is a supported second physical target.
        The macOS <code>esp32-sim</code> runs their shared AgentOS, App source, Tool catalog,
        workspace and View contracts with development adapters. It is the normal development loop,
        not a second desktop product and not a CPU/peripheral emulator.
      </p>
      <p>
        The current resident Harness is Pi Agent. Ordinary source Apps can already be installed,
        updated and uninstalled independently. Harness replacement is a planned runtime boundary,
        not an implemented claim. See <a href="/docs/current-boundaries">Current boundaries</a>
        for the fact/target split.
      </p>
    </>,
  },
  {
    slug: "getting-started",
    title: "Getting started",
    description: "The shortest verified path from an empty checkout to a resident Agent running in the shared ESP32 simulator.",
    render: () => <>
      <h1>Getting started</h1>
      <DocLead>
        This is the fastest path from a clean checkout to PocketPi running locally. You will
        use the macOS product-contract simulator, preserve one workspace between launches, and
        verify the resident Agent before writing an App.
      </DocLead>
      <PageGoal>
        A working 720×1280 runtime window, a persistent simulator workspace, and a clear distinction
        between simulator proof and physical-device proof.
      </PageGoal>

      <h2>Prerequisites</h2>
      <table>
        <thead><tr><th>You want to…</th><th>You need</th></tr></thead>
        <tbody>
          <tr><td>Run the simulator</td><td>macOS, Rust stable, Bun and a logged-in <code>codex</code> CLI</td></tr>
          <tr><td>Regenerate the shared View SDK</td><td>A PocketJS checkout at the revision pinned by <code>tools/xtask</code></td></tr>
          <tr><td>Write/package an ordinary App</td><td>A text editor; packaging itself needs no PocketJS/Bun compile step</td></tr>
          <tr><td>Build physical firmware</td><td>The esp-rs/ESP-IDF toolchain, pinned Rust nightly and <code>espflash</code></td></tr>
        </tbody>
      </table>
      <p>
        Install <a href="https://rust-lang.org/tools/install/" target="_blank" rel="noreferrer">Rust with rustup</a> and
        <a href="https://bun.com/docs/installation" target="_blank" rel="noreferrer"> Bun</a> before starting.
        Physical targets additionally need the
        <a href="https://docs.espressif.com/projects/rust/book/getting-started/toolchain.html" target="_blank" rel="noreferrer"> ESP Rust toolchain</a> and
        <a href="https://github.com/esp-rs/espflash/blob/main/espflash/README.md" target="_blank" rel="noreferrer"> espflash</a>.
      </p>

      <h2>1. Clone PocketPi</h2>
      <Code>{`git clone https://github.com/pocket-stack/pocket-pi.git
cd pocket-pi`}</Code>
      <p>
        Normal simulator and firmware builds use the generated Pi Agent bundle and View SDK resources
        committed to this repository. You do not need a PocketJS checkout to start using PocketPi.
      </p>

      <h2>2. Start the simulator</h2>
      <Code>{`cargo xtask run esp32-sim \\
  --backend codex \\
  --workspace target/esp32-workspace`}</Code>
      <p>
        This builds and starts the simulator with the committed Pi Agent and View SDK assets. Reuse
        the same <code>--workspace</code> path so Agent files, installed Apps and App SQLite state
        survive subsequent launches.
      </p>

      <h2>3. Verify the first successful boot</h2>
      <p>You should see:</p>
      <ul>
        <li>a 720×1280 product surface scaled into a macOS window;</li>
        <li>the Pi Agent Root View with Chat, Files, Apps and Settings;</li>
        <li>Agent status moving from <strong>STARTING</strong> to <strong>IDLE</strong>;</li>
        <li>an App installer listening at <code>http://127.0.0.1:8080</code>;</li>
        <li><code>memory.md</code> and <code>notes.txt</code> inside the chosen workspace.</li>
      </ul>
      <p>
        Submit a small prompt such as <em>“List the files in your workspace.”</em> A complete answer
        proves that the resident Agent guest, model backend, Tool router and simulated workspace are
        connected.
      </p>

      <h2>4. Restart without losing the workspace</h2>
      <p>
        Stop the simulator and run the same command again. Workspace files persist because they live
        under the path you supplied. The conversation itself currently lives in the resident
        QuickJS heap and is not restored after reboot; that is a documented current boundary, not a
        workspace failure.
      </p>

      <h2>What this run proves</h2>
      <table>
        <thead><tr><th>Proved by the simulator</th><th>Still requires hardware</th></tr></thead>
        <tbody>
          <tr><td>AgentOS ownership, App source loading, Tools, workspace, schedules, fixed Views and macOS adapters</td><td>ESP32 boot, PSRAM pressure, LittleFS, NVS, Wi-Fi, LCD scanout, touch controller and live board transport</td></tr>
        </tbody>
      </table>
      <p>
        Next, build and install a real source App in <a href="/docs/app-quickstart">Build your first App</a>.
        For the full simulator CLI and provider choices, see <a href="/docs/simulator">Run the simulator</a>.
      </p>

      <h2>Regenerate System assets only when changing them</h2>
      <p>
        Rebuild the resident Pi Agent bundle independently. A PocketJS checkout is required only to
        regenerate the shared View SDK resource pack, and <code>xtask</code> verifies its exact pinned
        revision before writing the generated resource back into PocketPi.
      </p>
      <Code>{`cargo xtask build pi-agent

git clone https://github.com/pocket-stack/pocketjs.git ../pocketjs
git -C ../pocketjs checkout e12cf12f82cc60b636368119d49a06eb9ed2a3d5
POCKETJS_ROOT=../pocketjs cargo xtask build view-sdk`}</Code>
    </>,
  },
  {
    slug: "mental-model",
    title: "The mental model",
    description: "The smallest set of concepts that explains how people, Agents, Apps, PocketJS and the native host work together.",
    render: () => <>
      <h1>The mental model</h1>
      <DocLead>
        PocketPi becomes simpler when you follow ownership instead of implementation files.
        The Agent owns intent and its workspace; an App owns domain behavior and durable product
        state; PocketJS executes bounded JavaScript and renders the fixed View; the native host owns
        trusted mechanisms.
      </DocLead>
      <PageGoal>
        The vocabulary needed to predict what happens at boot, during a Tool call, after a UI tap,
        while an App is closed, and when the device restarts.
      </PageGoal>

      <h2>Start with actors, not layers</h2>
      <table>
        <thead><tr><th>Actor</th><th>Decides</th><th>Does not own</th></tr></thead>
        <tbody>
          <tr><td><strong>Person</strong></td><td>Prompts, navigation, review and direct UI intents</td><td>Provider transport or App database mutation</td></tr>
          <tr><td><strong>Agent</strong></td><td>Why and when to use capabilities; cross-App coordination</td><td>App schema, raw credentials or View synchronization</td></tr>
          <tr><td><strong>App</strong></td><td>How a domain operation validates, calls services, persists and presents data</td><td>Global workspace, hardware drivers or other Apps&apos; data</td></tr>
          <tr><td><strong>Native host</strong></td><td>Whether a capability is allowed and how bounded mechanisms reach hardware/services</td><td>Robinhood/Exa product semantics and View policy</td></tr>
        </tbody>
      </table>

      <h2>Then learn the three App parts</h2>
      <Code>{`Data
  durable App-owned SQLite/files

Actions
  named behavior shared by Agent Tools, UI events and schedules

View
  fixed PocketJS interface projected from bounded Data`}</Code>
      <p>
        Projection is the Data-to-View binding. It has no independent durable state, so it is not a
        fourth App concept. A View may keep small in-memory presentation state, but SQLite remains
        the durable product truth.
      </p>

      <h2>Follow one Action through the system</h2>
      <ol>
        <li><strong>Admission:</strong> a Tool route, UI event or App schedule selects a local Action name.</li>
        <li><strong>Execution:</strong> the Action Guest validates arguments and may call allowed native services.</li>
        <li><strong>Persistence:</strong> <code>PocketPi.data.transaction()</code> writes App-owned SQLite.</li>
        <li><strong>Invalidation:</strong> a successful commit increments an in-memory App revision.</li>
        <li><strong>Projection:</strong> only a visible stale View re-runs its bounded queries at a frame boundary.</li>
        <li><strong>Rendering:</strong> PocketJS reconciles the fixed View and the host displays its DrawList.</li>
      </ol>
      <Fact>
        The Agent never calls an <code>update_view</code> Tool. It changes product state through an
        Action; the View follows the App revision and projects the resulting Data.
      </Fact>

      <h2>What “resident” changes</h2>
      <p>
        Pi Agent&apos;s Agent loop and Root View share one System Guest created at boot. Opening an
        ordinary App changes which View owns the display and input; it does not destroy the Agent,
        its pending model request or its Tool registry. Ordinary View and Action Guests are bounded
        and may be evicted, so durable state cannot live only in JavaScript objects.
      </p>

      <h2>What runs where</h2>
      <Code>{`Device hardware
└── target OS / RTOS and native host
    ├── credentials, transport, storage enforcement, lifecycle
    ├── AppSupervisor, routing, schedules and recovery
    └── PocketJS / QuickJS platform
        ├── resident Pi Agent System Guest
        ├── ordinary View Guests
        └── ordinary Action Guests`}</Code>
      <p>
        This is where the RTOS boundary belongs: as one implementation layer beneath the product
        runtime. App developers normally work above it. Host developers use it when providing the
        native composition for a target.
      </p>

      <h2>Current versus target</h2>
      <table>
        <thead><tr><th>Area</th><th>Current on <code>upstream/main</code></th><th>Direction</th></tr></thead>
        <tbody>
          <tr><td>Resident Harness</td><td><Status>Implemented</Status> Pi Agent only</td><td>Build-time Pi or DeepSeek Harness behind one guest/host contract</td></tr>
          <tr><td>Ordinary Apps</td><td><Status>Implemented</Status> source install/update/uninstall</td><td>Broader App ecosystem without moving policy into firmware</td></tr>
          <tr><td>Hardware</td><td><Status>Implemented</Status> ESP32-P4 reference target and ESP32-S3 supported target</td><td>More native hosts only after a concrete board need exists</td></tr>
          <tr><td>Playground</td><td>Not implemented</td><td>Move the macOS product simulator to the web later</td></tr>
        </tbody>
      </table>
      <p>Deep implementation detail lives in <SourceLink path="docs/agentos-architecture.md" />.</p>
    </>,
  },
];
