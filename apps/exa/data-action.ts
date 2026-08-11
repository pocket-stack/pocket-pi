// Headless Exa data plane. Only search history consumed by the fixed View is
// persisted; fetched documents are returned directly to the Agent.
import { __pumpNet, fetch } from "@pocketjs/framework/net";

const nativeDb = (globalThis as any).db;
const handle = nativeDb.open("exa");
if (handle < 0) throw new Error("open exa.sqlite");

const SCHEMA_VERSION = 5;
const RETENTION_DAYS = 7;
const RETENTION_SECONDS = RETENTION_DAYS * 24 * 60 * 60;
const HTTP_TIMEOUT_MS = 60_000;

function dbError(): string { return String(nativeDb.lastError(handle) || "SQLite operation failed"); }
function exec(sql: string): void { if (nativeDb.exec(handle, sql) !== 0) throw new Error(dbError()); }
function query(sql: string, args: any[] = []): any {
  const result = JSON.parse(nativeDb.query(handle, sql, JSON.stringify(args)));
  if (result.error) throw new Error(String(result.error));
  return result;
}
function run(sql: string, args: any[] = []): any { return query(sql, args); }

const version = Number(query("PRAGMA user_version")?.rows?.[0]?.[0] ?? 0);
if (version !== SCHEMA_VERSION) {
  exec(`
    DROP TABLE IF EXISTS searches;
    CREATE TABLE searches (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      query TEXT NOT NULL,
      searched_at INTEGER NOT NULL,
      status TEXT NOT NULL,
      result_count INTEGER NOT NULL DEFAULT 0,
      top_title TEXT,
      error TEXT
    );
    CREATE INDEX searches_retention ON searches(searched_at);
    PRAGMA user_version=${SCHEMA_VERSION};
  `);
}

function now(): number { return Math.floor(Date.now() / 1000); }
function cleanupExpired(referenceTime: number): void {
  const cutoff = referenceTime - RETENTION_SECONDS;
  run("DELETE FROM searches WHERE searched_at < ?", [cutoff]);
}

async function post(path: "/search" | "/contents", body: any): Promise<any> {
  const response = await fetch(`https://api.exa.ai${path}`, {
    method: "POST",
    headers: { accept: "application/json", "content-type": "application/json" },
    body: JSON.stringify(body),
    timeoutMs: HTTP_TIMEOUT_MS,
    maxBytes: 96 * 1024,
  });
  const value = await response.json<any>();
  if (!response.ok) throw new Error(`Exa HTTP ${response.status}: ${JSON.stringify(value)}`);
  return value;
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

async function search(args: any): Promise<any> {
  const searchQuery = String(args.query ?? "").trim();
  if (!searchQuery) throw new Error("query is required");
  const searchedAt = now();
  try {
    const body: any = {
      query: searchQuery,
      type: args.searchType ?? "auto",
      numResults: Math.max(1, Math.min(10, Number(args.numResults ?? 10))),
      contents: { highlights: { maxCharacters: 800 } },
    };
    for (const key of [
      "includeDomains", "excludeDomains", "startPublishedDate", "endPublishedDate",
      "category", "userLocation", "additionalQueries", "moderation",
    ]) {
      if (args[key] !== undefined) body[key] = args[key];
    }
    if (args.maxAgeHours !== undefined) body.contents.maxAgeHours = args.maxAgeHours;
    const value = await post("/search", body);
    const results = Array.isArray(value?.results) ? value.results : [];
    const topTitle = typeof results[0]?.title === "string" ? results[0].title : null;
    transaction(() => {
      run(
        "INSERT INTO searches(query,searched_at,status,result_count,top_title,error) VALUES(?,?,?,?,?,NULL)",
        [searchQuery, searchedAt, "ok", results.length, topTitle],
      );
      cleanupExpired(searchedAt);
    });
    return value;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    transaction(() => {
      run(
        "INSERT INTO searches(query,searched_at,status,result_count,top_title,error) VALUES(?,?,?,0,NULL,?)",
        [searchQuery, searchedAt, "error", message],
      );
      cleanupExpired(searchedAt);
    });
    throw error;
  }
}

async function fetchDocument(args: any): Promise<any> {
  const requestedUrl = String(args.url ?? "").trim();
  if (!requestedUrl) throw new Error("url is required");
  const request: any = {
    urls: [requestedUrl],
    text: {
      maxCharacters: Math.max(200, Math.min(12000, Number(args.maxCharacters ?? 6000))),
      includeHtmlTags: false,
    },
  };
  if (args.maxAgeHours !== undefined) request.maxAgeHours = args.maxAgeHours;
  return post("/contents", request);
}

let pendingResult: string | undefined;
let active = false;

function failure(error: unknown): string {
  return JSON.stringify({
    text: error instanceof Error ? error.message : String(error),
    isError: true,
  });
}

function begin(action: () => Promise<any>): void {
  if (active) throw new Error("Exa Data Action is already running");
  active = true;
  pendingResult = undefined;
  action().then(
    (value) => {
      pendingResult = JSON.stringify({ text: JSON.stringify(value), isError: false });
      active = false;
    },
    (error) => {
      pendingResult = failure(error);
      active = false;
    },
  );
}

(globalThis as any).PocketPiData = {
  beginInvokeTask(name: string) {
    begin(async () => { throw new Error("Unknown Exa Data Action: " + name); });
  },
  beginInvokeTool(name: string, argsLine: string) {
    try {
      const args = JSON.parse(argsLine);
      begin(() => name === "research.search" ? search(args)
        : name === "research.fetch" ? fetchDocument(args)
        : Promise.reject(new Error("Unknown Exa tool: " + name)));
    } catch (error) {
      pendingResult = failure(error);
      active = false;
    }
  },
  tick() { __pumpNet(); },
  pollResult() {
    const result = pendingResult;
    pendingResult = undefined;
    return result;
  },
};
