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

Build any ordinary App as a standalone install artifact:

```sh
POCKETJS_ROOT=/path/to/pocketjs cargo xtask build app <id> [credentials.json]
```

The result is `target/pocketapps/<id>.pocketapp`. HTTP and UART are only transport
ingress. Installation is create-only: an existing App id must be uninstalled
before it can be installed again.
The build refuses a PocketJS checkout other than the revision pinned by Pocket
Pi so App artifacts cannot drift from the runtime modules linked into firmware.
