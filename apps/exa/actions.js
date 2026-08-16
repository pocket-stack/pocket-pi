// Only search history consumed by the fixed View is persisted. Fetched
// documents are returned directly to the Agent.
const RETENTION_SECONDS = 7 * 24 * 60 * 60;

function now() {
  return Math.floor(Date.now() / 1000);
}

function cleanupExpired(referenceTime) {
  PocketPi.data.query("DELETE FROM searches WHERE searched_at < ?", [referenceTime - RETENTION_SECONDS]);
}

async function post(path, body) {
  const response = await fetch(`https://api.exa.ai${path}`, {
    method: "POST",
    headers: { accept: "application/json", "content-type": "application/json" },
    body: JSON.stringify(body),
    timeoutMs: PocketPi.actionContext.remainingMs(),
    maxBytes: 96 * 1024,
  });
  const value = await response.json();
  if (!response.ok) throw new Error(`Exa HTTP ${response.status}: ${JSON.stringify(value)}`);
  return value;
}

async function search(args) {
  const query = String(args.query ?? "").trim();
  if (!query) throw new Error("query is required");
  const searchedAt = now();
  try {
    const body = {
      query,
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
    PocketPi.data.transaction(() => {
      PocketPi.data.query(
        "INSERT INTO searches(query,searched_at,status,result_count,top_title,error) VALUES(?,?,?,?,?,NULL)",
        [query, searchedAt, "ok", results.length, topTitle],
      );
      cleanupExpired(searchedAt);
    });
    return value;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    PocketPi.data.transaction(() => {
      PocketPi.data.query(
        "INSERT INTO searches(query,searched_at,status,result_count,top_title,error) VALUES(?,?,?,0,NULL,?)",
        [query, searchedAt, "error", message],
      );
      cleanupExpired(searchedAt);
    });
    throw error;
  }
}

async function fetchDocument(args) {
  const url = String(args.url ?? "").trim();
  if (!url) throw new Error("url is required");
  const request = {
    urls: [url],
    text: {
      maxCharacters: Math.max(200, Math.min(12000, Number(args.maxCharacters ?? 6000))),
      includeHtmlTags: false,
    },
  };
  if (args.maxAgeHours !== undefined) request.maxAgeHours = args.maxAgeHours;
  return post("/contents", request);
}

PocketPi.defineActions({ search, fetch: fetchDocument });
