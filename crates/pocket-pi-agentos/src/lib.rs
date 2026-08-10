//! Pocket Pi's App runtime: one isolated PocketJS Guest per active App,
//! app-owned FS/SQLite state, namespaced Agent tools and native AppTask wakes.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context as _, Result};
use pocket_db::{DbModule, Storage as DbStorage};
use pocket_fs::{FsModule, Storage as FsStorage};
use pocket_mod::qjs::{CatchResultExt as _, Function, Object};
use pocket_mod::Guest;
use pocket_pi_embedded::{AgentEvent, GuestAgent, ModelBackend, ToolHost, ToolResult};
use pocket_ui_surface::UiSurface;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const ROOT_APP_ID: &str = "pi-agent";
pub const ROBINHOOD_APP_ID: &str = "robinhood";
pub const EXA_APP_ID: &str = "exa";
pub const BUILTIN_RELEASE: &str = "builtin-v1";
pub const VIEWPORT: (f32, f32) = (720.0, 1280.0);

// App tools must enqueue remote work quickly. Keep a defensive bound for the
// Tool Router handoff and local read-only diagnostics; remote work itself runs
// in the Data Action runner and never holds the App/UI owner.
const TOOL_TIMEOUT: Duration = Duration::from_secs(45);
const ROOT_JS: &str = include_str!("../../../apps/pi-agent/dist/app.js");
const ROOT_AGENT_JS: &str = include_str!("../../../apps/pi-agent/dist/agent.js");
const ROOT_PAK: &[u8] = include_bytes!("../../../apps/pi-agent/dist/app.pak");
const ROOT_DESCRIPTOR: &str = include_str!("../../../apps/pi-agent/agent-app.json");
const ROOT_POCKET: &str = include_str!("../../../apps/pi-agent/pocket.json");
const ROBINHOOD_JS: &str = include_str!("../../../apps/robinhood/dist/app.js");
const ROBINHOOD_DATA_JS: &str = include_str!("../../../apps/robinhood/dist/data-action.js");
const ROBINHOOD_PAK: &[u8] = include_bytes!("../../../apps/robinhood/dist/app.pak");
const ROBINHOOD_DESCRIPTOR: &str = include_str!("../../../apps/robinhood/agent-app.json");
const ROBINHOOD_POCKET: &str = include_str!("../../../apps/robinhood/pocket.json");
const EXA_JS: &str = include_str!("../../../apps/exa/dist/app.js");
const EXA_DATA_JS: &str = include_str!("../../../apps/exa/dist/data-action.js");
const EXA_PAK: &[u8] = include_bytes!("../../../apps/exa/dist/app.pak");
const EXA_DESCRIPTOR: &str = include_str!("../../../apps/exa/agent-app.json");
const EXA_POCKET: &str = include_str!("../../../apps/exa/pocket.json");

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppDescriptor {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub data_version: u32,
    #[serde(default)]
    pub tools: Vec<Value>,
    #[serde(default)]
    pub tasks: Vec<String>,
    #[serde(default)]
    pub schedules: Vec<AppSchedule>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSchedule {
    pub id: String,
    pub every_minutes: u64,
    pub task: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Clone)]
struct BuiltinApp {
    descriptor: AppDescriptor,
    descriptor_json: &'static str,
    pocket_json: &'static str,
    js: &'static str,
    data_js: Option<&'static str>,
    pak: &'static [u8],
}

#[derive(Clone)]
pub struct AppCatalog {
    apps: BTreeMap<String, BuiltinApp>,
    tool_owner: BTreeMap<String, String>,
}

impl AppCatalog {
    pub fn builtin() -> Result<Self> {
        let mut apps = BTreeMap::new();
        for (descriptor_json, pocket_json, js, data_js, pak) in [
            (ROOT_DESCRIPTOR, ROOT_POCKET, ROOT_JS, None, ROOT_PAK),
            (
                ROBINHOOD_DESCRIPTOR,
                ROBINHOOD_POCKET,
                ROBINHOOD_JS,
                Some(ROBINHOOD_DATA_JS),
                ROBINHOOD_PAK,
            ),
            (
                EXA_DESCRIPTOR,
                EXA_POCKET,
                EXA_JS,
                Some(EXA_DATA_JS),
                EXA_PAK,
            ),
        ] {
            let descriptor: AppDescriptor =
                serde_json::from_str(descriptor_json).context("parse built-in agent-app.json")?;
            if apps.contains_key(&descriptor.id) {
                anyhow::bail!("duplicate built-in App id: {}", descriptor.id);
            }
            apps.insert(
                descriptor.id.clone(),
                BuiltinApp {
                    descriptor,
                    descriptor_json,
                    pocket_json,
                    js,
                    data_js,
                    pak,
                },
            );
        }
        let mut tool_owner = BTreeMap::new();
        for app in apps.values() {
            for tool in &app.descriptor.tools {
                let name = tool
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("{} tool is missing name", app.descriptor.id))?;
                if !name.starts_with(&format!("{}.", app.descriptor.id))
                    && !(app.descriptor.id == EXA_APP_ID && name.starts_with("research."))
                {
                    anyhow::bail!("App {} owns non-namespaced tool {name}", app.descriptor.id);
                }
                if tool_owner
                    .insert(name.to_owned(), app.descriptor.id.clone())
                    .is_some()
                {
                    anyhow::bail!("duplicate App tool: {name}");
                }
            }
        }
        Ok(Self { apps, tool_owner })
    }

    pub fn tool_definitions(&self) -> Vec<Value> {
        self.apps
            .values()
            .flat_map(|app| app.descriptor.tools.clone())
            .collect()
    }

    pub fn app_for_tool(&self, name: &str) -> Option<&str> {
        self.tool_owner.get(name).map(String::as_str)
    }

    pub fn descriptors(&self) -> impl Iterator<Item = &AppDescriptor> {
        self.apps.values().map(|app| &app.descriptor)
    }

    fn app(&self, id: &str) -> Option<&BuiltinApp> {
        self.apps.get(id)
    }
}

/// The native transport/security boundary used by App bundles. Implementations
/// own TLS, credentials and MCP sessions; Apps own operation selection,
/// normalization, SQLite and View behavior.
pub trait AppServiceHost: Send + Sync {
    fn call(
        &self,
        app_id: &str,
        service: &str,
        operation: &str,
        args: &Value,
    ) -> Result<Value, String>;

    /// True while a native App worker owns network/TLS activity.
    fn busy(&self) -> bool {
        false
    }
}

/// One SQLite module instance per App. View and background Data Action guests
/// share this owner, so ESP32's `unix-none` VFS never has two independent
/// connections racing on the same file. Network waits never hold this mutex;
/// only bounded SQLite operations and transactions do.
type SharedDb = Arc<Mutex<DbModule>>;
type AppRevision = Arc<AtomicU32>;

const DATA_ACTION_QUEUE: usize = 8;
pub const DATA_ACTION_STACK_BYTES: usize = 128 * 1024;

#[derive(Clone, Copy)]
enum DataActionKind {
    Task,
    Tool,
}

struct DataActionRequest {
    run_id: u64,
    app_id: String,
    kind: DataActionKind,
    name: String,
    args: Value,
}

#[derive(Clone)]
struct DataAppConfig {
    app_id: String,
    source_path: PathBuf,
    database: SharedDb,
    revision: AppRevision,
}

struct DataActionRuntime {
    guest: Guest,
    _database: SharedDb,
    _revision: AppRevision,
}

impl DataActionRuntime {
    fn load(config: &DataAppConfig, services: Arc<dyn AppServiceHost>) -> Result<Self> {
        let guest = Guest::new()?;
        mount_shared_db(&guest, config.database.clone())?;
        mount_services(&guest, config.app_id.clone(), services)?;
        mount_data_lifecycle(&guest, config.revision.clone())?;
        let source = std::fs::read_to_string(&config.source_path)
            .with_context(|| format!("read {} Data Action", config.app_id))?;
        guest.eval(&format!("{}-data-action", config.app_id), &source)?;
        anyhow::ensure!(
            guest.with(|ctx| ctx.globals().get::<_, Object>("PocketPiData").is_ok()),
            "{} Data Action installed no PocketPiData",
            config.app_id
        );
        Ok(Self {
            guest,
            _database: config.database.clone(),
            _revision: config.revision.clone(),
        })
    }

    fn invoke(&self, request: &DataActionRequest) -> Result<ToolResult> {
        let method = match request.kind {
            DataActionKind::Task => "invokeTask",
            DataActionKind::Tool => "invokeTool",
        };
        let line: String = self.guest.with(|ctx| {
            let data: Object = ctx
                .globals()
                .get("PocketPiData")
                .map_err(|error| anyhow!("PocketPiData missing: {error}"))?;
            let function: Function = data
                .get(method)
                .map_err(|error| anyhow!("PocketPiData.{method} missing: {error}"))?;
            function
                .call::<_, String>((request.name.clone(), request.args.to_string()))
                .catch(&ctx)
                .map_err(|error| anyhow!("PocketPiData.{method}: {error}"))
        })?;
        let value: Value = serde_json::from_str(&line).context("parse Data Action result")?;
        Ok(ToolResult {
            text: value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or(&line)
                .to_owned(),
            details: value.get("details").cloned().unwrap_or(Value::Null),
            is_error: value
                .get("isError")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            terminate: false,
        })
    }
}

struct AppDataRunner {
    tx: mpsc::SyncSender<DataActionRequest>,
    next_run_id: AtomicU32,
    busy: Arc<AtomicBool>,
}

impl AppDataRunner {
    fn start(configs: Vec<DataAppConfig>, services: Arc<dyn AppServiceHost>) -> Result<Self> {
        let (tx, rx) = mpsc::sync_channel::<DataActionRequest>(DATA_ACTION_QUEUE);
        let busy = Arc::new(AtomicBool::new(false));
        let worker_busy = busy.clone();
        std::thread::Builder::new()
            .name("app-data".to_owned())
            .stack_size(DATA_ACTION_STACK_BYTES)
            .spawn(move || {
                let configs = configs
                    .into_iter()
                    .map(|config| (config.app_id.clone(), config))
                    .collect::<BTreeMap<_, _>>();
                let mut runtimes = BTreeMap::<String, DataActionRuntime>::new();
                while let Ok(request) = rx.recv() {
                    worker_busy.store(true, Ordering::Release);
                    let result = (|| -> Result<ToolResult> {
                        if !runtimes.contains_key(&request.app_id) {
                            let config = configs
                                .get(&request.app_id)
                                .ok_or_else(|| anyhow!("{} has no Data Action", request.app_id))?;
                            runtimes.insert(
                                request.app_id.clone(),
                                DataActionRuntime::load(config, services.clone())?,
                            );
                        }
                        runtimes
                            .get(&request.app_id)
                            .expect("Data Action runtime inserted")
                            .invoke(&request)
                    })();
                    match result {
                        Ok(result) if result.is_error => log::warn!(
                            "App Data Action run={} {}.{} failed: {}",
                            request.run_id,
                            request.app_id,
                            request.name,
                            result.text
                        ),
                        Ok(result) => log::info!(
                            "App Data Action run={} {}.{} completed: {}",
                            request.run_id,
                            request.app_id,
                            request.name,
                            result.text
                        ),
                        Err(error) => log::error!(
                            "App Data Action run={} {}.{} crashed: {error:#}",
                            request.run_id,
                            request.app_id,
                            request.name
                        ),
                    }
                    worker_busy.store(false, Ordering::Release);
                }
            })
            .context("start App Data Action runner")?;
        Ok(Self {
            tx,
            next_run_id: AtomicU32::new(1),
            busy,
        })
    }

    fn enqueue(&self, app_id: &str, kind: DataActionKind, name: &str, args: Value) -> Result<u64> {
        let run_id = u64::from(self.next_run_id.fetch_add(1, Ordering::Relaxed));
        self.tx
            .try_send(DataActionRequest {
                run_id,
                app_id: app_id.to_owned(),
                kind,
                name: name.to_owned(),
                args,
            })
            .map_err(|error| anyhow!("queue App Data Action: {error}"))?;
        Ok(run_id)
    }

    fn busy(&self) -> bool {
        self.busy.load(Ordering::Acquire)
    }
}

pub struct AppRuntime {
    descriptor: AppDescriptor,
    guest: Guest,
    surface: UiSurface,
    _fs: Rc<RefCell<FsModule>>,
    _db: SharedDb,
    revision: AppRevision,
    last_seen_revision: Cell<u32>,
    #[cfg(test)]
    projection_refreshes: Cell<u32>,
}

impl AppRuntime {
    fn load(
        app: &BuiltinApp,
        release_dir: &Path,
        fs_root: &Path,
        tmp_root: &Path,
        db: SharedDb,
        revision: AppRevision,
        services: Arc<dyn AppServiceHost>,
    ) -> Result<Self> {
        let descriptor: AppDescriptor = serde_json::from_slice(
            &std::fs::read(release_dir.join("agent-app.json")).context("read agent-app.json")?,
        )
        .context("parse installed agent-app.json")?;
        anyhow::ensure!(
            descriptor.id == app.descriptor.id,
            "installed App id mismatch"
        );
        let manifest: Value = serde_json::from_slice(
            &std::fs::read(release_dir.join("pocket.json")).context("read pocket.json")?,
        )
        .context("parse installed pocket.json")?;
        anyhow::ensure!(
            manifest.get("pocket").and_then(Value::as_u64) == Some(2),
            "App requires pocket.json v2"
        );
        anyhow::ensure!(
            manifest.get("name").and_then(Value::as_str) == Some(app.descriptor.id.as_str()),
            "pocket.json name does not match App id"
        );
        for capability in manifest
            .pointer("/engine/capabilities/requires")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            anyhow::ensure!(
                matches!(capability, "data.fs" | "data.sqlite"),
                "unsupported App capability: {capability}"
            );
        }

        std::fs::create_dir_all(fs_root)?;
        let guest = Guest::new()?;
        let surface = UiSurface::new(VIEWPORT);
        let pak = std::fs::read(release_dir.join("app.pak")).context("read App pak")?;
        surface.feed_pak(&pak);
        surface.mount(&guest)?;

        let fs = Rc::new(RefCell::new(FsModule::with_quota(
            FsStorage::Dir {
                root: fs_root.to_owned(),
                tmp: tmp_root.to_owned(),
            },
            2 * 1024 * 1024,
        )));
        pocket_fs::mount(&guest, fs.clone())?;
        mount_shared_db(&guest, db.clone())?;

        mount_services(&guest, descriptor.id.clone(), services)?;
        let source =
            std::fs::read_to_string(release_dir.join("app.js")).context("read App bundle")?;
        eval_bundle(&guest, &descriptor.id, &source)?;
        anyhow::ensure!(
            guest.has_frame(),
            "{} bundle installed no frame()",
            descriptor.id
        );

        let last_seen_revision = revision.load(Ordering::Acquire);
        Ok(Self {
            descriptor,
            guest,
            surface,
            _fs: fs,
            _db: db,
            revision,
            last_seen_revision: Cell::new(last_seen_revision),
            #[cfg(test)]
            projection_refreshes: Cell::new(0),
        })
    }

    pub fn id(&self) -> &str {
        &self.descriptor.id
    }

    pub fn frame(&self) -> Result<()> {
        self.advance(true)
    }

    fn projection_is_stale(&self) -> bool {
        self.revision.load(Ordering::Acquire) != self.last_seen_revision.get()
    }

    fn advance(&self, render_surface: bool) -> Result<()> {
        // A normal frame never queries SQLite. It only compares one in-memory
        // App revision and lets the View refresh a bounded projection when a
        // committed Data Action made that revision stale.
        self.call_method("tick", ()).map(|_: String| ())?;
        if render_surface {
            let current_revision = self.revision.load(Ordering::Acquire);
            if current_revision != self.last_seen_revision.get() {
                // The counter is sampled once at the foreground frame
                // boundary. Any number of commits since the previous frame is
                // therefore coalesced into one bounded projection refresh.
                // A commit racing this query remains visible as a newer
                // revision and is picked up on the following frame.
                self.call_method::<_, String>(
                    "dataChanged",
                    (json!([{"topic":"app","revision":current_revision}]).to_string(),),
                )?;
                self.last_seen_revision.set(current_revision);
                #[cfg(test)]
                self.projection_refreshes
                    .set(self.projection_refreshes.get().saturating_add(1));
            }
        }
        if render_surface {
            self.guest.frame(0)?;
            self.surface.tick();
        }
        Ok(())
    }

    pub fn update(&self, projection: &Value) -> Result<()> {
        self.call_method("update", (projection.to_string(),))
            .map(|_: String| ())
    }

    pub fn tap(&self, x: u16, y: u16) -> Result<Value> {
        let line: String = self.call_method("tap", (x as i32, y as i32))?;
        if line.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&line).context("parse App tap action")
    }

    pub fn invoke_tool(&self, name: &str, args_json: &str) -> Result<ToolResult> {
        let line: String =
            self.call_method("invokeTool", (name.to_owned(), args_json.to_owned()))?;
        self.tool_result(&line)
    }

    pub fn invoke_task(&self, name: &str, args: &Value) -> Result<ToolResult> {
        let line: String = self.call_method("invokeTask", (name.to_owned(), args.to_string()))?;
        self.tool_result(&line)
    }

    pub fn with_ui<R>(&self, f: impl FnOnce(&mut pocketjs_core::Ui) -> R) -> R {
        self.surface.with_ui(f)
    }

    fn tool_result(&self, line: &str) -> Result<ToolResult> {
        let value: Value = serde_json::from_str(line).context("parse App result")?;
        Ok(ToolResult {
            text: value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or(line)
                .to_owned(),
            details: value.get("details").cloned().unwrap_or(Value::Null),
            is_error: value
                .get("isError")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            terminate: false,
        })
    }

    fn call_method<A, R>(&self, name: &str, args: A) -> Result<R>
    where
        A: for<'js> pocket_mod::qjs::function::IntoArgs<'js>,
        R: for<'js> pocket_mod::qjs::FromJs<'js>,
    {
        self.guest.with(|ctx| {
            let app: Object = ctx
                .globals()
                .get("PocketPiApp")
                .map_err(|error| anyhow!("PocketPiApp missing: {error}"))?;
            let function: Function = app
                .get(name)
                .map_err(|error| anyhow!("PocketPiApp.{name} missing: {error}"))?;
            function
                .call::<_, R>(args)
                .catch(&ctx)
                .map_err(|error| anyhow!("PocketPiApp.{name}: {error}"))
        })
    }
}

#[cfg(not(target_os = "espidf"))]
fn eval_bundle(guest: &Guest, label: &str, source: &str) -> Result<()> {
    guest.eval(label, source)
}

#[cfg(target_os = "espidf")]
fn eval_bundle(guest: &Guest, label: &str, source: &str) -> Result<()> {
    guest.eval(label, source)
}

fn mount_shared_db(guest: &Guest, db: SharedDb) -> Result<()> {
    guest.mount("db", |ctx, ns| {
        let module = db.clone();
        ns.set(
            "open",
            Function::new(ctx.clone(), move |name: String| -> i32 {
                module.lock().map_or(-1, |mut db| db.open(&name))
            })?,
        )?;
        let module = db.clone();
        ns.set(
            "close",
            Function::new(ctx.clone(), move |handle: i32| {
                if let Ok(mut db) = module.lock() {
                    db.close(handle);
                }
            })?,
        )?;
        let module = db.clone();
        ns.set(
            "exec",
            Function::new(ctx.clone(), move |handle: i32, sql: String| -> i32 {
                module.lock().map_or(1, |mut db| db.exec(handle, &sql))
            })?,
        )?;
        let module = db.clone();
        ns.set(
            "query",
            Function::new(
                ctx.clone(),
                move |handle: i32, sql: String, args: String| -> String {
                    module.lock().map_or_else(
                        |_| json!({"error":"App database owner is unavailable"}).to_string(),
                        |mut db| db.query(handle, &sql, &args),
                    )
                },
            )?,
        )?;
        let module = db;
        ns.set(
            "lastError",
            Function::new(ctx.clone(), move |handle: i32| -> String {
                module.lock().map_or_else(
                    |_| "App database owner is unavailable".to_owned(),
                    |db| db.last_error(handle),
                )
            })?,
        )?;
        Ok(())
    })
}

fn mount_services(guest: &Guest, app_id: String, services: Arc<dyn AppServiceHost>) -> Result<()> {
    guest.mount("services", |ctx, ns| {
        ns.set(
            "call",
            Function::new(
                ctx.clone(),
                move |service: String, operation: String, args_json: String| -> String {
                    let args = serde_json::from_str(&args_json).unwrap_or(Value::Null);
                    match services.call(&app_id, &service, &operation, &args) {
                        Ok(value) => json!({"ok":true,"value":value}).to_string(),
                        Err(error) => json!({"ok":false,"error":error}).to_string(),
                    }
                },
            )?,
        )?;
        Ok(())
    })
}

fn mount_data_lifecycle(guest: &Guest, revision: AppRevision) -> Result<()> {
    guest.mount("app", |ctx, ns| {
        ns.set(
            "commit",
            Function::new(ctx.clone(), move || -> f64 {
                // Called only after a successful App-owned SQLite COMMIT.
                // Release pairs with the foreground View's Acquire load.
                revision.fetch_add(1, Ordering::Release).saturating_add(1) as f64
            })?,
        )?;
        Ok(())
    })
}

pub struct AppSupervisor {
    workspace: PathBuf,
    catalog: AppCatalog,
    services: Arc<dyn AppServiceHost>,
    data_runner: AppDataRunner,
    /// The Pi Agent System App is booted once and remains resident for the
    /// entire supervisor lifetime. Foreground navigation never replaces it.
    system: AppRuntime,
    agent: Option<GuestAgent>,
    /// The small v1 catalog is loaded once at supervisor startup. Background
    /// Data Actions do not advance these Views, and navigation only selects
    /// an already resident surface.
    runtimes: BTreeMap<String, AppRuntime>,
    active_app: Option<String>,
    schedules: AppScheduleStore,
}

impl AppSupervisor {
    pub fn new(workspace: impl Into<PathBuf>, services: Arc<dyn AppServiceHost>) -> Result<Self> {
        let workspace = workspace.into();
        let catalog = AppCatalog::builtin()?;
        seed_builtin_releases(&workspace, &catalog)?;
        let schedules = AppScheduleStore::load(&workspace, &catalog)?;
        let mut databases = BTreeMap::new();
        let mut revisions = BTreeMap::new();
        for descriptor in catalog.descriptors() {
            let (_, _, db_root, _) = paths(&workspace, &descriptor.id);
            std::fs::create_dir_all(&db_root)?;
            reset_development_database(&workspace, descriptor, &db_root)?;
            databases.insert(
                descriptor.id.clone(),
                Arc::new(Mutex::new(DbModule::new(DbStorage::Dir(db_root)))),
            );
            revisions.insert(descriptor.id.clone(), Arc::new(AtomicU32::new(0)));
        }
        let data_configs = catalog
            .descriptors()
            .filter_map(|descriptor| {
                let (release_dir, _, _, _) = paths(&workspace, &descriptor.id);
                let source_path = release_dir.join("data-action.js");
                source_path.is_file().then(|| DataAppConfig {
                    app_id: descriptor.id.clone(),
                    source_path,
                    database: databases
                        .get(&descriptor.id)
                        .expect("database created for descriptor")
                        .clone(),
                    revision: revisions
                        .get(&descriptor.id)
                        .expect("revision created for descriptor")
                        .clone(),
                })
            })
            .collect();
        let data_runner = AppDataRunner::start(data_configs, services.clone())?;
        log::info!("preloading View Runtime: {ROOT_APP_ID}");
        let system = load_runtime(
            &workspace,
            &catalog,
            &databases,
            &revisions,
            ROOT_APP_ID,
            services.clone(),
        )?;
        log::info!("preloaded View Runtime: {ROOT_APP_ID}");
        // v1 has a small fixed App catalog. Load every ordinary View once at
        // boot so foreground navigation is only a surface switch. Background
        // Data Actions remain separate and are still loaded on demand.
        let ordinary_app_ids = catalog
            .descriptors()
            .filter(|descriptor| descriptor.id != ROOT_APP_ID)
            .map(|descriptor| descriptor.id.clone())
            .collect::<Vec<_>>();
        let mut runtimes = BTreeMap::new();
        for app_id in ordinary_app_ids {
            log::info!("preloading View Runtime: {app_id}");
            let runtime = load_runtime(
                &workspace,
                &catalog,
                &databases,
                &revisions,
                &app_id,
                services.clone(),
            )?;
            log::info!("preloaded View Runtime: {app_id}");
            runtimes.insert(app_id, runtime);
        }
        Ok(Self {
            workspace,
            catalog,
            services,
            data_runner,
            system,
            agent: None,
            runtimes,
            active_app: None,
            schedules,
        })
    }

    pub fn catalog(&self) -> &AppCatalog {
        &self.catalog
    }

    pub fn services_busy(&self) -> bool {
        self.services.busy() || self.data_runner.busy()
    }

    pub fn active_id(&self) -> &str {
        self.active_app.as_deref().unwrap_or(ROOT_APP_ID)
    }

    pub fn open(&mut self, app_id: &str) -> Result<()> {
        if self.active_id() == app_id {
            return Ok(());
        }
        if app_id == ROOT_APP_ID {
            self.active_app = None;
            return Ok(());
        }

        anyhow::ensure!(self.runtimes.contains_key(app_id), "unknown App: {app_id}");
        self.active_app = Some(app_id.to_owned());
        Ok(())
    }

    pub fn boot_agent(
        &mut self,
        config_json: &str,
        backend: Arc<dyn ModelBackend>,
        tools: Arc<dyn ToolHost>,
    ) -> Result<()> {
        anyhow::ensure!(
            self.agent.is_none(),
            "Pi Agent System App is already booted"
        );
        let release = paths(&self.workspace, ROOT_APP_ID).0;
        let agent_source = std::fs::read_to_string(release.join("agent.js"))
            .context("read Pi Agent System App loop bundle")?;
        self.agent = Some(
            GuestAgent::mount_source(
                &self.system.guest,
                config_json,
                backend,
                tools,
                &agent_source,
                Arc::new(|_| {}),
            )
            .map_err(|error| anyhow!(error))?,
        );
        Ok(())
    }

    pub fn prompt_agent(&self, text: &str) -> Result<()> {
        let agent = self
            .agent
            .as_ref()
            .ok_or_else(|| anyhow!("Pi Agent System App is not booted"))?;
        agent
            .prompt(&self.system.guest, text)
            .map_err(|error| anyhow!(error))
    }

    pub fn abort_agent(&self) -> Result<()> {
        let agent = self
            .agent
            .as_ref()
            .ok_or_else(|| anyhow!("Pi Agent System App is not booted"))?;
        agent
            .abort(&self.system.guest)
            .map_err(|error| anyhow!(error))
    }

    /// Advance the resident System App and cheap View bookkeeping. Only the
    /// selected surface is rendered; Data Actions progress on their own runner.
    pub fn frame(&self) -> Result<Vec<AgentEvent>> {
        self.frame_render(true)
    }

    /// Advance the Agent every host tick, but only ask the selected PocketJS
    /// View to produce a new DrawList when the host knows it is dirty.
    pub fn frame_render(&self, render_selected: bool) -> Result<Vec<AgentEvent>> {
        let events = match &self.agent {
            Some(agent) => agent
                .tick(&self.system.guest)
                .map_err(|error| anyhow!(error))?,
            None => Vec::new(),
        };
        self.system
            .advance(render_selected && self.active_app.is_none())?;
        for (app_id, runtime) in &self.runtimes {
            runtime
                .advance(render_selected && self.active_app.as_deref() == Some(app_id.as_str()))?;
        }
        Ok(events)
    }

    pub fn update_root(&self, projection: &Value) -> Result<()> {
        self.system.update(projection)
    }

    pub fn tap(&self, x: u16, y: u16) -> Result<Value> {
        self.active().tap(x, y)
    }

    pub fn with_ui<R>(&self, f: impl FnOnce(&mut pocketjs_core::Ui) -> R) -> R {
        self.active().with_ui(f)
    }

    /// A single atomic comparison lets the host wake the active View after a
    /// background commit. It never queries SQLite and closed Apps stay idle.
    pub fn active_projection_is_stale(&self) -> bool {
        self.active().projection_is_stale()
    }

    pub fn invoke_tool(&mut self, name: &str, args_json: &str) -> ToolResult {
        let Some(app_id) = self.catalog.app_for_tool(name).map(str::to_owned) else {
            return tool_error(format!("unknown App tool: {name}"));
        };
        if name.ends_with(".storage_status") {
            return self
                .with_runtime(&app_id, |runtime| runtime.invoke_tool(name, args_json))
                .unwrap_or_else(|error| tool_error(format!("{name}: {error:#}")));
        }
        let args = serde_json::from_str(args_json).unwrap_or(Value::Null);
        match self
            .data_runner
            .enqueue(&app_id, DataActionKind::Tool, name, args)
        {
            Ok(run_id) => ToolResult {
                text: format!("Queued {name} as App Data Action {run_id}"),
                details: json!({"status":"queued","runId":run_id,"app":app_id}),
                is_error: false,
                terminate: false,
            },
            Err(error) => tool_error(format!("{name}: {error:#}")),
        }
    }

    /// Runs a task requested by the currently visible App. The host calls this
    /// after presenting the App's immediate pressed/loading state so slow
    /// native services never hide touch feedback.
    pub fn invoke_active_task(&mut self, name: &str, args: &Value) -> ToolResult {
        let app_id = self.active_id().to_owned();
        match self
            .data_runner
            .enqueue(&app_id, DataActionKind::Task, name, args.clone())
        {
            Ok(run_id) => ToolResult {
                text: format!("Queued {app_id}.{name} as App Data Action {run_id}"),
                details: json!({"status":"queued","runId":run_id,"app":app_id}),
                is_error: false,
                terminate: false,
            },
            Err(error) => tool_error(format!("{app_id}.{name}: {error:#}")),
        }
    }

    pub fn poll_due_tasks(&mut self) -> Vec<(String, ToolResult)> {
        let mut results = Vec::new();
        while let Some(due) = self.schedules.claim_due() {
            let label = format!("{}.{}", due.app_id, due.task);
            let result = match self.data_runner.enqueue(
                &due.app_id,
                DataActionKind::Task,
                &due.task,
                due.args.clone(),
            ) {
                Ok(run_id) => ToolResult {
                    text: format!("Queued {label} as App Data Action {run_id}"),
                    details: json!({"status":"queued","runId":run_id}),
                    is_error: false,
                    terminate: false,
                },
                Err(error) => tool_error(format!("{label}: {error:#}")),
            };
            self.schedules.finish(&due, !result.is_error);
            results.push((label, result));
        }
        results
    }

    fn with_runtime<R>(
        &mut self,
        app_id: &str,
        f: impl FnOnce(&AppRuntime) -> Result<R>,
    ) -> Result<R> {
        if app_id == ROOT_APP_ID {
            let result = f(&self.system)?;
            self.system.advance(self.active_app.is_none())?;
            return Ok(result);
        }
        let runtime = self
            .runtimes
            .get(app_id)
            .ok_or_else(|| anyhow!("unknown App: {app_id}"))?;
        let result = f(runtime)?;
        runtime.advance(self.active_app.as_deref() == Some(app_id))?;
        Ok(result)
    }

    fn active(&self) -> &AppRuntime {
        self.active_app
            .as_deref()
            .and_then(|app_id| self.runtimes.get(app_id))
            .unwrap_or(&self.system)
    }
}

fn load_runtime(
    workspace: &Path,
    catalog: &AppCatalog,
    databases: &BTreeMap<String, SharedDb>,
    revisions: &BTreeMap<String, AppRevision>,
    app_id: &str,
    services: Arc<dyn AppServiceHost>,
) -> Result<AppRuntime> {
    let app = catalog
        .app(app_id)
        .ok_or_else(|| anyhow!("unknown App: {app_id}"))?;
    let (release_dir, fs_root, db_root, tmp_root) = paths(workspace, app_id);
    let db = databases
        .get(app_id)
        .cloned()
        .ok_or_else(|| anyhow!("App {app_id} has no database owner"))?;
    let revision = revisions
        .get(app_id)
        .cloned()
        .ok_or_else(|| anyhow!("App {app_id} has no revision owner"))?;
    let _ = db_root;
    AppRuntime::load(
        app,
        &release_dir,
        &fs_root,
        &tmp_root,
        db,
        revision,
        services,
    )
    .with_context(|| format!("load App {app_id}"))
}

fn paths(workspace: &Path, app_id: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    if app_id == ROOT_APP_ID {
        (
            workspace.join("data/view/releases").join(BUILTIN_RELEASE),
            workspace.to_owned(),
            workspace.join("data"),
            workspace.join(".system/tmp/pi-agent"),
        )
    } else {
        let root = workspace.join("apps").join(app_id);
        (
            root.join("releases").join(BUILTIN_RELEASE),
            root.join("data"),
            root.join("data"),
            root.join("tmp"),
        )
    }
}

fn reset_development_database(
    workspace: &Path,
    descriptor: &AppDescriptor,
    db_root: &Path,
) -> Result<()> {
    if descriptor.data_version == 0 {
        return Ok(());
    }
    let marker = workspace
        .join("apps")
        .join(&descriptor.id)
        .join(".data-version");
    let expected = descriptor.data_version.to_string();
    if std::fs::read_to_string(&marker).is_ok_and(|value| value == expected) {
        return Ok(());
    }
    let database = db_root.join(format!("{}.sqlite", descriptor.id));
    for path in [
        database.clone(),
        database.with_extension("sqlite-wal"),
        database.with_extension("sqlite-shm"),
        database.with_extension("sqlite-journal"),
    ] {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).with_context(|| format!("reset {}", path.display())),
        }
    }
    atomic_write(&marker, expected.as_bytes())
}

fn seed_builtin_releases(workspace: &Path, catalog: &AppCatalog) -> Result<()> {
    for app in catalog.apps.values() {
        let (release_dir, fs_root, db_root, tmp_root) = paths(workspace, &app.descriptor.id);
        std::fs::create_dir_all(&release_dir)?;
        std::fs::create_dir_all(&fs_root)?;
        std::fs::create_dir_all(&db_root)?;
        std::fs::create_dir_all(&tmp_root)?;
        atomic_write(&release_dir.join("app.js"), app.js.as_bytes())?;
        if let Some(data_js) = app.data_js {
            atomic_write(&release_dir.join("data-action.js"), data_js.as_bytes())?;
        }
        if app.descriptor.id == ROOT_APP_ID {
            atomic_write(&release_dir.join("agent.js"), ROOT_AGENT_JS.as_bytes())?;
        }
        atomic_write(&release_dir.join("app.pak"), app.pak)?;
        atomic_write(
            &release_dir.join("agent-app.json"),
            app.descriptor_json.as_bytes(),
        )?;
        atomic_write(&release_dir.join("pocket.json"), app.pocket_json.as_bytes())?;
        atomic_write(
            &release_dir.join("plan.json"),
            &serde_json::to_vec_pretty(&json!({
                "runtime":"pocket-pi-agentos",
                "pocketjsRevision":"9c809bbd047ddc75c27caa4990951a78d942477a",
                "app":app.descriptor.id,
                "modules":["ui","data.fs","data.sqlite"]
            }))?,
        )?;
        let current = if app.descriptor.id == ROOT_APP_ID {
            workspace.join("data/view/current")
        } else {
            workspace
                .join("apps")
                .join(&app.descriptor.id)
                .join("current")
        };
        atomic_write(&current, BUILTIN_RELEASE.as_bytes())?;
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if std::fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(());
    }
    let parent = path.parent().ok_or_else(|| anyhow!("path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("release")
    ));
    {
        use std::io::Write as _;
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(&temporary, path)?;
    Ok(())
}

pub struct AppToolRequest {
    pub name: String,
    pub args_json: String,
    response: mpsc::Sender<ToolResult>,
}

pub struct RoutedToolHost {
    native: Arc<dyn ToolHost>,
    catalog: AppCatalog,
    app_tx: mpsc::Sender<AppToolRequest>,
}

impl RoutedToolHost {
    pub fn new(
        native: Arc<dyn ToolHost>,
        catalog: AppCatalog,
    ) -> (Self, mpsc::Receiver<AppToolRequest>) {
        let (app_tx, app_rx) = mpsc::channel();
        (
            Self {
                native,
                catalog,
                app_tx,
            },
            app_rx,
        )
    }
}

impl ToolHost for RoutedToolHost {
    fn definitions(&self) -> Vec<Value> {
        let mut definitions = self.native.definitions();
        definitions.extend(self.catalog.tool_definitions());
        definitions
    }

    fn execute(&self, call_id: &str, name: &str, args_json: &str) -> ToolResult {
        if self.catalog.app_for_tool(name).is_none() {
            return self.native.execute(call_id, name, args_json);
        }
        let (response, response_rx) = mpsc::channel();
        if self
            .app_tx
            .send(AppToolRequest {
                name: name.to_owned(),
                args_json: args_json.to_owned(),
                response,
            })
            .is_err()
        {
            return tool_error("App Supervisor is unavailable");
        }
        response_rx
            .recv_timeout(TOOL_TIMEOUT)
            .unwrap_or_else(|_| tool_error(format!("App Tool timed out after {TOOL_TIMEOUT:?}")))
    }
}

impl AppToolRequest {
    pub fn handle(self, supervisor: &mut AppSupervisor) {
        let result = supervisor.invoke_tool(&self.name, &self.args_json);
        let _ = self.response.send(result);
    }
}

fn tool_error(text: impl Into<String>) -> ToolResult {
    ToolResult {
        text: text.into(),
        details: Value::Null,
        is_error: true,
        terminate: false,
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredSchedule {
    app_id: String,
    schedule_id: String,
    task: String,
    args: Value,
    every_seconds: u64,
    next_run_at: u64,
    last_ok: Option<bool>,
}

struct DueTask {
    app_id: String,
    schedule_id: String,
    task: String,
    args: Value,
    scheduled_at: u64,
}

struct AppScheduleStore {
    paths: BTreeMap<String, PathBuf>,
    schedules: Vec<StoredSchedule>,
}

impl AppScheduleStore {
    fn load(workspace: &Path, catalog: &AppCatalog) -> Result<Self> {
        // AppTask declarations travel with the App release, while their
        // mutable scheduler cursor belongs to that App's private data root.
        // The former central file is read once only to migrate existing
        // installs; it is never written again.
        let legacy_path = workspace.join(".system/schedules.json");
        let mut legacy: Vec<StoredSchedule> = std::fs::read(&legacy_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let now = unix_seconds();
        let mut paths = BTreeMap::new();
        let mut schedules = Vec::new();
        for app in catalog.descriptors() {
            let path = if app.id == ROOT_APP_ID {
                workspace.join("data/.system/schedules.json")
            } else {
                workspace
                    .join("apps")
                    .join(&app.id)
                    .join("data/.system/schedules.json")
            };
            let mut prior: Vec<StoredSchedule> = std::fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            paths.insert(app.id.clone(), path);
            for declaration in &app.schedules {
                anyhow::ensure!(
                    app.tasks.iter().any(|task| task == &declaration.task),
                    "{}.{} schedule references missing task {}",
                    app.id,
                    declaration.id,
                    declaration.task
                );
                let every_seconds = declaration.every_minutes.saturating_mul(60).max(60);
                let existing = prior
                    .iter_mut()
                    .chain(legacy.iter_mut())
                    .find(|item| item.app_id == app.id && item.schedule_id == declaration.id);
                schedules.push(match existing {
                    Some(item)
                        if item.task == declaration.task && item.every_seconds == every_seconds =>
                    {
                        item.clone()
                    }
                    _ => StoredSchedule {
                        app_id: app.id.clone(),
                        schedule_id: declaration.id.clone(),
                        task: declaration.task.clone(),
                        args: declaration.args.clone(),
                        every_seconds,
                        next_run_at: now.saturating_add(every_seconds),
                        last_ok: None,
                    },
                });
            }
        }
        let store = Self { paths, schedules };
        store.persist()?;
        if legacy_path.exists() {
            std::fs::remove_file(&legacy_path)
                .with_context(|| format!("remove migrated {}", legacy_path.display()))?;
        }
        Ok(store)
    }

    fn claim_due(&mut self) -> Option<DueTask> {
        let now = unix_seconds();
        let item = self
            .schedules
            .iter_mut()
            .filter(|item| item.next_run_at <= now)
            .min_by_key(|item| item.next_run_at)?;
        let scheduled_at = item.next_run_at;
        while item.next_run_at <= now {
            item.next_run_at = item.next_run_at.saturating_add(item.every_seconds);
        }
        let due = DueTask {
            app_id: item.app_id.clone(),
            schedule_id: item.schedule_id.clone(),
            task: item.task.clone(),
            args: item.args.clone(),
            scheduled_at,
        };
        let _ = self.persist();
        Some(due)
    }

    fn finish(&mut self, due: &DueTask, ok: bool) {
        if let Some(item) = self
            .schedules
            .iter_mut()
            .find(|item| item.app_id == due.app_id && item.schedule_id == due.schedule_id)
        {
            item.last_ok = Some(ok);
        }
        log::info!(
            "AppTask {}.{} scheduled_at={} ok={ok}",
            due.app_id,
            due.task,
            due.scheduled_at
        );
        let _ = self.persist();
    }

    fn persist(&self) -> Result<()> {
        for (app_id, path) in &self.paths {
            let schedules = self
                .schedules
                .iter()
                .filter(|schedule| &schedule.app_id == app_id)
                .collect::<Vec<_>>();
            atomic_write(path, &serde_json::to_vec(&schedules)?)?;
        }
        Ok(())
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    struct NoServices;

    impl AppServiceHost for NoServices {
        fn call(
            &self,
            _app_id: &str,
            _service: &str,
            _operation: &str,
            _args: &Value,
        ) -> Result<Value, String> {
            Err("not used by Pi Agent UI tests".into())
        }
    }

    struct BackgroundBackend;

    impl ModelBackend for BackgroundBackend {
        fn complete(
            &self,
            _request_json: &str,
            on_delta: &mut dyn FnMut(&str),
        ) -> Result<String, String> {
            std::thread::sleep(Duration::from_millis(20));
            on_delta("background-ok");
            Ok(r#"{"text":"background-ok"}"#.to_owned())
        }
    }

    struct NoTools;

    impl ToolHost for NoTools {
        fn definitions(&self) -> Vec<Value> {
            Vec::new()
        }

        fn execute(&self, _call_id: &str, name: &str, _args_json: &str) -> ToolResult {
            tool_error(format!("unexpected tool: {name}"))
        }
    }

    struct ToolCallingBackend(AtomicUsize);

    impl ModelBackend for ToolCallingBackend {
        fn complete(
            &self,
            _request_json: &str,
            on_delta: &mut dyn FnMut(&str),
        ) -> Result<String, String> {
            if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                Ok(serde_json::json!({
                    "toolCall":{
                        "id":"call_portfolio",
                        "name":"robinhood.get_portfolio",
                        "arguments":{"account_number":"SIM-001"}
                    }
                })
                .to_string())
            } else {
                on_delta("tool-ok");
                Ok(r#"{"text":"tool-ok"}"#.to_owned())
            }
        }
    }

    struct RobinhoodServices;

    impl AppServiceHost for RobinhoodServices {
        fn call(
            &self,
            app_id: &str,
            service: &str,
            operation: &str,
            _args: &Value,
        ) -> Result<Value, String> {
            if (app_id, service, operation) == ("robinhood", "mcp.client", "callTool") {
                Ok(json!({"equity":"100.00","day_pnl":"1.00","week_pnl":"2.00"}))
            } else {
                Err("unexpected service call".to_owned())
            }
        }
    }

    #[test]
    fn catalog_exposes_namespaced_app_tools() {
        let catalog = AppCatalog::builtin().unwrap();
        let names = catalog
            .tool_definitions()
            .into_iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        assert!(names.contains(&"robinhood.get_portfolio".to_owned()));
        assert!(names.contains(&"research.search".to_owned()));
    }

    #[test]
    fn data_version_resets_only_that_app_database_once() {
        let temp = tempfile::tempdir().unwrap();
        let db_root = temp.path().join("apps/exa/data");
        std::fs::create_dir_all(&db_root).unwrap();
        let database = db_root.join("exa.sqlite");
        std::fs::write(&database, "old-schema").unwrap();
        let mut descriptor = AppDescriptor {
            id: EXA_APP_ID.to_owned(),
            version: "1.0.0".to_owned(),
            data_version: 3,
            tools: Vec::new(),
            tasks: Vec::new(),
            schedules: Vec::new(),
        };

        reset_development_database(temp.path(), &descriptor, &db_root).unwrap();
        assert!(!database.exists());
        std::fs::write(&database, "current-schema").unwrap();
        reset_development_database(temp.path(), &descriptor, &db_root).unwrap();
        assert_eq!(
            std::fs::read_to_string(&database).unwrap(),
            "current-schema"
        );

        descriptor.data_version = 4;
        reset_development_database(temp.path(), &descriptor, &db_root).unwrap();
        assert!(!database.exists());
    }

    #[test]
    fn app_task_schedule_state_is_owned_by_each_app_data_root() {
        let temp = tempfile::tempdir().unwrap();
        let legacy = temp.path().join(".system/schedules.json");
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(
            &legacy,
            serde_json::to_vec(&vec![StoredSchedule {
                app_id: ROBINHOOD_APP_ID.to_owned(),
                schedule_id: "portfolio-refresh".to_owned(),
                task: "refreshPortfolio".to_owned(),
                args: json!({}),
                every_seconds: 300,
                next_run_at: unix_seconds().saturating_add(123),
                last_ok: Some(true),
            }])
            .unwrap(),
        )
        .unwrap();

        let _supervisor = AppSupervisor::new(temp.path(), Arc::new(NoServices)).unwrap();
        let app_state = temp
            .path()
            .join("apps/robinhood/data/.system/schedules.json");
        let stored: Vec<StoredSchedule> =
            serde_json::from_slice(&std::fs::read(app_state).unwrap()).unwrap();

        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].app_id, ROBINHOOD_APP_ID);
        assert_eq!(stored[0].last_ok, Some(true));
        assert!(!legacy.exists());
    }

    #[test]
    fn root_view_keeps_legacy_tabs_and_adds_app_navigation() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("agent-note.txt"), "top-level workspace").unwrap();
        let supervisor = AppSupervisor::new(temp.path(), Arc::new(NoServices)).unwrap();
        supervisor
            .update_root(&json!({
                "agent":"IDLE",
                "model":"test",
                "messages":[{"role":"assistant","text":"ready"}],
            }))
            .unwrap();

        // Files remains a Pi Agent screen and its PocketJS FS is rooted at
        // the complete workspace, not only /workspace/data.
        assert_eq!(supervisor.tap(270, 1220).unwrap(), Value::Null);
        supervisor.frame().unwrap();

        // Apps is the only new bottom tab; its cards navigate through the
        // native App Supervisor instead of embedding another App's View.
        assert_eq!(supervisor.tap(450, 1220).unwrap(), Value::Null);
        let action = supervisor.tap(300, 220).unwrap();
        assert_eq!(action["type"], "navigate");
        assert_eq!(action["app"], ROBINHOOD_APP_ID);

        // Settings stays in the Root App, while the command is returned to
        // the host that owns Wi-Fi credentials and drivers.
        assert_eq!(supervisor.tap(630, 1220).unwrap(), Value::Null);
        let action = supervisor.tap(560, 170).unwrap();
        assert_eq!(action["type"], "settings");
        assert_eq!(action["command"], "scan");
    }

    #[test]
    fn pi_agent_system_app_survives_foreground_app_navigation() {
        let temp = tempfile::tempdir().unwrap();
        let mut supervisor = AppSupervisor::new(temp.path(), Arc::new(NoServices)).unwrap();
        assert_eq!(supervisor.runtimes.len(), 2, "all ordinary Views preload");
        let system_guest = &supervisor.system.guest as *const Guest as usize;
        let exa_guest = &supervisor
            .runtimes
            .get(EXA_APP_ID)
            .expect("Exa runtime preloaded")
            .guest as *const Guest as usize;
        supervisor
            .boot_agent(
                r#"{"model":"offline"}"#,
                Arc::new(BackgroundBackend),
                Arc::new(NoTools),
            )
            .unwrap();
        supervisor.prompt_agent("keep working").unwrap();
        supervisor.open(ROBINHOOD_APP_ID).unwrap();

        let mut saw_delta = false;
        let mut finished = false;
        for _ in 0..100 {
            for event in supervisor.frame().unwrap() {
                saw_delta |=
                    matches!(event, AgentEvent::ResponseText(ref text) if text == "background-ok");
                finished |= event == AgentEvent::Done;
            }
            if finished {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }

        assert!(saw_delta && finished);
        assert_eq!(supervisor.active_id(), ROBINHOOD_APP_ID);
        assert_eq!(
            &supervisor.system.guest as *const Guest as usize,
            system_guest
        );
        let robinhood_guest = &supervisor
            .runtimes
            .get(ROBINHOOD_APP_ID)
            .expect("Robinhood runtime")
            .guest as *const Guest as usize;
        supervisor.open(ROOT_APP_ID).unwrap();
        assert_eq!(supervisor.active_id(), ROOT_APP_ID);
        assert_eq!(
            &supervisor.system.guest as *const Guest as usize,
            system_guest
        );
        supervisor.open(ROBINHOOD_APP_ID).unwrap();
        assert_eq!(
            &supervisor
                .runtimes
                .get(ROBINHOOD_APP_ID)
                .expect("cached Robinhood runtime")
                .guest as *const Guest as usize,
            robinhood_guest
        );
        supervisor.open(EXA_APP_ID).unwrap();
        assert_eq!(
            &supervisor
                .runtimes
                .get(EXA_APP_ID)
                .expect("resident Exa runtime")
                .guest as *const Guest as usize,
            exa_guest
        );
    }

    #[test]
    fn background_agent_routes_headless_app_tool_while_exa_is_foreground() {
        let temp = tempfile::tempdir().unwrap();
        let mut supervisor = AppSupervisor::new(temp.path(), Arc::new(RobinhoodServices)).unwrap();
        let (routed, app_rx) = RoutedToolHost::new(Arc::new(NoTools), supervisor.catalog.clone());
        supervisor
            .boot_agent(
                r#"{"model":"offline"}"#,
                Arc::new(ToolCallingBackend(AtomicUsize::new(0))),
                Arc::new(routed),
            )
            .unwrap();
        supervisor.prompt_agent("check portfolio").unwrap();
        supervisor.open(EXA_APP_ID).unwrap();

        let mut finished = false;
        for _ in 0..200 {
            while let Ok(request) = app_rx.try_recv() {
                request.handle(&mut supervisor);
            }
            finished |= supervisor
                .frame()
                .unwrap()
                .into_iter()
                .any(|event| event == AgentEvent::Done);
            if finished {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }

        assert!(finished);
        assert_eq!(supervisor.active_id(), EXA_APP_ID);
        assert!(temp
            .path()
            .join("apps/robinhood/data/robinhood.sqlite")
            .exists());
    }

    #[test]
    fn app_revisions_coalesce_at_the_foreground_frame_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let mut supervisor = AppSupervisor::new(temp.path(), Arc::new(NoServices)).unwrap();
        supervisor.open(EXA_APP_ID).unwrap();

        let revision = supervisor
            .runtimes
            .get(EXA_APP_ID)
            .unwrap()
            .revision
            .clone();
        revision.fetch_add(1, Ordering::Release);
        revision.fetch_add(1, Ordering::Release);
        revision.fetch_add(1, Ordering::Release);
        assert!(supervisor.active_projection_is_stale());

        // A host tick that does not render the selected View never reloads its
        // SQLite projection. The next rendered frame observes only the newest
        // revision and therefore folds all three commits into one reload.
        supervisor.frame_render(false).unwrap();
        let exa = supervisor.runtimes.get(EXA_APP_ID).unwrap();
        assert_eq!(exa.projection_refreshes.get(), 0);
        assert_eq!(exa.last_seen_revision.get(), 0);

        supervisor.frame_render(true).unwrap();
        let exa = supervisor.runtimes.get(EXA_APP_ID).unwrap();
        assert_eq!(exa.projection_refreshes.get(), 1);
        assert_eq!(exa.last_seen_revision.get(), 3);
        assert!(!supervisor.active_projection_is_stale());

        for _ in 0..5 {
            supervisor.frame_render(true).unwrap();
        }
        assert_eq!(
            supervisor
                .runtimes
                .get(EXA_APP_ID)
                .unwrap()
                .projection_refreshes
                .get(),
            1
        );

        // Inactive Apps never query. They catch up exactly once when selected
        // again, no matter how many commits happened while they were hidden.
        supervisor.open(ROOT_APP_ID).unwrap();
        revision.fetch_add(1, Ordering::Release);
        revision.fetch_add(1, Ordering::Release);
        supervisor.frame_render(true).unwrap();
        assert_eq!(
            supervisor
                .runtimes
                .get(EXA_APP_ID)
                .unwrap()
                .projection_refreshes
                .get(),
            1
        );
        supervisor.open(EXA_APP_ID).unwrap();
        supervisor.frame_render(true).unwrap();
        let exa = supervisor.runtimes.get(EXA_APP_ID).unwrap();
        assert_eq!(exa.projection_refreshes.get(), 2);
        assert_eq!(exa.last_seen_revision.get(), 5);
    }
}
