globalThis.__piPersistResult = null;
globalThis.__piPersistError = null;
globalThis.__piPersist = function(sessionDir) {
  try {
    const P = globalThis.PiFull;
    if (!P) throw new Error("PiFull not loaded \u2014 run the bundle first");
    const CWD = "/pocket-pi";
    const sm1 = P.SessionManager.create(CWD, sessionDir);
    sm1.appendMessage({ role: "user", content: [{ type: "text", text: "remember the number 42" }] });
    sm1.appendMessage({ role: "assistant", content: [{ type: "text", text: "noted: 42" }] });
    const wrote = sm1.buildSessionContext().messages.length;
    const sessionId = sm1.getSessionId();
    const sm2 = P.SessionManager.continueRecent(CWD, sessionDir);
    const ctx = sm2.buildSessionContext();
    const texts = ctx.messages.map(
      (m) => Array.isArray(m.content) ? m.content.map((c) => c && c.text || "").join("") : String(m.content)
    );
    globalThis.__piPersistResult = {
      sessionId,
      wrote,
      resumedCount: ctx.messages.length,
      resumedId: sm2.getSessionId(),
      texts
    };
  } catch (e) {
    globalThis.__piPersistError = String(e && e.stack || e);
  }
};
