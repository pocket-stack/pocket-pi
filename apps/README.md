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

Package the source-loaded Exa App directly, without PocketJS, Bun or a compile
step:

```sh
cargo xtask package app exa path/to/exa-credentials.json
```

The result is `target/pocketapps/<id>.pocketapp`. HTTP and UART are only transport
ingress. Installation is create-only: an existing App id must be uninstalled
before it can be installed again.

Robinhood remains on the transitional Bundle path until its source migration:

```sh
POCKETJS_ROOT=/path/to/pocketjs cargo xtask build app robinhood path/to/robinhood-credentials.json
```
