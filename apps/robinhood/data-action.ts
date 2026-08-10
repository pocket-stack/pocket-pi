// Headless Robinhood data plane. Provider bodies are normalized in memory,
// then domain tables are changed in one transaction. The foreground View
// never sees provider payloads and never writes business state.
const nativeDb = (globalThis as any).db;
const handle = nativeDb.open("robinhood");
if (handle < 0) throw new Error("open robinhood.sqlite");

const SCHEMA_VERSION = 4;

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
  CREATE TABLE IF NOT EXISTS equity_historicals (
    account_number TEXT NOT NULL,
    span TEXT NOT NULL,
    point_time TEXT NOT NULL,
    price TEXT,
    open TEXT,
    high TEXT,
    low TEXT,
    close TEXT,
    volume TEXT,
    observed_at INTEGER NOT NULL,
    PRIMARY KEY(account_number, span, point_time)
  );
  CREATE TABLE IF NOT EXISTS pnl_trades (
    account_number TEXT NOT NULL,
    trade_id TEXT NOT NULL,
    occurred_at TEXT,
    symbol TEXT,
    side TEXT,
    quantity TEXT,
    price TEXT,
    realized_pnl TEXT,
    observed_at INTEGER NOT NULL,
    PRIMARY KEY(account_number, trade_id)
  );
  CREATE TABLE IF NOT EXISTS order_reviews (
    review_id TEXT PRIMARY KEY,
    account_number TEXT,
    symbol TEXT,
    side TEXT,
    quantity TEXT,
    limit_price TEXT,
    estimated_cost TEXT,
    state TEXT,
    observed_at INTEGER NOT NULL
  );
    PRAGMA user_version=4;
  `);
}

type DomainUpdate = { operation: string; args: any; value: any; observedAt: number };

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
  return deep(value, ["account_number", "accountNumber", "account"])
    || String(args?.account_number ?? args?.accountNumber ?? "");
}
function list(value: any, names: string[]): any[] {
  const nested = deepArray(value, names);
  return nested.length ? nested : Array.isArray(value) ? value : value ? [value] : [];
}

function callTool(operation: string, args: any): any {
  const envelope = JSON.parse((globalThis as any).services.call(
    "mcp.client",
    "callTool",
    JSON.stringify({ connection: "robinhood", name: operation, arguments: args }),
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
  for (const item of rows) {
    const account = accountNumber(item, {});
    if (!account) continue;
    const accountType = (deep(item, ["nickname", "account_type", "type"]) || "").toUpperCase();
    const agentic = bool(item, ["agentic_allowed", "agenticAllowed"]);
    const label = agentic ? "AGENTIC"
      : accountType.includes("IRA") || accountType.includes("RETIRE") ? "RETIREMENT"
      : accountType.includes("JOINT") ? "JOINT" : "PERSONAL";
    run(
      "INSERT INTO accounts(account_number,label,suffix,account_type,status,agentic_allowed,updated_at) VALUES(?,?,?,?,?,?,?)",
      [account, label, account.slice(-4), accountType, (deep(item, ["status"]) || "active").toUpperCase(), agentic ? 1 : 0, observedAt],
    );
  }
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
  for (const item of rows) {
    const symbol = deep(item, ["symbol"]);
    if (!symbol) continue;
    run(
      "INSERT INTO positions(account_number,symbol,quantity,average_price,market_value,observed_at) VALUES(?,?,?,?,?,?)",
      [account, symbol, deep(item, ["quantity", "shares"]), deep(item, ["average_price", "averagePrice", "average_buy_price"]), deep(item, ["market_value", "marketValue", "equity"]), observedAt],
    );
  }
}

function saveActivities(value: any, args: any, observedAt: number): void {
  const account = accountNumber(value, args);
  if (!account) throw new Error("Robinhood activities are missing account_number");
  const rows = list(value, ["orders", "activities", "results"]).slice(0, 64);
  run("DELETE FROM activities WHERE account_number=?", [account]);
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
    run(
      `INSERT INTO activities(account_number,activity_id,occurred_at,observed_at,symbol,side,quantity,price,amount,state,activity_type)
       VALUES(?,?,?,?,?,?,?,?,?,?,?)`,
      [account, activityId, occurredAt, observedAt, symbol, side, quantity, price, amount, (deep(item, ["state", "status"]) || "RECENT").toUpperCase(), (deep(item, ["type", "order_type"]) || "ORDER").toUpperCase()],
    );
  });
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

function saveHistoricals(value: any, args: any, observedAt: number): void {
  const account = accountNumber(value, args);
  const span = String(args?.span ?? args?.interval ?? "default");
  const rows = list(value, ["historicals", "results", "data"]);
  run("DELETE FROM equity_historicals WHERE account_number=? AND span=?", [account, span]);
  rows.forEach((item, index) => {
    const pointTime = deep(item, ["begins_at", "timestamp", "time", "date"]) || String(index);
    run(
      `INSERT INTO equity_historicals(account_number,span,point_time,price,open,high,low,close,volume,observed_at)
       VALUES(?,?,?,?,?,?,?,?,?,?)`,
      [account, span, pointTime, deep(item, ["price", "adjusted_close", "close_price"]), deep(item, ["open_price", "open"]), deep(item, ["high_price", "high"]), deep(item, ["low_price", "low"]), deep(item, ["close_price", "close"]), deep(item, ["volume"]), observedAt],
    );
  });
}

function savePnlTrades(value: any, args: any, observedAt: number): void {
  const account = accountNumber(value, args);
  const rows = list(value, ["trades", "results", "data"]);
  run("DELETE FROM pnl_trades WHERE account_number=?", [account]);
  rows.forEach((item, index) => {
    const occurredAt = deep(item, ["created_at", "updated_at", "date", "timestamp"]);
    const tradeId = deep(item, ["id", "trade_id", "tradeId"]) || [occurredAt || observedAt, deep(item, ["symbol"]) || "TRADE", index].join(":");
    run(
      `INSERT INTO pnl_trades(account_number,trade_id,occurred_at,symbol,side,quantity,price,realized_pnl,observed_at)
       VALUES(?,?,?,?,?,?,?,?,?)`,
      [account, tradeId, occurredAt, deep(item, ["symbol"]), (deep(item, ["side"]) || "").toUpperCase(), deep(item, ["quantity", "shares"]), deep(item, ["price", "average_price"]), deep(item, ["realized_pnl", "pnl", "amount"]), observedAt],
    );
  });
}

function saveOrderReview(value: any, args: any, observedAt: number): void {
  const account = accountNumber(value, args);
  const symbol = deep(value, ["symbol"]) || text(args?.symbol);
  const side = (deep(value, ["side"]) || text(args?.side) || "").toUpperCase();
  const quantity = deep(value, ["quantity", "shares"]) || text(args?.quantity);
  const limitPrice = deep(value, ["limit_price", "limitPrice", "price"]) || text(args?.limit_price ?? args?.price);
  const reviewId = deep(value, ["id", "review_id", "reviewId"]) || [account, symbol, side, quantity, limitPrice, observedAt].join(":");
  run(
    `INSERT OR REPLACE INTO order_reviews(review_id,account_number,symbol,side,quantity,limit_price,estimated_cost,state,observed_at)
     VALUES(?,?,?,?,?,?,?,?,?)`,
    [reviewId, account, symbol, side, quantity, limitPrice, deep(value, ["estimated_cost", "estimatedCost", "total"]), deep(value, ["state", "status"]), observedAt],
  );
}

function save(update: DomainUpdate): void {
  if (update.operation === "get_accounts") saveAccounts(update.value, update.observedAt);
  else if (update.operation === "get_portfolio") savePortfolio(update.value, update.args, update.observedAt);
  else if (update.operation === "get_equity_positions") savePositions(update.value, update.args, update.observedAt);
  else if (update.operation === "get_equity_orders") saveActivities(update.value, update.args, update.observedAt);
  else if (update.operation === "get_realized_pnl") saveRealizedPnl(update.value, update.args, update.observedAt);
  else if (update.operation === "get_equity_historicals") saveHistoricals(update.value, update.args, update.observedAt);
  else if (update.operation === "get_pnl_trade_history") savePnlTrades(update.value, update.args, update.observedAt);
  else if (update.operation === "review_equity_order") saveOrderReview(update.value, update.args, update.observedAt);
  else throw new Error("No Robinhood table mapping for " + update.operation);
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
    if (!terminalError) for (const update of updates) save(update);
    run(
      `INSERT INTO refresh_runs(started_at,completed_at,status,operation_count,success_count,error)
       VALUES(?,?,?,?,?,?)`,
      [startedAt, now(), status, operationCount, terminalError ? 0 : updates.length, terminalError || errors.join(" | ") || null],
    );
  });
  if (terminalError) throw new Error(terminalError);
  return { status, operationCount, successCount: updates.length };
}

const toolOperations: Record<string, string> = {
  "robinhood.get_accounts": "get_accounts",
  "robinhood.get_portfolio": "get_portfolio",
  "robinhood.get_equity_positions": "get_equity_positions",
  "robinhood.get_equity_orders": "get_equity_orders",
  "robinhood.get_equity_historicals": "get_equity_historicals",
  "robinhood.get_realized_pnl": "get_realized_pnl",
  "robinhood.get_pnl_trade_history": "get_pnl_trade_history",
  "robinhood.review_equity_order": "review_equity_order",
};

function invokeMappedTool(name: string, args: any): any {
  const operation = toolOperations[name];
  if (!operation) throw new Error("Unknown Robinhood tool: " + name);
  const value = callTool(operation, args);
  transaction(() => save({ operation, args, value, observedAt: now() }));
  return value;
}

(globalThis as any).PocketPiData = {
  invokeTask(name: string) {
    try {
      if (name !== "refreshPortfolio") throw new Error("Unknown Robinhood Data Action: " + name);
      const value = refreshPortfolio();
      return JSON.stringify({ text: JSON.stringify(value), details: value, isError: false });
    } catch (error) {
      return JSON.stringify({ text: error instanceof Error ? error.message : String(error), isError: true });
    }
  },
  invokeTool(name: string, argsLine: string) {
    try {
      const args = JSON.parse(argsLine);
      const value = name === "robinhood.refresh_portfolio" ? refreshPortfolio() : invokeMappedTool(name, args);
      return JSON.stringify({ text: JSON.stringify(value), details: value, isError: false });
    } catch (error) {
      return JSON.stringify({ text: error instanceof Error ? error.message : String(error), isError: true });
    }
  },
};
