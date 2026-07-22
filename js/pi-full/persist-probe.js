// Session persistence (M7): write a session to disk with pi's unmodified
// SessionManager (backed by Pocket Pi's fs builtin), then resume it in a fresh
// manager over the same directory and confirm the history round-trips. Offline —
// no LLM, just the disk store. Results land on globalThis for the Rust side.

globalThis.__piPersistResult = null;
globalThis.__piPersistError = null;

globalThis.__piPersist = function (sessionDir) {
  try {
    const P = globalThis.PiFull;
    if (!P) throw new Error("PiFull not loaded — run the bundle first");
    const CWD = "/pocket-pi";

    // Write a couple of messages through a persistent SessionManager.
    const sm1 = P.SessionManager.create(CWD, sessionDir);
    sm1.appendMessage({ role: "user", content: [{ type: "text", text: "remember the number 42" }] });
    sm1.appendMessage({ role: "assistant", content: [{ type: "text", text: "noted: 42" }] });
    const wrote = sm1.buildSessionContext().messages.length;
    const sessionId = sm1.getSessionId();

    // Resume in a fresh manager over the same dir (exercises the header peek +
    // full-file read path through our fs).
    const sm2 = P.SessionManager.continueRecent(CWD, sessionDir);
    const ctx = sm2.buildSessionContext();
    const texts = ctx.messages.map((m) =>
      Array.isArray(m.content) ? m.content.map((c) => (c && c.text) || "").join("") : String(m.content),
    );

    globalThis.__piPersistResult = {
      sessionId,
      wrote,
      resumedCount: ctx.messages.length,
      resumedId: sm2.getSessionId(),
      texts,
    };
  } catch (e) {
    globalThis.__piPersistError = String((e && e.stack) || e);
  }
};
