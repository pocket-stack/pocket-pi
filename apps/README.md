# Pocket Pi built-in Apps

Each directory is one independently versioned PocketJS App source. The checked
in `dist/app.js` and `dist/app.pak` files are target artifacts embedded into the
firmware as the built-in release, then seeded into `/workspace` on boot. This
is not yet a signed install, rollback, or recovery-UI mechanism.

- `pi-agent`: the privileged Root View; its filesystem mount is `/workspace`.
- `robinhood`: curated MCP tools, `refreshPortfolio` AppTask, SQLite and View.
- `exa`: Exa search/fetch tools, SQLite search history and View.

Build all three from the pinned upstream PocketJS checkout:

```sh
POCKETJS_ROOT=/path/to/pocketjs-main cargo xtask build agentos-apps
```

The task refuses a checkout other than revision
`9c809bbd047ddc75c27caa4990951a78d942477a`, so generated recovery bundles
cannot silently drift from the runtime modules linked into the firmware.
