# Pocket Pi Apps

Each directory contains one independently versioned PocketJS App source.

- `pi-agent` is the privileged, resident System App. It owns `/workspace` and is
  the only App included in firmware.
- `robinhood` and `exa` are ordinary Apps. They own their Tools, Data Actions,
  SQLite state and Views, and are never embedded or seeded by firmware.

Build the System App when developing its source:

```sh
POCKETJS_ROOT=/path/to/pocketjs cargo xtask build pi-agent
```

Build any ordinary App as a standalone install artifact:

```sh
POCKETJS_ROOT=/path/to/pocketjs cargo xtask build app <id> [credentials.json]
```

The result is `target/pocketapps/<id>.pocketapp`. Every ordinary App reaches the
device through the Installer; HTTP is only the current upload ingress. The build
refuses a PocketJS checkout other than the revision pinned by Pocket Pi so App
artifacts cannot drift from the runtime modules linked into firmware.
