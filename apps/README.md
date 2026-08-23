# Pocket Pi Apps

Each directory contains one independently versioned PocketJS App source.

- `pi-agent` is the privileged, resident System App. It owns `/workspace`; its
  Actions, View and Agent loop are embedded in Firmware rather than installed
  as a `.pocketapp`.
- `robinhood` and `exa` are ordinary Apps. They own their Tools, Actions,
  SQLite state and Views, and are never embedded or seeded by firmware.

Build the System App when developing its source:

```sh
cargo xtask build pi-agent
```

This updates `apps/pi-agent/dist/agent.js`, which is committed and embedded by
Firmware builds. Rebuild the separate View SDK resource pack only after editing
`system/view-sdk-pack.ts`:

```sh
POCKETJS_ROOT=/path/to/pocketjs cargo xtask build view-sdk
```

`POCKETJS_ROOT` must point to the PocketJS revision pinned by Pocket Pi. It is
not required for normal App packaging, simulator builds or Firmware builds.

Package either ordinary App directly, without PocketJS, Bun or a compile step:

```sh
cargo xtask package app exa path/to/exa-credentials.json
cargo xtask package app robinhood path/to/robinhood-credentials.json
```

The result is `target/pocketapps/<id>.pocketapp`. HTTP and UART are only transport
ingress. Uploading a package for an existing App updates its source while
preserving its SQLite data and native credentials. Package an update without a
credentials file:

```sh
cargo xtask package app exa
```

`version` identifies the App release shown to the user. `schemaVersion` changes
only when the SQLite shape changes. A package that raises `schemaVersion` from
N to N+1 must contain `migrations/<N+1>.sql`; fresh installs always create the
final shape from `schema.sql`. Migration files contain only schema/data SQL;
Pocket Pi owns their transaction and `PRAGMA user_version`.
