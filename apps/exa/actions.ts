// Headless Exa data plane. Only search history consumed by the fixed View is
// persisted; fetched documents are returned directly to the Agent.
import { __pumpNet, fetch } from "@pocketjs/framework/net";

const SCHEMA_VERSION = 5;
const RETENTION_DAYS = 7;
const RETENTION_SECONDS = RETENTION_DAYS * 24 * 60 * 60;

const version = Number(PocketPi.data.query("PRAGMA user_version")[0]?.user_version ?? 0);
if (version !== SCHEMA_VERSION) {
  PocketPi.data.exec(`
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
  PocketPi.data.query("DELETE FROM searches WHERE searched_at < ?", [cutoff]);
}

async function post(path: "/search" | "/contents", body: any): Promise<any> {
  const response = await fetch(`https://api.exa.ai${path}`, {
    method: "POST",
    headers: { accept: "application/json", "content-type": "application/json" },
    body: JSON.stringify(body),
    timeoutMs: PocketPi.actionContext.remainingMs(),
    maxBytes: 96 * 1024,
  });
  const value = await response.json<any>();
  if (!response.ok) throw new Error(`Exa HTTP ${response.status}: ${JSON.stringify(value)}`);
  return value;
}

function transaction(action: () => void): void {
  PocketPi.data.transaction(action);
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
      PocketPi.data.query(
        "INSERT INTO searches(query,searched_at,status,result_count,top_title,error) VALUES(?,?,?,?,?,NULL)",
        [searchQuery, searchedAt, "ok", results.length, topTitle],
      );
      cleanupExpired(searchedAt);
    });
    return value;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    transaction(() => {
      PocketPi.data.query(
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

PocketPi.defineActions(
  { search, fetch: fetchDocument },
  { pump: __pumpNet },
);
