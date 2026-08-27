# Pocket Pi App authoring

An ordinary App is exactly **Data + Actions + View**. It is source-loaded by
Framework API 1; there is no build step, package manager, DOM, Node.js or React.

## Read order

1. Read this file, `DATA_ACTIONS.md` and `VIEW.md` before creating an App.
2. Read `HTTP.md` only when the App needs `net.http`.
3. Search `pocketpi.d.ts` with `grep` when an exact name, signature or allowed
   value is unclear.

## Candidate layout

Create or edit only `apps/<id>/checkout/`:

```text
apps/<id>/checkout/
├── app.json       identity, capabilities, Agent tools and schedules
├── schema.sql     initial SQLite schema; required and non-empty
├── actions.js     all state-changing or external operations
├── view.js        fixed UI backed by bounded projections
├── migrations/    optional N.sql files for installed schema upgrades
└── assets/        optional JSON resources declared in app.json
```

Only these names are accepted. Every candidate needs the four root files.

For a new App, create the directory with the normal `write` tool. For an
installed App, call `app.checkout({id})`; it preserves an existing checkout and
never copies live data or credentials.

## Minimal app.json

```json
{
  "format": 1,
  "frameworkApi": 1,
  "id": "expenses",
  "title": "Expense Log",
  "description": "Record local spending",
  "version": "1.0.0",
  "schemaVersion": 1,
  "capabilities": ["data.sqlite"],
  "resources": {},
  "toolNamespace": "expenses",
  "tools": [],
  "schedules": []
}
```

- `id` is the directory name and contains only letters, digits, `.`, `-`, `_`.
- Allowed capabilities are `data.sqlite`, `data.fs`, and `net.http`.
- Every Agent tool name starts with `<toolNamespace>.` and maps to one Action.
- Every schedule names one Action and uses `everyMinutes`.
- JSON assets must be declared as
  `"name":{"path":"assets/file.json","type":"json"}`; declarations and
  files must match exactly.

## Validate and submit

Call `app.validate({path:"apps/<id>/checkout"})` after creating the files and
after each fix. It runs schema setup/migration, Actions, projections, View and
layout against scratch state. It does not modify the live release or live data.
Use its exact error and `screenText`; do not guess around a failure.

When validation succeeds, call `app.submit` with the same path. Submission
moves the candidate to the existing physical confirmation flow. For an update,
change `version`. Change `schemaVersion` only when the SQLite shape changes and
add every required `migrations/N.sql`. Migration files contain only SQL changes;
the runtime owns the transaction and `PRAGMA user_version`.
