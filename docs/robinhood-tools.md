# Robinhood Tool contract

Pocket Pi snapshots the upstream Robinhood Trading MCP catalog, but does not put
all upstream schemas in every model request. The checked-in snapshot currently
contains 54 upstream Tools. The initial Agent catalog contains three small Pocket
Pi Tools:

- `robinhood.search_tools` returns matching upstream descriptions and exact JSON
  Schemas from the local snapshot. It performs no provider request.
- `robinhood.call` validates one upstream call against that schema and routes it
  through the native static allowlist.
- `robinhood.refresh_portfolio` refreshes the bounded fixed-View projection.

This is Pocket Pi's cross-model fallback for deferred Tool loading. It follows
the same load-on-demand design as OpenAI client-executed Tool Search, but is not
the Responses API's native `tool_search` wire protocol. Native Tool Search is
currently limited to supported OpenAI models, while Pocket Pi also supports
other model backends. If a backend gains native deferred loading, it can expose
the same snapshot as dynamically loaded normal function definitions without
changing the Robinhood Action source.

References:

- Robinhood Agentic Trading: <https://robinhood.com/us/en/support/articles/trading-with-your-agent/>
- Robinhood Agentic Trading overview: <https://robinhood.com/us/en/support/articles/agentic-trading-overview/>
- OpenAI Tool Search: <https://developers.openai.com/api/docs/guides/tools-tool-search>
- GitHub Copilot CLI Tool Search: <https://docs.github.com/en/copilot/concepts/agents/copilot-cli/tool-search>

## Catalog and schema source

`apps/robinhood/assets/tool-catalog.json` is the Robinhood App-owned runtime
catalog captured from an
authenticated MCP `initialize` and `tools/list` exchange with
`https://agent.robinhood.com/mcp/trading`. It stores the upstream name,
input schema, and one combined description containing upstream usage guidance
plus Pocket Pi safety and persistence behavior. Credentials are never written to the catalog, App
release, SQLite, or Agent context. They may be transported inside the temporary
`.pocketapp`; Installer strips them and stores them in native NVS before the App
is activated. Updating this catalog is an explicit App
maintenance change and must be reviewed together with the native allowlist.

This file and the catalog have different roles:

- `assets/tool-catalog.json` is executable source data declared by `app.json`.
  The runtime exposes its frozen JSON value through
  `PocketPi.resources.get("toolCatalog")`; `searchTools()` and
  `validatedProviderCall()` use it directly. Native enforcement separately uses
  `app.json.providerOperations`, so catalog and allowlist changes are reviewed
  together.
- `docs/robinhood-tools.md` is human-facing maintenance documentation. It is not imported by
  the build or read by the device runtime.

This catalog is not a global AgentOS requirement. Robinhood uses deferred
lookup because exposing 54 complete schemas in every model request would be
wasteful. Small Apps such as Exa declare their few Tools directly and need no
private catalog. Any future large App owns its own catalog under that App.

The 54 Tools are split into eight searchable domains. Each domain has at most
nine Tools: `account_portfolio`, `equity_trading`, `equity_market_data`,
`option_trading`, `option_market_data`, `watchlists`, `scanners`, and `indexes`.
Exact-name lookup is preferred when the Agent already knows the upstream name.

The current upstream schemas use `type`, `properties`, `required`,
`additionalProperties`, `items`, `minimum`, and `maximum`. Pocket Pi validates
all of those keywords before crossing the native service boundary. The native
allowlist is built from `app.json.providerOperations`, so a Tool must be
present in both the snapshot and the installed App descriptor.

## Minimal persistence policy

SQLite is a fixed-View projection, not a generic Tool cache or audit log. A
provider result commits only when the current Robinhood View consumes it.

| Upstream operation | SQLite effect | Reason |
| --- | --- | --- |
| `get_accounts` | Replace `accounts` | Account selector and Agentic-account badge |
| `get_portfolio` | Upsert `portfolio_current` and `total_value` | Dashboard values and chart |
| `get_equity_positions` | Replace that account's `positions` | Positions View |
| `get_equity_orders` | Replace that account's `activities` | Activity View |
| `get_realized_pnl` | Upsert day or week P&L in `portfolio_current` | Dashboard P&L |
| `place_equity_order`, `cancel_equity_order` | Upsert only the returned order state in `activities` | The Tool result directly changes Activity; no second provider call is needed |
| Other 47 upstream Tools | None | Current fixed View does not consume the result |

`refresh_portfolio` is the only aggregate refresh. It obtains accounts first,
batches the required per-account portfolio/position/order/P&L calls, and writes
their bounded projections plus one `refresh_runs` record in a transaction. One
successful transaction emits one App revision; the foreground View then
re-queries its bounded projection at the frame boundary. Direct-return Tools do
not call `app.commit()` and therefore cause no SQLite write or View invalidation.

Equity place/cancel deliberately does not start a nested MCP refresh after the
provider action. It uses the returned order payload to update `activities` in
one short transaction. This keeps the App Action within its bounded stack
and avoids extra network, CPU, SQLite, and revision work. Portfolio and
positions converge on the next normal scheduled or explicit refresh.

No `tool_runs`, raw-response, quote cache, fundamentals cache, options cache,
watchlist cache, or scanner cache is created while the fixed View does not need
it.

## Agent execution and safety

An Agent first searches for the operation, reads the returned full description
and schema, then calls `robinhood.call` with the exact upstream name and a
schema-valid `arguments` object. The App completion path returns the actual
provider result to the pending Agent Tool call; a queued receipt is never
mistaken for completion. The provider JSON is serialized once as ToolResult
`text`; it is not duplicated into `details`, so large market-data and options
responses do not retain a second copy in the QuickJS/Agent bridge.

Review Tools are explicitly described as non-submitting. Place, cancel, and
exercise Tools describe their real account effects, functional account limits,
and parameter contracts without embedding an interaction or authorization
policy. A repeated real-money order must reuse the same `ref_id`; an ambiguous
transport result must not be retried with a new ID.
