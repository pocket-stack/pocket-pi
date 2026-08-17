// Every provider Tool returns its live result to the Agent. Only data consumed
// by the fixed View is normalized into SQLite.
const toolCatalog = PocketPi.resources.get("toolCatalog");
function run(sql, args = []) {
  return PocketPi.data.query(sql, args);
}
function insertRows(sql, rows) {
  if (!rows.length)
    return;
  const values = rows.map((row) => "(" + row.map(() => "?").join(",") + ")").join(","), args = [];
  for (const row of rows)
    args.push(...row);
  run(sql + " VALUES " + values, args);
}
const providerTools = toolCatalog.tools, providerToolByName = new Map(providerTools.map((tool) => [tool.name, tool]));
function now() {
  return Math.floor(Date.now() / 1000);
}
function text(value) {
  return value === null || value === void 0 || typeof value === "object" ? null : String(value);
}
function deep(value, names) {
  if (value === null || value === void 0 || typeof value !== "object")
    return null;
  for (const name of names) {
    const found = text(value[name]);
    if (found !== null)
      return found;
  }
  for (const child of Object.values(value)) {
    const found = deep(child, names);
    if (found !== null)
      return found;
  }
  return null;
}
function deepArray(value, names) {
  if (value === null || value === void 0 || typeof value !== "object")
    return [];
  for (const name of names)
    if (Array.isArray(value[name]))
      return value[name];
  for (const child of Object.values(value)) {
    const found = deepArray(child, names);
    if (found.length)
      return found;
  }
  return [];
}
function bool(value, names) {
  const raw = deep(value, names)?.toLowerCase();
  return raw === "true" || raw === "1" || raw === "yes";
}
function number(value) {
  if (value === null)
    return null;
  const parsed = Number(value.replace(/[$,%]/g, ""));
  return Number.isFinite(parsed) ? parsed : null;
}
function accountNumber(value, args) {
  const requested = args?.account_number;
  return requested === null || requested === void 0 || requested === "" ? deep(value, ["account_number", "accountNumber", "account"]) || "" : String(requested);
}
function list(value, names) {
  const nested = deepArray(value, names);
  return nested.length ? nested : Array.isArray(value) ? value : value ? [value] : [];
}
function searchTools(args) {
  const exact = Array.isArray(args?.names) ? new Set(args.names.filter((name) => typeof name === "string")) : new Set, words = String(args?.query ?? "").toLowerCase().split(/[^a-z0-9_]+/).filter(Boolean), namespace = typeof args?.namespace === "string" ? args.namespace : "";
  if (!exact.size && !words.length)
    throw Error("search_tools requires query or names");
  const limit = Math.max(1, Math.min(8, Number(args?.limit ?? 5) || 5)), matches = providerTools.filter((tool) => !namespace || tool.namespace === namespace).map((tool) => {
    const name = tool.name.toLowerCase(), haystack = name + " " + tool.description.toLowerCase(), score = exact.has(tool.name) ? 1000 : words.reduce((total, word) => total + (name === word ? 100 : name.includes(word) ? 20 : haystack.includes(word) ? 3 : 0), 0);
    return { tool, score };
  }).filter((item) => item.score > 0).sort((left, right) => right.score - left.score || left.tool.name.localeCompare(right.tool.name)).slice(0, limit).map(({ tool }) => ({
    name: tool.name,
    namespace: tool.namespace,
    description: tool.description,
    inputSchema: tool.inputSchema
  }));
  return {
    source: toolCatalog.source,
    protocolVersion: toolCatalog.protocolVersion,
    matches
  };
}
function matchesType(value, expected) {
  if (expected === "null")
    return value === null;
  if (expected === "array")
    return Array.isArray(value);
  if (expected === "object")
    return value !== null && typeof value === "object" && !Array.isArray(value);
  if (expected === "integer")
    return typeof value === "number" && Number.isInteger(value);
  if (expected === "number")
    return typeof value === "number" && Number.isFinite(value);
  return typeof value === expected;
}
function validateSchema(value, schema, path = "arguments") {
  const types = Array.isArray(schema?.type) ? schema.type : schema?.type ? [schema.type] : [];
  if (types.length && !types.some((expected) => matchesType(value, expected)))
    throw Error(path + " must be " + types.join(" or "));
  if (value === null)
    return;
  if (typeof value === "number") {
    if (typeof schema.minimum === "number" && value < schema.minimum)
      throw Error(path + " is below minimum " + schema.minimum);
    if (typeof schema.maximum === "number" && value > schema.maximum)
      throw Error(path + " exceeds maximum " + schema.maximum);
  }
  if (Array.isArray(value)) {
    if (schema.items)
      value.forEach((item, index) => validateSchema(item, schema.items, path + "[" + index + "]"));
    return;
  }
  if (typeof value !== "object")
    return;
  const properties = schema.properties || {};
  for (const required of schema.required || [])
    if (!(required in value))
      throw Error(path + "." + required + " is required");
  if (schema.additionalProperties === !1) {
    for (const key of Object.keys(value))
      if (!(key in properties))
        throw Error(path + "." + key + " is not allowed");
  }
  for (const [key, child] of Object.entries(value))
    if (properties[key])
      validateSchema(child, properties[key], path + "." + key);
}
function validatedProviderCall(args) {
  const operation = typeof args?.name === "string" ? args.name : "", tool = providerToolByName.get(operation);
  if (!tool)
    throw Error("Unknown Robinhood provider Tool: " + operation);
  const providerArgs = args?.arguments;
  validateSchema(providerArgs, tool.inputSchema);
  return invokeProviderTool(operation, providerArgs);
}
function retryableOperation(name) {
  return name.startsWith("get_") || name.startsWith("review_") || name === "search" || name === "run_scan";
}
function callTool(operation, args) {
  return PocketPi.services.call("mcp.client", "callTool", {
    connection: "robinhood",
    name: operation,
    arguments: args,
    retryable: retryableOperation(operation)
  });
}
function callTools(calls) {
  const value = PocketPi.services.call("mcp.client", "callTools", {
    connection: "robinhood",
    calls: calls.map((call) => ({ name: call.operation, arguments: call.args })),
    retryable: calls.every((call) => retryableOperation(call.operation))
  });
  return Array.isArray(value?.results) ? value.results : [];
}
function transaction(action) {
  PocketPi.data.transaction(action);
}
function saveAccounts(value, observedAt) {
  const rows = list(value, ["accounts"]);
  run("DELETE FROM accounts");
  const values = [];
  for (const item of rows) {
    const account = accountNumber(item, {});
    if (!account)
      continue;
    const accountType = (deep(item, ["nickname", "account_type", "type"]) || "").toUpperCase(), agentic = bool(item, ["agentic_allowed", "agenticAllowed"]), label = agentic ? "AGENTIC" : accountType.includes("IRA") || accountType.includes("RETIRE") ? "RETIREMENT" : accountType.includes("JOINT") ? "JOINT" : "PERSONAL";
    values.push([account, label, account.slice(-4), accountType, (deep(item, ["status"]) || "active").toUpperCase(), agentic ? 1 : 0, observedAt]);
  }
  insertRows("INSERT INTO accounts(account_number,label,suffix,account_type,status,agentic_allowed,updated_at)", values);
}
function savePortfolio(value, args, observedAt) {
  const account = accountNumber(value, args);
  if (!account)
    throw Error("Robinhood portfolio is missing account_number");
  const cash = deep(value, ["cash", "cash_available", "withdrawable_amount"]), buyingPower = deep(value, ["buying_power", "buyingPower"]), dayPnl = deep(value, ["day_pnl", "dayPnl", "equity_change"]), weekPnl = deep(value, ["week_pnl", "weekPnl"]);
  run(`INSERT INTO portfolio_current(account_number,cash,buying_power,day_pnl,week_pnl,observed_at)
     VALUES(?,?,?,?,?,?)
     ON CONFLICT(account_number) DO UPDATE SET
       cash=excluded.cash,buying_power=excluded.buying_power,
       day_pnl=COALESCE(excluded.day_pnl,portfolio_current.day_pnl),
       week_pnl=COALESCE(excluded.week_pnl,portfolio_current.week_pnl),
       observed_at=excluded.observed_at`, [account, cash, buyingPower, dayPnl, weekPnl, observedAt]);
  const total = deep(value, ["total_value", "equity", "total_equity", "portfolio_value", "market_value"]);
  if (total !== null)
    run("INSERT OR REPLACE INTO total_value(account_number,observed_at,value) VALUES(?,?,?)", [account, observedAt, total]);
}
function savePositions(value, args, observedAt) {
  const account = accountNumber(value, args);
  if (!account)
    throw Error("Robinhood positions are missing account_number");
  const rows = list(value, ["positions"]).slice(0, 64);
  run("DELETE FROM positions WHERE account_number=?", [account]);
  const values = [];
  for (const item of rows) {
    const symbol = deep(item, ["symbol"]);
    if (!symbol)
      continue;
    values.push([account, symbol, deep(item, ["quantity", "shares"]), deep(item, ["average_price", "averagePrice", "average_buy_price"]), deep(item, ["market_value", "marketValue", "equity"]), observedAt]);
  }
  insertRows("INSERT INTO positions(account_number,symbol,quantity,average_price,market_value,observed_at)", values);
}
function saveActivities(value, args, observedAt) {
  const account = accountNumber(value, args);
  if (!account)
    throw Error("Robinhood activities are missing account_number");
  const rows = list(value, ["orders", "activities", "results"]).slice(0, 64);
  run("DELETE FROM activities WHERE account_number=?", [account]);
  const values = [];
  rows.forEach((item, index) => {
    const symbol = deep(item, ["symbol"]), side = (deep(item, ["side"]) || "").toUpperCase(), quantity = deep(item, ["executed_quantity", "cumulative_quantity", "quantity"]), price = deep(item, ["average_price", "averagePrice", "executed_price", "price"]), occurredAt = deep(item, ["last_transaction_at", "created_at", "updated_at", "date"]), activityId = deep(item, ["id", "order_id", "orderId", "activity_id"]) || [occurredAt || observedAt, symbol || "ORDER", side, index].join(":"), quantityNumber = number(quantity), priceNumber = number(price), amount = quantityNumber !== null && priceNumber !== null ? String(quantityNumber * priceNumber) : price || quantity;
    values.push([account, activityId, occurredAt, observedAt, symbol, side, quantity, price, amount, (deep(item, ["state", "status"]) || "RECENT").toUpperCase(), (deep(item, ["type", "order_type"]) || "ORDER").toUpperCase()]);
  });
  insertRows("INSERT INTO activities(account_number,activity_id,occurred_at,observed_at,symbol,side,quantity,price,amount,state,activity_type)", values);
}
function saveRealizedPnl(value, args, observedAt) {
  const account = accountNumber(value, args);
  if (!account)
    throw Error("Robinhood P&L is missing account_number");
  const pnl = deep(value, ["total_returns", "realized_pnl", "total", "amount", "day_pnl", "week_pnl"]), span = String(args?.span ?? "day");
  run(`INSERT INTO portfolio_current(account_number,day_pnl,week_pnl,observed_at) VALUES(?,?,?,?)
     ON CONFLICT(account_number) DO UPDATE SET
       day_pnl=COALESCE(excluded.day_pnl,portfolio_current.day_pnl),
       week_pnl=COALESCE(excluded.week_pnl,portfolio_current.week_pnl),
       observed_at=MAX(portfolio_current.observed_at,excluded.observed_at)`, [account, span === "week" ? null : pnl, span === "week" ? pnl : null, observedAt]);
}
function saveDomainUpdate(update) {
  if (update.operation === "get_accounts")
    saveAccounts(update.value, update.observedAt);
  else if (update.operation === "get_portfolio")
    savePortfolio(update.value, update.args, update.observedAt);
  else if (update.operation === "get_equity_positions")
    savePositions(update.value, update.args, update.observedAt);
  else if (update.operation === "get_equity_orders")
    saveActivities(update.value, update.args, update.observedAt);
  else if (update.operation === "get_realized_pnl")
    saveRealizedPnl(update.value, update.args, update.observedAt);
  else
    return !1;
  return !0;
}
function isTransportFailure(message) {
  const value = message.toLowerCase();
  return value.includes("esp_err_http_connect") || value.includes("timeout") || value.includes("tls") || value.includes("socket") || value.includes("network");
}
function refreshPortfolio() {
  const startedAt = now(), updates = [], errors = [];
  let operationCount = 0;
  const request = (operation, args) => {
    operationCount += 1;
    try {
      const value = callTool(operation, args);
      updates.push({ operation, args, value, observedAt: now() });
      return value;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      errors.push(operation + ": " + message);
      if (operation === "get_accounts" || isTransportFailure(message))
        throw error;
      return null;
    }
  };
  let terminalError = null;
  try {
    const accountsValue = request("get_accounts", {}), accountNumbers = list(accountsValue, ["accounts"]).map((item) => accountNumber(item, {})).filter(Boolean);
    if (!accountNumbers.length)
      throw Error("Robinhood returned no brokerage accounts");
    const recentOrdersSince = new Date((startedAt - 604800) * 1000).toISOString().slice(0, 10), calls = [];
    for (const account of accountNumbers) {
      const args = { account_number: account };
      calls.push({ operation: "get_portfolio", args }, { operation: "get_equity_positions", args }, { operation: "get_equity_orders", args: { ...args, created_at_gte: recentOrdersSince } }, { operation: "get_realized_pnl", args: { ...args, span: "day", asset_classes: ["equity"] } }, { operation: "get_realized_pnl", args: { ...args, span: "week", asset_classes: ["equity"] } });
    }
    operationCount += calls.length;
    const results = callTools(calls);
    if (results.length !== calls.length)
      throw Error("Robinhood batch returned an incomplete result set");
    results.forEach((result, index) => {
      const call = calls[index];
      if (result?.ok)
        updates.push({ operation: call.operation, args: call.args, value: result.value, observedAt: now() });
      else
        errors.push(call.operation + ": " + String(result?.error || "unknown provider error"));
    });
    if (!updates.some((update) => update.operation === "get_portfolio"))
      throw Error("Robinhood batch returned no portfolio data");
  } catch (error) {
    terminalError = error instanceof Error ? error.message : String(error);
  }
  const status = terminalError ? "failed" : errors.length ? "partial" : "succeeded";
  transaction(() => {
    if (!terminalError)
      for (const update of updates)
        saveDomainUpdate(update);
    run(`INSERT INTO refresh_runs(started_at,completed_at,status,operation_count,success_count,error)
       VALUES(?,?,?,?,?,?)`, [startedAt, now(), status, operationCount, terminalError ? 0 : updates.length, terminalError || errors.join(" | ") || null]);
  });
  if (terminalError)
    throw Error(terminalError);
  return { status, operationCount, successCount: updates.length };
}
function saveEquityAction(operation, value, args, observedAt) {
  const account = String(args?.account_number ?? "");
  if (!account)
    return;
  const activityId = String(args?.order_id ?? deep(value, ["order_id", "orderId", "id"]) ?? "") || [observedAt, args?.symbol || "EQUITY", operation].join(":"), state = operation === "cancel_equity_order" ? deep(value, ["state", "status"]) || "CANCEL_REQUESTED" : deep(value, ["state", "status"]) || "SUBMITTED";
  run(`INSERT INTO activities(account_number,activity_id,occurred_at,observed_at,symbol,side,quantity,price,amount,state,activity_type)
     VALUES(?,?,?,?,?,?,?,?,?,?,?)
     ON CONFLICT(account_number,activity_id) DO UPDATE SET
       occurred_at=COALESCE(excluded.occurred_at,activities.occurred_at),
       observed_at=excluded.observed_at,
       symbol=COALESCE(excluded.symbol,activities.symbol),
       side=COALESCE(excluded.side,activities.side),
       quantity=COALESCE(excluded.quantity,activities.quantity),
       price=COALESCE(excluded.price,activities.price),
       state=excluded.state,
       activity_type=excluded.activity_type`, [
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
    operation === "cancel_equity_order" ? "EQUITY ORDER CANCEL" : "EQUITY ORDER"
  ]);
}
function invokeProviderTool(operation, args) {
  const value = callTool(operation, args), update = { operation, args, value, observedAt: now() };
  if (["get_accounts", "get_portfolio", "get_equity_positions", "get_equity_orders", "get_realized_pnl"].includes(operation))
    transaction(() => saveDomainUpdate(update));
  if (operation === "place_equity_order" || operation === "cancel_equity_order")
    transaction(() => saveEquityAction(operation, value, args, update.observedAt));
  return value;
}
PocketPi.defineActions({
  refreshPortfolio,
  searchTools,
  call: validatedProviderCall
});
