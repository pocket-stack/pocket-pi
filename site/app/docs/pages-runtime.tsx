import type { DocRecord } from "./doc-components";
import { Code, DocLead, Fact, PageGoal, SourceLink, Status } from "./doc-components";

export const RUNTIME_DOCS: DocRecord[] = [
  {
    slug: "runtime-flow",
    title: "Runtime flow",
    description: "Trace boot, prompts, Tool calls, UI Actions, schedules, SQLite revisions and rendering end to end.",
    render: () => <>
      <h1>Runtime flow</h1>
      <DocLead>
        This page follows observable product events through their owners. Use it when you need to
        answer “what runs next?” without collapsing Agent reasoning, App behavior, JavaScript execution,
        native transport and rendering into one box.
      </DocLead>
      <PageGoal>
        End-to-end traces for boot, Agent Tool, UI Action, App schedule and View refresh, with the
        thread/lifecycle boundaries that keep the resident Agent responsive.
      </PageGoal>

      <h2>Boot</h2>
      <ol>
        <li>The host mounts storage and loads native model, transport, credential and hardware adapters.</li>
        <li><code>InstalledAppIndex</code> seeds the firmware-embedded Pi Agent System release.</li>
        <li>Any approved interrupted ordinary App update is recovered before the App index is loaded.</li>
        <li>Installed ordinary source releases are read from <code>apps/&lt;id&gt;/release</code>.</li>
        <li><code>AppSupervisor</code> creates the resident System Guest and loads its Root View plus Agent loop.</li>
        <li>Native Tools and installed App Tool definitions become one routed Tool catalog.</li>
        <li>The host begins normal frame, model-event, install and schedule polling.</li>
      </ol>
      <Code>{`boot
  → storage + native adapters
  → recover approved update
  → load System App + installed ordinary Apps
  → create resident Agent Guest
  → merge native Tools + App Tools
  → STARTING → IDLE`}</Code>

      <h2>Person prompt → Agent Tool → App Action</h2>
      <ol>
        <li>The Root View emits the narrow <code>agent.submit</code> command.</li>
        <li>The host passes the prompt to the resident Harness; model work occurs off the UI frame.</li>
        <li>The Harness selects a public Tool from the merged catalog.</li>
        <li><code>RoutedToolHost</code> keeps native Tools native and routes App Tools by installed Tool name.</li>
        <li>The request enters the single ordinary Action queue with one absolute 80-second deadline.</li>
        <li>An Action Guest is loaded/reused, receives <code>source: &quot;tool&quot;</code>, and runs the named function.</li>
        <li>The completed JSON-serializable result returns through the pending Agent Tool call.</li>
        <li>The resident Harness continues its model turn and streams text events back to the Root View.</li>
      </ol>

      <h2>UI event → same App Action</h2>
      <Code>{`pointer down / tap
  → PocketJS hit test
  → onPress returns PocketPi.action(name, args)
  → host queues the named Action
  → Action Guest executes with source: "ui"
  → result is logged/handled by host
  → committed Data invalidates the View`}</Code>
      <p>
        The View does not call the Action function in its own Guest. This preserves one mutation path
        and keeps provider/SQLite work away from touch and frame callbacks.
      </p>

      <h2>App schedule → headless Action</h2>
      <Code>{`native clock reaches due declaration
  → App schedule store claims it
  → same Action queue
  → Action Guest executes with source: "schedule"
  → success recorded only after completion
  → View may remain closed`}</Code>
      <p>
        A deterministic App schedule does not wake the model. An Agent wake schedule instead injects a
        prompt into the resident Harness when the Agent is idle.
      </p>

      <h2>Transaction → revision → View</h2>
      <Code>{`Action Guest
  PocketPi.data.transaction()
      BEGIN IMMEDIATE
      SQL writes
      COMMIT
      app.commit()
          ↓
  atomic App revision increments
          ↓
foreground frame compares in-memory revision
          ↓ stale only
  Projection queries through read-only View mount
          ↓
  View.state updates → PocketJS reconcile → DrawList → host display`}</Code>
      <p>
        Multiple commits before the next frame coalesce into one refresh. A background View performs
        no query; it catches up when foregrounded. Revision contains no data; it only says the durable
        source of truth may have changed.
      </p>

      <h2>Responsiveness boundary</h2>
      <p>
        Model requests, native Tools, HTTP/MCP and ordinary Actions must not block a host frame or touch
        callback. The host advances the resident Agent every frame even while another App owns the
        visible View. This lets a person navigate while an Agent turn or App service call is in flight.
      </p>
      <Fact>
        Foreground is a display/input selection, not an Agent lifecycle switch. Opening an App does not
        recreate Pi Agent or move the model loop into that App.
      </Fact>
    </>,
  },
  {
    slug: "guests-lifecycle",
    title: "Guests and lifecycle",
    description: "Understand QuickJS Guest isolation, the resident System Guest, independent View/Action LRUs and foreground behavior.",
    render: () => <>
      <h1>Guests and lifecycle</h1>
      <DocLead>
        PocketJS/QuickJS is one execution substrate, but each Guest is an isolated runtime and context
        with its own globals, objects, promises, job queue and heap. A Guest is not a thread and not a
        second framework.
      </DocLead>
      <PageGoal>
        The exact maximum Guest model, why ordinary Apps split View and Action execution, what eviction
        destroys, what SQLite preserves and how navigation interacts with the resident Agent.
      </PageGoal>

      <h2>Maximum resident Guest shape</h2>
      <Code>{`1 resident Pi Agent System Guest
+ up to 3 ordinary View Guests       independent LRU
+ up to 3 ordinary Action Guests     independent LRU
= up to 7 QuickJS Guests`}</Code>
      <p>
        The platform links one PocketJS/QuickJS implementation into firmware. Every Guest creation
        allocates an isolated runtime/context on that substrate. The System Framework is evaluated
        inside a Guest; it does not consume its own cache slot.
      </p>

      <h2>System Guest</h2>
      <ul>
        <li>created once during <code>AppSupervisor</code> boot;</li>
        <li>contains the Pi Agent loop, conversation heap, Tool registry and Root View;</li>
        <li>outside both ordinary-App LRU caches;</li>
        <li>advanced even when an ordinary App is foreground;</li>
        <li>destroyed only by shutdown or explicit System restart, not by navigation.</li>
      </ul>

      <h2>Ordinary View Guest</h2>
      <ul>
        <li>loads platform Framework, shared View SDK, declared JSON resources and <code>view.js</code>;</li>
        <li>receives a read-only mount of the App&apos;s shared native SQLite owner;</li>
        <li>owns View state, retained UI nodes, hit testing and Projection bindings;</li>
        <li>ticks and renders only when foreground;</li>
        <li>may remain cached after navigation, but must tolerate eviction.</li>
      </ul>

      <h2>Ordinary Action Guest</h2>
      <ul>
        <li>loads platform Framework, net SDK when allowed, resources and <code>actions.js</code>;</li>
        <li>receives writable App SQLite operations and scoped native services;</li>
        <li>runs Tool, UI and schedule Actions through the same dispatch surface;</li>
        <li>may exist while its View is closed or may not exist while its View is open;</li>
        <li>executes one admitted Action at a time in v1.</li>
      </ul>

      <h2>Why two ordinary caches</h2>
      <p>
        View lifetime follows navigation; Action lifetime follows capability use. Combining them would
        keep provider promises and response objects alive with the UI, or force a long Action to block
        View lifecycle. Independent LRUs let each side be bounded by its actual usage while sharing one
        durable App boundary.
      </p>

      <h2>What is shared and what is not</h2>
      <table>
        <thead><tr><th>Shared by an App</th><th>Not shared between Guests</th></tr></thead>
        <tbody>
          <tr><td>native SQLite owner and database file</td><td>JavaScript objects, globals and closures</td></tr>
          <tr><td>data root and installed release source</td><td>promises and job queues</td></tr>
          <tr><td>atomic revision counter</td><td>View reactive state</td></tr>
          <tr><td>manifest capabilities/resources</td><td>network response objects and call stacks</td></tr>
        </tbody>
      </table>

      <h2>Eviction contract</h2>
      <p>
        Eviction discards the entire ordinary Guest heap. The next use evaluates the Framework and App
        source again. SQLite/files, installed source and native schedule state remain. Therefore App
        initialization must be deterministic and must not assume an earlier JavaScript object still exists.
      </p>
      <Fact>
        If losing an ordinary Guest loses important product information, that information was stored in
        the wrong place. Persist it in App Data, then rebuild bounded presentation state from a Projection.
      </Fact>
    </>,
  },
  {
    slug: "layers-ownership",
    title: "Layers and ownership",
    description: "Place PocketPi, PocketJS, the native host, Apps and the target platform in the right architectural layer.",
    render: () => <>
      <h1>Layers and ownership</h1>
      <DocLead>
        The architecture is easiest to maintain when product policy stays in editable App/System source
        and mechanisms that require trust, bounds or hardware authority stay native.
      </DocLead>
      <PageGoal>
        A positive layer definition, an ownership table for common changes, repository boundaries and
        a single place for the lower target OS/RTOS relationship.
      </PageGoal>

      <h2>Runtime composition</h2>
      <Code>{`PocketPi product runtime
├── resident Harness + System App
├── App model, routing, schedules, lifecycle and recovery
├── installable ordinary Apps
├── PocketJS / QuickJS execution and UI
└── native host capabilities
    └── target platform: hardware + drivers + OS/RTOS services`}</Code>
      <p>
        PocketPi is the product-level runtime across these pieces. It is not defined by the
        lower scheduler/kernel layer; the target host uses that layer to implement storage, networking,
        clocks, threads, drivers and display/touch.
      </p>

      <h2>Layer responsibilities</h2>
      <table>
        <thead><tr><th>Layer</th><th>Owns</th><th>Does not own</th></tr></thead>
        <tbody>
          <tr><td><strong>Agent Harness</strong></td><td>model/tool loop, messages, turn state and Tool calling</td><td>device credentials, App schema or rendering</td></tr>
          <tr><td><strong>PocketPi</strong></td><td>resident System lifecycle, App catalog/routing, schedules, install/update/uninstall and ownership rules</td><td>generic UI layout implementation or domain product logic</td></tr>
          <tr><td><strong>PocketJS</strong></td><td>Guest/module contracts, retained UI, layout, text and DrawList generation</td><td>which App is installed, which Tool belongs to it or which App is foreground</td></tr>
          <tr><td><strong>Ordinary App</strong></td><td>domain Tools, Actions, schema, provider mapping, resources and View</td><td>global workspace, raw secrets or hardware drivers</td></tr>
          <tr><td><strong>Native host</strong></td><td>hardware, storage enforcement, credentials, TLS/transport, limits and rendering adapter</td><td>Robinhood/Exa semantics or View policy</td></tr>
        </tbody>
      </table>

      <h2>Mechanism versus policy examples</h2>
      <table>
        <thead><tr><th>Mechanism stays native</th><th>Policy stays in source</th></tr></thead>
        <tbody>
          <tr><td>credential storage and header application</td><td>which credential id/endpoint an App declares</td></tr>
          <tr><td>one SQLite owner and read/write enforcement</td><td>tables, queries, normalization and retention</td></tr>
          <tr><td>Tool route lookup and Action deadline</td><td>Tool description, JSON Schema and Action behavior</td></tr>
          <tr><td>touch coordinates and PocketJS DrawList rendering</td><td>layout, screens, content and interaction events</td></tr>
          <tr><td>scheduler clock and durable cursor</td><td>which App Action runs and with what args</td></tr>
        </tbody>
      </table>

      <h2>Changes and delivery boundary</h2>
      <table>
        <thead><tr><th>Change</th><th>Delivery</th></tr></thead>
        <tbody>
          <tr><td>native host, PocketJS platform, AppSupervisor or System Framework</td><td>Firmware build and flash</td></tr>
          <tr><td>Pi Agent Root View or resident Harness bundle</td><td>Firmware build and flash today</td></tr>
          <tr><td>ordinary App manifest, schema, Actions, resources or View</td><td>Package and install/update <code>.pocketapp</code></td></tr>
          <tr><td>ordinary App Data</td><td>Action transaction; no release delivery</td></tr>
        </tbody>
      </table>

      <h2>Repository map</h2>
      <Code>{`apps/                       System App and ordinary App source
system/                     shared Framework, net SDK and View SDK
crates/pocket-pi-agentos/   AppSupervisor and App contracts
crates/pocket-pi-embedded/  embedded resident Harness bridge
crates/pocket-pi-tools/     native workspace/device/schedule Tools
crates/pocket-pi-protocols/ model/provider codecs
hosts/esp32-sim/            macOS product-contract simulator
firmware/esp32-common/      shared ESP-IDF AgentOS host mechanisms
firmware/esp32-p4/          reference physical host
firmware/esp32-s3/          Waveshare ESP32-S3-Touch-LCD-4.3 host
tools/                      build, provision and App ingress CLIs`}</Code>
      <p>Canonical implementation design: <SourceLink path="docs/agentos-architecture.md" />.</p>
    </>,
  },
  {
    slug: "harnesses",
    title: "Harness boundary",
    description: "Separate the resident Agent Harness from the device runtime and distinguish the current Pi implementation from the planned DeepSeek Harness adapter.",
    render: () => <>
      <h1>Harness boundary</h1>
      <DocLead>
        The Agent Harness owns the model/tool turn loop inside the resident System Guest. PocketPi
        owns the device experience around it. Making the Harness replaceable means keeping that
        experience and host contract stable while swapping the guest-side loop.
      </DocLead>
      <PageGoal>
        A precise current/target split, the stable contract a Harness must satisfy, and the parity bar
        for adding DeepSeek Harness without turning PocketPi into a second product.
      </PageGoal>

      <h2>Current status</h2>
      <p>
        <Status>Implemented</Status> The firmware-embedded resident Harness is
        <code>pi-agent-core</code>. It is built into <code>apps/pi-agent/dist/agent.js</code> and shares
        the Pi Agent System Guest with the Root View. There is no runtime Harness selector on
        <code>upstream/main</code> today.
      </p>
      <p>
        Choosing <code>--backend deepseek</code> selects a <em>model provider</em> for the current Pi
        Harness. It does not run DeepSeek Harness. Model backend and Agent Harness are separate axes.
      </p>

      <h2>Stable PocketPi contract</h2>
      <Code>{`Native host → resident guest
  host.startModel(request)
  host.startTool(callId, name, args)
  host.poll()

Resident guest → PocketPi
  boot(config)
  prompt(text)
  tick()
  drain()
  replaceAppContext(definitions, installedApps)`}</Code>
      <p>
        Model requests/results, Tool definitions/results and Agent events cross this narrow boundary.
        AppSupervisor, native Tool/App routing, provider codecs, workspace, schedules, Root View and
        SystemFacts should not need to know which Harness implements the turn loop.
      </p>

      <h2>Target: build-time Harness replacement</h2>
      <p>
        <Status>Research target</Status> The next step is to build either Pi Agent or DeepSeek Harness
        into the same resident System App. Adaptation belongs in guest JavaScript, its prelude and the
        build task, not in a fork of DeepSeek Harness and not in parallel Rust runtime logic.
      </p>
      <Code>{`PocketPi
└── resident System Guest
    ├── Pi adapter  → pi-agent-core
    └── DSH adapter → DeepSeek Harness core

Both expose the same PocketPiEmbedded contract
Both consume the same host model/tool contract
Both drive the same Root View and App ecosystem`}</Code>

      <h2>Parity comes before new features</h2>
      <p>A first DeepSeek Harness integration is accepted only when the observable product remains aligned:</p>
      <ul>
        <li>boot reaches ready and the Root View follows the same status transitions;</li>
        <li>prompt text streams incrementally while hidden reasoning stays out of chat text;</li>
        <li>busy prompts and mid-turn Tool replacement follow the current admission behavior;</li>
        <li>native and App Tools remain sequential and App install/uninstall updates the next turn&apos;s catalog;</li>
        <li>provider errors surface through the same fault state;</li>
        <li>workspace, schedules, UI and native credentials remain owned by PocketPi;</li>
        <li>reboot conversation behavior remains identical until persistence is deliberately added;</li>
        <li>heap and firmware-size growth are measured on QuickJS-ng and physical firmware.</li>
      </ul>

      <h2>Explicitly deferred from first parity</h2>
      <ul>
        <li>third-party DeepSeek Harness plugin installation or on-device profile scanning;</li>
        <li>DeepSeek Harness web/client UI, because the PocketJS Root View remains the product UI;</li>
        <li>subagents, skills, plan/todo, compaction and session persistence plugins;</li>
        <li>mid-turn steering semantics and a separate schedule plugin;</li>
        <li>multiple in-process Agents before async-context propagation is proven.</li>
      </ul>

      <h2>Why this boundary matters</h2>
      <p>
        PocketPi should support more than one Agent loop without becoming a generic collection
        of harness ports. The device runtime remains the product: local ownership, native capabilities,
        App lifecycle and fixed Views. Harness-specific ecosystem ports can later become separate
        Pi Agent and DeepSeek Harness compatibility layers when there are additional hardware/platform
        reasons to distribute them independently.
      </p>
      <Fact>
        Replaceable Harness is a target architecture backed by a QuickJS feasibility handoff. It is
        not an implemented feature of the current public branch and must not be described as shipped.
      </Fact>
    </>,
  },
];
