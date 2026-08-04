# Agent tool architecture

The SideChat tree contains the right use cases, but it mixes four different
kinds of authority under one `Pi Agent` node: authentication, read-only data,
model reasoning, and irreversible broker actions. The embedded design keeps all
of those capabilities while making the authority transitions explicit.

## Capability graph

```text
Credential providers (not model tools)
├── Codex ChatGPT device login [experimental adapter]
├── OpenAI Platform API key [supported fallback]
└── Robinhood OAuth/PKCE

Read-only evidence tools
├── research.search           Exa discovery; never treated as primary evidence
├── research.fetch            bounded HTTPS fetch with URL/size/type policy
├── sec.company_facts         SEC filing/XBRL facts
├── sec.filing                filing metadata and selected sections
├── news.query                timestamped news discovery
├── issuer.ir                 issuer-controlled IR releases and filings
├── portfolio.accounts        Robinhood account projection
├── portfolio.snapshot        positions, cash, equity and freshness metadata
└── portfolio.orders          open/recent orders for reconciliation

Reasoning outputs (data, not side effects)
├── thesis.compose            claims linked to an evidence set
└── order.propose             bounded OrderIntent with expiry and rationale

Host-only deterministic controls
├── risk.evaluate             pure policy decision over intent + fresh state
├── approval.confirm          physical/user confirmation ticket
├── execution.submit          privileged, idempotent broker write
└── execution.reconcile       broker truth -> durable order state

Display/event projection
└── status, freshness, positions, proposals, approvals and broker receipts
```

Authentication is infrastructure, not a tool the model may invoke. Research
and portfolio operations are model-callable only when read-only. `risk.evaluate`
is deterministic host code. `execution.submit` is not registered in the model's
tool list; it accepts only a valid approval ticket produced by the host.

## Evidence contract

Every research result is normalized into an `EvidenceRef` with source class,
source identifier hash, publication time, observation time and freshness. Exa
is a discovery source, SEC filings/XBRL are regulatory primary sources, issuer
IR is first-party but promotional, and news is secondary. A URL or search
snippet alone cannot authorize an order.

A thesis references an immutable evidence-set hash. An `OrderIntent` references
that thesis/evidence set and expires quickly. The risk gate rejects intents when
portfolio or price data is stale, their evidence set has changed, the account is
not the selected Agentic account, or any device limit is exceeded.

## Execution state machine

```text
Draft
  -> Proposed
  -> RiskRejected
  -> AwaitingConfirmation
  -> Approved(ticket hash + expiry)
  -> Submitting(idempotency key)
  -> BrokerAccepted(broker order id)
  -> Reconciled(Filled | PartiallyFilled | Cancelled | Rejected | Unknown)
```

There is no direct `Proposed -> Submitting` transition. A reboot never retries a
write merely because the previous response was lost: it first reconciles the
idempotency key/order ledger against broker truth. `Unknown` freezes further
automated writes until reconciliation succeeds.

## Deployment stages

1. Register only research and Robinhood read-only tools.
2. Allow the model to emit and display `OrderIntent`; do not compile a submit
   surface into the model tool registry.
3. Add deterministic risk evaluation and paper execution.
4. Add physical confirmation with short-lived approval tickets.
5. Enable live submission only after encrypted credentials, secure boot/flash
   policy, crash recovery, stale-data tests, idempotency and a kill switch pass.

The live Robinhood MCP tool names and schemas must be discovered with
`tools/list` after OAuth. Conceptual operations such as positions or orders are
not assumed to exist until the server advertises them.
