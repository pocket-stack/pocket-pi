# Data and Actions

Data is App-owned SQLite/files. Actions are the only place that mutates state or
calls external services. A View only reads bounded projections.

## schema.sql

Write the complete initial schema and useful initial rows. Do not set
`PRAGMA user_version` or add transaction statements; the runtime does both.

```sql
CREATE TABLE expenses(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  label TEXT NOT NULL,
  cents INTEGER NOT NULL CHECK(cents >= 0)
);
```

## actions.js

Define Actions once. `args` comes from a tool, UI event or schedule; `source` is
`tool`, `ui` or `schedule`.

```js
function addExpense(args) {
  const label = String(args.label ?? "").trim();
  const cents = Number(args.cents);
  if (!label) throw new Error("label is required");
  if (!Number.isInteger(cents) || cents < 0) throw new Error("cents must be a non-negative integer");
  return PocketPi.data.transaction(() => {
    PocketPi.data.query("INSERT INTO expenses(label, cents) VALUES(?, ?)", [label, cents]);
    return PocketPi.data.query(
      "SELECT id, label, cents FROM expenses WHERE id = last_insert_rowid()",
    )[0];
  });
}

PocketPi.defineActions({ addExpense });
```

- `PocketPi.data.query(sql, params)` returns row objects and accepts positional
  arrays or named objects.
- `PocketPi.data.exec(sql)` executes SQL without parameters.
- `PocketPi.data.transaction(fn)` commits atomically and refreshes the active
  View after success. Prefer it for mutations.
- If an Action changes `data.fs` without a SQLite transaction, call
  `PocketPi.data.commit()` after the durable write.
- Return small JSON-serializable results. Throw `Error` with a concrete message
  for invalid input or external failure.

## Agent tools and schedules

Every declared tool/schedule Action must exist in `defineActions`.

```json
{
  "toolNamespace": "expenses",
  "tools": [{
    "name": "expenses.add",
    "action": "addExpense",
    "description": "Record an expense.",
    "parameters": {
      "type": "object",
      "properties": {
        "label": {"type": "string"},
        "cents": {"type": "integer", "minimum": 0}
      },
      "required": ["label", "cents"],
      "additionalProperties": false
    }
  }],
  "schedules": [{
    "id": "weekly-pass",
    "everyMinutes": 10080,
    "action": "addExpense",
    "args": {"label": "Transit pass", "cents": 2500}
  }]
}
```

The same Action may serve Agent, UI and schedule callers. Keep domain behavior
inside it instead of duplicating logic per caller.

## Projection into View

Declare bounded queries in `view.js`. They run at mount and refresh after a
committed Action while the View is foregrounded.

```js
const expenses = View.state([]);
PocketPi.projection.many(
  "SELECT id, label, cents FROM expenses ORDER BY id DESC LIMIT 8",
  {},
  (rows) => expenses.set(rows),
);
```

Use `projection.one` for zero/one row and `projection.many` with an explicit
`LIMIT` for lists. Keep pagination/visible-row limits in the App.
