import type { DocRecord } from "./doc-components";
import { Code, DocLead, Fact, PageGoal, SourceLink } from "./doc-components";

export const EXAMPLE_DOCS: DocRecord[] = [
  {
    slug: "exa-example",
    title: "Exa App walkthrough",
    description: "Trace a complete HTTP-backed research App from Tool schema through Action, local history and fixed View.",
    render: () => <>
      <h1>Exa App walkthrough</h1>
      <DocLead>
        Exa is the smaller real integration. It demonstrates two public research Tools, exact
        credential-safe HTTP policy, Action-owned provider mapping, bounded local search history and a
        fixed View that never receives provider transport.
      </DocLead>
      <PageGoal>
        A source-level example of where a network App puts Tools, provider calls, retention policy,
        SQLite writes, Projection pagination and simulator fixtures.
      </PageGoal>

      <h2>Product boundary</h2>
      <Code>{`Data      searches table: bounded local history consumed by the View
Actions   search + fetch
View      recent search status/results history

Public Tools
  research.search
  research.fetch`}</Code>

      <h2>Manifest and native policy</h2>
      <p>
        Exa declares <code>data.sqlite</code> and <code>net.http</code>. Its HTTP policy permits POST to
        exactly <code>https://api.exa.ai/search</code> and <code>/contents</code>, allows only accept and
        content-type request headers, and binds native credential id <code>exa.api-key</code> to
        <code>x-api-key</code>.
      </p>
      <p>
        The manifest&apos;s Tool schemas describe search mode, categories, domain/date filters, result
        bounds and content freshness. The Agent receives those product-facing choices; it never receives
        the credential or a generic HTTP Tool.
      </p>

      <h2>Search Action</h2>
      <ol>
        <li>Validate and normalize the query/options.</li>
        <li>POST a bounded JSON request with the remaining Action deadline.</li>
        <li>Return the full live Exa response to the Agent.</li>
        <li>Persist only query, time, status, result count, top title or error for the fixed history View.</li>
        <li>Delete history older than seven days inside the same transaction.</li>
      </ol>
      <p>
        This is an important App rule: SQLite is the fixed View/product projection, not a generic cache
        of every provider payload. Fetched document text is returned to the Agent but is not duplicated
        into local history because the current View does not consume it.
      </p>

      <h2>Failure is also useful product state</h2>
      <p>
        If the provider call fails, the Action commits a bounded error history row and then rethrows.
        The Agent receives an error Tool result while the fixed View can explain that the latest search
        failed. The App does not turn failure into an empty success.
      </p>

      <h2>View and pagination</h2>
      <p>
        The View initially projects eleven rows to show ten and detect <code>hasMore</code>. It increases
        a bounded limit in pages up to 50 total history rows, renders six at a time and uses a ScrollRail
        rather than materializing an unbounded list. A manual <code>binding.refresh()</code> supports
        pagination; App revisions handle new search commits.
      </p>

      <h2>Simulator behavior</h2>
      <p>
        The simulator&apos;s native App service returns deterministic Exa-shaped fixtures. The exact
        <code>actions.js</code>, SQLite transaction and <code>view.js</code> run unchanged, but no real Exa
        credential or network result is proven there.
      </p>

      <h2>What to copy into a new App</h2>
      <ul>
        <li>exact endpoint/header policy rather than a broad network proxy;</li>
        <li>one Action that returns live provider value and persists only View-owned product state;</li>
        <li>explicit request body and response-size bounds;</li>
        <li>retention cleanup in the same durable transaction;</li>
        <li>bounded Projection pagination and visible-row rendering.</li>
      </ul>
      <p>
        Read the source: <SourceLink path="apps/exa/app.json">app.json</SourceLink>,
        {" "}<SourceLink path="apps/exa/schema.sql">schema.sql</SourceLink>,
        {" "}<SourceLink path="apps/exa/actions.js">actions.js</SourceLink>,
        {" "}<SourceLink path="apps/exa/view.js">view.js</SourceLink>.
      </p>
    </>,
  },
  {
    slug: "robinhood-example",
    title: "Robinhood App walkthrough",
    description: "Study deferred provider Tool lookup, MCP policy, selective persistence, scheduled aggregation and safety boundaries.",
    render: () => <>
      <h1>Robinhood App walkthrough</h1>
      <DocLead>
        Robinhood demonstrates a larger App without making firmware own the product. Native code keeps
        OAuth/MCP transport and an exact operation allowlist; the App owns a checked-in Tool catalog,
        validation, mapping, selective SQLite persistence, schedule and fixed portfolio View.
      </DocLead>
      <PageGoal>
        A concrete pattern for large provider catalogs, deferred Tool discovery, batch aggregation,
        product-state selection and high-impact operation safety.
      </PageGoal>

      <h2>Why the Agent sees three Tools, not 54 schemas</h2>
      <table>
        <thead><tr><th>Public Tool</th><th>Responsibility</th></tr></thead>
        <tbody>
          <tr><td><code>robinhood.search_tools</code></td><td>Search the checked-in 54-Tool catalog locally and return exact provider schema/safety guidance</td></tr>
          <tr><td><code>robinhood.call</code></td><td>Validate one exact provider operation and invoke it through native MCP policy</td></tr>
          <tr><td><code>robinhood.refresh_portfolio</code></td><td>Run the App-owned aggregate that refreshes bounded dashboard Data</td></tr>
        </tbody>
      </table>
      <p>
        This is a cross-model deferred-loading pattern. It avoids putting every complete provider schema
        into every model request while keeping the catalog executable and reviewable inside the App.
        It is not a native provider-specific Tool Search wire protocol.
      </p>

      <h2>Two independent allowlists</h2>
      <ol>
        <li><code>assets/tool-catalog.json</code> contains the provider name, exact input schema and combined usage/safety description.</li>
        <li><code>app.json.providerOperations</code> contains the operations native MCP transport may invoke.</li>
      </ol>
      <p>
        A provider Tool must exist in both. The App validates the selected schema before crossing the
        native boundary; the host independently enforces the installed allowlist. Catalog and allowlist
        changes must be reviewed together.
      </p>

      <h2>Selective persistence</h2>
      <table>
        <thead><tr><th>Provider result</th><th>SQLite effect</th><th>Reason</th></tr></thead>
        <tbody>
          <tr><td>accounts</td><td>replace account rows</td><td>account selector and status</td></tr>
          <tr><td>portfolio</td><td>upsert current portfolio and value</td><td>dashboard and chart</td></tr>
          <tr><td>positions</td><td>replace per-account positions</td><td>positions View</td></tr>
          <tr><td>orders</td><td>replace per-account activity</td><td>activity View</td></tr>
          <tr><td>realized P&amp;L</td><td>upsert day/week values</td><td>dashboard metrics</td></tr>
          <tr><td>place/cancel result</td><td>upsert returned order state only</td><td>directly changes visible activity</td></tr>
          <tr><td>other provider Tools</td><td>none</td><td>current fixed View does not consume them</td></tr>
        </tbody>
      </table>
      <p>
        There is no generic raw-response log, quote cache or schema-less Tool cache. Live provider
        results still return to the Agent even when SQLite does not persist them.
      </p>

      <h2>Scheduled aggregate</h2>
      <p>
        A five-minute App schedule routes to <code>refreshPortfolio</code>. It loads accounts, batches
        required per-account calls, normalizes bounded dashboard tables and writes one
        <code>refresh_runs</code> record in a transaction. One successful transaction publishes one
        revision. The View may remain closed during the whole operation.
      </p>

      <h2>View ownership</h2>
      <p>
        The fixed View projects accounts, portfolio, totals, positions, activity and chart points from
        local SQLite. It owns account selection, screen/span choice and bounded scrolling as presentation
        state. It never owns OAuth, MCP sessions or raw provider responses.
      </p>

      <h2>Real-account safety</h2>
      <ul>
        <li>Tool descriptions distinguish read/review operations from real account or real-money effects.</li>
        <li>The App validates provider arguments locally before native transport.</li>
        <li>A real-money retry must reuse the same provider <code>ref_id</code>.</li>
        <li>An ambiguous transport result must not be retried with a new id.</li>
        <li>Native allowlisting is not a substitute for product-level confirmation and risk controls.</li>
      </ul>
      <Fact>
        The repository App exposes real-action provider operations, so development and demonstrations
        must explicitly choose read-only scenarios unless the user has authorized account-changing work.
      </Fact>
      <p>
        Read the source: <SourceLink path="apps/robinhood/app.json">app.json</SourceLink>,
        {" "}<SourceLink path="apps/robinhood/schema.sql">schema.sql</SourceLink>,
        {" "}<SourceLink path="apps/robinhood/actions.js">actions.js</SourceLink>,
        {" "}<SourceLink path="apps/robinhood/view.js">view.js</SourceLink>,
        {" "}<SourceLink path="docs/robinhood-tools.md">Tool contract</SourceLink>.
      </p>
    </>,
  },
];
