# Pocket Pi — architecture & the scheduling verdict

## The layering

Three files carry the design:

| Layer | File | Role |
|---|---|---|
| Guest bundle | `crates/pocket-pi/js/agent.bundle.js` | pi's `Agent` + loop + our streamFns, one IIFE |
| Prelude | `crates/pocket-pi/js/prelude.js` | the Web/Node globals QuickJS lacks (timers, `AbortController`, `structuredClone`, `crypto.randomUUID`, a `process` stub) |
| Runtime | `crates/pocket-pi/src/{lib,http}.rs` | QuickJS embed, the `host` native namespace, the HTTP mailbox, the frame pump |

The guest realm is **capability-free**: no `fs`, no `child_process`, no network
except `host.http`. Everything the agent can affect, it affects through a named
native op. This is deliberately the PocketJS `⟨Cores, Surfaces, Guest⟩` posture —
the guest expresses intent; native code decides what that intent can reach.

## Why pi's core fits at all

The recon that preceded this runtime established the load-bearing fact: pi's
agent **loop** (`@mariozechner/pi-agent-core`) is ~43 KB of pure JS that imports
only `{ EventStream, streamSimple, validateToolArguments }` from `pi-ai` and
touches almost no globals (`Promise`, `Set`, `Map`, `Date.now`, async generators
— all in QuickJS). Everything heavy in pi is the **CLI/TUI** (`pi-tui`, `chalk`,
`node:child_process`) and the **provider layer** (`@anthropic-ai/sdk`, a 553 KB
generated model table, TypeBox). Pocket Pi keeps the loop and cuts the rest:

- pi-ai is aliased at bundle time to `js/src/pi-ai-stub.js`, four symbols
  (`EventStream`, `AssistantMessageEventStream`, a throwing `streamSimple`, a
  permissive `validateToolArguments`, plus `parseStreamingJson`).
- The provider is our own `streamFn` (`js/src/anthropic-stream.js`) — when the
  Agent is constructed with a `streamFn`, it never reaches into pi-ai's provider
  registry, so none of the SDKs are needed.

## The one native op that matters: streaming HTTPS

QuickJS can't do TLS or block, and pi's loop is `await streamFn(...)` then
`for await (const event of response)`. Pocket Pi resolves this with a **mailbox**:

1. `host.http.start(requestJson)` spawns an OS thread (`src/http.rs`) that does
   the `ureq` POST and reads the Anthropic SSE body line by line, pushing each
   decoded `data:` JSON into a per-turn `VecDeque`.
2. Each frame, `globalThis.__catpiPump()` (in `anthropic-stream.js`) calls
   `host.http.drain(turnId)`, maps the Anthropic deltas onto pi's running
   `partial` message exactly as pi-ai's own provider does — `text_delta`,
   `input_json_delta` (streamed tool arguments), `thinking_delta` — and pushes
   pi `AssistantMessageEvent`s into the `AssistantMessageEventStream` the loop is
   iterating.
3. The QuickJS job queue drains, so the `await` resumes and the loop advances.

The JS side never sees a byte or a socket. SSE parsing lives in Rust; pi sees
only complete, typed events. That is why no `fetch`, `ReadableStream`, or
`TextDecoder` shim is required.

## The frame `pump()`

```
pump():
  __catpiTimers()      # fire due setTimeout callbacks (retry backoff, etc.)
  __catpiPump()        # drain HTTP mailboxes → push LLM events into pi streams
  drain_jobs()         # run QuickJS microtasks until the queue is empty →
                       #   the agent loop advances, tool `execute()` runs,
                       #   subscribe() fires, host.emit() buffers events
  flush_events()       # deliver buffered events to the host callback
```

One `pump()` is one frame. Nothing here spins: if no chunk arrived and no timer
is due, `__catpiPump` returns immediately and the job queue is empty. The host
chooses the cadence.

## The scheduling verdict — is a low-Hz pump worth it?

**Yes, decisively, for a headless/idle agent; with a caveat for interactive typing.**

The question the `cat` brief raised: is it worth borrowing PocketJS's frame
scheduler for an agent, e.g. coalescing to 2 fps when headless? The measured
shape of an agent turn answers it.

- **Where the wall-clock goes.** In a real turn, ~99% of elapsed time is the
  model streaming over the network. The runtime's own work per turn — building
  the request, mapping a few hundred SSE deltas, draining the job queue — is a
  few milliseconds total. The pump does nothing but check two mailboxes the vast
  majority of frames.
- **What a slow pump costs.** The only thing a lower pump rate adds is *delivery
  latency of already-arrived bytes*: a token that lands in the mailbox mid-frame
  waits at most one frame period to reach the UI. At 2 Hz that's ≤500 ms; against
  multi-second model latency it is imperceptible, and for a headless agent (no
  human watching tokens stream) it is free. The runtime's included tests drive a
  full multi-turn tool loop to completion at **2 Hz** and a live turn at **4 Hz**
  with no loss of correctness — the mailbox buffers everything between frames.
- **What a slow pump saves.** A hot `while(true)` poll loop burns a core doing
  nothing for the seconds-long gaps between token bursts. Coalescing to a
  heartbeat drops that to near-zero: the thread sleeps between frames, and macOS
  App-Nap / occlusion can suspend it entirely. For an always-on desktop widget
  like `cat` — the intended host — this is the difference between a background
  process you forget about and a space heater.

**The caveat and the resolution.** One frame of latency is only invisible while a
human isn't in a tight loop with the runtime. Two cases want a faster pump:

1. **Live token streaming to a visible UI** — 2 Hz makes streamed text arrive in
   250–500 ms chunks, which reads as stutter. Pump at 15–30 Hz *only while a turn
   is streaming and a human is watching*.
2. **Interactive text input** in the same realm (not pi's concern, but the host's).

So the right policy is **adaptive**, and Pocket Pi leaves the cadence entirely to
the host so it can implement exactly that:

| Host state | Pump rate | Rationale |
|---|---|---|
| Idle (no turn in flight) | event-driven / ~1 Hz keepalive | nothing to do; wake on user input |
| Turn streaming, headless | 2 Hz | LLM latency ≫ frame; delivery lag invisible |
| Turn streaming, visible UI | 15–30 Hz | smooth token rendering for a watching human |

`cat` uses exactly this: its widget already runs PocketJS's demand-render
governor, so it pumps Pocket Pi from the same frame it renders the cat — fast
while the cat is "thinking" on screen, coalesced to a trickle when the desktop is
quiet. The agent runtime and the UI runtime share one clock. That symmetry —
*two runtimes, one heartbeat* — is the whole reason to put pi on a PocketJS-style
scheduler rather than its native event loop.

## What is intentionally missing

This is a PoC substrate. Not implemented: multiple providers, pi's session
persistence / compaction / settings, real TypeBox tool-arg coercion, and OAuth
device login. The seams for all of them exist (the `streamFn` is swappable, tools
are native, config is JSON) — they're scope, not blockers.
