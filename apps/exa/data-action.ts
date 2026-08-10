// Headless Exa data plane. Search and document responses are normalized into
// App-owned domain rows before one transaction invalidates the View cache.
const nativeDb = (globalThis as any).db;
const handle = nativeDb.open("exa");
if (handle < 0) throw new Error("open exa.sqlite");

const SCHEMA_VERSION = 3;

function dbError(): string { return String(nativeDb.lastError(handle) || "SQLite operation failed"); }
function exec(sql: string): void { if (nativeDb.exec(handle, sql) !== 0) throw new Error(dbError()); }
function query(sql: string, args: any[] = []): any {
  const result = JSON.parse(nativeDb.query(handle, sql, JSON.stringify(args)));
  if (result.error) throw new Error(String(result.error));
  return result;
}
function run(sql: string, args: any[] = []): any { return query(sql, args); }

const version = query("PRAGMA user_version") as { user_version?: number } | null;
if (Number(version?.user_version ?? 0) !== SCHEMA_VERSION) {
  exec(`
    CREATE TABLE IF NOT EXISTS searches (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    query TEXT NOT NULL,
    searched_at INTEGER NOT NULL,
    status TEXT NOT NULL,
    result_count INTEGER NOT NULL DEFAULT 0,
    error TEXT
  );
  CREATE INDEX IF NOT EXISTS searches_recent ON searches(id DESC);
  CREATE TABLE IF NOT EXISTS search_results (
    search_id INTEGER NOT NULL,
    rank INTEGER NOT NULL,
    title TEXT,
    url TEXT NOT NULL,
    published_at TEXT,
    author TEXT,
    score TEXT,
    highlight TEXT,
    PRIMARY KEY(search_id, rank)
  );
  CREATE INDEX IF NOT EXISTS search_results_url ON search_results(url);
  CREATE TABLE IF NOT EXISTS documents (
    url TEXT PRIMARY KEY,
    fetched_at INTEGER NOT NULL,
    title TEXT,
    published_at TEXT,
    author TEXT,
    text TEXT NOT NULL
  );
  CREATE INDEX IF NOT EXISTS documents_recent ON documents(fetched_at DESC);
    PRAGMA user_version=3;
  `);
}

function now(): number { return Math.floor(Date.now() / 1000); }
function text(value: unknown): string | null {
  return value === null || value === undefined || typeof value === "object" ? null : String(value);
}
function firstText(value: any, names: string[]): string | null {
  if (!value || typeof value !== "object") return null;
  for (const name of names) {
    const found = text(value[name]);
    if (found !== null) return found;
  }
  return null;
}
function highlight(value: any): string | null {
  if (Array.isArray(value?.highlights)) return value.highlights.map(String).join("\n").slice(0, 1200);
  return firstText(value, ["highlight", "summary", "text"])?.slice(0, 1200) ?? null;
}

function post(path: string, body: any): any {
  const envelope = JSON.parse((globalThis as any).services.call(
    "net.http",
    "post",
    JSON.stringify({ connection: "exa", path, body }),
  ));
  if (!envelope.ok) throw new Error(envelope.error || "Exa service failed");
  return envelope.value;
}

function transaction(action: () => void): void {
  exec("BEGIN IMMEDIATE");
  try {
    action();
    exec("COMMIT");
  } catch (error) {
    try { exec("ROLLBACK"); } catch {}
    throw error;
  }
  (globalThis as any).app.commit();
}

function search(args: any): any {
  const searchQuery = String(args.query ?? "").trim();
  if (!searchQuery) throw new Error("query is required");
  const searchedAt = now();
  try {
    const body: any = {
      query: searchQuery,
      type: args.searchType ?? "auto",
      numResults: Math.max(1, Math.min(8, Number(args.numResults ?? 5))),
      contents: { highlights: { maxCharacters: 800 } },
    };
    for (const key of ["includeDomains", "excludeDomains", "startPublishedDate", "endPublishedDate"]) {
      if (args[key] !== undefined) body[key] = args[key];
    }
    const value = post("/search", body);
    const results = Array.isArray(value?.results) ? value.results.slice(0, 8) : [];
    transaction(() => {
      const inserted = run(
        "INSERT INTO searches(query,searched_at,status,result_count,error) VALUES(?,?,?,?,NULL)",
        [searchQuery, searchedAt, "ok", results.length],
      );
      const searchId = Number(inserted.lastInsertRowid || 0);
      results.forEach((item: any, rank: number) => {
        const url = firstText(item, ["url", "id"]);
        if (!url) return;
        run(
          `INSERT INTO search_results(search_id,rank,title,url,published_at,author,score,highlight)
           VALUES(?,?,?,?,?,?,?,?)`,
          [searchId, rank, firstText(item, ["title"]), url, firstText(item, ["publishedDate", "published_at", "date"]), firstText(item, ["author"]), firstText(item, ["score"]), highlight(item)],
        );
      });
    });
    return value;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    transaction(() => run(
      "INSERT INTO searches(query,searched_at,status,result_count,error) VALUES(?,?,?,0,?)",
      [searchQuery, searchedAt, "error", message],
    ));
    throw error;
  }
}

function fetchDocument(args: any): any {
  const requestedUrl = String(args.url ?? "").trim();
  if (!requestedUrl) throw new Error("url is required");
  const response = post("/contents", {
    urls: [requestedUrl],
    text: {
      maxCharacters: Math.max(200, Math.min(12000, Number(args.maxCharacters ?? 6000))),
      includeHtmlTags: false,
    },
  });
  const document = Array.isArray(response?.results) ? response.results[0] : response;
  if (!document) throw new Error("Exa returned no document");
  const url = firstText(document, ["url", "id"]) || requestedUrl;
  const body = firstText(document, ["text", "content", "summary"]) || "";
  transaction(() => run(
    `INSERT INTO documents(url,fetched_at,title,published_at,author,text) VALUES(?,?,?,?,?,?)
     ON CONFLICT(url) DO UPDATE SET fetched_at=excluded.fetched_at,title=excluded.title,
       published_at=excluded.published_at,author=excluded.author,text=excluded.text`,
    [url, now(), firstText(document, ["title"]), firstText(document, ["publishedDate", "published_at", "date"]), firstText(document, ["author"]), body],
  ));
  return {
    status: "ok",
    provider: "exa",
    requestId: response?.requestId,
    document,
    costDollars: response?.costDollars,
  };
}

(globalThis as any).PocketPiData = {
  invokeTask(name: string) {
    return JSON.stringify({ text: "Unknown Exa Data Action: " + name, isError: true });
  },
  invokeTool(name: string, argsLine: string) {
    try {
      const args = JSON.parse(argsLine);
      const value = name === "research.search" ? search(args)
        : name === "research.fetch" ? fetchDocument(args)
        : (() => { throw new Error("Unknown Exa tool: " + name); })();
      return JSON.stringify({ text: JSON.stringify(value), details: value, isError: false });
    } catch (error) {
      return JSON.stringify({
        text: error instanceof Error ? error.message : String(error),
        isError: true,
      });
    }
  },
};
