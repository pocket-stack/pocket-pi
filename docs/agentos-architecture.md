# Pocket Pi Agent-native Runtime

Pocket Pi is a device runtime, not an App running on a general-purpose OS. The
first supported host is ESP32-P4; `hosts/esp32-p4-sim` is a development and
product-contract simulator, not a second product.

Ordinary Apps are installed and executed as raw JavaScript source. The device
does not compile them, and changing an App does not rebuild or flash Firmware.
Uploading the same App id updates that App's source while preserving its SQLite
data and native credentials. The Pi Agent is the one firmware-embedded System
Bundle.

## Core model

```text
App = Data + Actions + View
```

- **Data** is App-owned SQLite state and is the durable truth.
- **Actions** are actor-neutral JavaScript functions. Agent Tools, UI events and
  Schedules route to the same named functions.
- **View** is the fixed PocketJS UI shipped by the App release.
- **Projection** is only the Data -> View binding layer. It is not a fourth App
  concept and does not own durable state.

Three principles govern the implementation:

1. **Mechanism stays native; policy stays editable.** Native code owns hardware,
   enforcement and bounded lifecycle. Product behavior,
   Actions, Projection declarations and View behavior stay in JavaScript.
2. **One execution substrate, many bounded Guests.** PocketJS/QuickJS is the
   only JavaScript substrate. Every Guest owns an isolated QuickJS runtime and
   context; there is no second JS engine and no shared JS heap.
3. **Every actor crosses the same Action boundary.** Tool, UI and Schedule are
   sources of an Action request, not separate business runtimes.

## Runtime layers

```text
Hardware
└── ESP-IDF Firmware host                         Rust
    ├── hardware, transport, credentials, limits
    ├── AppSupervisor and install/update/uninstall lifecycle
    └── PocketJS runtime platform                 Rust + QuickJS C
        ├── Pocket Pi System Framework            raw JavaScript
        ├── resident Pi Agent System Guest
        │   ├── Agent loop                         bundled JavaScript
        │   └── Chat / Files / Apps / Settings    raw View JavaScript
        ├── ordinary source View Guests           LRU, maximum 3
        └── ordinary source Action Guests         LRU, maximum 3
```

“PocketJS runtime” has two useful scopes:

- At the platform scope it is the single JS execution implementation linked
  into Firmware: QuickJS plus PocketJS modules and UI.
- At the instance scope each `Guest::new()` creates one isolated QuickJS
  runtime/context. A Guest is therefore comparable to an isolated JS service
  instance, not to another copy of the PocketJS platform.

The JS System Framework is evaluated inside every View and Action Guest before
the App entrypoint. It does not consume a seventh ordinary-App cache slot and does
not create another Guest.

Maximum resident JS Guests are currently:

```text
1 resident Pi Agent System Guest
+ up to 3 ordinary View Guests
+ up to 3 ordinary Action Guests
= up to 7 Guests
```

The two LRUs are deliberately independent. A closed App may retain a recent
Action Guest, and a visible App may have no Action Guest until an Action is
invoked. The resident System Guest is outside both LRUs and navigation never
drops it.

## Ownership and build boundaries

| Component | Owner/language | Lifetime | Changes require |
|---|---|---|---|
| ESP-IDF host | `firmware/esp32-p4`, Rust | device boot | Firmware build/flash |
| PocketJS platform | pinned PocketJS crates + QuickJS, Rust/C | Firmware lifetime | Firmware build/flash |
| AppSupervisor mechanisms | `crates/pocket-pi-agentos`, Rust | Firmware lifetime | Firmware build/flash |
| System Framework v1 | `system/framework.js`, JavaScript | evaluated per Guest | Firmware build/flash |
| Pi Agent System App | `apps/pi-agent`, raw View JavaScript + bundled Agent loop | resident | Firmware build/flash |
| Ordinary App | `apps/<id>`, raw JavaScript + SQL | LRU Guests | package + install/update |
| App Data | per-App SQLite + files | survives Guest eviction and restart | Action transaction |

Firmware contains the Pi Agent System App so a blank device can boot. System App
installation and replacement are deliberately outside the current contract.

## JS System Framework v1

`system/framework.js` is the App-facing policy layer. It currently provides:

- `PocketPi.defineActions({...})`
- `PocketPi.defineView({...})`
- `PocketPi.defineSystem({...})` for the resident System App's native facts
- `PocketPi.action(name, args)` for UI Action events
- `PocketPi.command(name, args)` and `PocketPi.navigate(app)` for narrow native
  mechanisms
- `PocketPi.data.query(...)`, `.exec(...)` and `.transaction(...)`
- `PocketPi.services.call(...)` and `PocketPi.actionContext.remainingMs()`
- `PocketPi.projection.one(...)` and `.many(...)` for bounded SQLite bindings

The Framework is platform-owned, not part of the Pi Agent System App. Firmware
currently provisions the one framework source at `system/framework.js`, and
every Guest evaluates that source before its App entrypoint. This gives the SDK one
explicit owner without inventing an updater or coupling its lifecycle to App
installation; independent Framework distribution is deferred.

`PocketPiSystem` is the private native-facing ABI used to configure a Guest,
start/tick/poll Actions, refresh Projection bindings and dispatch View input.
Ordinary Apps declare `frameworkApi: 1` in `app.json`; incompatible Apps are
rejected before activation.

The framework is intentionally one file. There is no service registry, plugin
container, dependency solver or compatibility shim.

Because the same framework is present in every Guest, native enforcement remains
authoritative: an ordinary App may emit `apps.open`, but privileged device,
Installer and Agent commands are accepted only from the resident System App.

## Actions

An App defines named functions once:

```js
PocketPi.defineActions({
  refreshPortfolio,
  search,
  fetch: fetchDocument,
});
```

Native routing supplies this envelope:

```json
{"action":"refreshPortfolio","args":{},"source":"tool|ui|schedule"}
```

`source` is context, not a second dispatch model. The same `ActionRunner` owns a
bounded queue and an LRU of at most three Action Guests. Only one Action executes
at a time in v1, giving SQLite and constrained ESP memory a simple deterministic
boundary.

Native owns the absolute deadline, credential-safe transport and capability
checks. JavaScript owns provider calls, response normalization, SQLite
transactions and returned domain results.

## Data, Projection and View

Each App has one native `DbModule` owner shared by its Action and View Guests.
The View mount exposes query only; SQL writes are rejected. The Action mount can
write and calls `app.commit()` after a successful transaction.

`app.commit()` increments the App's in-memory revision. At the next foreground
frame the View Guest refreshes each declared bounded Projection once. Multiple
commits before that frame coalesce into one refresh. Closed Views do not poll
SQLite and the Agent never calls an `update_view` tool.

The Pi Agent System View receives native `SystemFacts` for hardware and lifecycle
state. Those facts are not called Projection: UI/page policy remains in the JS
System App, while native supplies only facts and executes narrow commands such
as Wi-Fi connect, App install/uninstall and restart.

### Viewport contract

The host owns the physical display, touch controller and logical viewport. It
passes one positive `Viewport { width, height }` to `AppSupervisor`; the
supervisor creates every PocketJS `UiSurface` with that size. PocketJS exposes
the mounted size through its native `ui.__viewport` object, and the shared View
SDK publishes the validated, immutable App-facing value:

```js
View.viewport // { width, height, orientation }
```

The same viewport is used for layout, rendering and touch coordinates. Rust
does not choose App page layouts. Apps compose semantic `Row`, `Column` and
shared View SDK components, while PocketJS's Taffy layout engine resolves their
actual bounds. Apps may choose a different composition from
`View.viewport.orientation`; they do not receive a board name and should not
encode panel-specific pixel coordinates.

Reusable geometry belongs in the View SDK. For example, an App gives
`View.Sparkline` values and labels; the SDK derives canvas points from the
current viewport. This keeps raw drawing coordinates out of domain Apps without
turning the runtime into a general responsive-layout system.

## Ordinary Source App contract

An ordinary `.pocketapp` contains:

```text
app.json
schema.sql
actions.js
view.js
assets/              optional JSON resources declared by app.json
migrations/N.sql     optional schema N-1 to N migration
credentials.json     initial-install transport input, removed before activation
```

`schema.sql`, `actions.js` and `view.js` are the execution source, not build
artifacts. `assets/` contains only manifest-declared JSON resources. Packaging
does not require PocketJS, Bun or a compile step. The firmware-embedded System
App uses the same shared View SDK with `app.json`, `view.js` and its local
`text.js`; only its resident Agent loop remains a built `agent.js`. The native
seed adds `plan.json`. These System files are not accepted in an ordinary
`.pocketapp`.

App `version` identifies the source release shown to the user. Integer
`schemaVersion` identifies only the SQLite shape and changes only when that
shape changes. The two versions deliberately do not advance together.

## Install, update and uninstall

Before activation the Installer validates paths, size, manifest identity,
capabilities, credential declarations, resources and Framework API.
For a new App, `AppSupervisor`:

1. initializes `schema.sql`, loads `actions.js` against the App's SQLite owner
   and verifies every Tool/Schedule Action name;
2. evaluates the shared View SDK and `view.js` against the same SQLite owner
   through a read-only surface;
3. moves the staged source to the single `apps/<id>/release` location;
4. registers credentials, Tools, Schedules and the two LRU configurations.

Any failure removes the incomplete App. There are no release pointers, retained
versions or rollback. For an existing App id, the same review UI applies an
update through the same supervisor path:

1. rejects credentials, native-permission changes, schema downgrades and missing
   `migrations/N.sql` steps;
2. copies the quiescent SQLite database and rehearses the migrations plus
   candidate Actions/View against that copy;
3. runs the same migrations in one live SQLite transaction;
4. swaps the single source release and replaces Tools, Schedules and cached
   Action/View runtimes;
5. removes the temporary old source after the new App is active.

`.update/release` exists only after physical confirmation and is the complete
recovery signal. If power is lost, boot reruns any uncommitted SQLite migration
and finishes the source swap. There is no persistent installation record,
release history or rollback mechanism. Explicit uninstall still removes all
App-owned state.

Migration files contain only schema/data statements. The runtime owns the
transaction and `PRAGMA user_version`; Apps must not put transaction control or
set `user_version` inside `migrations/N.sql`.

## Minimal native capability surface

Native retains only boundaries that JavaScript cannot safely own:

- display/touch, clocks, Wi-Fi and restart mechanisms;
- QuickJS Guest creation, memory/stack bounds and the two 3-entry LRUs;
- filesystem roots and quotas;
- per-App SQLite ownership, read/write enforcement and revision counters;
- credentials, TLS, endpoint/operation allowlists and model/provider transport;
- scheduler clock/cursor and Action queue admission;
- package validation and install/update/uninstall recovery;
- rendering of PocketJS draw output.

Robinhood/Exa schema, provider mapping, Actions, Projection SQL and all product
Views remain App-owned source.

## Explicitly not implemented

- a general on-device ES module loader for multi-file App source trees;
- on-device TypeScript, TSX or JSX transformation;
- independent update or API migration of the System Framework itself;
- System App replacement or independent update, release history and rollback;
- a package dependency graph, plugin framework or second JavaScript runtime.

The contract stays deliberately small: one Actions entrypoint, one View
entrypoint, one final schema, conventional forward SQL migrations and optional
declared JSON resources. Modules, JSX or a larger component system should be
added only when a concrete App cannot be expressed cleanly with this boundary.

## Evidence

The core contracts are exercised in `crates/pocket-pi-agentos/src/lib.rs`:

- View and Action LRUs retain the three most recent Guests.
- Tool routing uses each App's declared Action.
- App revision commits coalesce at the foreground frame boundary.
- code-only update preserves SQLite data and native credentials.
- schema update rejects missing steps and preserves rows through migration.
- boot completes an approved update interrupted before activation.
- Schedule success is recorded only after its Action actually completes.
- Projection and SQLite errors propagate instead of becoming an empty View.
- uninstall removes App-owned routing, Guests, schedules, credentials and data.

`hosts/esp32-p4-sim` adds end-to-end tests for Exa and Robinhood Action writes,
fixed Views and install/restart restoration. Simulator evidence does not replace
an ESP32 build, boot or physical touch/network acceptance test.
