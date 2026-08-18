# Pocket Pi Apps

Each directory contains one independently versioned PocketJS App source.

- `pi-agent` is the privileged, resident System App. It owns `/workspace` and is
  embedded in Firmware; it is not an installable `.pocketapp`.
- `robinhood` and `exa` are ordinary Apps. They own their Tools, Actions,
  SQLite state and Views, and are never embedded or seeded by firmware.

Build the System App when developing its source:

```sh
POCKETJS_ROOT=/path/to/pocketjs cargo xtask build pi-agent
```

This updates the generated bundles embedded by the Firmware build.

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
