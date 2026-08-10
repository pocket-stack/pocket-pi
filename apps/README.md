# Pocket Pi built-in Apps

Each directory is one independently versioned PocketJS App source. The checked
in `dist/app.js` and `dist/app.pak` files are target artifacts embedded into the
firmware as the recovery release, then seeded into `/workspace` on boot.

- `pi-agent`: the privileged Root View; its filesystem mount is `/workspace`.
- `robinhood`: curated MCP tools, `refreshPortfolio` AppTask, SQLite and View.
- `exa`: Exa search/fetch tools, SQLite search history and View.

Build all three from the pinned FS/DB branch checkout:

```sh
POCKETJS_ROOT=/path/to/pocketjs-feat-fs-surface cargo xtask build agentos-apps
```

The task refuses a checkout other than revision
`afc8d4e8e877dac7f9b0c01b5c0d667642009fc0`, so generated recovery bundles
cannot silently drift from the runtime modules linked into the firmware.
