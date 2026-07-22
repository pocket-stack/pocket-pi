// Shared streaming core for Pocket Pi providers.
//
// Every provider (Anthropic, OpenAI, …) builds an Anthropic/OpenAI request,
// hands it to the native HTTP op, and maps that provider's SSE deltas onto pi's
// running `partial` message. The turn bookkeeping and the once-per-frame pump
// are identical across providers — only the per-event mapping differs — so they
// live here, and each provider supplies just an `apply(turn, event)` function.

const activeTurns = new Map();

export function emptyUsage() {
  return {
    input: 0,
    output: 0,
    cacheRead: 0,
    cacheWrite: 0,
    totalTokens: 0,
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
  };
}

export function safeParse(text) {
  try {
    return JSON.parse(text);
  } catch {
    return {};
  }
}

/// Start a turn: POST via the native mailbox and register how to map its events.
/// `apply(turn, event)` returns true when the turn is finished.
export function beginTurn(stream, partial, request, apply, signal) {
  let turnId;
  try {
    turnId = host.http.start(JSON.stringify(request));
  } catch (e) {
    partial.stopReason = "error";
    partial.errorMessage = String(e && e.message ? e.message : e);
    stream.push({ type: "error", reason: "error", error: partial });
    stream.end();
    return;
  }

  const turn = { stream, partial, apply, done: false };
  activeTurns.set(turnId, turn);

  const abort = () => {
    try {
      host.http.cancel(turnId);
    } catch {}
    if (!turn.done) {
      partial.stopReason = "aborted";
      stream.push({ type: "error", reason: "aborted", error: partial });
      stream.end();
    }
    activeTurns.delete(turnId);
  };
  if (signal) {
    if (signal.aborted) abort();
    else signal.addEventListener("abort", () => activeTurns.has(turnId) && abort());
  }
}

// Called once per host frame: drain every active turn's HTTP mailbox and feed
// its decoded events through the provider's mapper.
globalThis.__catpiPump = function () {
  if (activeTurns.size === 0) return;
  for (const [turnId, turn] of activeTurns) {
    let out;
    try {
      out = JSON.parse(host.http.drain(turnId));
    } catch (e) {
      fail(turn, String(e));
      activeTurns.delete(turnId);
      continue;
    }
    let finished = false;
    for (const line of out.lines) {
      const ev = safeParseOrNull(line);
      if (ev === null) continue;
      if (turn.apply(turn, ev)) {
        finished = true;
        break;
      }
    }
    if (out.error && !finished) {
      fail(turn, out.error);
      finished = true;
    } else if (out.done && !finished && !turn.stream.done) {
      fail(turn, "stream ended without a terminal event");
      finished = true;
    }
    if (finished) activeTurns.delete(turnId);
  }
};

function fail(turn, message) {
  if (turn.done) return;
  turn.done = true;
  turn.partial.stopReason = "error";
  turn.partial.errorMessage = message;
  turn.stream.push({ type: "error", reason: "error", error: turn.partial });
  turn.stream.end();
}

function safeParseOrNull(line) {
  try {
    return JSON.parse(line);
  } catch {
    return null;
  }
}
