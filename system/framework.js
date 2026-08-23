(() => {
  let appId = "";
  let actions = null;
  const actionPumps = [];
  let actionResult;
  let actionRunning = false;
  let view = null;
  let systemHooks = null;
  let database = -1;
  let resources = Object.freeze({});
  const bindings = [];

  function fail(message) {
    throw new Error(`Pocket Pi System Framework: ${message}`);
  }

  function result(value, isError) {
    const text = isError
      ? value instanceof Error ? value.message : String(value)
      : typeof value === "string" ? value : JSON.stringify(value);
    return JSON.stringify({ text, isError });
  }

  function finish(promise) {
    Promise.resolve(promise).then(
      (value) => {
        actionResult = result(value, false);
        actionRunning = false;
      },
      (error) => {
        actionResult = result(error, true);
        actionRunning = false;
      },
    );
  }

  function defineActions(definitions, options) {
    if (actions) fail("Actions already defined");
    if (!definitions || typeof definitions !== "object") fail("Actions must be an object");
    for (const [name, action] of Object.entries(definitions)) {
      if (!name || typeof action !== "function") fail(`invalid Action: ${name}`);
    }
    actions = Object.freeze({ ...definitions });
    if (options?.pump !== undefined) registerActionPump(options.pump);
  }

  function registerActionPump(pump) {
    if (typeof pump !== "function") fail("Action pump must be a function");
    actionPumps.push(pump);
  }

  function defineView(definition) {
    if (view) fail("View already defined");
    if (!definition || typeof definition !== "object") fail("View must be an object");
    view = definition;
  }

  function defineSystem(definition) {
    if (appId !== "pi-agent") fail("System hooks belong to the resident System App");
    if (systemHooks) fail("System hooks already defined");
    if (!definition || typeof definition.update !== "function" || typeof definition.telemetryVisible !== "function") {
      fail("System hooks require update and telemetryVisible");
    }
    systemHooks = definition;
  }

  function event(value) {
    if (value === undefined || value === null || value === "") return "";
    return typeof value === "string" ? value : JSON.stringify(value);
  }

  function dbHandle() {
    if (database >= 0) return database;
    if (!appId) fail("Guest is not configured");
    database = globalThis.db.open(appId);
    if (database < 0) fail(`cannot open ${appId}.sqlite`);
    return database;
  }

  function rows(sql, params) {
    const line = globalThis.db.query(dbHandle(), sql, JSON.stringify(params ?? {}));
    const value = JSON.parse(line);
    if (value.error) fail(value.error);
    const columns = value.cols ?? [];
    return (value.rows ?? []).map((row) => Object.fromEntries(columns.map((column, index) => [column, row[index]])));
  }

  function exec(sql) {
    if (typeof globalThis.db.exec !== "function") fail("Data writes are unavailable in this Guest");
    if (globalThis.db.exec(dbHandle(), sql) !== 0) {
      fail(globalThis.db.lastError(dbHandle()) || "SQLite operation failed");
    }
  }

  function transaction(action) {
    if (typeof action !== "function") fail("Data transaction requires a function");
    exec("BEGIN IMMEDIATE");
    try {
      const value = action();
      exec("COMMIT");
      commit();
      return value;
    } catch (error) {
      try { exec("ROLLBACK"); } catch {}
      throw error;
    }
  }

  function commit() {
    if (typeof globalThis.app?.commit !== "function") fail("Data commit is unavailable");
    globalThis.app.commit();
  }

  function callService(service, operation, args = {}) {
    if (typeof globalThis.services?.call !== "function") fail("Native services are unavailable");
    const envelope = JSON.parse(globalThis.services.call(service, operation, JSON.stringify(args)));
    if (!envelope.ok) throw new Error(envelope.error || `${service}.${operation} failed`);
    return envelope.value;
  }

  function bind(cardinality, sql, params, apply) {
    if (typeof sql !== "string" || !sql.trim()) fail("Projection SQL is required");
    if (typeof apply !== "function") fail("Projection apply must be a function");
    const binding = {
      refresh() {
        const values = rows(sql, typeof params === "function" ? params() : params);
        apply(cardinality === "one" ? values[0] ?? null : values);
      },
    };
    bindings.push(binding);
    binding.refresh();
    return Object.freeze(binding);
  }

  function refreshBindings() {
    for (const binding of bindings) binding.refresh();
  }

  function freeze(value) {
    if (!value || typeof value !== "object" || Object.isFrozen(value)) return value;
    for (const child of Object.values(value)) freeze(child);
    return Object.freeze(value);
  }

  globalThis.PocketPi = Object.freeze({
    frameworkApi: 1,
    defineActions,
    defineView,
    defineSystem,
    action: (action, args = {}) => ({ type: "action", action, args }),
    command: (command, args = {}) => ({ type: "command", command, args }),
    navigate: (app) => ({ type: "command", command: "apps.open", args: { app } }),
    data: Object.freeze({ query: rows, exec, transaction, commit }),
    resources: Object.freeze({
      get(name) {
        if (!Object.hasOwn(resources, name)) fail(`unknown resource: ${name}`);
        return resources[name];
      },
    }),
    services: Object.freeze({ call: callService }),
    actionContext: Object.freeze({
      remainingMs() {
        if (typeof globalThis.app?.remainingMs !== "function") fail("Action deadline is unavailable");
        return globalThis.app.remainingMs();
      },
    }),
    projection: Object.freeze({
      one: (sql, params, apply) => bind("one", sql, params, apply),
      many: (sql, params, apply) => bind("many", sql, params, apply),
    }),
  });

  globalThis.PocketPiSystem = Object.freeze({
    configure(id, resourceLine = "{}") {
      if (appId) fail("Guest already configured");
      appId = id;
      resources = freeze(JSON.parse(resourceLine));
    },
    actionNames() {
      return JSON.stringify(Object.keys(actions ?? {}));
    },
    hasView() {
      return view !== null;
    },
    telemetryVisible() {
      return systemHooks?.telemetryVisible() === true;
    },
    registerActionPump,
    beginAction(line) {
      if (!actions) fail("Actions not defined");
      if (actionRunning) fail("Action already running");
      const request = JSON.parse(line);
      const action = actions[request.action];
      if (!action) fail(`unknown Action: ${request.action}`);
      actionRunning = true;
      actionResult = undefined;
      try {
        finish(action(request.args ?? {}, Object.freeze({ source: request.source })));
      } catch (error) {
        actionResult = result(error, true);
        actionRunning = false;
      }
    },
    tickAction() {
      for (const pump of actionPumps) pump();
    },
    pollActionResult() {
      const value = actionResult;
      actionResult = undefined;
      return value;
    },
    tickView() {
      return event(view?.tick?.());
    },
    dataChanged() {
      refreshBindings();
      return event(view?.dataChanged?.());
    },
    updateView(line) {
      return event(systemHooks?.update(JSON.parse(line)));
    },
    tap(x, y) {
      return event(view?.tap?.(x, y));
    },
    pointerDown(x, y) {
      return event(view?.pointerDown?.(x, y));
    },
    pointerUp() {
      return event(view?.pointerUp?.());
    },
  });
})();
