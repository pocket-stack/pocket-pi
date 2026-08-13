import toolCatalog from "./tool-catalog.json";

// Headless Robinhood data plane. Every declared provider Tool returns its live
// result to the Agent. Only data consumed by the fixed foreground View is
// normalized into SQLite; transient research/watchlist/options payloads never
// become App state merely because a Tool was called.
const nativeDb = (globalThis as any).db;
const handle = nativeDb.open("robinhood");
if (handle < 0) throw new Error("open robinhood.sqlite");

const SCHEMA_VERSION = 5;

function dbError(): string { return String(nativeDb.lastError(handle) || "SQLite operation failed"); }
function exec(sql: string): void { if (nativeDb.exec(handle, sql) !== 0) throw new Error(dbError()); }
function query(sql: string, args: any[] = []): any {
  const result = JSON.parse(nativeDb.query(handle, sql, JSON.stringify(args)));
  if (result.error) throw new Error(String(result.error));
  return result;
}
function run(sql: string, args: any[] = []): any { return query(sql, args); }
function insertRows(sql: string, rows: any[][]): void {
  if (!rows.length) return;
  const values = rows.map((row) => "(" + row.map(() => "?").join(",") + ")").join(",");
  const args: any[] = [];
  for (const row of rows) args.push(...row);
  run(sql + " VALUES " + values, args);
}

const version = Number(query("PRAGMA user_version")?.rows?.[0]?.[0] ?? 0);
if (version !== SCHEMA_VERSION) {
  exec(`
    CREATE TABLE IF NOT EXISTS accounts (
    account_number TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    suffix TEXT NOT NULL,
    account_type TEXT,
    status TEXT NOT NULL,
    agentic_allowed INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
  );
  CREATE TABLE IF NOT EXISTS portfolio_current (
    account_number TEXT PRIMARY KEY,
    cash TEXT,
    buying_power TEXT,
    day_pnl TEXT,
    week_pnl TEXT,
    observed_at INTEGER NOT NULL
  );
  CREATE TABLE IF NOT EXISTS total_value (
    account_number TEXT NOT NULL,
    observed_at INTEGER NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY(account_number, observed_at)
  );
  CREATE TABLE IF NOT EXISTS positions (
    account_number TEXT NOT NULL,
    symbol TEXT NOT NULL,
    quantity TEXT,
    average_price TEXT,
    market_value TEXT,
    observed_at INTEGER NOT NULL,
    PRIMARY KEY(account_number, symbol)
  );
  CREATE TABLE IF NOT EXISTS activities (
    account_number TEXT NOT NULL,
    activity_id TEXT NOT NULL,
    occurred_at TEXT,
    observed_at INTEGER NOT NULL,
    symbol TEXT,
    side TEXT,
    quantity TEXT,
    price TEXT,
    amount TEXT,
    state TEXT,
    activity_type TEXT,
    PRIMARY KEY(account_number, activity_id)
  );
  CREATE INDEX IF NOT EXISTS activities_account_recent ON activities(account_number, occurred_at DESC, observed_at DESC);
  CREATE TABLE IF NOT EXISTS refresh_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at INTEGER NOT NULL,
    completed_at INTEGER NOT NULL,
    status TEXT NOT NULL,
    operation_count INTEGER NOT NULL,
    success_count INTEGER NOT NULL,
    error TEXT
  );
  CREATE INDEX IF NOT EXISTS refresh_runs_recent ON refresh_runs(id DESC);
    PRAGMA user_version=5;
  `);
}

type DomainUpdate = { operation: string; args: any; value: any; observedAt: number };
type ToolMetadata = { name: string; namespace: string; description: string; inputSchema: any };

const providerTools = (toolCatalog as any).tools as ToolMetadata[];
const providerToolByName = new Map(providerTools.map((tool) => [tool.name, tool]));

function now(): number { return Math.floor(Date.now() / 1000); }
function text(value: unknown): string | null {
  return value === null || value === undefined || typeof value === "object" ? null : String(value);
}
function deep(value: any, names: string[]): string | null {
  if (value === null || value === undefined || typeof value !== "object") return null;
  for (const name of names) {
    const found = text(value[name]);
    if (found !== null) return found;
  }
  for (const child of Object.values(value)) {
    const found = deep(child, names);
    if (found !== null) return found;
  }
  return null;
}
function deepArray(value: any, names: string[]): any[] {
  if (value === null || value === undefined || typeof value !== "object") return [];
  for (const name of names) if (Array.isArray(value[name])) return value[name];
  for (const child of Object.values(value)) {
    const found = deepArray(child, names);
    if (found.length) return found;
  }
  return [];
}
function bool(value: any, names: string[]): boolean {
  const raw = deep(value, names)?.toLowerCase();
  return raw === "true" || raw === "1" || raw === "yes";
}
function number(value: string | null): number | null {
  if (value === null) return null;
  const parsed = Number(value.replace(/[$,%]/g, ""));
  return Number.isFinite(parsed) ? parsed : null;
}
function accountNumber(value: any, args: any): string {
  const requested = args?.account_number;
  return requested === null || requested === undefined || requested === ""
    ? deep(value, ["account_number", "accountNumber", "account"]) || ""
    : String(requested);
}
function list(value: any, names: string[]): any[] {
  const nested = deepArray(value, names);
  return nested.length ? nested : Array.isArray(value) ? value : value ? [value] : [];
}

function searchTools(args: any): any {
  const exact = Array.isArray(args?.names)
    ? new Set(args.names.filter((name: any) => typeof name === "string"))
    : new Set<string>();
  const words = String(args?.query ?? "").toLowerCase().split(/[^a-z0-9_]+/).filter(Boolean);
  const namespace = typeof args?.namespace === "string" ? args.namespace : "";
  if (!exact.size && !words.length) throw new Error("search_tools requires query or names");
  const limit = Math.max(1, Math.min(8, Number(args?.limit ?? 5) || 5));
  const matches = providerTools
    .filter((tool) => !namespace || tool.namespace === namespace)
    .map((tool) => {
      const name = tool.name.toLowerCase();
      const haystack = name + " " + tool.description.toLowerCase();
      const score = exact.has(tool.name) ? 1000
        : words.reduce((total, word) => total + (name === word ? 100 : name.includes(word) ? 20 : haystack.includes(word) ? 3 : 0), 0);
      return { tool, score };
    })
    .filter((item) => item.score > 0)
    .sort((left, right) => right.score - left.score || left.tool.name.localeCompare(right.tool.name))
    .slice(0, limit)
    .map(({ tool }) => ({
      name: tool.name,
      namespace: tool.namespace,
      description: tool.description,
      inputSchema: tool.inputSchema,
    }));
  return {
    source: (toolCatalog as any).source,
    protocolVersion: (toolCatalog as any).protocolVersion,
    matches,
  };
}

function matchesType(value: any, expected: string): boolean {
  if (expected === "null") return value === null;
  if (expected === "array") return Array.isArray(value);
  if (expected === "object") return value !== null && typeof value === "object" && !Array.isArray(value);
  if (expected === "integer") return typeof value === "number" && Number.isInteger(value);
  if (expected === "number") return typeof value === "number" && Number.isFinite(value);
  return typeof value === expected;
}

function validateSchema(value: any, schema: any, path = "arguments"): void {
  const types = Array.isArray(schema?.type) ? schema.type : schema?.type ? [schema.type] : [];
  if (types.length && !types.some((expected: string) => matchesType(value, expected))) {
    throw new Error(path + " must be " + types.join(" or "));
  }
  if (value === null) return;
  if (typeof value === "number") {
    if (typeof schema.minimum === "number" && value < schema.minimum) throw new Error(path + " is below minimum " + schema.minimum);
    if (typeof schema.maximum === "number" && value > schema.maximum) throw new Error(path + " exceeds maximum " + schema.maximum);
  }
  if (Array.isArray(value)) {
    if (schema.items) value.forEach((item, index) => validateSchema(item, schema.items, path + "[" + index + "]"));
    return;
  }
  if (typeof value !== "object") return;
  const properties = schema.properties || {};
  for (const required of schema.required || []) {
    if (!(required in value)) throw new Error(path + "." + required + " is required");
  }
  if (schema.additionalProperties === false) {
    for (const key of Object.keys(value)) {
      if (!(key in properties)) throw new Error(path + "." + key + " is not allowed");
    }
  }
  for (const [key, child] of Object.entries(value)) {
    if (properties[key]) validateSchema(child, properties[key], path + "." + key);
  }
}

function validatedProviderCall(args: any): any {
  const operation = typeof args?.name === "string" ? args.name : "";
  const tool = providerToolByName.get(operation);
  if (!tool) throw new Error("Unknown Robinhood provider Tool: " + operation);
  const providerArgs = args?.arguments;
  validateSchema(providerArgs, tool.inputSchema);
  return invokeProviderTool(operation, providerArgs);
}

function retryableOperation(name: string): boolean {
  return name.startsWith("get_") || name.startsWith("review_") || name === "search" || name === "run_scan";
}

function callTool(operation: string, args: any): any {
  const envelope = JSON.parse((globalThis as any).services.call(
    "mcp.client",
    "callTool",
    JSON.stringify({
      connection: "robinhood",
      name: operation,
      arguments: args,
      retryable: retryableOperation(operation),
    }),
  ));
  if (!envelope.ok) throw new Error(envelope.error || "Robinhood service failed");
  return envelope.value;
}

function callTools(calls: Array<{ operation: string; args: any }>): any[] {
  const envelope = JSON.parse((globalThis as any).services.call(
    "mcp.client",
    "callTools",
    JSON.stringify({
      connection: "robinhood",
      calls: calls.map((call) => ({ name: call.operation, arguments: call.args })),
      retryable: calls.every((call) => retryableOperation(call.operation)),
    }),
  ));
  if (!envelope.ok) throw new Error(envelope.error || "Robinhood batch service failed");
  return Array.isArray(envelope.value?.results) ? envelope.value.results : [];
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

function saveAccounts(value: any, observedAt: number): void {
  const rows = list(value, ["accounts"]);
  run("DELETE FROM accounts");
  const values: any[][] = [];
  for (const item of rows) {
    const account = accountNumber(item, {});
    if (!account) continue;
    const accountType = (deep(item, ["nickname", "account_type", "type"]) || "").toUpperCase();
    const agentic = bool(item, ["agentic_allowed", "agenticAllowed"]);
    const label = agentic ? "AGENTIC"
      : accountType.includes("IRA") || accountType.includes("RETIRE") ? "RETIREMENT"
      : accountType.includes("JOINT") ? "JOINT" : "PERSONAL";
    values.push([account, label, account.slice(-4), accountType, (deep(item, ["status"]) || "active").toUpperCase(), agentic ? 1 : 0, observedAt]);
  }
  insertRows("INSERT INTO accounts(account_number,label,suffix,account_type,status,agentic_allowed,updated_at)", values);
}

function savePortfolio(value: any, args: any, observedAt: number): void {
  const account = accountNumber(value, args);
  if (!account) throw new Error("Robinhood portfolio is missing account_number");
  const cash = deep(value, ["cash", "cash_available", "withdrawable_amount"]);
  const buyingPower = deep(value, ["buying_power", "buyingPower"]);
  const dayPnl = deep(value, ["day_pnl", "dayPnl", "equity_change"]);
  const weekPnl = deep(value, ["week_pnl", "weekPnl"]);
  run(
    `INSERT INTO portfolio_current(account_number,cash,buying_power,day_pnl,week_pnl,observed_at)
     VALUES(?,?,?,?,?,?)
     ON CONFLICT(account_number) DO UPDATE SET
       cash=excluded.cash,buying_power=excluded.buying_power,
       day_pnl=COALESCE(excluded.day_pnl,portfolio_current.day_pnl),
       week_pnl=COALESCE(excluded.week_pnl,portfolio_current.week_pnl),
       observed_at=excluded.observed_at`,
    [account, cash, buyingPower, dayPnl, weekPnl, observedAt],
  );
  const total = deep(value, ["total_value", "equity", "total_equity", "portfolio_value", "market_value"]);
  if (total !== null) {
    run("INSERT OR REPLACE INTO total_value(account_number,observed_at,value) VALUES(?,?,?)", [account, observedAt, total]);
  }
}

function savePositions(value: any, args: any, observedAt: number): void {
  const account = accountNumber(value, args);
  if (!account) throw new Error("Robinhood positions are missing account_number");
  const rows = list(value, ["positions"]).slice(0, 64);
  run("DELETE FROM positions WHERE account_number=?", [account]);
  const values: any[][] = [];
  for (const item of rows) {
    const symbol = deep(item, ["symbol"]);
    if (!symbol) continue;
    values.push([account, symbol, deep(item, ["quantity", "shares"]), deep(item, ["average_price", "averagePrice", "average_buy_price"]), deep(item, ["market_value", "marketValue", "equity"]), observedAt]);
  }
  insertRows("INSERT INTO positions(account_number,symbol,quantity,average_price,market_value,observed_at)", values);
}

function saveActivities(value: any, args: any, observedAt: number): void {
  const account = accountNumber(value, args);
  if (!account) throw new Error("Robinhood activities are missing account_number");
  const rows = list(value, ["orders", "activities", "results"]).slice(0, 64);
  run("DELETE FROM activities WHERE account_number=?", [account]);
  const values: any[][] = [];
  rows.forEach((item, index) => {
    const symbol = deep(item, ["symbol"]);
    const side = (deep(item, ["side"]) || "").toUpperCase();
    const quantity = deep(item, ["executed_quantity", "cumulative_quantity", "quantity"]);
    const price = deep(item, ["average_price", "averagePrice", "executed_price", "price"]);
    const occurredAt = deep(item, ["last_transaction_at", "created_at", "updated_at", "date"]);
    const explicitId = deep(item, ["id", "order_id", "orderId", "activity_id"]);
    const activityId = explicitId || [occurredAt || observedAt, symbol || "ORDER", side, index].join(":");
    const quantityNumber = number(quantity);
    const priceNumber = number(price);
    const amount = quantityNumber !== null && priceNumber !== null ? String(quantityNumber * priceNumber) : price || quantity;
    values.push([account, activityId, occurredAt, observedAt, symbol, side, quantity, price, amount, (deep(item, ["state", "status"]) || "RECENT").toUpperCase(), (deep(item, ["type", "order_type"]) || "ORDER").toUpperCase()]);
  });
  insertRows("INSERT INTO activities(account_number,activity_id,occurred_at,observed_at,symbol,side,quantity,price,amount,state,activity_type)", values);
}

function saveRealizedPnl(value: any, args: any, observedAt: number): void {
  const account = accountNumber(value, args);
  if (!account) throw new Error("Robinhood P&L is missing account_number");
  const pnl = deep(value, ["total_returns", "realized_pnl", "total", "amount", "day_pnl", "week_pnl"]);
  const span = String(args?.span ?? "day");
  run(
    `INSERT INTO portfolio_current(account_number,day_pnl,week_pnl,observed_at) VALUES(?,?,?,?)
     ON CONFLICT(account_number) DO UPDATE SET
       day_pnl=COALESCE(excluded.day_pnl,portfolio_current.day_pnl),
       week_pnl=COALESCE(excluded.week_pnl,portfolio_current.week_pnl),
       observed_at=MAX(portfolio_current.observed_at,excluded.observed_at)`,
    [account, span === "week" ? null : pnl, span === "week" ? pnl : null, observedAt],
  );
}

function saveProjection(update: DomainUpdate): boolean {
  if (update.operation === "get_accounts") saveAccounts(update.value, update.observedAt);
  else if (update.operation === "get_portfolio") savePortfolio(update.value, update.args, update.observedAt);
  else if (update.operation === "get_equity_positions") savePositions(update.value, update.args, update.observedAt);
  else if (update.operation === "get_equity_orders") saveActivities(update.value, update.args, update.observedAt);
  else if (update.operation === "get_realized_pnl") saveRealizedPnl(update.value, update.args, update.observedAt);
  else return false;
  return true;
}

function isTransportFailure(message: string): boolean {
  const value = message.toLowerCase();
  return value.includes("esp_err_http_connect") || value.includes("timeout")
    || value.includes("tls") || value.includes("socket") || value.includes("network");
}

function refreshPortfolio(): any {
  const startedAt = now();
  const updates: DomainUpdate[] = [];
  const errors: string[] = [];
  let operationCount = 0;

  const request = (operation: string, args: any): any | null => {
    operationCount += 1;
    try {
      const value = callTool(operation, args);
      updates.push({ operation, args, value, observedAt: now() });
      return value;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      errors.push(operation + ": " + message);
      if (operation === "get_accounts" || isTransportFailure(message)) throw error;
      return null;
    }
  };

  let terminalError: string | null = null;
  try {
    const accountsValue = request("get_accounts", {});
    const accountRows = list(accountsValue, ["accounts"]);
    const accountNumbers = accountRows.map((item) => accountNumber(item, {})).filter(Boolean);
    if (!accountNumbers.length) throw new Error("Robinhood returned no brokerage accounts");
    const recentOrdersSince = new Date((startedAt - 7 * 24 * 60 * 60) * 1000).toISOString().slice(0, 10);
    const calls: Array<{ operation: string; args: any }> = [];
    for (const account of accountNumbers) {
      const args = { account_number: account };
      calls.push(
        { operation: "get_portfolio", args },
        { operation: "get_equity_positions", args },
        { operation: "get_equity_orders", args: { ...args, created_at_gte: recentOrdersSince } },
        { operation: "get_realized_pnl", args: { ...args, span: "day", asset_classes: ["equity"] } },
        { operation: "get_realized_pnl", args: { ...args, span: "week", asset_classes: ["equity"] } },
      );
    }
    operationCount += calls.length;
    const results = callTools(calls);
    if (results.length !== calls.length) throw new Error("Robinhood batch returned an incomplete result set");
    results.forEach((result, index) => {
      const call = calls[index];
      if (result?.ok) {
        updates.push({ operation: call.operation, args: call.args, value: result.value, observedAt: now() });
      } else {
        errors.push(call.operation + ": " + String(result?.error || "unknown provider error"));
      }
    });
    if (!updates.some((update) => update.operation === "get_portfolio")) {
      throw new Error("Robinhood batch returned no portfolio data");
    }
  } catch (error) {
    terminalError = error instanceof Error ? error.message : String(error);
  }

  const status = terminalError ? "failed" : errors.length ? "partial" : "succeeded";
  transaction(() => {
    if (!terminalError) for (const update of updates) saveProjection(update);
    run(
      `INSERT INTO refresh_runs(started_at,completed_at,status,operation_count,success_count,error)
       VALUES(?,?,?,?,?,?)`,
      [startedAt, now(), status, operationCount, terminalError ? 0 : updates.length, terminalError || errors.join(" | ") || null],
    );
  });
  if (terminalError) throw new Error(terminalError);
  return { status, operationCount, successCount: updates.length };
}

function saveEquityAction(operation: string, value: any, args: any, observedAt: number): void {
  const account = String(args?.account_number ?? "");
  if (!account) return;
  const orderId = String(args?.order_id ?? deep(value, ["order_id", "orderId", "id"]) ?? "");
  const activityId = orderId || [observedAt, args?.symbol || "EQUITY", operation].join(":");
  const state = operation === "cancel_equity_order"
    ? deep(value, ["state", "status"]) || "CANCEL_REQUESTED"
    : deep(value, ["state", "status"]) || "SUBMITTED";
  run(
    `INSERT INTO activities(account_number,activity_id,occurred_at,observed_at,symbol,side,quantity,price,amount,state,activity_type)
     VALUES(?,?,?,?,?,?,?,?,?,?,?)
     ON CONFLICT(account_number,activity_id) DO UPDATE SET
       occurred_at=COALESCE(excluded.occurred_at,activities.occurred_at),
       observed_at=excluded.observed_at,
       symbol=COALESCE(excluded.symbol,activities.symbol),
       side=COALESCE(excluded.side,activities.side),
       quantity=COALESCE(excluded.quantity,activities.quantity),
       price=COALESCE(excluded.price,activities.price),
       state=excluded.state,
       activity_type=excluded.activity_type`,
    [
      account,
      activityId,
      deep(value, ["created_at", "updated_at", "last_transaction_at"]),
      observedAt,
      args?.symbol ?? deep(value, ["symbol"]),
      args?.side ?? deep(value, ["side"]),
      args?.quantity ?? args?.dollar_amount ?? deep(value, ["quantity", "executed_quantity"]),
      args?.limit_price ?? args?.stop_price ?? deep(value, ["price", "average_price"]),
      args?.dollar_amount ?? null,
      String(state).toUpperCase(),
      operation === "cancel_equity_order" ? "EQUITY ORDER CANCEL" : "EQUITY ORDER",
    ],
  );
}

function invokeProviderTool(operation: string, args: any): any {
  const value = callTool(operation, args);
  const update = { operation, args, value, observedAt: now() };
  if (["get_accounts", "get_portfolio", "get_equity_positions", "get_equity_orders", "get_realized_pnl"].includes(operation)) {
    transaction(() => saveProjection(update));
  }
  if (operation === "place_equity_order" || operation === "cancel_equity_order") {
    // The order result directly affects the View's activity projection. Avoid
    // nesting a second MCP request in the same 128 KiB QuickJS call stack;
    // portfolio and positions converge on the normal refresh task.
    transaction(() => saveEquityAction(operation, value, args, update.observedAt));
  }
  return value;
}

function success(value: any): string {
  // The Agent consumes Tool results through text. Keeping the same provider
  // object in details would retain a second copy in QuickJS and again in the
  // Rust/Agent message bridge, which is especially expensive for market-data
  // and options responses.
  return JSON.stringify({ text: JSON.stringify(value), isError: false });
}

(globalThis as any).PocketPiData = {
  invokeTask(name: string) {
    try {
      if (name !== "refreshPortfolio") throw new Error("Unknown Robinhood Data Action: " + name);
      const value = refreshPortfolio();
      return success(value);
    } catch (error) {
      return JSON.stringify({ text: error instanceof Error ? error.message : String(error), isError: true });
    }
  },
  invokeTool(name: string, argsLine: string) {
    try {
      const args = JSON.parse(argsLine);
      const value = name === "robinhood.refresh_portfolio" ? refreshPortfolio()
        : name === "robinhood.search_tools" ? searchTools(args)
        : name === "robinhood.call" ? validatedProviderCall(args)
        : (() => { throw new Error("Unknown Robinhood Tool: " + name); })();
      return success(value);
    } catch (error) {
      return JSON.stringify({ text: error instanceof Error ? error.message : String(error), isError: true });
    }
  },
};
