import type { DocRecord } from "./doc-components";
import { Code, DocLead, Fact, PageGoal } from "./doc-components";

export const SECURITY_DOCS: DocRecord[] = [
  {
    slug: "security",
    title: "Trust and capabilities",
    description: "The security model for Apps, native mechanisms, package review, credentials and provider policy.",
    render: () => <>
      <h1>Trust and capabilities</h1>
      <DocLead>
        PocketPi does not trust App source to define its own authority. Apps declare intent;
        native code validates the package, mounts scoped capabilities, retains credentials and enforces
        exact transport and lifecycle boundaries.
      </DocLead>
      <PageGoal>
        A reviewable trust model for deciding what belongs in App source, what must stay native, what
        a package asks for and what human confirmation does or does not authorize.
      </PageGoal>

      <h2>Trust zones</h2>
      <table>
        <thead><tr><th>Zone</th><th>Trusted for</th><th>Not trusted for</th></tr></thead>
        <tbody>
          <tr><td>Native host/runtime</td><td>capability enforcement, credential storage, storage roots, deadlines, package lifecycle</td><td>App domain meaning or provider response mapping</td></tr>
          <tr><td>Resident System App</td><td>top-level workspace UI, narrow system commands, Agent interaction</td><td>raw native secrets or arbitrary host calls</td></tr>
          <tr><td>Ordinary App source</td><td>declared domain behavior and fixed View</td><td>other Apps, top-level workspace, undeclared endpoints/operations</td></tr>
          <tr><td>Agent/model output</td><td>intent, reasoning and selection among advertised Tools</td><td>credentials, direct database writes or bypassing confirmation</td></tr>
          <tr><td>Package transport</td><td>moving one complete candidate artifact</td><td>activating it or writing live App/runtime state</td></tr>
        </tbody>
      </table>

      <h2>Capability declaration</h2>
      <Code>{`"capabilities": ["data.sqlite", "net.http"]`}</Code>
      <p>The current manifest accepts:</p>
      <ul>
        <li><code>data.sqlite</code> for the App-owned database contract;</li>
        <li><code>data.fs</code> as a recognized App-local filesystem capability;</li>
        <li><code>net.http</code> for the bounded Action <code>fetch()</code> surface.</li>
      </ul>
      <p>
        Duplicate or unknown capabilities reject the App. A declaration does not override native
        enforcement: the host still decides which module/service surface is mounted and which App id owns it.
      </p>

      <h2>Network policy is more specific than a capability</h2>
      <p>
        <code>net.http</code> allows the HTTP mechanism, while <code>nativeServices.http</code> declares
        exact methods, URLs, request headers and optional credential binding. MCP connections declare
        exact connection URLs, credential binding and a separate operation allowlist. App JavaScript
        cannot turn those into an unrestricted proxy.
      </p>

      <h2>Credential lifecycle</h2>
      <ol>
        <li>The manifest declares stable credential ids and how native transport consumes them.</li>
        <li>A first-install <code>credentials.json</code> carries exactly those values.</li>
        <li>The Installer strips the file before App activation and writes values to native storage.</li>
        <li>At request time, native code applies the value only to an allowlisted operation.</li>
        <li>Update packages omit credentials and preserve installed native values.</li>
        <li>Uninstall removes credentials owned by that App.</li>
      </ol>
      <Fact>
        A raw credential never needs to enter App source, App SQLite, the Agent workspace, Tool
        arguments, the fixed View or model context.
      </Fact>

      <h2>Human review</h2>
      <p>
        Both HTTP and UART ingress stop at one review screen. Review makes the candidate identity,
        version, Tools, schedules and network/credential needs visible before lifecycle mutation.
        It is an installation boundary, not blanket approval for every future real-world action an
        App Tool might perform.
      </p>

      <h2>Product-level safety still belongs to the App</h2>
      <p>
        An allowlisted provider operation is necessary but not sufficient for high-impact behavior.
        The App must describe side effects accurately, validate arguments, use idempotency identifiers
        where offered, avoid unsafe retries after ambiguous responses and introduce explicit product
        confirmation where the domain requires it.
      </p>
    </>,
  },
  {
    slug: "data-isolation",
    title: "Data isolation",
    description: "The ownership matrix for Agent workspace, App SQLite/files, resources, credentials, Guests and native state.",
    render: () => <>
      <h1>Data isolation</h1>
      <DocLead>
        Every durable or sensitive resource has one owner. Isolation is expressed through native roots,
        one SQLite owner, Guest-specific mounts and lifecycle deletion, not through a naming convention
        that App code is expected to honor voluntarily.
      </DocLead>
      <PageGoal>
        An exact answer to who may read or write each state class, how View/Action access differs and
        what crosses between Pi Agent, ordinary Apps and native services.
      </PageGoal>

      <h2>Ownership matrix</h2>
      <table>
        <thead><tr><th>Resource</th><th>Owner</th><th>Read</th><th>Write</th></tr></thead>
        <tbody>
          <tr><td>Top-level <code>/workspace</code></td><td>Pi Agent</td><td>resident workspace Tools/System mechanisms</td><td>resident workspace Tools/System mechanisms</td></tr>
          <tr><td>Ordinary App SQLite</td><td>that App</td><td>Action + read-only View Projection</td><td>Action transaction only</td></tr>
          <tr><td>Ordinary App files/data root</td><td>that App</td><td>only mounts scoped to that App</td><td>only mounted App-local mechanisms</td></tr>
          <tr><td>Packaged JSON resources</td><td>App release</td><td>frozen value in that App&apos;s Guests</td><td>never at runtime</td></tr>
          <tr><td>Credential values</td><td>native host on behalf of App</td><td>native request adapter only</td><td>Installer/provisioning lifecycle</td></tr>
          <tr><td>Wi-Fi/model configuration</td><td>native host</td><td>bounded System facts</td><td>native Settings/provisioning commands</td></tr>
          <tr><td>JavaScript heap</td><td>one Guest</td><td>that Guest</td><td>that Guest</td></tr>
        </tbody>
      </table>

      <h2>One SQLite owner</h2>
      <p>
        View and Action Guests do not open competing embedded database connections. A native
        <code>DbModule</code> owner serializes operations for the App&apos;s SQLite file. The View mount
        enables <code>PRAGMA query_only</code> around reads; write operations are not exposed there.
      </p>

      <h2>View isolation</h2>
      <p>
        A View receives bounded query results, declared JSON resources, its own presentation heap and
        pointer input. It does not receive provider responses, raw credentials or the Action call stack.
        Returning <code>PocketPi.action()</code> asks native routing to perform a mutation elsewhere.
      </p>

      <h2>Agent isolation</h2>
      <p>
        The Agent sees public Tool schemas and Tool results. It does not see App table files or native
        credentials. Cross-App coordination happens through public capabilities, not by joining private
        databases or walking another App&apos;s data root.
      </p>

      <h2>Guest isolation</h2>
      <p>
        QuickJS globals, objects, promises and job queues never cross Guest boundaries. Shared Framework
        and App source are evaluated separately in each Guest. Data that must coordinate those isolated
        instances crosses a native contract or durable App state.
      </p>

      <h2>Isolation after lifecycle changes</h2>
      <ul>
        <li>View/Action Guest eviction removes only transient heap and retained nodes.</li>
        <li>Update replaces source and cached Guests while preserving compatible App Data.</li>
        <li>Uninstall removes source, private data root, schedule state, Tool routes, credentials and cached Guests.</li>
        <li>Pi Agent&apos;s System lifecycle and workspace are unaffected by ordinary App uninstall.</li>
      </ul>
    </>,
  },
  {
    slug: "lifecycle-recovery",
    title: "Lifecycle and recovery",
    description: "How staging, human review, candidate rehearsal, power-loss recovery and destructive uninstall protect live state.",
    render: () => <>
      <h1>Lifecycle and recovery</h1>
      <DocLead>
        App lifecycle is centralized in <code>AppSupervisor</code>. Ingress adapters cannot partially
        install an App, and candidate JavaScript cannot decide when it becomes live.
      </DocLead>
      <PageGoal>
        The state machine from untrusted bytes to active App, the failure boundary at each stage, what
        a power loss recovers and why recovery is not the same as rollback.
      </PageGoal>

      <h2>Fresh install state machine</h2>
      <Code>{`receive complete package
  → stage outside live App root
  → validate archive + manifest + source
  → reserve one review slot
  → human confirms
  → initialize candidate Data
  → evaluate Actions + View
  → store credentials natively
  → move one release into place
  → register Tools + schedules
  → active`}</Code>
      <p>
        Any failure before activation removes incomplete candidate state. HTTP/UART never writes the
        live App root, Tool catalog or credentials directly.
      </p>

      <h2>Update state machine</h2>
      <Code>{`receive candidate for existing id
  → reject credentials / permission changes / schema downgrade
  → review current vs candidate version/schema
  → human confirms
  → wait for quiescent App boundary
  → copy SQLite
  → rehearse migrations + Actions + View
  → write .update/release recovery signal
  → migrate live DB in one transaction
  → swap single source release
  → replace routes, schedules and cached Guests
  → remove temporary old source`}</Code>

      <h2>Power-loss recovery</h2>
      <p>
        The presence of <code>.update/release</code> means a person already approved a complete
        candidate and activation did not finish. On boot, the runtime re-runs any uncommitted migration
        and completes the source swap before loading the installed App index.
      </p>
      <p>
        SQLite transaction semantics keep a partially applied migration from becoming the durable
        shape. Candidate source is complete before the recovery signal exists.
      </p>

      <h2>Recovery is not rollback</h2>
      <ul>
        <li>There is one active release, not a retained version history.</li>
        <li>Recovery finishes an approved forward update; it does not choose an older version.</li>
        <li>Schema downgrade is rejected.</li>
        <li>An App cannot request its own update through an Agent Tool today.</li>
      </ul>

      <h2>Uninstall is the reverse ownership operation</h2>
      <p>
        After explicit destructive UI intent, the same supervisor removes every resource owned by the
        ordinary App: routes, schedules, cached runtimes, native credentials/session state, release
        source and full data root. A restart must not rediscover the App.
      </p>
      <Fact>
        Back up or export domain data before uninstall if the product needs retention. The current
        runtime intentionally does not keep orphaned App data or offer undelete.
      </Fact>
    </>,
  },
];
