//! Pocket Pi's App runtime: bounded PocketJS Guest caches, app-owned FS/SQLite
//! state, namespaced Agent tools and native App Schedule wakes.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context as _, Result};
use pocket_db::{DbModule, Storage as DbStorage};
use pocket_fs::{FsModule, Storage as FsStorage};
use pocket_mod::qjs::{CatchResultExt as _, Function, Object};
use pocket_mod::Guest;
pub use pocket_net::{HttpRequest, NetFailure, TransportCompletion};
use pocket_net::{HttpTransport, NetSurface};
use pocket_pi_embedded::{AgentEvent, GuestAgent, ModelBackend, ToolHost, ToolResult};
use pocket_ui_surface::UiSurface;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const ROOT_APP_ID: &str = "pi-agent";
pub const POCKETJS_REVISION: &str = "e12cf12f82cc60b636368119d49a06eb9ed2a3d5";
pub const SYSTEM_FRAMEWORK_API: u32 = 1;
const VIEWPORT: (f32, f32) = (720.0, 1280.0);
pub const MAX_POCKETAPP_BYTES: usize = 2 * 1024 * 1024;
const MAX_JSON_RESOURCE_BYTES: usize = 256 * 1024;
const MAX_RESOURCE_BYTES: usize = 512 * 1024;
const VIEW_RUNTIME_LIMIT: usize = 3;
const ACTION_RUNTIME_LIMIT: usize = 3;

fn take_runtime<T>(runtimes: &mut Vec<(String, T)>, app_id: &str) -> Option<T> {
    runtimes
        .iter()
        .position(|(cached_id, _)| cached_id == app_id)
        .map(|index| runtimes.remove(index).1)
}

fn make_runtime_room<T>(
    runtimes: &mut Vec<(String, T)>,
    limit: usize,
    protected_app: Option<&str>,
) {
    if runtimes.len() < limit {
        return;
    }
    let index = protected_app
        .and_then(|protected| runtimes.iter().position(|(app_id, _)| app_id != protected))
        .unwrap_or(0);
    runtimes.remove(index);
}

#[cfg(target_os = "espidf")]
struct PsramAllocator;

#[cfg(target_os = "espidf")]
unsafe impl pocket_mod::qjs::allocator::Allocator for PsramAllocator {
    fn alloc(&mut self, size: usize) -> *mut u8 {
        unsafe { heap_caps_malloc(size, PSRAM_CAPS).cast() }
    }

    fn calloc(&mut self, count: usize, size: usize) -> *mut u8 {
        unsafe { heap_caps_calloc(count, size, PSRAM_CAPS).cast() }
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8) {
        heap_caps_free(ptr.cast());
    }

    unsafe fn realloc(&mut self, ptr: *mut u8, new_size: usize) -> *mut u8 {
        heap_caps_realloc(ptr.cast(), new_size, PSRAM_CAPS).cast()
    }

    unsafe fn usable_size(ptr: *mut u8) -> usize {
        heap_caps_get_allocated_size(ptr.cast())
    }
}

#[cfg(target_os = "espidf")]
const PSRAM_CAPS: u32 = 4 | 1024;

#[cfg(target_os = "espidf")]
unsafe extern "C" {
    fn heap_caps_malloc(size: usize, caps: u32) -> *mut core::ffi::c_void;
    fn heap_caps_calloc(count: usize, size: usize, caps: u32) -> *mut core::ffi::c_void;
    fn heap_caps_realloc(
        ptr: *mut core::ffi::c_void,
        size: usize,
        caps: u32,
    ) -> *mut core::ffi::c_void;
    fn heap_caps_free(ptr: *mut core::ffi::c_void);
    fn heap_caps_get_allocated_size(ptr: *mut core::ffi::c_void) -> usize;
}

#[cfg(target_os = "espidf")]
fn new_app_guest() -> Result<Guest> {
    Guest::new_with_alloc(PsramAllocator)
}

#[cfg(not(target_os = "espidf"))]
fn new_app_guest() -> Result<Guest> {
    Guest::new()
}

// One end-to-end budget starts when the Agent routes an App Tool. Queueing,
// Action execution and native transport all consume this same deadline.
pub const APP_ACTION_TIMEOUT: Duration = Duration::from_secs(80);

fn new_action_deadline() -> Instant {
    Instant::now() + APP_ACTION_TIMEOUT
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppDescriptor {
    #[serde(default)]
    pub format: u32,
    #[serde(default)]
    pub framework_api: u32,
    pub id: String,
    #[serde(default)]
    pub title: String,
    pub description: String,
    pub version: String,
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub tool_namespace: String,
    #[serde(default)]
    pub tools: Vec<Value>,
    #[serde(default)]
    pub provider_operations: Vec<String>,
    #[serde(default)]
    pub schedules: Vec<AppSchedule>,
    #[serde(default)]
    pub native_services: NativeServices,
    #[serde(default)]
    pub resources: BTreeMap<String, AppResource>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppResource {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppSchedule {
    pub id: String,
    pub every_minutes: u64,
    pub action: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeServices {
    #[serde(default)]
    pub http: Vec<HttpServicePolicy>,
    #[serde(default)]
    pub mcp: Vec<McpServicePolicy>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialBinding {
    pub id: String,
    pub header: String,
    #[serde(default)]
    pub prefix: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpServicePolicy {
    pub method: String,
    pub urls: Vec<String>,
    #[serde(default)]
    pub allowed_request_headers: Vec<String>,
    pub credential: Option<CredentialBinding>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServicePolicy {
    pub connection: String,
    pub url: String,
    pub credential: CredentialBinding,
}

pub struct StagedApp {
    pub descriptor: AppDescriptor,
    pub release_dir: PathBuf,
    pub credentials: BTreeMap<String, String>,
}

pub fn stage_pocketapp(package: &Path, staging_dir: &Path) -> Result<StagedApp> {
    anyhow::ensure!(
        std::fs::metadata(package)
            .context("inspect .pocketapp package")?
            .len()
            <= MAX_POCKETAPP_BYTES as u64,
        "package exceeds 2 MiB"
    );
    let bytes = std::fs::read(package).context("read .pocketapp package")?;
    stage_pocketapp_bytes(&bytes, staging_dir)
}

pub fn stage_pocketapp_bytes(bytes: &[u8], staging_dir: &Path) -> Result<StagedApp> {
    anyhow::ensure!(bytes.len() <= MAX_POCKETAPP_BYTES, "package exceeds 2 MiB");
    anyhow::ensure!(
        !staging_dir.exists(),
        "App staging directory already exists"
    );
    std::fs::create_dir_all(staging_dir)?;
    let result = (|| -> Result<StagedApp> {
        let mut seen = BTreeSet::new();
        let mut credentials: BTreeMap<String, String> = BTreeMap::new();
        let mut total = 0usize;
        let mut offset = 0usize;
        let mut finished = false;
        while offset + 512 <= bytes.len() {
            let header = &bytes[offset..offset + 512];
            if header.iter().all(|byte| *byte == 0) {
                anyhow::ensure!(
                    bytes[offset..].iter().all(|byte| *byte == 0),
                    "App package contains data after its end marker"
                );
                finished = true;
                break;
            }
            anyhow::ensure!(
                header[156] == 0 || header[156] == b'0',
                "App package contains a non-file"
            );
            anyhow::ensure!(
                tar_octal(&header[148..156])?
                    == header
                        .iter()
                        .enumerate()
                        .map(|(index, byte)| {
                            if (148..156).contains(&index) {
                                u64::from(b' ')
                            } else {
                                u64::from(*byte)
                            }
                        })
                        .sum::<u64>(),
                "App package header checksum is invalid"
            );
            let name_end = header[..100]
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(100);
            let name = std::str::from_utf8(&header[..name_end])?;
            let path = package_file_path(name)?;
            anyhow::ensure!(
                seen.insert(name.to_owned()),
                "duplicate App package file: {name}"
            );
            let size = usize::try_from(tar_octal(&header[124..136])?)
                .context("App package file is too large")?;
            total = total.saturating_add(size);
            anyhow::ensure!(total <= MAX_POCKETAPP_BYTES, "App package exceeds 2 MiB");
            let data_start = offset + 512;
            let data_end = data_start
                .checked_add(size)
                .filter(|end| *end <= bytes.len())
                .ok_or_else(|| anyhow!("App package file is truncated"))?;
            let contents = &bytes[data_start..data_end];
            if name == "credentials.json" {
                credentials = serde_json::from_slice(contents).context("parse credentials.json")?;
                anyhow::ensure!(
                    credentials.len() <= 16
                        && credentials.iter().all(|(id, value)| !id.is_empty()
                            && !value.is_empty()
                            && value.len() <= 4096),
                    "credentials.json contains invalid credentials"
                );
            } else {
                let path = staging_dir.join(path);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut file = std::fs::File::create(path)?;
                file.write_all(contents)?;
                file.sync_all()?;
            }
            let padded = size
                .checked_add(511)
                .map(|value| value / 512 * 512)
                .ok_or_else(|| anyhow!("App package file is too large"))?;
            offset = data_start
                .checked_add(padded)
                .ok_or_else(|| anyhow!("App package is too large"))?;
        }
        anyhow::ensure!(finished, "App package is missing its end marker");
        anyhow::ensure!(seen.contains("app.json"), "App package is missing app.json");
        let descriptor_id: String =
            serde_json::from_slice::<Value>(&std::fs::read(staging_dir.join("app.json"))?)?["id"]
                .as_str()
                .ok_or_else(|| anyhow!("app.json is missing id"))?
                .to_owned();
        anyhow::ensure!(
            descriptor_id != ROOT_APP_ID,
            "the Pi Agent System App is built into Firmware"
        );
        let app = load_source_release(staging_dir, &descriptor_id)?;
        validate_package_credentials(&app.descriptor, &credentials)?;
        Ok(StagedApp {
            descriptor: app.descriptor,
            release_dir: staging_dir.to_owned(),
            credentials,
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(staging_dir);
    }
    result
}

fn package_file_path(name: &str) -> Result<PathBuf> {
    const ROOT_FILES: &[&str] = &[
        "app.json",
        "actions.js",
        "schema.sql",
        "view.js",
        "credentials.json",
    ];
    if ROOT_FILES.contains(&name) {
        return Ok(PathBuf::from(name));
    }
    let asset = name
        .strip_prefix("assets/")
        .filter(|path| !path.is_empty())
        .ok_or_else(|| anyhow!("unexpected App package file: {name}"))?;
    anyhow::ensure!(
        !name.contains('\\') && name.len() <= 100,
        "invalid App asset path: {name}"
    );
    for component in asset.split('/') {
        ensure_safe_component(component, "App asset path")?;
    }
    Ok(PathBuf::from(name))
}

fn tar_octal(field: &[u8]) -> Result<u64> {
    let text =
        std::str::from_utf8(field)?.trim_matches(|character| character == '\0' || character == ' ');
    anyhow::ensure!(
        !text.is_empty(),
        "App package contains invalid tar metadata"
    );
    u64::from_str_radix(text, 8).context("App package contains invalid tar metadata")
}

#[derive(Clone, Copy)]
pub struct SystemAppBundle {
    pub descriptor_json: &'static str,
    pub view_js: &'static str,
    pub text_js: &'static str,
    pub agent_js: &'static str,
}

impl SystemAppBundle {
    pub const fn new(
        descriptor_json: &'static str,
        view_js: &'static str,
        text_js: &'static str,
        agent_js: &'static str,
    ) -> Self {
        Self {
            descriptor_json,
            view_js,
            text_js,
            agent_js,
        }
    }
}

pub const fn system_app_bundle() -> SystemAppBundle {
    SystemAppBundle::new(
        include_str!("../../../apps/pi-agent/app.json"),
        include_str!("../../../apps/pi-agent/view.js"),
        include_str!("../../../apps/pi-agent/text.js"),
        include_str!("../../../apps/pi-agent/dist/agent.js"),
    )
}

pub const fn system_framework() -> &'static str {
    include_str!("../../../system/framework.js")
}

pub const fn system_view_sdk() -> &'static str {
    include_str!("../../../system/view-sdk.js")
}

pub const fn system_net_sdk() -> &'static str {
    include_str!("../../../system/net-sdk.js")
}

pub const fn system_view_pak() -> &'static [u8] {
    include_bytes!("../../../system/view-sdk.pak")
}

#[derive(Clone, Debug)]
struct InstalledApp {
    descriptor: AppDescriptor,
    release_dir: PathBuf,
    resources: Arc<str>,
}

#[derive(Clone, Debug)]
pub struct InstalledAppIndex {
    inner: Arc<RwLock<InstalledApps>>,
}

#[derive(Debug)]
struct InstalledApps {
    apps: BTreeMap<String, InstalledApp>,
    tool_routes: BTreeMap<String, ToolRoute>,
}

#[derive(Clone, Debug)]
struct ToolRoute {
    app_id: String,
    action: String,
}

impl InstalledAppIndex {
    pub fn load(workspace: &Path, system: SystemAppBundle) -> Result<Self> {
        seed_system_app(workspace, system)?;
        seed_system_runtime(workspace)?;
        let mut apps = BTreeMap::new();
        let root = load_system_release(&workspace.join("system/app"))?;
        apps.insert(ROOT_APP_ID.to_owned(), root);

        let apps_root = workspace.join("apps");
        if let Ok(entries) = std::fs::read_dir(&apps_root) {
            for entry in entries {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let app_id = entry
                    .file_name()
                    .to_str()
                    .ok_or_else(|| anyhow!("installed App id is not UTF-8"))?
                    .to_owned();
                let release = entry.path().join("release");
                if !release.is_dir() {
                    continue;
                }
                let installed = load_source_release(&release, &app_id)?;
                anyhow::ensure!(
                    apps.insert(app_id.clone(), installed).is_none(),
                    "duplicate installed App id: {app_id}"
                );
            }
        }

        let mut tool_routes = BTreeMap::new();
        for app in apps.values() {
            validate_app_tools(app, &tool_routes)?;
            add_app_tools(app, &mut tool_routes);
        }
        Ok(Self {
            inner: Arc::new(RwLock::new(InstalledApps { apps, tool_routes })),
        })
    }

    fn tool_definitions(&self) -> Vec<Value> {
        self.inner
            .read()
            .expect("installed App index lock")
            .apps
            .values()
            .flat_map(|app| app.descriptor.tools.iter().map(public_tool_definition))
            .collect()
    }

    fn route_for_tool(&self, name: &str) -> Option<ToolRoute> {
        self.inner
            .read()
            .expect("installed App index lock")
            .tool_routes
            .get(name)
            .cloned()
    }

    pub fn descriptors(&self) -> Vec<AppDescriptor> {
        self.inner
            .read()
            .expect("installed App index lock")
            .apps
            .values()
            .map(|app| app.descriptor.clone())
            .collect()
    }

    pub fn descriptor(&self, id: &str) -> Option<AppDescriptor> {
        self.inner
            .read()
            .expect("installed App index lock")
            .apps
            .get(id)
            .map(|app| app.descriptor.clone())
    }

    pub fn provider_operation_allowed(&self, app_id: &str, operation: &str) -> bool {
        self.inner
            .read()
            .expect("installed App index lock")
            .apps
            .get(app_id)
            .is_some_and(|app| {
                app.descriptor
                    .provider_operations
                    .iter()
                    .any(|item| item == operation)
            })
    }

    pub fn http_policy(&self, app_id: &str, method: &str, url: &str) -> Option<HttpServicePolicy> {
        self.inner
            .read()
            .expect("installed App index lock")
            .apps
            .get(app_id)?
            .descriptor
            .native_services
            .http
            .iter()
            .find(|policy| policy.method == method && policy.urls.iter().any(|item| item == url))
            .cloned()
    }

    pub fn mcp_policy(&self, app_id: &str, connection: &str) -> Option<McpServicePolicy> {
        self.inner
            .read()
            .expect("installed App index lock")
            .apps
            .get(app_id)?
            .descriptor
            .native_services
            .mcp
            .iter()
            .find(|policy| policy.connection == connection)
            .cloned()
    }

    pub fn credential_ids(&self) -> BTreeSet<String> {
        let mut ids = BTreeSet::new();
        for descriptor in self.descriptors() {
            ids.extend(
                descriptor
                    .native_services
                    .http
                    .iter()
                    .filter_map(|policy| policy.credential.as_ref())
                    .chain(
                        descriptor
                            .native_services
                            .mcp
                            .iter()
                            .map(|policy| &policy.credential),
                    )
                    .map(|credential| credential.id.clone()),
            );
        }
        ids
    }

    fn app(&self, id: &str) -> Option<InstalledApp> {
        self.inner
            .read()
            .expect("installed App index lock")
            .apps
            .get(id)
            .cloned()
    }

    fn apps(&self) -> Vec<InstalledApp> {
        self.inner
            .read()
            .expect("installed App index lock")
            .apps
            .values()
            .cloned()
            .collect()
    }

    fn validate_insert(&self, app: &InstalledApp) -> Result<()> {
        let index = self.inner.read().expect("installed App index lock");
        anyhow::ensure!(
            !index.apps.contains_key(&app.descriptor.id),
            "App {} is already installed",
            app.descriptor.id
        );
        validate_app_tools(app, &index.tool_routes)?;
        let installed_credentials = index
            .apps
            .values()
            .flat_map(|installed| descriptor_credential_ids(&installed.descriptor))
            .collect::<BTreeSet<_>>();
        anyhow::ensure!(
            descriptor_credential_ids(&app.descriptor).is_disjoint(&installed_credentials),
            "App {} reuses an installed credential id",
            app.descriptor.id
        );
        Ok(())
    }

    fn insert_validated(&self, app: InstalledApp) {
        let mut index = self.inner.write().expect("installed App index lock");
        add_app_tools(&app, &mut index.tool_routes);
        index.apps.insert(app.descriptor.id.clone(), app);
    }

    fn remove(&self, app_id: &str) -> Option<InstalledApp> {
        let mut index = self.inner.write().expect("installed App index lock");
        let app = index.apps.remove(app_id)?;
        index.tool_routes.retain(|_, route| route.app_id != app_id);
        Some(app)
    }
}

fn validate_app_tools(app: &InstalledApp, routes: &BTreeMap<String, ToolRoute>) -> Result<()> {
    for tool in &app.descriptor.tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("{} tool is missing name", app.descriptor.id))?;
        anyhow::ensure!(
            name.starts_with(&format!("{}.", app.descriptor.tool_namespace)),
            "App {} owns non-namespaced tool {name}",
            app.descriptor.id
        );
        let action = tool
            .get("action")
            .and_then(Value::as_str)
            .filter(|action| !action.is_empty())
            .ok_or_else(|| anyhow!("{name} tool is missing action"))?;
        anyhow::ensure!(!routes.contains_key(name), "duplicate App tool: {name}");
        anyhow::ensure!(
            !action.contains('.'),
            "App {} tool {name} has invalid Action {action}",
            app.descriptor.id
        );
    }
    Ok(())
}

fn add_app_tools(app: &InstalledApp, routes: &mut BTreeMap<String, ToolRoute>) {
    for tool in &app.descriptor.tools {
        let name = tool["name"]
            .as_str()
            .expect("validated App tool name")
            .to_owned();
        routes.insert(
            name,
            ToolRoute {
                app_id: app.descriptor.id.clone(),
                action: tool["action"]
                    .as_str()
                    .expect("validated App tool Action")
                    .to_owned(),
            },
        );
    }
}

fn public_tool_definition(tool: &Value) -> Value {
    let mut definition = tool.clone();
    if let Some(object) = definition.as_object_mut() {
        object.remove("action");
    }
    definition
}

fn declared_actions(descriptor: &AppDescriptor) -> BTreeSet<String> {
    descriptor
        .tools
        .iter()
        .filter_map(|tool| tool.get("action").and_then(Value::as_str))
        .chain(
            descriptor
                .schedules
                .iter()
                .map(|schedule| schedule.action.as_str()),
        )
        .map(str::to_owned)
        .collect()
}

fn validate_action_contract(descriptor: &AppDescriptor, runtime: &ActionRuntime) -> Result<()> {
    let available = runtime.action_names()?;
    let missing = declared_actions(descriptor)
        .difference(&available)
        .cloned()
        .collect::<Vec<_>>();
    anyhow::ensure!(
        missing.is_empty(),
        "App {} is missing declared Actions: {}",
        descriptor.id,
        missing.join(", ")
    );
    Ok(())
}

fn descriptor_credential_ids(descriptor: &AppDescriptor) -> BTreeSet<String> {
    descriptor
        .native_services
        .http
        .iter()
        .filter_map(|policy| policy.credential.as_ref())
        .chain(
            descriptor
                .native_services
                .mcp
                .iter()
                .map(|policy| &policy.credential),
        )
        .map(|credential| credential.id.clone())
        .collect()
}

fn validate_package_credentials(
    descriptor: &AppDescriptor,
    credentials: &BTreeMap<String, String>,
) -> Result<()> {
    anyhow::ensure!(
        credentials.keys().cloned().collect::<BTreeSet<_>>()
            == descriptor_credential_ids(descriptor),
        "credentials.json ids do not match app.json"
    );
    Ok(())
}

fn load_system_release(release_dir: &Path) -> Result<InstalledApp> {
    validate_system_release_root(release_dir)?;
    for required in ["app.json", "plan.json", "view.js", "text.js", "agent.js"] {
        anyhow::ensure!(
            release_dir.join(required).is_file(),
            "System App release is missing {required}"
        );
    }

    let mut descriptor: AppDescriptor = serde_json::from_slice(
        &std::fs::read(release_dir.join("app.json")).context("read installed app.json")?,
    )
    .context("parse installed app.json")?;
    anyhow::ensure!(
        descriptor.id == ROOT_APP_ID,
        "installed System App id mismatch"
    );
    anyhow::ensure!(descriptor.format == 1, "System App requires format 1");
    anyhow::ensure!(
        descriptor.framework_api == SYSTEM_FRAMEWORK_API,
        "System App requires unsupported Framework API {}",
        descriptor.framework_api
    );
    validate_descriptor(&mut descriptor)?;
    let plan: Value = serde_json::from_slice(
        &std::fs::read(release_dir.join("plan.json")).context("read installed plan.json")?,
    )
    .context("parse installed plan.json")?;
    anyhow::ensure!(
        plan.get("runtime").and_then(Value::as_str) == Some("pocket-pi-agentos")
            && plan.get("pocketjsRevision").and_then(Value::as_str) == Some(POCKETJS_REVISION)
            && plan.get("frameworkApi").and_then(Value::as_u64)
                == Some(u64::from(SYSTEM_FRAMEWORK_API))
            && plan.get("app").and_then(Value::as_str) == Some(descriptor.id.as_str()),
        "App {} plan.json does not target this runtime",
        descriptor.id
    );
    Ok(InstalledApp {
        descriptor,
        release_dir: release_dir.to_owned(),
        resources: Arc::from("{}"),
    })
}

fn validate_system_release_root(release_dir: &Path) -> Result<()> {
    let allowed = BTreeSet::from(["app.json", "plan.json", "view.js", "text.js", "agent.js"]);
    for entry in std::fs::read_dir(release_dir)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| anyhow!("App Bundle contains a non-UTF-8 file"))?
            .to_owned();
        anyhow::ensure!(
            entry.file_type()?.is_file() && allowed.contains(name.as_str()),
            "unexpected App Bundle file: {name}"
        );
    }
    Ok(())
}

fn load_source_release(release_dir: &Path, expected_id: &str) -> Result<InstalledApp> {
    for required in ["app.json", "schema.sql", "actions.js", "view.js"] {
        anyhow::ensure!(
            release_dir.join(required).is_file(),
            "App {expected_id} source is missing {required}"
        );
    }
    let mut descriptor: AppDescriptor = serde_json::from_slice(
        &std::fs::read(release_dir.join("app.json")).context("read source app.json")?,
    )
    .context("parse source app.json")?;
    anyhow::ensure!(descriptor.id == expected_id, "installed App id mismatch");
    anyhow::ensure!(
        descriptor.format == 1,
        "App {} requires format 1",
        descriptor.id
    );
    anyhow::ensure!(
        descriptor.framework_api == SYSTEM_FRAMEWORK_API,
        "App {} requires unsupported Framework API {}",
        descriptor.id,
        descriptor.framework_api
    );
    anyhow::ensure!(
        descriptor.schema_version > 0 && descriptor.schema_version <= i32::MAX as u32,
        "App {} has an invalid schemaVersion",
        descriptor.id
    );
    validate_descriptor(&mut descriptor)?;
    validate_source_root(release_dir)?;
    let resources = load_source_resources(release_dir, &descriptor.resources)?;
    Ok(InstalledApp {
        descriptor,
        release_dir: release_dir.to_owned(),
        resources: resources.into(),
    })
}

fn validate_descriptor(descriptor: &mut AppDescriptor) -> Result<()> {
    ensure_safe_component(&descriptor.id, "App id")?;
    anyhow::ensure!(
        !descriptor.title.is_empty(),
        "App {} has no title",
        descriptor.id
    );
    anyhow::ensure!(
        !descriptor.version.is_empty(),
        "App {} has no version",
        descriptor.id
    );
    if descriptor.tool_namespace.is_empty() {
        descriptor.tool_namespace.clone_from(&descriptor.id);
    }
    let provider_operations = descriptor
        .provider_operations
        .iter()
        .collect::<BTreeSet<_>>();
    anyhow::ensure!(
        provider_operations.len() == descriptor.provider_operations.len()
            && provider_operations
                .iter()
                .all(|operation| !operation.is_empty()),
        "App {} declares invalid provider operations",
        descriptor.id
    );
    let capabilities = descriptor.capabilities.iter().collect::<BTreeSet<_>>();
    anyhow::ensure!(
        capabilities.len() == descriptor.capabilities.len()
            && capabilities.iter().all(|capability| {
                matches!(capability.as_str(), "data.fs" | "data.sqlite" | "net.http")
            }),
        "App {} declares unsupported capabilities",
        descriptor.id
    );
    for schedule in &descriptor.schedules {
        anyhow::ensure!(
            !schedule.action.is_empty() && !schedule.action.contains('.'),
            "{}.{} schedule has invalid Action {}",
            descriptor.id,
            schedule.id,
            schedule.action
        );
    }
    Ok(())
}

fn validate_source_root(release_dir: &Path) -> Result<()> {
    let allowed = BTreeSet::from(["app.json", "schema.sql", "actions.js", "view.js", "assets"]);
    for entry in std::fs::read_dir(release_dir)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| anyhow!("source App contains a non-UTF-8 file"))?
            .to_owned();
        anyhow::ensure!(
            allowed.contains(name.as_str()),
            "unexpected source App file: {name}"
        );
        let kind = entry.file_type()?;
        anyhow::ensure!(
            (name == "assets" && kind.is_dir()) || (name != "assets" && kind.is_file()),
            "invalid source App file: {name}"
        );
    }
    Ok(())
}

fn load_source_resources(
    release_dir: &Path,
    declarations: &BTreeMap<String, AppResource>,
) -> Result<String> {
    let actual = collect_asset_files(&release_dir.join("assets"))?;
    let declared = declarations
        .values()
        .map(|resource| resource.path.clone())
        .collect::<BTreeSet<_>>();
    anyhow::ensure!(
        declared.len() == declarations.len() && actual == declared,
        "source App assets must exactly match app.json resources"
    );
    let mut resources = BTreeMap::new();
    let mut total = 0usize;
    for (name, resource) in declarations {
        ensure_safe_component(name, "App resource name")?;
        validate_resource_path(&resource.path)?;
        anyhow::ensure!(
            resource.kind == "json",
            "unsupported App resource type: {}",
            resource.kind
        );
        let path = release_dir.join(&resource.path);
        let size = usize::try_from(std::fs::metadata(&path)?.len())?;
        anyhow::ensure!(
            size <= MAX_JSON_RESOURCE_BYTES,
            "App JSON resource is too large"
        );
        total = total.saturating_add(size);
        anyhow::ensure!(total <= MAX_RESOURCE_BYTES, "App resources are too large");
        resources.insert(
            name.clone(),
            serde_json::from_slice::<Value>(&std::fs::read(path)?)
                .with_context(|| format!("parse App resource {name}"))?,
        );
    }
    Ok(serde_json::to_string(&resources)?)
}

fn collect_asset_files(root: &Path) -> Result<BTreeSet<String>> {
    if !root.exists() {
        return Ok(BTreeSet::new());
    }
    let mut pending = vec![(root.to_owned(), "assets".to_owned())];
    let mut files = BTreeSet::new();
    while let Some((directory, prefix)) = pending.pop() {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .to_str()
                .ok_or_else(|| anyhow!("App asset path is not UTF-8"))?
                .to_owned();
            ensure_safe_component(&name, "App asset path")?;
            let path = format!("{prefix}/{name}");
            let kind = entry.file_type()?;
            if kind.is_dir() {
                pending.push((entry.path(), path));
            } else {
                anyhow::ensure!(kind.is_file(), "App asset is not a regular file: {path}");
                files.insert(path);
            }
        }
    }
    Ok(files)
}

fn validate_resource_path(path: &str) -> Result<()> {
    let normalized = package_file_path(path)?;
    anyhow::ensure!(
        path.starts_with("assets/") && normalized == Path::new(path),
        "invalid App resource path: {path}"
    );
    Ok(())
}

fn ensure_safe_component(value: &str, label: &str) -> Result<()> {
    anyhow::ensure!(
        !value.is_empty()
            && value != "."
            && value != ".."
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')),
        "invalid {label}: {value}"
    );
    Ok(())
}

/// The native transport/security boundary used by Apps. Implementations
/// own TLS, credentials and MCP sessions; Apps own operation selection,
/// normalization, SQLite and View behavior.
pub trait AppServiceHost: Send + Sync {
    /// Execute one policy-checked synchronous service call without outliving
    /// the App Action's absolute deadline.
    fn call(
        &self,
        app_id: &str,
        service: &str,
        operation: &str,
        args: &Value,
        deadline: Instant,
    ) -> Result<Value, String>;

    /// Execute one policy-checked PocketJS HTTP request. This is called only
    /// from the native NET worker, never from the QuickJS Action thread.
    fn http(
        &self,
        _app_id: &str,
        request: HttpRequest,
        _deadline: Instant,
    ) -> std::result::Result<TransportCompletion, NetFailure> {
        Err(NetFailure::new(
            "unavailable",
            format!("HTTP is unavailable for handle {}", request.handle),
        ))
    }

    /// True while a native App worker owns network/TLS activity.
    fn busy(&self) -> bool {
        false
    }

    fn store_credentials(
        &self,
        credentials: &BTreeMap<String, String>,
    ) -> std::result::Result<(), String> {
        if credentials.is_empty() {
            Ok(())
        } else {
            Err("App credential storage is unavailable".into())
        }
    }

    fn remove_app_state(
        &self,
        _app_id: &str,
        _credential_ids: &[String],
    ) -> std::result::Result<(), String> {
        Ok(())
    }
}

/// One SQLite module instance per App. View and background Action guests
/// share this owner, so ESP32's `unix-none` VFS never has two independent
/// connections racing on the same file. Network waits never hold this mutex;
/// only bounded SQLite operations and transactions do.
type SharedDb = Arc<Mutex<DbModule>>;
type AppRevision = Arc<AtomicU32>;

const ACTION_QUEUE: usize = 8;
const NET_COMPLETION_QUEUE: usize = 2;
const NET_WORKER_STACK_BYTES: usize = 96 * 1024;
pub const ACTION_STACK_BYTES: usize = 192 * 1024;

#[derive(Clone, Copy)]
enum ActionSource {
    Tool,
    Ui,
    Schedule,
}

impl ActionSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Tool => "tool",
            Self::Ui => "ui",
            Self::Schedule => "schedule",
        }
    }
}

struct ActionRequest {
    run_id: u64,
    app_id: String,
    source: ActionSource,
    action: String,
    args: Value,
    deadline: Instant,
    completion: ActionCompletion,
}

enum ActionCompletion {
    None,
    Response(mpsc::Sender<ToolResult>),
    Schedule(String),
}

enum ActionMessage {
    Run(ActionRequest),
    RemoveApp {
        app_id: String,
        done: mpsc::Sender<Result<()>>,
    },
}

struct ScheduleActionResult {
    app_id: String,
    schedule_id: String,
    result: ToolResult,
}

#[derive(Clone)]
struct ActionConfig {
    app_id: String,
    source_path: PathBuf,
    framework: Arc<str>,
    net_sdk: Option<Arc<str>>,
    resources: Arc<str>,
    database: SharedDb,
    revision: AppRevision,
    net: bool,
}

struct AppHttpRequest {
    request: HttpRequest,
    deadline: Instant,
}

struct AppNetTransport {
    requests: mpsc::SyncSender<AppHttpRequest>,
    completions: mpsc::Receiver<TransportCompletion>,
    cancelled: BTreeSet<i32>,
    action_deadline: Rc<Cell<Option<Instant>>>,
}

impl AppNetTransport {
    fn start(
        app_id: String,
        services: Arc<dyn AppServiceHost>,
        action_deadline: Rc<Cell<Option<Instant>>>,
    ) -> Result<Self> {
        let (request_tx, request_rx) = mpsc::sync_channel::<AppHttpRequest>(NET_COMPLETION_QUEUE);
        let (completion_tx, completion_rx) =
            mpsc::sync_channel::<TransportCompletion>(NET_COMPLETION_QUEUE);
        let worker_name = format!("net-{app_id}");
        std::thread::Builder::new()
            .name(worker_name)
            .stack_size(NET_WORKER_STACK_BYTES)
            .spawn(move || {
                while let Ok(mut work) = request_rx.recv() {
                    let handle = work.request.handle;
                    let completion = match remaining_timeout_ms(work.deadline) {
                        Ok(remaining_ms) => {
                            work.request.timeout_ms = work.request.timeout_ms.min(remaining_ms);
                            services
                                .http(&app_id, work.request, work.deadline)
                                .unwrap_or_else(|failure| TransportCompletion::Error {
                                    handle,
                                    failure,
                                })
                        }
                        Err(failure) => TransportCompletion::Error { handle, failure },
                    };
                    if completion_tx.send(completion).is_err() {
                        break;
                    }
                }
            })
            .context("start App NET worker")?;
        Ok(Self {
            requests: request_tx,
            completions: completion_rx,
            cancelled: BTreeSet::new(),
            action_deadline,
        })
    }
}

impl HttpTransport for AppNetTransport {
    fn start(&mut self, mut request: HttpRequest) -> std::result::Result<(), NetFailure> {
        let deadline = self.action_deadline.get().ok_or_else(|| {
            NetFailure::new("unavailable", "HTTP request has no active App Action")
        })?;
        let remaining_ms = remaining_timeout_ms(deadline)?;
        request.timeout_ms = request.timeout_ms.min(remaining_ms);
        self.requests
            .try_send(AppHttpRequest { request, deadline })
            .map_err(|_| NetFailure::new("busy", "native HTTP worker queue is full"))
    }

    fn cancel(&mut self, handle: i32) {
        self.cancelled.insert(handle);
    }

    fn drain(&mut self, completions: &mut Vec<TransportCompletion>) {
        while let Ok(completion) = self.completions.try_recv() {
            let handle = match &completion {
                TransportCompletion::Done { handle, .. }
                | TransportCompletion::Error { handle, .. } => *handle,
            };
            if !self.cancelled.remove(&handle) {
                completions.push(completion);
            }
        }
    }
}

fn remaining_timeout_ms(deadline: Instant) -> std::result::Result<u32, NetFailure> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| NetFailure::new("timeout", "App Action deadline expired"))?;
    Ok(remaining.as_millis().clamp(1, u128::from(u32::MAX)) as u32)
}

struct ActionRuntime {
    guest: Guest,
    net: Option<NetSurface<AppNetTransport>>,
    action_deadline: Rc<Cell<Option<Instant>>>,
    _database: SharedDb,
    _revision: AppRevision,
}

impl ActionRuntime {
    fn load(config: &ActionConfig, services: Arc<dyn AppServiceHost>) -> Result<Self> {
        let guest = new_app_guest().context("create App Action Guest")?;
        mount_db(&guest, config.database.clone(), true)?;
        let action_deadline = Rc::new(Cell::new(None));
        mount_data_lifecycle(&guest, config.revision.clone(), action_deadline.clone())?;
        let net = if config.net {
            let surface = NetSurface::new(AppNetTransport::start(
                config.app_id.clone(),
                services,
                action_deadline.clone(),
            )?);
            surface.mount(&guest)?;
            Some(surface)
        } else {
            mount_services(
                &guest,
                config.app_id.clone(),
                services,
                action_deadline.clone(),
            )?;
            None
        };
        install_system_framework(&guest, &config.app_id, &config.framework, &config.resources)?;
        if let Some(net_sdk) = &config.net_sdk {
            guest.eval("pocket-pi-net-sdk", net_sdk)?;
        }
        let source = std::fs::read_to_string(&config.source_path)
            .with_context(|| format!("read {} Action", config.app_id))?;
        guest.eval(&format!("{}-actions", config.app_id), &source)?;
        let runtime = Self {
            guest,
            net,
            action_deadline,
            _database: config.database.clone(),
            _revision: config.revision.clone(),
        };
        Ok(runtime)
    }

    fn action_names(&self) -> Result<BTreeSet<String>> {
        let line: String = self.guest.with(|ctx| -> Result<String> {
            let system: Object = ctx.globals().get("PocketPiSystem")?;
            let names: Function = system.get("actionNames")?;
            Ok(names.call::<_, String>(())?)
        })?;
        Ok(serde_json::from_str::<Vec<String>>(&line)?
            .into_iter()
            .collect())
    }

    fn invoke(&self, request: &ActionRequest) -> Result<ToolResult> {
        anyhow::ensure!(
            Instant::now() < request.deadline,
            "{} timed out before its Action started",
            request.action
        );
        self.action_deadline.set(Some(request.deadline));
        let result = self.invoke_action(request);
        self.action_deadline.set(None);
        result
    }

    fn invoke_action(&self, request: &ActionRequest) -> Result<ToolResult> {
        self.guest.with(|ctx| -> Result<()> {
            let system: Object = ctx.globals().get("PocketPiSystem")?;
            let begin: Function = system.get("beginAction")?;
            begin.call::<_, ()>((json!({
                "action": request.action,
                "args": request.args,
                "source": request.source.as_str(),
            })
            .to_string(),))?;
            Ok(())
        })?;
        loop {
            if let Some(net) = &self.net {
                net.begin_tick();
            }
            let line = self.guest.with(|ctx| -> Result<Option<String>> {
                let system: Object = ctx.globals().get("PocketPiSystem")?;
                let tick: Function = system.get("tickAction")?;
                tick.call::<_, ()>(())?;
                let poll: Function = system.get("pollActionResult")?;
                Ok(poll.call::<_, Option<String>>(())?)
            })?;
            self.guest.drain_jobs();
            if let Some(line) = line {
                return parse_action_result(&line);
            }
            anyhow::ensure!(
                Instant::now() < request.deadline,
                "Action {} timed out",
                request.action
            );
            yield_scheduler_tick();
        }
    }
}

fn parse_action_result(line: &str) -> Result<ToolResult> {
    let value: Value = serde_json::from_str(line).context("parse Action result")?;
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

struct ActionRunner {
    tx: mpsc::SyncSender<ActionMessage>,
    configs: Arc<Mutex<BTreeMap<String, ActionConfig>>>,
    next_run_id: AtomicU32,
    pending: Arc<AtomicU32>,
    schedule_results: mpsc::Receiver<ScheduleActionResult>,
}

impl ActionRunner {
    fn start(configs: Vec<ActionConfig>, services: Arc<dyn AppServiceHost>) -> Result<Self> {
        let (tx, rx) = mpsc::sync_channel::<ActionMessage>(ACTION_QUEUE);
        let (schedule_tx, schedule_results) = mpsc::channel();
        let configs = Arc::new(Mutex::new(
            configs
                .into_iter()
                .map(|config| (config.app_id.clone(), config))
                .collect::<BTreeMap<_, _>>(),
        ));
        let worker_configs = configs.clone();
        let pending = Arc::new(AtomicU32::new(0));
        let worker_pending = pending.clone();
        std::thread::Builder::new()
            .name("app-actions".to_owned())
            .stack_size(ACTION_STACK_BYTES)
            .spawn(move || {
                let mut runtimes = Vec::<(String, ActionRuntime)>::new();
                while let Ok(message) = rx.recv() {
                    let request = match message {
                        ActionMessage::Run(request) => request,
                        ActionMessage::RemoveApp { app_id, done } => {
                            let result = worker_configs
                                .lock()
                                .map_err(|_| anyhow!("App Action config lock was poisoned"))
                                .map(|mut configs| {
                                    configs.remove(&app_id);
                                    runtimes
                                        .retain(|(runtime_app_id, _)| runtime_app_id != &app_id);
                                });
                            let _ = done.send(result);
                            continue;
                        }
                    };
                    let result = (|| -> Result<ToolResult> {
                        let runtime = match take_runtime(&mut runtimes, &request.app_id) {
                            Some(runtime) => runtime,
                            None => {
                                make_runtime_room(&mut runtimes, ACTION_RUNTIME_LIMIT, None);
                                let config = worker_configs
                                    .lock()
                                    .map_err(|_| anyhow!("App Action config lock was poisoned"))?
                                    .get(&request.app_id)
                                    .cloned()
                                    .ok_or_else(|| anyhow!("{} has no Action", request.app_id))?;
                                ActionRuntime::load(&config, services.clone())?
                            }
                        };
                        let result = runtime.invoke(&request);
                        if result.is_ok() || Instant::now() < request.deadline {
                            runtimes.push((request.app_id.clone(), runtime));
                        }
                        result
                    })();
                    let result = match result {
                        Ok(result) if result.is_error => {
                            log::warn!(
                                "App Action run={} {}.{} failed: {}",
                                request.run_id,
                                request.app_id,
                                request.action,
                                result.text
                            );
                            result
                        }
                        Ok(result) => result,
                        Err(error) => {
                            log::error!(
                                "App Action run={} {}.{} crashed: {error:#}",
                                request.run_id,
                                request.app_id,
                                request.action
                            );
                            tool_error(format!("{}: {error:#}", request.action))
                        }
                    };
                    match request.completion {
                        ActionCompletion::Schedule(schedule_id) => {
                            let _ = schedule_tx.send(ScheduleActionResult {
                                app_id: request.app_id.clone(),
                                schedule_id,
                                result,
                            });
                        }
                        ActionCompletion::Response(response) => {
                            let _ = response.send(result);
                        }
                        ActionCompletion::None => {}
                    }
                    worker_pending.fetch_sub(1, Ordering::AcqRel);
                }
            })
            .context("start App Action runner")?;
        Ok(Self {
            tx,
            configs,
            next_run_id: AtomicU32::new(1),
            pending,
            schedule_results,
        })
    }

    fn enqueue(
        &self,
        app_id: &str,
        source: ActionSource,
        action: &str,
        args: Value,
        deadline: Instant,
        completion: ActionCompletion,
    ) -> Result<u64> {
        let run_id = u64::from(self.next_run_id.fetch_add(1, Ordering::Relaxed));
        self.pending.fetch_add(1, Ordering::AcqRel);
        if let Err(error) = self.tx.try_send(ActionMessage::Run(ActionRequest {
            run_id,
            app_id: app_id.to_owned(),
            source,
            action: action.to_owned(),
            args,
            deadline,
            completion,
        })) {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            return Err(anyhow!("queue App Action: {error}"));
        }
        Ok(run_id)
    }

    fn busy(&self) -> bool {
        self.pending.load(Ordering::Acquire) != 0
    }

    fn drain_schedule_results(&self) -> Vec<ScheduleActionResult> {
        self.schedule_results.try_iter().collect()
    }

    fn register(&self, config: ActionConfig) -> Result<()> {
        let mut configs = self
            .configs
            .lock()
            .map_err(|_| anyhow!("App Action config lock was poisoned"))?;
        anyhow::ensure!(
            !configs.contains_key(&config.app_id),
            "App {} Action is already registered",
            config.app_id
        );
        configs.insert(config.app_id.clone(), config);
        Ok(())
    }

    fn unregister(&self, app_id: &str) {
        if let Ok(mut configs) = self.configs.lock() {
            configs.remove(app_id);
        }
    }

    fn remove_app(&self, app_id: &str) -> Result<()> {
        let (done, response) = mpsc::channel();
        self.tx
            .send(ActionMessage::RemoveApp {
                app_id: app_id.to_owned(),
                done,
            })
            .context("stop App Action runtime")?;
        response.recv().context("App Action runner stopped")?
    }
}

#[derive(Clone)]
struct AppRuntimeAssets {
    framework: Arc<str>,
    net_sdk: Arc<str>,
    view_sdk: Arc<str>,
    view_pak: PathBuf,
}

struct ViewRuntime {
    guest: Guest,
    surface: UiSurface,
    _fs: Rc<RefCell<FsModule>>,
    _db: SharedDb,
    revision: AppRevision,
    last_seen_revision: Cell<u32>,
    #[cfg(test)]
    projection_refreshes: Cell<u32>,
    #[cfg(test)]
    ticks: Cell<u32>,
}

impl ViewRuntime {
    fn load(
        app: &InstalledApp,
        assets: &AppRuntimeAssets,
        fs_root: &Path,
        tmp_root: &Path,
        db: SharedDb,
        revision: AppRevision,
    ) -> Result<Self> {
        std::fs::create_dir_all(fs_root)?;
        let guest = new_app_guest().context("create App View Guest")?;
        let surface = UiSurface::new(VIEWPORT);
        let system = app.descriptor.id == ROOT_APP_ID;
        let pak = std::fs::read(&assets.view_pak).context("read Pocket Pi View resources")?;
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
        mount_db(&guest, db.clone(), false)?;
        install_system_framework(
            &guest,
            &app.descriptor.id,
            &assets.framework,
            &app.resources,
        )?;
        guest.eval("pocket-pi-view-sdk", &assets.view_sdk)?;
        if system {
            let text = std::fs::read_to_string(app.release_dir.join("text.js"))
                .context("read pi-agent text.js")?;
            guest.eval("pi-agent-text.js", &text)?;
        }
        let source = std::fs::read_to_string(app.release_dir.join("view.js"))
            .with_context(|| format!("read {} view.js", app.descriptor.id))?;
        guest.eval(&format!("{}-view.js", app.descriptor.id), &source)?;
        anyhow::ensure!(
            guest.with(|ctx| -> Result<bool> {
                let system: Object = ctx.globals().get("PocketPiSystem")?;
                let has_view: Function = system.get("hasView")?;
                Ok(has_view.call::<_, bool>(())?)
            })?,
            "{} installed no View",
            app.descriptor.id
        );
        let last_seen_revision = revision.load(Ordering::Acquire);
        Ok(Self {
            guest,
            surface,
            _fs: fs,
            _db: db,
            revision,
            last_seen_revision: Cell::new(last_seen_revision),
            #[cfg(test)]
            projection_refreshes: Cell::new(0),
            #[cfg(test)]
            ticks: Cell::new(0),
        })
    }

    fn projection_is_stale(&self) -> bool {
        self.revision.load(Ordering::Acquire) != self.last_seen_revision.get()
    }

    fn advance(&self, render_surface: bool) -> Result<()> {
        // A normal frame never queries SQLite. It only compares one in-memory
        // App revision and lets the View refresh a bounded projection when a
        // committed Action made that revision stale.
        self.call_method("tickView", ()).map(|_: String| ())?;
        #[cfg(test)]
        self.ticks.set(self.ticks.get().saturating_add(1));
        if render_surface {
            let current_revision = self.revision.load(Ordering::Acquire);
            if current_revision != self.last_seen_revision.get() {
                // The counter is sampled once at the foreground frame
                // boundary. Any number of commits since the previous frame is
                // therefore coalesced into one bounded projection refresh.
                // A commit racing this query remains visible as a newer
                // revision and is picked up on the following frame.
                self.call_method::<_, String>("dataChanged", ())?;
                self.last_seen_revision.set(current_revision);
                #[cfg(test)]
                self.projection_refreshes
                    .set(self.projection_refreshes.get().saturating_add(1));
            }
        }
        if render_surface {
            self.surface.tick();
        }
        Ok(())
    }

    fn update(&self, projection: &Value) -> Result<()> {
        self.call_method("updateView", (projection.to_string(),))
            .map(|_: String| ())
    }

    fn tap(&self, x: u16, y: u16) -> Result<Value> {
        let line: String = self.call_method("tap", (x as i32, y as i32))?;
        if line.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&line).context("parse App tap action")
    }

    fn pointer_down(&self, x: u16, y: u16) -> Result<()> {
        self.call_method("pointerDown", (x as i32, y as i32))
            .map(|_: String| ())
    }

    fn pointer_up(&self) -> Result<()> {
        self.call_method("pointerUp", ()).map(|_: String| ())
    }

    fn with_ui<R>(&self, f: impl FnOnce(&mut pocketjs_core::Ui) -> R) -> R {
        self.surface.with_ui(f)
    }

    fn call_method<A, R>(&self, name: &str, args: A) -> Result<R>
    where
        A: for<'js> pocket_mod::qjs::function::IntoArgs<'js>,
        R: for<'js> pocket_mod::qjs::FromJs<'js>,
    {
        self.guest.with(|ctx| {
            let app: Object = ctx
                .globals()
                .get("PocketPiSystem")
                .map_err(|error| anyhow!("PocketPiSystem missing: {error}"))?;
            let function: Function = app
                .get(name)
                .map_err(|error| anyhow!("PocketPiSystem.{name} missing: {error}"))?;
            function
                .call::<_, R>(args)
                .catch(&ctx)
                .map_err(|error| anyhow!("PocketPiSystem.{name}: {error}"))
        })
    }
}

fn install_system_framework(
    guest: &Guest,
    app_id: &str,
    source: &str,
    resources: &str,
) -> Result<()> {
    guest.eval("pocket-pi-system-framework", source)?;
    guest.with(|ctx| -> Result<()> {
        let public: Object = ctx.globals().get("PocketPi")?;
        let api: u32 = public.get("frameworkApi")?;
        anyhow::ensure!(
            api == SYSTEM_FRAMEWORK_API,
            "Pocket Pi System Framework API mismatch"
        );
        let system: Object = ctx.globals().get("PocketPiSystem")?;
        let configure: Function = system.get("configure")?;
        configure.call::<_, ()>((app_id, resources))?;
        Ok(())
    })
}

fn mount_db(guest: &Guest, db: SharedDb, writable: bool) -> Result<()> {
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
        if writable {
            let module = db.clone();
            ns.set(
                "exec",
                Function::new(ctx.clone(), move |handle: i32, sql: String| -> i32 {
                    let result = module.lock().map_or(1, |mut db| db.exec(handle, &sql));
                    yield_after_db_call();
                    result
                })?,
            )?;
        }
        let module = db.clone();
        ns.set(
            "query",
            Function::new(
                ctx.clone(),
                move |handle: i32, sql: String, args: String| -> String {
                    let result = module.lock().map_or_else(
                        |_| json!({"error":"App database owner is unavailable"}).to_string(),
                        |mut db| {
                            if writable {
                                return db.query(handle, &sql, &args);
                            }
                            if db.exec(handle, "PRAGMA query_only=ON") != 0 {
                                return json!({"error":db.last_error(handle)}).to_string();
                            }
                            let result = db.query(handle, &sql, &args);
                            if db.exec(handle, "PRAGMA query_only=OFF") != 0 {
                                return json!({"error":db.last_error(handle)}).to_string();
                            }
                            result
                        },
                    );
                    yield_after_db_call();
                    result
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

fn mount_services(
    guest: &Guest,
    app_id: String,
    services: Arc<dyn AppServiceHost>,
    action_deadline: Rc<Cell<Option<Instant>>>,
) -> Result<()> {
    guest.mount("services", |ctx, ns| {
        ns.set(
            "call",
            Function::new(
                ctx.clone(),
                move |service: String, operation: String, args_json: String| -> String {
                    let args = serde_json::from_str(&args_json).unwrap_or(Value::Null);
                    let result = action_deadline
                        .get()
                        .ok_or_else(|| "App service call has no active Action".to_owned())
                        .and_then(|deadline| {
                            if Instant::now() >= deadline {
                                Err("App Action deadline expired".to_owned())
                            } else {
                                services.call(&app_id, &service, &operation, &args, deadline)
                            }
                        });
                    match result {
                        Ok(value) => json!({"ok":true,"value":value}).to_string(),
                        Err(error) => json!({"ok":false,"error":error}).to_string(),
                    }
                },
            )?,
        )?;
        Ok(())
    })
}

#[cfg(target_os = "espidf")]
fn yield_scheduler_tick() {
    unsafe extern "C" {
        fn vTaskDelay(ticks: u32);
    }
    // The firmware runs FreeRTOS at 100 Hz. Block through one scheduler tick
    // instead of sub-tick polling so the IDLE task gets a scheduling chance.
    unsafe { vTaskDelay(1) };
}

#[cfg(not(target_os = "espidf"))]
fn yield_scheduler_tick() {
    std::thread::sleep(Duration::from_millis(5));
}

#[cfg(target_os = "espidf")]
fn yield_after_db_call() {
    yield_scheduler_tick();
}

#[cfg(not(target_os = "espidf"))]
fn yield_after_db_call() {}

fn mount_data_lifecycle(
    guest: &Guest,
    revision: AppRevision,
    action_deadline: Rc<Cell<Option<Instant>>>,
) -> Result<()> {
    guest.mount("app", |ctx, ns| {
        ns.set(
            "commit",
            Function::new(ctx.clone(), move || -> f64 {
                // Called only after a successful App-owned SQLite COMMIT.
                // Release pairs with the foreground View's Acquire load.
                revision.fetch_add(1, Ordering::Release).saturating_add(1) as f64
            })?,
        )?;
        ns.set(
            "remainingMs",
            Function::new(ctx.clone(), move || -> f64 {
                action_deadline.get().map_or(1, |deadline| {
                    deadline
                        .checked_duration_since(Instant::now())
                        .map_or(1, |remaining| {
                            remaining.as_millis().clamp(1, u128::from(u32::MAX)) as u32
                        })
                }) as f64
            })?,
        )?;
        Ok(())
    })
}

pub struct AppSupervisor {
    workspace: PathBuf,
    catalog: InstalledAppIndex,
    services: Arc<dyn AppServiceHost>,
    assets: AppRuntimeAssets,
    action_runner: ActionRunner,
    databases: BTreeMap<String, SharedDb>,
    revisions: BTreeMap<String, AppRevision>,
    /// The Pi Agent System App is booted once and remains resident for the
    /// entire supervisor lifetime. Foreground navigation never replaces it.
    system: ViewRuntime,
    agent: Option<GuestAgent>,
    agent_tools: Option<Arc<dyn ToolHost>>,
    // Least recently used first. Only the selected View advances.
    runtimes: Vec<(String, ViewRuntime)>,
    active_app: Option<String>,
    schedules: AppScheduleStore,
}

impl AppSupervisor {
    pub fn new(
        workspace: impl Into<PathBuf>,
        catalog: InstalledAppIndex,
        services: Arc<dyn AppServiceHost>,
    ) -> Result<Self> {
        let workspace = workspace.into();
        let assets = AppRuntimeAssets {
            framework: std::fs::read_to_string(workspace.join("system/framework.js"))
                .context("read Pocket Pi System Framework")?
                .into(),
            net_sdk: std::fs::read_to_string(workspace.join("system/net-sdk.js"))
                .context("read Pocket Pi NET SDK")?
                .into(),
            view_sdk: std::fs::read_to_string(workspace.join("system/view-sdk.js"))
                .context("read Pocket Pi View SDK")?
                .into(),
            view_pak: workspace.join("system/view-sdk.pak"),
        };
        let schedules = AppScheduleStore::load(&workspace, &catalog)?;
        let mut databases = BTreeMap::new();
        let mut revisions = BTreeMap::new();
        for app in catalog.apps() {
            let descriptor = &app.descriptor;
            let (_, db_root, _) = data_paths(&workspace, &descriptor.id);
            std::fs::create_dir_all(&db_root)?;
            let database = Arc::new(Mutex::new(DbModule::new(DbStorage::Dir(db_root))));
            if descriptor.id != ROOT_APP_ID {
                prepare_source_database(&app, &database)?;
            }
            databases.insert(descriptor.id.clone(), database);
            revisions.insert(descriptor.id.clone(), Arc::new(AtomicU32::new(0)));
        }
        let action_configs = catalog
            .apps()
            .into_iter()
            .filter_map(|app| {
                let descriptor = &app.descriptor;
                let source_path = app.release_dir.join("actions.js");
                source_path.is_file().then(|| ActionConfig {
                    app_id: descriptor.id.clone(),
                    source_path,
                    framework: assets.framework.clone(),
                    net_sdk: (descriptor.id != ROOT_APP_ID
                        && descriptor
                            .capabilities
                            .iter()
                            .any(|capability| capability == "net.http"))
                    .then(|| assets.net_sdk.clone()),
                    resources: app.resources.clone(),
                    database: databases
                        .get(&descriptor.id)
                        .expect("database created for descriptor")
                        .clone(),
                    revision: revisions
                        .get(&descriptor.id)
                        .expect("revision created for descriptor")
                        .clone(),
                    net: descriptor
                        .capabilities
                        .iter()
                        .any(|capability| capability == "net.http"),
                })
            })
            .collect();
        let action_runner = ActionRunner::start(action_configs, services.clone())?;
        log::info!("loading Pi Agent System App");
        let system = load_runtime(
            &workspace,
            &catalog,
            &databases,
            &revisions,
            &assets,
            ROOT_APP_ID,
        )?;
        log::info!("Pi Agent System App loaded");
        Ok(Self {
            workspace,
            catalog,
            services,
            assets,
            action_runner,
            databases,
            revisions,
            system,
            agent: None,
            agent_tools: None,
            runtimes: Vec::new(),
            active_app: None,
            schedules,
        })
    }

    pub fn catalog(&self) -> &InstalledAppIndex {
        &self.catalog
    }

    pub fn services_busy(&self) -> bool {
        self.services.busy() || self.action_runner.busy()
    }

    pub fn activate_app(
        &mut self,
        staging_release: &Path,
        credentials: BTreeMap<String, String>,
    ) -> Result<AppDescriptor> {
        anyhow::ensure!(!self.services_busy(), "App services are busy");
        anyhow::ensure!(staging_release.is_dir(), "staged App release is missing");
        anyhow::ensure!(
            !staging_release.join("credentials.json").exists(),
            "credentials.json must be removed before activation"
        );
        let descriptor: AppDescriptor = serde_json::from_slice(
            &std::fs::read(staging_release.join("app.json")).context("read staged app.json")?,
        )
        .context("parse staged app.json")?;
        ensure_safe_component(&descriptor.id, "App id")?;
        anyhow::ensure!(
            descriptor.id != ROOT_APP_ID,
            "the Pi Agent System App is built into Firmware"
        );
        let app_root = self.workspace.join("apps").join(&descriptor.id);
        anyhow::ensure!(
            self.catalog.app(&descriptor.id).is_none(),
            "App {} is already installed",
            descriptor.id
        );
        if app_root.exists() {
            std::fs::remove_dir_all(&app_root)
                .with_context(|| format!("clear incomplete App {}", descriptor.id))?;
        }

        let credential_ids = credentials.keys().cloned().collect::<Vec<_>>();
        let result = self.activate_new_app(staging_release, descriptor, credentials, &app_root);
        if result.is_err() {
            if let Some(app_id) = app_root.file_name().and_then(|value| value.to_str()) {
                self.action_runner.unregister(app_id);
                let _ = self.services.remove_app_state(app_id, &credential_ids);
            }
            let _ = std::fs::remove_dir_all(&app_root);
        }
        result
    }

    fn activate_new_app(
        &mut self,
        staging_release: &Path,
        descriptor: AppDescriptor,
        credentials: BTreeMap<String, String>,
        app_root: &Path,
    ) -> Result<AppDescriptor> {
        let mut app = load_source_release(staging_release, &descriptor.id)?;
        validate_package_credentials(&app.descriptor, &credentials)?;
        self.catalog.validate_insert(&app)?;

        let (fs_root, db_root, tmp_root) = data_paths(&self.workspace, &descriptor.id);
        std::fs::create_dir_all(&db_root)?;
        std::fs::create_dir_all(&tmp_root)?;
        let database = Arc::new(Mutex::new(DbModule::new(DbStorage::Dir(db_root))));
        prepare_source_database(&app, &database)?;
        let revision = Arc::new(AtomicU32::new(0));
        let net = app
            .descriptor
            .capabilities
            .iter()
            .any(|capability| capability == "net.http");
        let has_actions = {
            let staged_action = staging_release.join("actions.js");
            let candidate = ActionConfig {
                app_id: descriptor.id.clone(),
                source_path: staged_action,
                framework: self.assets.framework.clone(),
                net_sdk: net.then(|| self.assets.net_sdk.clone()),
                resources: app.resources.clone(),
                database: database.clone(),
                revision: revision.clone(),
                net,
            };
            let runtime = ActionRuntime::load(&candidate, self.services.clone())?;
            validate_action_contract(&app.descriptor, &runtime)?;
            !runtime.action_names()?.is_empty()
        };
        let runtime = ViewRuntime::load(
            &app,
            &self.assets,
            &fs_root,
            &tmp_root,
            database.clone(),
            revision.clone(),
        )?;

        let schedules = new_schedules(&app.descriptor);
        let schedule_path = app_root.join("data/.system/schedules.json");
        atomic_write(&schedule_path, &serde_json::to_vec(&schedules)?)?;

        let release_dir = app_root.join("release");
        anyhow::ensure!(!release_dir.exists(), "App is already installed");
        std::fs::rename(staging_release, &release_dir).context("move staged App release")?;
        app.release_dir = release_dir.clone();

        if has_actions {
            self.action_runner.register(ActionConfig {
                app_id: descriptor.id.clone(),
                source_path: release_dir.join("actions.js"),
                framework: self.assets.framework.clone(),
                net_sdk: net.then(|| self.assets.net_sdk.clone()),
                resources: app.resources.clone(),
                database: database.clone(),
                revision: revision.clone(),
                net,
            })?;
        }
        self.services
            .store_credentials(&credentials)
            .map_err(anyhow::Error::msg)?;
        if let (Some(agent), Some(tools)) = (&self.agent, &self.agent_tools) {
            let mut next = tools.definitions();
            next.extend(app.descriptor.tools.iter().map(public_tool_definition));
            agent
                .replace_tools(&self.system.guest, next)
                .map_err(anyhow::Error::msg)?;
        }

        self.catalog.insert_validated(app);
        self.databases.insert(descriptor.id.clone(), database);
        self.revisions.insert(descriptor.id.clone(), revision);
        make_runtime_room(
            &mut self.runtimes,
            VIEW_RUNTIME_LIMIT,
            self.active_app.as_deref(),
        );
        self.runtimes.push((descriptor.id.clone(), runtime));
        self.schedules
            .register(descriptor.id.clone(), schedule_path, schedules);
        Ok(descriptor)
    }

    pub fn uninstall_app(&mut self, app_id: &str) -> Result<AppDescriptor> {
        anyhow::ensure!(app_id != ROOT_APP_ID, "cannot uninstall the System App");
        anyhow::ensure!(!self.services_busy(), "App services are busy");
        let app = self
            .catalog
            .app(app_id)
            .ok_or_else(|| anyhow!("unknown App: {app_id}"))?;

        let removed_tools = app
            .descriptor
            .tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<BTreeSet<_>>();
        if let (Some(agent), Some(tools)) = (&self.agent, &self.agent_tools) {
            let next = tools
                .definitions()
                .into_iter()
                .filter(|tool| {
                    tool.get("name")
                        .and_then(Value::as_str)
                        .is_none_or(|name| !removed_tools.contains(name))
                })
                .collect();
            agent
                .replace_tools(&self.system.guest, next)
                .map_err(anyhow::Error::msg)?;
        }

        self.schedules.remove(app_id);
        self.action_runner.remove_app(app_id)?;
        if self.active_app.as_deref() == Some(app_id) {
            self.active_app = None;
        }
        self.runtimes
            .retain(|(runtime_app_id, _)| runtime_app_id != app_id);
        self.databases.remove(app_id);
        self.revisions.remove(app_id);

        let credential_ids = descriptor_credential_ids(&app.descriptor)
            .into_iter()
            .collect::<Vec<_>>();
        self.services
            .remove_app_state(app_id, &credential_ids)
            .map_err(anyhow::Error::msg)?;
        std::fs::remove_dir_all(self.workspace.join("apps").join(app_id))
            .with_context(|| format!("delete App {app_id}"))?;
        self.catalog
            .remove(app_id)
            .expect("installed App remains indexed during uninstall");
        Ok(app.descriptor)
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

        anyhow::ensure!(self.catalog.app(app_id).is_some(), "unknown App: {app_id}");
        let runtime = match take_runtime(&mut self.runtimes, app_id) {
            Some(runtime) => runtime,
            None => {
                make_runtime_room(
                    &mut self.runtimes,
                    VIEW_RUNTIME_LIMIT,
                    self.active_app.as_deref(),
                );
                load_runtime(
                    &self.workspace,
                    &self.catalog,
                    &self.databases,
                    &self.revisions,
                    &self.assets,
                    app_id,
                )?
            }
        };
        self.runtimes.push((app_id.to_owned(), runtime));
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
        let release = self
            .catalog
            .app(ROOT_APP_ID)
            .expect("System App remains installed")
            .release_dir;
        let source = std::fs::read_to_string(release.join("agent.js"))
            .context("read Pi Agent System App loop bundle")?;
        self.agent = Some(
            GuestAgent::mount_source(
                &self.system.guest,
                config_json,
                backend,
                tools.clone(),
                &source,
            )
            .map_err(anyhow::Error::msg)?,
        );
        self.agent_tools = Some(tools);
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

    /// Advance the resident System App and render the selected View.
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
        if let Some(app_id) = &self.active_app {
            self.cached_view(app_id)
                .expect("active ordinary App remains loaded")
                .advance(render_selected)?;
        } else {
            self.system.advance(render_selected)?;
        }
        Ok(events)
    }

    pub fn update_system(&self, facts: &Value) -> Result<()> {
        self.system.update(facts)
    }

    pub fn system_telemetry_visible(&self) -> Result<bool> {
        if self.active_app.is_some() {
            return Ok(false);
        }
        self.system.call_method("telemetryVisible", ())
    }

    pub fn tap(&self, x: u16, y: u16) -> Result<Value> {
        let event = self.active().tap(x, y)?;
        if event.get("type").and_then(Value::as_str) == Some("command") {
            let command = event.get("command").and_then(Value::as_str).unwrap_or("");
            if !view_command_allowed(self.active_id(), command) {
                log::warn!(
                    "App {} attempted privileged command {command}",
                    self.active_id()
                );
                return Ok(Value::Null);
            }
        }
        Ok(event)
    }

    pub fn pointer_down(&self, x: u16, y: u16) -> Result<()> {
        self.active().pointer_down(x, y)
    }

    pub fn pointer_up(&self) -> Result<()> {
        self.active().pointer_up()
    }

    pub fn with_ui<R>(&self, f: impl FnOnce(&mut pocketjs_core::Ui) -> R) -> R {
        self.active().with_ui(f)
    }

    /// A single atomic comparison lets the host wake the active View after a
    /// background commit. It never queries SQLite and closed Apps stay idle.
    pub fn active_projection_is_stale(&self) -> bool {
        self.active().projection_is_stale()
    }

    fn begin_agent_tool(
        &self,
        name: &str,
        args_json: &str,
        deadline: Instant,
        response: mpsc::Sender<ToolResult>,
    ) {
        let Some(route) = self.catalog.route_for_tool(name) else {
            let _ = response.send(tool_error(format!("unknown App tool: {name}")));
            return;
        };
        let args = serde_json::from_str(args_json).unwrap_or(Value::Null);
        match self.action_runner.enqueue(
            &route.app_id,
            ActionSource::Tool,
            &route.action,
            args,
            deadline,
            ActionCompletion::Response(response.clone()),
        ) {
            Ok(_) => {}
            Err(error) => {
                let _ = response.send(tool_error(format!("{name}: {error:#}")));
            }
        }
    }

    /// Runs an Action requested by the currently visible App. The host calls this
    /// after presenting the App's immediate pressed/loading state so slow
    /// native services never hide touch feedback.
    pub fn invoke_active_action(&mut self, action: &str, args: &Value) -> ToolResult {
        let app_id = self.active_id().to_owned();
        match self.action_runner.enqueue(
            &app_id,
            ActionSource::Ui,
            action,
            args.clone(),
            new_action_deadline(),
            ActionCompletion::None,
        ) {
            Ok(run_id) => ToolResult {
                text: format!("Queued {app_id}.{action} as App Action {run_id}"),
                details: json!({"status":"queued","runId":run_id,"app":app_id}),
                is_error: false,
                terminate: false,
            },
            Err(error) => tool_error(format!("{app_id}.{action}: {error:#}")),
        }
    }

    pub fn poll_due_actions(&mut self) -> Vec<(String, ToolResult)> {
        let mut results = Vec::new();
        for completed in self.action_runner.drain_schedule_results() {
            self.schedules.finish(
                &completed.app_id,
                &completed.schedule_id,
                !completed.result.is_error,
            );
            results.push((completed.app_id, completed.result));
        }
        while let Some(due) = self.schedules.claim_due() {
            let label = format!("{}.{}", due.app_id, due.action);
            let result = match self.action_runner.enqueue(
                &due.app_id,
                ActionSource::Schedule,
                &due.action,
                due.args.clone(),
                new_action_deadline(),
                ActionCompletion::Schedule(due.schedule_id.clone()),
            ) {
                Ok(run_id) => ToolResult {
                    text: format!("Queued {label} as App Action {run_id}"),
                    details: json!({"status":"queued","runId":run_id}),
                    is_error: false,
                    terminate: false,
                },
                Err(error) => tool_error(format!("{label}: {error:#}")),
            };
            if result.is_error {
                self.schedules.finish(&due.app_id, &due.schedule_id, false);
            }
            results.push((label, result));
        }
        results
    }

    fn active(&self) -> &ViewRuntime {
        self.active_app
            .as_deref()
            .and_then(|app_id| self.cached_view(app_id))
            .unwrap_or(&self.system)
    }

    fn cached_view(&self, app_id: &str) -> Option<&ViewRuntime> {
        self.runtimes
            .iter()
            .find(|(cached_id, _)| cached_id == app_id)
            .map(|(_, runtime)| runtime)
    }
}

fn view_command_allowed(app_id: &str, command: &str) -> bool {
    app_id == ROOT_APP_ID || command == "apps.open"
}

fn load_runtime(
    workspace: &Path,
    catalog: &InstalledAppIndex,
    databases: &BTreeMap<String, SharedDb>,
    revisions: &BTreeMap<String, AppRevision>,
    assets: &AppRuntimeAssets,
    app_id: &str,
) -> Result<ViewRuntime> {
    let app = catalog
        .app(app_id)
        .ok_or_else(|| anyhow!("unknown App: {app_id}"))?;
    let (fs_root, _, tmp_root) = data_paths(workspace, app_id);
    let db = databases
        .get(app_id)
        .cloned()
        .ok_or_else(|| anyhow!("App {app_id} has no database owner"))?;
    let revision = revisions
        .get(app_id)
        .cloned()
        .ok_or_else(|| anyhow!("App {app_id} has no revision owner"))?;
    ViewRuntime::load(&app, assets, &fs_root, &tmp_root, db, revision)
        .with_context(|| format!("load App {app_id}"))
}

fn data_paths(workspace: &Path, app_id: &str) -> (PathBuf, PathBuf, PathBuf) {
    if app_id == ROOT_APP_ID {
        (
            workspace.to_owned(),
            workspace.join("data"),
            workspace.join(".system/tmp/pi-agent"),
        )
    } else {
        let root = workspace.join("apps").join(app_id);
        (root.join("data"), root.join("data"), root.join("tmp"))
    }
}

fn prepare_source_database(app: &InstalledApp, database: &SharedDb) -> Result<()> {
    let mut database = database
        .lock()
        .map_err(|_| anyhow!("App database lock was poisoned"))?;
    let handle = database.open(&app.descriptor.id);
    anyhow::ensure!(handle >= 0, "open {}.sqlite", app.descriptor.id);
    let version: Value =
        serde_json::from_str(&database.query(handle, "PRAGMA user_version", "[]"))?;
    anyhow::ensure!(
        version.get("error").is_none(),
        "read {} schema version",
        app.descriptor.id
    );
    let current = version["rows"][0][0]
        .as_u64()
        .ok_or_else(|| anyhow!("invalid {} schema version", app.descriptor.id))?;
    let expected = u64::from(app.descriptor.schema_version);
    if current == expected {
        return Ok(());
    }
    anyhow::ensure!(
        current == 0,
        "App {} data schema is {current}; source requires {expected}",
        app.descriptor.id
    );
    let schema = std::fs::read_to_string(app.release_dir.join("schema.sql"))
        .context("read source schema.sql")?;
    anyhow::ensure!(
        !schema.trim().is_empty(),
        "App {} schema.sql is empty",
        app.descriptor.id
    );
    let sql = format!(
        "BEGIN IMMEDIATE;\n{schema}\nPRAGMA user_version={};\nCOMMIT;",
        app.descriptor.schema_version
    );
    if database.exec(handle, &sql) != 0 {
        let error = database.last_error(handle);
        let _ = database.exec(handle, "ROLLBACK");
        anyhow::bail!("initialize {} schema: {error}", app.descriptor.id);
    }
    Ok(())
}

fn seed_system_app(workspace: &Path, bundle: SystemAppBundle) -> Result<()> {
    let release_dir = workspace.join("system/app");
    std::fs::create_dir_all(&release_dir)?;
    for legacy in ["app.js", "app.pak", "pocket.json"] {
        let path = release_dir.join(legacy);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    atomic_write(&release_dir.join("view.js"), bundle.view_js.as_bytes())?;
    atomic_write(&release_dir.join("text.js"), bundle.text_js.as_bytes())?;
    atomic_write(&release_dir.join("agent.js"), bundle.agent_js.as_bytes())?;
    atomic_write(
        &release_dir.join("app.json"),
        bundle.descriptor_json.as_bytes(),
    )?;
    let descriptor: Value = serde_json::from_str(bundle.descriptor_json)?;
    let modules = descriptor
        .get("capabilities")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    atomic_write(
        &release_dir.join("plan.json"),
        &serde_json::to_vec_pretty(&json!({
            "runtime":"pocket-pi-agentos",
            "pocketjsRevision":POCKETJS_REVISION,
            "frameworkApi":SYSTEM_FRAMEWORK_API,
            "app":ROOT_APP_ID,
            "modules":modules
        }))?,
    )?;
    Ok(())
}

fn seed_system_runtime(workspace: &Path) -> Result<()> {
    atomic_write(
        &workspace.join("system/framework.js"),
        system_framework().as_bytes(),
    )?;
    atomic_write(
        &workspace.join("system/net-sdk.js"),
        system_net_sdk().as_bytes(),
    )?;
    atomic_write(
        &workspace.join("system/view-sdk.js"),
        system_view_sdk().as_bytes(),
    )?;
    atomic_write(&workspace.join("system/view-sdk.pak"), system_view_pak())
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
    name: String,
    args_json: String,
    deadline: Instant,
    response: mpsc::Sender<ToolResult>,
}

pub struct RoutedToolHost {
    native: Arc<dyn ToolHost>,
    catalog: InstalledAppIndex,
    app_tx: mpsc::Sender<AppToolRequest>,
}

impl RoutedToolHost {
    pub fn new(
        native: Arc<dyn ToolHost>,
        catalog: InstalledAppIndex,
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
        if self.catalog.route_for_tool(name).is_none() {
            return self.native.execute(call_id, name, args_json);
        }
        let (response, response_rx) = mpsc::channel();
        let deadline = new_action_deadline();
        if self
            .app_tx
            .send(AppToolRequest {
                name: name.to_owned(),
                args_json: args_json.to_owned(),
                deadline,
                response,
            })
            .is_err()
        {
            return tool_error("App Supervisor is unavailable");
        }
        loop {
            if Instant::now() >= deadline {
                return tool_error(format!("App Tool timed out after {APP_ACTION_TIMEOUT:?}"));
            }
            match response_rx.try_recv() {
                Ok(result) => return result,
                Err(mpsc::TryRecvError::Disconnected) => {
                    return tool_error("App Action response channel closed");
                }
                Err(mpsc::TryRecvError::Empty) => yield_scheduler_tick(),
            }
        }
    }
}

impl AppToolRequest {
    pub fn handle(self, supervisor: &AppSupervisor) {
        supervisor.begin_agent_tool(&self.name, &self.args_json, self.deadline, self.response);
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
    action: String,
    args: Value,
    every_seconds: u64,
    next_run_at: u64,
    last_ok: Option<bool>,
}

struct DueAction {
    app_id: String,
    schedule_id: String,
    action: String,
    args: Value,
}

struct AppScheduleStore {
    paths: BTreeMap<String, PathBuf>,
    schedules: Vec<StoredSchedule>,
}

impl AppScheduleStore {
    fn load(workspace: &Path, catalog: &InstalledAppIndex) -> Result<Self> {
        // App Schedule declarations travel with the App release, while their
        // mutable scheduler cursor belongs to that App's private data root.
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
                let every_seconds = declaration.every_minutes.saturating_mul(60).max(60);
                let existing = prior
                    .iter_mut()
                    .find(|item| item.app_id == app.id && item.schedule_id == declaration.id);
                schedules.push(match existing {
                    Some(item)
                        if item.action == declaration.action
                            && item.every_seconds == every_seconds =>
                    {
                        item.clone()
                    }
                    _ => StoredSchedule {
                        app_id: app.id.clone(),
                        schedule_id: declaration.id.clone(),
                        action: declaration.action.clone(),
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
        Ok(store)
    }

    fn claim_due(&mut self) -> Option<DueAction> {
        let now = unix_seconds();
        let item = self
            .schedules
            .iter_mut()
            .filter(|item| item.next_run_at <= now)
            .min_by_key(|item| item.next_run_at)?;
        item.next_run_at = next_schedule_run(item.next_run_at, item.every_seconds, now);
        let due = DueAction {
            app_id: item.app_id.clone(),
            schedule_id: item.schedule_id.clone(),
            action: item.action.clone(),
            args: item.args.clone(),
        };
        let _ = self.persist();
        Some(due)
    }

    fn register(&mut self, app_id: String, path: PathBuf, schedules: Vec<StoredSchedule>) {
        self.paths.insert(app_id, path);
        self.schedules.extend(schedules);
    }

    fn remove(&mut self, app_id: &str) {
        self.paths.remove(app_id);
        self.schedules.retain(|schedule| schedule.app_id != app_id);
    }

    fn finish(&mut self, app_id: &str, schedule_id: &str, ok: bool) {
        let Some(item) = self
            .schedules
            .iter_mut()
            .find(|item| item.app_id == app_id && item.schedule_id == schedule_id)
        else {
            return;
        };
        item.last_ok = Some(ok);
        log::info!("App Schedule {app_id}.{schedule_id} ok={ok}");
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

fn new_schedules(descriptor: &AppDescriptor) -> Vec<StoredSchedule> {
    let now = unix_seconds();
    descriptor
        .schedules
        .iter()
        .map(|declaration| {
            let every_seconds = declaration.every_minutes.saturating_mul(60).max(60);
            StoredSchedule {
                app_id: descriptor.id.clone(),
                schedule_id: declaration.id.clone(),
                action: declaration.action.clone(),
                args: declaration.args.clone(),
                every_seconds,
                next_run_at: now.saturating_add(every_seconds),
                last_ok: None,
            }
        })
        .collect()
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn next_schedule_run(current: u64, interval: u64, now: u64) -> u64 {
    let elapsed_intervals = now.saturating_sub(current) / interval;
    current.saturating_add(interval.saturating_mul(elapsed_intervals.saturating_add(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoServices;

    impl AppServiceHost for NoServices {
        fn call(
            &self,
            _app_id: &str,
            _service: &str,
            _operation: &str,
            _args: &Value,
            _deadline: Instant,
        ) -> std::result::Result<Value, String> {
            Err("unexpected App service call".into())
        }
    }

    #[derive(Default)]
    struct TrackingServices {
        credentials: Mutex<BTreeMap<String, String>>,
        removed_apps: Mutex<Vec<String>>,
    }

    impl AppServiceHost for TrackingServices {
        fn call(
            &self,
            _app_id: &str,
            _service: &str,
            _operation: &str,
            _args: &Value,
            _deadline: Instant,
        ) -> std::result::Result<Value, String> {
            Err("simulated service failure".into())
        }

        fn store_credentials(
            &self,
            credentials: &BTreeMap<String, String>,
        ) -> std::result::Result<(), String> {
            self.credentials.lock().unwrap().extend(credentials.clone());
            Ok(())
        }

        fn remove_app_state(
            &self,
            app_id: &str,
            credential_ids: &[String],
        ) -> std::result::Result<(), String> {
            let mut credentials = self.credentials.lock().unwrap();
            for id in credential_ids {
                credentials.remove(id);
            }
            self.removed_apps.lock().unwrap().push(app_id.to_owned());
            Ok(())
        }
    }

    struct RecordingBackend(Arc<Mutex<Vec<Value>>>);

    impl ModelBackend for RecordingBackend {
        fn complete(
            &self,
            request_json: &str,
            _on_event: &mut dyn FnMut(pocket_pi_embedded::ModelStreamEvent),
        ) -> std::result::Result<String, String> {
            self.0
                .lock()
                .unwrap()
                .push(serde_json::from_str(request_json).unwrap());
            Ok(json!({
                "thinking":"",
                "text":"done",
                "toolCalls":[],
                "usage":{},
                "stopReason":"stop"
            })
            .to_string())
        }
    }

    struct NoTools;

    impl ToolHost for NoTools {
        fn definitions(&self) -> Vec<Value> {
            Vec::new()
        }

        fn execute(&self, _call_id: &str, name: &str, _args_json: &str) -> ToolResult {
            tool_error(format!("unexpected native Tool: {name}"))
        }
    }

    fn view_call(guest: &Guest, method: &str) -> String {
        guest
            .with(|ctx| {
                let system: Object = ctx.globals().get("PocketPiSystem")?;
                system.get::<_, Function>(method)?.call::<_, String>(())
            })
            .unwrap()
    }

    fn view_call_at(guest: &Guest, method: &str, x: i32, y: i32) -> String {
        guest
            .with(|ctx| {
                let system: Object = ctx.globals().get("PocketPiSystem")?;
                system.get::<_, Function>(method)?.call::<_, String>((x, y))
            })
            .unwrap()
    }

    #[test]
    fn catalog_uses_each_apps_declared_tool_namespace() {
        let temp = tempfile::tempdir().unwrap();
        install_fixture(
            temp.path(),
            "search",
            r#"{"id":"search","description":"Research","version":"1","toolNamespace":"research","tools":[{"name":"research.query","action":"query","parameters":{"type":"object"}}],"schedules":[]}"#,
        );
        let catalog = InstalledAppIndex::load(temp.path(), system_app_bundle()).unwrap();

        assert_eq!(
            catalog
                .route_for_tool("research.query")
                .map(|route| (route.app_id, route.action)),
            Some(("search".into(), "query".into()))
        );
        assert_eq!(catalog.descriptors().len(), 2);
    }

    #[test]
    fn only_the_system_app_can_issue_privileged_device_commands() {
        assert!(view_command_allowed("exa", "apps.open"));
        assert!(!view_command_allowed("exa", "device.restart"));
        assert!(view_command_allowed(ROOT_APP_ID, "device.restart"));
    }

    #[test]
    fn catalog_rejects_a_tool_outside_its_declared_namespace() {
        let temp = tempfile::tempdir().unwrap();
        install_fixture(
            temp.path(),
            "search",
            r#"{"id":"search","description":"Research","version":"1","tools":[{"name":"other.query","action":"query","parameters":{"type":"object"}}],"schedules":[]}"#,
        );
        let error = InstalledAppIndex::load(temp.path(), system_app_bundle()).unwrap_err();

        assert!(error.to_string().contains("non-namespaced tool"));
    }

    #[test]
    fn catalog_exposes_only_declared_native_policies() {
        let temp = tempfile::tempdir().unwrap();
        install_fixture(
            temp.path(),
            "search",
            r#"{"id":"search","description":"Research","version":"1","tools":[],"schedules":[],"nativeServices":{"http":[{"method":"POST","urls":["https://example.com/search"],"allowedRequestHeaders":["content-type"],"credential":{"id":"search.api-key","header":"x-api-key"}}],"mcp":[]}}"#,
        );
        let catalog = InstalledAppIndex::load(temp.path(), system_app_bundle()).unwrap();

        assert!(catalog
            .http_policy("search", "POST", "https://example.com/search")
            .is_some());
        assert!(catalog
            .http_policy("search", "GET", "https://example.com/search")
            .is_none());
        assert_eq!(
            catalog.credential_ids(),
            BTreeSet::from(["search.api-key".to_owned()])
        );
    }

    #[test]
    fn package_credentials_must_exactly_match_the_descriptor() {
        let descriptor = AppDescriptor {
            format: 1,
            framework_api: 1,
            id: "search".into(),
            title: "Search".into(),
            description: "Research".into(),
            version: "1".into(),
            schema_version: 1,
            capabilities: Vec::new(),
            tool_namespace: "search".into(),
            tools: Vec::new(),
            provider_operations: Vec::new(),
            schedules: Vec::new(),
            native_services: NativeServices {
                http: vec![HttpServicePolicy {
                    method: "POST".into(),
                    urls: vec!["https://example.com/search".into()],
                    allowed_request_headers: Vec::new(),
                    credential: Some(CredentialBinding {
                        id: "search.api-key".into(),
                        header: "authorization".into(),
                        prefix: "Bearer ".into(),
                    }),
                }],
                mcp: Vec::new(),
            },
            resources: BTreeMap::new(),
        };

        assert!(validate_package_credentials(&descriptor, &BTreeMap::new()).is_err());
        assert!(validate_package_credentials(
            &descriptor,
            &BTreeMap::from([("search.api-key".into(), "secret".into())])
        )
        .is_ok());
        assert!(validate_package_credentials(
            &descriptor,
            &BTreeMap::from([
                ("search.api-key".into(), "secret".into()),
                ("extra.api-key".into(), "secret".into())
            ])
        )
        .is_err());
    }

    #[test]
    fn descriptor_rejects_removed_fields() {
        let error = serde_json::from_str::<AppDescriptor>(
            r#"{"id":"notes","description":"Notes","version":"1","tools":[],"tasks":[],"schedules":[]}"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("unknown field `tasks`"));
    }

    #[test]
    fn projection_errors_propagate_to_the_view_runtime() {
        let guest = new_app_guest().unwrap();
        let database = Arc::new(Mutex::new(DbModule::new(DbStorage::Memory)));
        mount_db(&guest, database, false).unwrap();
        install_system_framework(&guest, "projection-test", system_framework(), "{}").unwrap();

        let error = guest
            .eval(
                "projection-error",
                r#"PocketPi.projection.one("SELECT 1 AS value", {}, () => { throw new Error("invalid projection"); });"#,
            )
            .unwrap_err();

        assert!(error.to_string().contains("invalid projection"));
    }

    #[test]
    fn raw_view_updates_only_bound_text_and_changed_props() {
        let guest = new_app_guest().unwrap();
        let surface = UiSurface::new(VIEWPORT);
        surface.feed_pak(system_view_pak());
        surface.mount(&guest).unwrap();
        install_system_framework(&guest, "raw-view-test", system_framework(), "{}").unwrap();
        guest
            .eval(
                "track-ui-ops",
                r#"
                  const nativeUi = globalThis.ui;
                  globalThis.__uiOps = [];
                  globalThis.ui = new Proxy(nativeUi, {
                    get(target, name) {
                      const value = target[name];
                      if (typeof value !== "function") return value;
                      return (...args) => {
                        globalThis.__uiOps.push(String(name));
                        return Reflect.apply(value, target, args);
                      };
                    },
                  });
                  globalThis.__takeUiOps = () => JSON.stringify(globalThis.__uiOps.splice(0));
                "#,
            )
            .unwrap();
        guest.eval("pocket-pi-view-sdk", system_view_sdk()).unwrap();
        guest
            .eval(
                "raw-view",
                r#"
                  const active = View.state(false);
                  const label = View.state("KEY");
                  globalThis.__updateLabel = () => label.set("KEY!");
                  View.mount(() => View.Screen({
                    children: View.Pressable({
                      style: { width: 120, height: 60, background: active.get() ? "accent" : "surface" },
                      onPress: () => {
                        active.set(true);
                        return PocketPi.action("key", { active: active.get() });
                      },
                      children: View.Text({ text: () => label.get() }),
                    }),
                  }));
                "#,
            )
            .unwrap();

        view_call(&guest, "tickView");
        let (before, node_before) = surface.with_ui(|ui| {
            ui.tick();
            (ui.draw().words.clone(), ui.hit_test_bounds(10.0, 10.0))
        });
        guest
            .eval("clear-ui-ops", "globalThis.__uiOps.length = 0")
            .unwrap();
        guest
            .eval("update-label", "globalThis.__updateLabel()")
            .unwrap();
        let text_operations = guest
            .with(|ctx| {
                ctx.globals()
                    .get::<_, Function>("__takeUiOps")?
                    .call::<_, String>(())
            })
            .map(|line| serde_json::from_str::<Vec<String>>(&line).unwrap())
            .unwrap();
        let (text_changed, text_node) = surface.with_ui(|ui| {
            ui.tick();
            (ui.draw().words.clone(), ui.hit_test_bounds(10.0, 10.0))
        });
        assert_eq!(text_operations, ["replaceText"]);
        assert_ne!(before, text_changed);
        assert_eq!(node_before, text_node);

        let event: Value = serde_json::from_str(&view_call_at(&guest, "tap", 10, 10)).unwrap();
        assert_eq!(
            event,
            json!({"type":"action","action":"key","args":{"active":true}})
        );
        guest
            .eval("clear-ui-ops", "globalThis.__uiOps.length = 0")
            .unwrap();

        view_call(&guest, "tickView");
        let (after, node_after) = surface.with_ui(|ui| {
            ui.tick();
            (ui.draw().words.clone(), ui.hit_test_bounds(10.0, 10.0))
        });
        let operations = guest
            .with(|ctx| {
                ctx.globals()
                    .get::<_, Function>("__takeUiOps")?
                    .call::<_, String>(())
            })
            .map(|line| serde_json::from_str::<Vec<String>>(&line).unwrap())
            .unwrap();
        assert_ne!(text_changed, after);
        assert_eq!(node_before, node_after);
        assert_eq!(operations, ["setProp"]);
    }

    #[test]
    fn raw_keyboard_routes_keys_and_releases_pressed_feedback() {
        let guest = new_app_guest().unwrap();
        let surface = UiSurface::new(VIEWPORT);
        surface.feed_pak(system_view_pak());
        surface.mount(&guest).unwrap();
        install_system_framework(&guest, "keyboard-test", system_framework(), "{}").unwrap();
        guest.eval("pocket-pi-view-sdk", system_view_sdk()).unwrap();
        guest
            .eval(
                "keyboard-view",
                r#"
                  View.mount(() => View.Keyboard({
                    layer: "lower",
                    onKey: (key) => PocketPi.action("key", { key }),
                  }));
                "#,
            )
            .unwrap();

        view_call(&guest, "tickView");
        let (before, node_before) = surface.with_ui(|ui| {
            ui.tick();
            (ui.draw().words.clone(), ui.hit_test_bounds(10.0, 10.0))
        });

        view_call_at(&guest, "pointerDown", 10, 10);
        let pressed = surface.with_ui(|ui| {
            ui.tick();
            ui.draw().words.clone()
        });
        let event: Value = serde_json::from_str(&view_call_at(&guest, "tap", 10, 10)).unwrap();
        assert_eq!(
            event,
            json!({"type":"action","action":"key","args":{"key":"q"}})
        );

        view_call(&guest, "pointerUp");
        let (released, node_after) = surface.with_ui(|ui| {
            ui.tick();
            (ui.draw().words.clone(), ui.hit_test_bounds(10.0, 10.0))
        });
        assert_ne!(before, pressed);
        assert_eq!(before, released);
        assert_eq!(node_before, node_after);

        let delete: Value = serde_json::from_str(&view_call_at(&guest, "tap", 650, 300)).unwrap();
        assert_eq!(
            delete,
            json!({"type":"action","action":"key","args":{"key":"Backspace"}})
        );
    }

    #[test]
    fn projection_refresh_renders_the_new_sqlite_state() {
        let guest = new_app_guest().unwrap();
        let surface = UiSurface::new(VIEWPORT);
        surface.feed_pak(system_view_pak());
        surface.mount(&guest).unwrap();
        let database = Arc::new(Mutex::new(DbModule::new(DbStorage::Memory)));
        {
            let mut database = database.lock().unwrap();
            let handle = database.open("raw-projection-test");
            assert_eq!(
                database.exec(
                    handle,
                    "CREATE TABLE state(value INTEGER); INSERT INTO state VALUES(1)"
                ),
                0
            );
        }
        mount_db(&guest, database.clone(), false).unwrap();
        install_system_framework(&guest, "raw-projection-test", system_framework(), "{}").unwrap();
        guest.eval("pocket-pi-view-sdk", system_view_sdk()).unwrap();
        guest
            .eval(
                "raw-projection-view",
                r#"
                  const model = View.state(0);
                  PocketPi.projection.one(
                    "SELECT value FROM state LIMIT 1",
                    {},
                    (row) => model.set(row.value),
                  );
                  View.mount(() => View.Text({ text: `VALUE ${model.get()}` }));
                "#,
            )
            .unwrap();

        guest
            .with(|ctx| {
                let system: Object = ctx.globals().get("PocketPiSystem")?;
                system.get::<_, Function>("tickView")?.call::<_, String>(())
            })
            .unwrap();
        let before = surface.with_ui(|ui| {
            ui.tick();
            ui.draw().words.clone()
        });
        {
            let mut database = database.lock().unwrap();
            let handle = database.open("raw-projection-test");
            assert_eq!(database.exec(handle, "UPDATE state SET value=2"), 0);
        }
        guest
            .with(|ctx| {
                let system: Object = ctx.globals().get("PocketPiSystem")?;
                system
                    .get::<_, Function>("dataChanged")?
                    .call::<_, String>(())
            })
            .unwrap();
        let after = surface.with_ui(|ui| {
            ui.tick();
            ui.draw().words.clone()
        });
        assert_ne!(before, after);
    }

    #[test]
    fn source_app_runs_schema_actions_projection_view_and_resources() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("device");
        let package = source_package(&[
            ("app.json", SOURCE_APP_JSON.as_bytes()),
            (
                "schema.sql",
                b"CREATE TABLE entries(value TEXT NOT NULL, source TEXT NOT NULL); INSERT INTO entries VALUES('SCHEMA','install');",
            ),
            ("actions.js", SOURCE_ACTIONS.as_bytes()),
            ("view.js", SOURCE_VIEW.as_bytes()),
            ("assets/settings.json", br#"{"label":"READY"}"#),
        ]);
        let staged = stage_pocketapp_bytes(&package, &temp.path().join("staged")).unwrap();
        let catalog = InstalledAppIndex::load(&workspace, system_app_bundle()).unwrap();
        let mut supervisor = AppSupervisor::new(&workspace, catalog, Arc::new(NoServices)).unwrap();
        supervisor
            .activate_app(&staged.release_dir, staged.credentials)
            .unwrap();
        supervisor.open("source").unwrap();
        supervisor.frame_render(true).unwrap();
        assert_eq!(
            supervisor.tap(10, 10).unwrap(),
            json!({"type":"action","action":"record","args":{}})
        );

        for source in [ActionSource::Tool, ActionSource::Ui, ActionSource::Schedule] {
            let (response, result) = mpsc::channel();
            supervisor
                .action_runner
                .enqueue(
                    "source",
                    source,
                    "record",
                    Value::Null,
                    new_action_deadline(),
                    ActionCompletion::Response(response),
                )
                .unwrap();
            let outcome = result.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(!outcome.is_error, "{}", outcome.text);
            let result: Value = serde_json::from_str(&outcome.text).unwrap();
            assert_eq!(result["label"], "READY");
            assert_eq!(result["frozen"], true);
            assert_eq!(result["source"], source.as_str());
        }

        supervisor.frame_render(true).unwrap();
        assert_eq!(
            supervisor
                .cached_view("source")
                .unwrap()
                .projection_refreshes
                .get(),
            1
        );
        assert_eq!(source_rows(&supervisor), 4);

        drop(supervisor);
        let catalog = InstalledAppIndex::load(&workspace, system_app_bundle()).unwrap();
        let restored = AppSupervisor::new(&workspace, catalog, Arc::new(NoServices)).unwrap();
        assert_eq!(source_rows(&restored), 4);
    }

    #[test]
    fn view_only_source_app_installs_without_an_action_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("device");
        let package = source_package(&[
            (
                "app.json",
                br#"{
                  "format":1,
                  "frameworkApi":1,
                  "id":"view-only",
                  "title":"View Only",
                  "description":"No Actions",
                  "version":"1",
                  "schemaVersion":1,
                  "capabilities":["data.sqlite"],
                  "tools":[],
                  "schedules":[]
                }"#,
            ),
            ("schema.sql", b"CREATE TABLE state(id INTEGER PRIMARY KEY);"),
            ("actions.js", b"PocketPi.defineActions({});"),
            (
                "view.js",
                b"View.mount(() => View.Text({ text: 'VIEW ONLY' }));",
            ),
        ]);
        let staged = stage_pocketapp_bytes(&package, &temp.path().join("staged")).unwrap();
        let catalog = InstalledAppIndex::load(&workspace, system_app_bundle()).unwrap();
        let mut supervisor = AppSupervisor::new(&workspace, catalog, Arc::new(NoServices)).unwrap();

        supervisor
            .activate_app(&staged.release_dir, staged.credentials)
            .unwrap();
        supervisor.open("view-only").unwrap();
        supervisor.frame_render(true).unwrap();

        assert_eq!(supervisor.active_id(), "view-only");
        assert!(!supervisor
            .action_runner
            .configs
            .lock()
            .unwrap()
            .contains_key("view-only"));
    }

    #[test]
    fn source_package_confines_declared_resources() {
        let temp = tempfile::tempdir().unwrap();
        let package = source_package(&[
            ("app.json", SOURCE_APP_JSON.as_bytes()),
            (
                "schema.sql",
                b"CREATE TABLE entries(value TEXT, source TEXT);",
            ),
            ("actions.js", SOURCE_ACTIONS.as_bytes()),
            ("view.js", SOURCE_VIEW.as_bytes()),
            ("assets/settings.json", br#"{"label":"READY"}"#),
            ("assets/undeclared.json", b"{}"),
        ]);
        let staging = temp.path().join("staged");
        assert!(stage_pocketapp_bytes(&package, &staging).is_err());
        assert!(!staging.exists());
        assert!(package_file_path("assets/../outside.json").is_err());
    }

    #[test]
    fn view_queries_cannot_write_sqlite() {
        let guest = new_app_guest().unwrap();
        let database = Arc::new(Mutex::new(DbModule::new(DbStorage::Memory)));
        {
            let mut database = database.lock().unwrap();
            let handle = database.open("readonly-test");
            assert_eq!(
                database.exec(
                    handle,
                    "CREATE TABLE items(value INTEGER); INSERT INTO items VALUES(1)"
                ),
                0
            );
        }
        mount_db(&guest, database.clone(), false).unwrap();
        install_system_framework(&guest, "readonly-test", system_framework(), "{}").unwrap();

        let error = guest
            .eval("view-write", r#"PocketPi.data.query("DELETE FROM items");"#)
            .unwrap_err();
        assert!(error.to_string().contains("readonly"));

        let remaining: Value = {
            let mut database = database.lock().unwrap();
            let handle = database.open("readonly-test");
            serde_json::from_str(&database.query(
                handle,
                "SELECT COUNT(*) AS count FROM items",
                "[]",
            ))
            .unwrap()
        };
        assert_eq!(remaining["rows"][0][0], 1);
    }

    #[test]
    fn app_tool_request_carries_the_single_80_second_deadline() {
        assert_eq!(APP_ACTION_TIMEOUT, Duration::from_secs(80));
        let temp = tempfile::tempdir().unwrap();
        install_fixture(
            temp.path(),
            "search",
            r#"{"id":"search","description":"Research","version":"1","toolNamespace":"research","tools":[{"name":"research.query","action":"query","parameters":{"type":"object"}}],"schedules":[]}"#,
        );
        let catalog = InstalledAppIndex::load(temp.path(), system_app_bundle()).unwrap();
        let (tools, requests) = RoutedToolHost::new(Arc::new(NoTools), catalog);
        let call = std::thread::spawn(move || tools.execute("call", "research.query", "{}"));
        let request = requests.recv().unwrap();
        let remaining = request.deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero());
        assert!(remaining <= APP_ACTION_TIMEOUT);
        request
            .response
            .send(ToolResult {
                text: "done".into(),
                ..ToolResult::default()
            })
            .unwrap();
        assert_eq!(call.join().unwrap().text, "done");
    }

    #[test]
    fn missed_schedules_advance_in_constant_time() {
        let hour = 60 * 60;
        let ten_years = 10 * 365 * 24 * hour;
        let next = next_schedule_run(hour, hour, ten_years);
        assert!(next > ten_years);
        assert_eq!(next % hour, 0);
    }

    #[test]
    fn schedule_completion_reports_the_action_result() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("actions.js");
        std::fs::write(
            &source,
            r#"PocketPi.defineActions({ run() { const end = Date.now() + 50; while (Date.now() < end) {} throw new Error("scheduled failure"); } });"#,
        )
        .unwrap();
        let runner = ActionRunner::start(
            vec![ActionConfig {
                app_id: "scheduled".into(),
                source_path: source,
                framework: Arc::from(system_framework()),
                net_sdk: None,
                resources: Arc::from("{}"),
                database: Arc::new(Mutex::new(DbModule::new(DbStorage::Memory))),
                revision: Arc::new(AtomicU32::new(0)),
                net: false,
            }],
            Arc::new(NoServices),
        )
        .unwrap();

        runner
            .enqueue(
                "scheduled",
                ActionSource::Schedule,
                "run",
                Value::Null,
                new_action_deadline(),
                ActionCompletion::Schedule("tick".into()),
            )
            .unwrap();
        assert!(runner.busy());

        let completed = (0..500)
            .find_map(|_| {
                let result = runner.drain_schedule_results().into_iter().next();
                if result.is_none() {
                    std::thread::sleep(Duration::from_millis(1));
                }
                result
            })
            .expect("scheduled Action result");
        assert_eq!(completed.app_id, "scheduled");
        assert_eq!(completed.schedule_id, "tick");
        assert!(completed.result.is_error);
        assert!(completed.result.text.contains("scheduled failure"));
    }

    #[test]
    fn failed_install_leaves_no_app() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("device");
        let staging = temp.path().join("staged-release");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(
            staging.join("app.json"),
            r#"{"format":1,"frameworkApi":1,"id":"broken","title":"Broken","description":"Broken","version":"1","schemaVersion":1,"capabilities":[],"resources":{},"tools":[],"schedules":[]}"#,
        )
        .unwrap();
        std::fs::write(
            staging.join("schema.sql"),
            "CREATE TABLE state(value INTEGER);",
        )
        .unwrap();
        std::fs::write(
            staging.join("actions.js"),
            "PocketPi.defineActions({ run() {} });",
        )
        .unwrap();
        std::fs::write(staging.join("view.js"), "").unwrap();

        let index = InstalledAppIndex::load(&workspace, system_app_bundle()).unwrap();
        let mut supervisor = AppSupervisor::new(&workspace, index, Arc::new(NoServices)).unwrap();
        supervisor
            .activate_app(&staging, BTreeMap::new())
            .unwrap_err();
        assert!(!workspace.join("apps/broken").exists());
    }

    #[test]
    fn installing_app_keeps_root_foreground_until_user_opens_it() {
        let temp = tempfile::tempdir().unwrap();
        let staging = temp.path().join("staged-exa");
        stage_checked_in_app(&staging, "exa");
        let catalog = InstalledAppIndex::load(temp.path(), system_app_bundle()).unwrap();
        let mut supervisor =
            AppSupervisor::new(temp.path(), catalog, Arc::new(TrackingServices::default()))
                .unwrap();

        let installed = supervisor
            .activate_app(
                &staging,
                BTreeMap::from([("exa.api-key".to_owned(), "secret".to_owned())]),
            )
            .unwrap();

        assert_eq!(installed.id, "exa");
        assert_eq!(supervisor.active_id(), ROOT_APP_ID);
        assert!(supervisor.catalog().app("exa").is_some());
        assert!(supervisor.cached_view("exa").is_some());

        supervisor.open("exa").unwrap();
        assert_eq!(supervisor.active_id(), "exa");
    }

    #[test]
    fn installing_an_existing_app_fails_without_changing_it() {
        let temp = tempfile::tempdir().unwrap();
        install_checked_in_app(temp.path(), "exa");
        let catalog = InstalledAppIndex::load(temp.path(), system_app_bundle()).unwrap();
        let mut supervisor =
            AppSupervisor::new(temp.path(), catalog, Arc::new(NoServices)).unwrap();
        let staging = temp.path().join("duplicate-exa");
        stage_checked_in_app(&staging, "exa");

        let error = supervisor
            .activate_app(&staging, BTreeMap::new())
            .unwrap_err();

        assert!(error.to_string().contains("already installed"));
        assert_eq!(
            supervisor.catalog().descriptor("exa").unwrap().version,
            "1.1.0"
        );
        assert!(temp.path().join("apps/exa/release").is_dir());
    }

    #[test]
    fn installing_clears_an_incomplete_app_root() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("device");
        let orphan = workspace.join("apps/exa/data/orphan.txt");
        std::fs::create_dir_all(orphan.parent().unwrap()).unwrap();
        std::fs::write(&orphan, "incomplete").unwrap();

        let staging = temp.path().join("staged-exa");
        stage_checked_in_app(&staging, "exa");
        let catalog = InstalledAppIndex::load(&workspace, system_app_bundle()).unwrap();
        assert!(catalog.descriptor("exa").is_none());
        let mut supervisor =
            AppSupervisor::new(&workspace, catalog, Arc::new(TrackingServices::default())).unwrap();

        supervisor
            .activate_app(
                &staging,
                BTreeMap::from([("exa.api-key".to_owned(), "secret".to_owned())]),
            )
            .unwrap();

        assert!(!orphan.exists());
        assert!(workspace.join("apps/exa/release").is_dir());
    }

    #[test]
    fn uninstall_removes_app_owned_state_and_allows_reinstall() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("device");
        let services = Arc::new(TrackingServices::default());
        let catalog = InstalledAppIndex::load(&workspace, system_app_bundle()).unwrap();
        let mut supervisor =
            AppSupervisor::new(&workspace, catalog.clone(), services.clone()).unwrap();
        let (tools, _requests) = RoutedToolHost::new(Arc::new(NoTools), catalog);
        let model_requests = Arc::new(Mutex::new(Vec::new()));
        supervisor
            .boot_agent(
                r#"{"model":"offline"}"#,
                Arc::new(RecordingBackend(model_requests.clone())),
                Arc::new(tools),
            )
            .unwrap();

        let staging = temp.path().join("staged-robinhood");
        stage_checked_in_app(&staging, "robinhood");
        let credentials = BTreeMap::from([(
            "robinhood.oauth-access-token".to_owned(),
            "secret".to_owned(),
        )]);
        supervisor
            .activate_app(&staging, credentials.clone())
            .unwrap();
        supervisor.open("robinhood").unwrap();
        std::fs::write(
            workspace.join("apps/robinhood/data/user-state"),
            "delete me",
        )
        .unwrap();

        let (response, result) = mpsc::channel();
        supervisor
            .action_runner
            .enqueue(
                "robinhood",
                ActionSource::Ui,
                "refreshPortfolio",
                Value::Null,
                new_action_deadline(),
                ActionCompletion::Response(response),
            )
            .unwrap();
        let _ = result.recv_timeout(Duration::from_secs(2)).unwrap();
        while supervisor.services_busy() {
            std::thread::yield_now();
        }

        assert_eq!(supervisor.schedules.schedules.len(), 1);
        assert!(supervisor.cached_view("robinhood").is_some());
        assert_eq!(*services.credentials.lock().unwrap(), credentials);
        assert!(supervisor.uninstall_app(ROOT_APP_ID).is_err());

        supervisor.uninstall_app("robinhood").unwrap();

        assert_eq!(supervisor.active_id(), ROOT_APP_ID);
        assert!(supervisor.catalog().descriptor("robinhood").is_none());
        assert!(supervisor.cached_view("robinhood").is_none());
        assert!(!supervisor.databases.contains_key("robinhood"));
        assert!(!supervisor.revisions.contains_key("robinhood"));
        assert!(supervisor.schedules.schedules.is_empty());
        assert!(!workspace.join("apps/robinhood").exists());
        assert!(services.credentials.lock().unwrap().is_empty());
        assert_eq!(
            *services.removed_apps.lock().unwrap(),
            vec!["robinhood".to_owned()]
        );

        let (response, result) = mpsc::channel();
        supervisor
            .action_runner
            .enqueue(
                "robinhood",
                ActionSource::Ui,
                "refreshPortfolio",
                Value::Null,
                new_action_deadline(),
                ActionCompletion::Response(response),
            )
            .unwrap();
        assert!(
            result
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .is_error
        );

        supervisor.prompt_agent("check available tools").unwrap();
        let mut done = false;
        for _ in 0..200 {
            done |= supervisor
                .frame()
                .unwrap()
                .iter()
                .any(|event| matches!(event, AgentEvent::Done));
            if done {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(done);
        let requests = model_requests.lock().unwrap();
        assert!(requests[0]["context"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|tool| !tool["name"]
                .as_str()
                .unwrap_or("")
                .starts_with("portfolio.")));
        drop(requests);

        let restarted = InstalledAppIndex::load(&workspace, system_app_bundle()).unwrap();
        assert!(restarted.descriptor("robinhood").is_none());

        let staging = temp.path().join("reinstall-robinhood");
        stage_checked_in_app(&staging, "robinhood");
        supervisor.activate_app(&staging, credentials).unwrap();
        assert!(supervisor.catalog().descriptor("robinhood").is_some());
    }

    #[test]
    fn app_revisions_coalesce_at_the_foreground_frame_boundary() {
        let temp = tempfile::tempdir().unwrap();
        install_checked_in_app(temp.path(), "exa");
        let catalog = InstalledAppIndex::load(temp.path(), system_app_bundle()).unwrap();
        let mut supervisor =
            AppSupervisor::new(temp.path(), catalog, Arc::new(NoServices)).unwrap();
        supervisor.open("exa").unwrap();

        let revision = supervisor.cached_view("exa").unwrap().revision.clone();
        revision.fetch_add(1, Ordering::Release);
        revision.fetch_add(1, Ordering::Release);
        revision.fetch_add(1, Ordering::Release);
        assert!(supervisor.active_projection_is_stale());

        supervisor.frame_render(false).unwrap();
        assert_eq!(
            supervisor
                .cached_view("exa")
                .unwrap()
                .projection_refreshes
                .get(),
            0
        );
        assert_eq!(
            supervisor
                .cached_view("exa")
                .unwrap()
                .last_seen_revision
                .get(),
            0
        );

        supervisor.frame_render(true).unwrap();
        assert_eq!(
            supervisor
                .cached_view("exa")
                .unwrap()
                .projection_refreshes
                .get(),
            1
        );
        assert_eq!(
            supervisor
                .cached_view("exa")
                .unwrap()
                .last_seen_revision
                .get(),
            3
        );
        assert!(!supervisor.active_projection_is_stale());

        for _ in 0..5 {
            supervisor.frame_render(true).unwrap();
        }
        assert_eq!(
            supervisor
                .cached_view("exa")
                .unwrap()
                .projection_refreshes
                .get(),
            1
        );

        supervisor.open(ROOT_APP_ID).unwrap();
        let background_ticks = supervisor.cached_view("exa").unwrap().ticks.get();
        revision.fetch_add(1, Ordering::Release);
        revision.fetch_add(1, Ordering::Release);
        for _ in 0..3 {
            supervisor.frame_render(true).unwrap();
        }
        assert_eq!(
            supervisor
                .cached_view("exa")
                .unwrap()
                .projection_refreshes
                .get(),
            1
        );
        assert_eq!(
            supervisor.cached_view("exa").unwrap().ticks.get(),
            background_ticks
        );

        supervisor.open("exa").unwrap();
        supervisor.frame_render(true).unwrap();
        assert_eq!(
            supervisor
                .cached_view("exa")
                .unwrap()
                .projection_refreshes
                .get(),
            2
        );
        assert_eq!(
            supervisor
                .cached_view("exa")
                .unwrap()
                .last_seen_revision
                .get(),
            5
        );
    }

    #[test]
    fn ordinary_views_load_on_demand_and_keep_the_three_most_recent() {
        let temp = tempfile::tempdir().unwrap();
        for app_id in ["one", "two", "three", "four"] {
            install_view_fixture(temp.path(), app_id);
        }
        let catalog = InstalledAppIndex::load(temp.path(), system_app_bundle()).unwrap();
        let mut supervisor =
            AppSupervisor::new(temp.path(), catalog, Arc::new(NoServices)).unwrap();
        assert!(supervisor.runtimes.is_empty());

        for app_id in ["one", "two", "three"] {
            supervisor.open(app_id).unwrap();
        }
        supervisor.open("one").unwrap();
        supervisor.open("four").unwrap();

        assert_eq!(
            supervisor
                .runtimes
                .iter()
                .map(|(app_id, _)| app_id.as_str())
                .collect::<Vec<_>>(),
            ["three", "one", "four"]
        );
        assert_eq!(supervisor.active_id(), "four");
    }

    #[test]
    fn actions_keep_the_three_most_recent_runtimes() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("actions.js");
        std::fs::write(
            &source,
            r#"
PocketPi.data.exec(`
  CREATE TABLE IF NOT EXISTS loads(value INTEGER NOT NULL);
  INSERT INTO loads(value) SELECT 0 WHERE NOT EXISTS (SELECT 1 FROM loads);
  UPDATE loads SET value=value+1;
`);
PocketPi.defineActions({ run() { return "ok"; } });
"#,
        )
        .unwrap();

        let mut databases = BTreeMap::new();
        let configs = ["one", "two", "three", "four"]
            .into_iter()
            .map(|app_id| {
                let database = Arc::new(Mutex::new(DbModule::new(DbStorage::Dir(
                    temp.path().join(app_id),
                ))));
                databases.insert(app_id, database.clone());
                ActionConfig {
                    app_id: app_id.into(),
                    source_path: source.clone(),
                    framework: Arc::from(include_str!("../../../system/framework.js")),
                    net_sdk: None,
                    resources: Arc::from("{}"),
                    database,
                    revision: Arc::new(AtomicU32::new(0)),
                    net: false,
                }
            })
            .collect();
        let runner = ActionRunner::start(configs, Arc::new(NoServices)).unwrap();

        for app_id in ["one", "two", "three", "one", "four", "two"] {
            let (response, rx) = mpsc::channel();
            runner
                .enqueue(
                    app_id,
                    ActionSource::Ui,
                    "run",
                    Value::Null,
                    new_action_deadline(),
                    ActionCompletion::Response(response),
                )
                .unwrap();
            assert!(!rx.recv_timeout(Duration::from_secs(2)).unwrap().is_error);
        }

        let load_count = |app_id: &str| {
            let mut database = databases[app_id].lock().unwrap();
            let handle = database.open(app_id);
            let value: Value =
                serde_json::from_str(&database.query(handle, "SELECT value FROM loads", "[]"))
                    .unwrap();
            value["rows"][0][0].as_i64().unwrap()
        };
        assert_eq!(load_count("one"), 1);
        assert_eq!(load_count("two"), 2);
        assert_eq!(load_count("three"), 1);
        assert_eq!(load_count("four"), 1);
    }

    const SOURCE_APP_JSON: &str = r#"{
      "format":1,
      "frameworkApi":1,
      "id":"source",
      "title":"Source App",
      "description":"Source runtime fixture",
      "version":"1",
      "schemaVersion":1,
      "capabilities":["data.sqlite"],
      "resources":{"settings":{"path":"assets/settings.json","type":"json"}},
      "tools":[{"name":"source.record","action":"record","parameters":{"type":"object"}}],
      "schedules":[{"id":"record","everyMinutes":1,"action":"record","args":{}}]
    }"#;

    const SOURCE_ACTIONS: &str = r#"
      PocketPi.defineActions({
        record(_args, context) {
          const settings = PocketPi.resources.get("settings");
          return PocketPi.data.transaction(() => {
            PocketPi.data.exec(`INSERT INTO entries VALUES('RUN','${context.source}')`);
            return {
              label: settings.label,
              frozen: Object.isFrozen(settings),
              source: context.source,
            };
          });
        },
      });
    "#;

    const SOURCE_VIEW: &str = r#"
      const count = View.state(0);
      PocketPi.projection.one(
        "SELECT COUNT(*) AS count FROM entries",
        {},
        (row) => count.set(row.count),
      );
      View.mount(() => View.Screen({
        children: View.ActionButton({
          label: `COUNT ${count.get()}`,
          onPress: () => PocketPi.action("record"),
        }),
      }));
    "#;

    fn source_package(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut archive = tar::Builder::new(&mut bytes);
            for (name, contents) in files {
                let mut header = tar::Header::new_gnu();
                header.set_size(contents.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                archive.append_data(&mut header, *name, *contents).unwrap();
            }
            archive.finish().unwrap();
        }
        bytes
    }

    fn source_rows(supervisor: &AppSupervisor) -> u64 {
        let mut database = supervisor.databases["source"].lock().unwrap();
        let handle = database.open("source");
        let result: Value =
            serde_json::from_str(&database.query(handle, "SELECT COUNT(*) FROM entries", "[]"))
                .unwrap();
        result["rows"][0][0].as_u64().unwrap()
    }

    fn install_view_fixture(workspace: &Path, app_id: &str) {
        let release = workspace.join("apps").join(app_id).join("release");
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(
            release.join("app.json"),
            json!({
                "format":1,
                "frameworkApi":1,
                "id":app_id,
                "title":app_id,
                "description":"View cache fixture",
                "version":"1",
                "schemaVersion":1,
                "capabilities":[],
                "resources":{},
                "tools":[],
                "schedules":[]
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            release.join("schema.sql"),
            "CREATE TABLE state(value INTEGER);",
        )
        .unwrap();
        std::fs::write(
            release.join("actions.js"),
            "PocketPi.defineActions({ run() {} });",
        )
        .unwrap();
        std::fs::write(
            release.join("view.js"),
            "View.mount(() => View.Text({ text: 'READY' }));",
        )
        .unwrap();
    }

    fn install_fixture(workspace: &Path, app_id: &str, descriptor: &str) {
        let release = workspace.join("apps").join(app_id).join("release");
        std::fs::create_dir_all(&release).unwrap();
        let mut descriptor: Value = serde_json::from_str(descriptor).unwrap();
        let descriptor = descriptor.as_object_mut().unwrap();
        descriptor.insert("format".into(), json!(1));
        descriptor.insert("frameworkApi".into(), json!(1));
        descriptor.insert("title".into(), json!("Fixture"));
        descriptor.insert("schemaVersion".into(), json!(1));
        descriptor.insert("capabilities".into(), json!([]));
        descriptor.insert("resources".into(), json!({}));
        std::fs::write(
            release.join("app.json"),
            serde_json::to_vec(descriptor).unwrap(),
        )
        .unwrap();
        std::fs::write(
            release.join("schema.sql"),
            "CREATE TABLE state(value INTEGER);",
        )
        .unwrap();
        std::fs::write(
            release.join("actions.js"),
            "PocketPi.defineActions({ query() {} });",
        )
        .unwrap();
        std::fs::write(
            release.join("view.js"),
            "View.mount(() => View.Text('FIXTURE'));",
        )
        .unwrap();
    }

    fn install_checked_in_app(workspace: &Path, app_id: &str) {
        let staging = workspace.join(format!(".staged-{app_id}"));
        stage_checked_in_app(&staging, app_id);
        let catalog = InstalledAppIndex::load(workspace, system_app_bundle()).unwrap();
        let mut supervisor =
            AppSupervisor::new(workspace, catalog, Arc::new(TrackingServices::default())).unwrap();
        let credentials = match app_id {
            "exa" => BTreeMap::from([("exa.api-key".to_owned(), "test-secret".to_owned())]),
            "robinhood" => BTreeMap::from([(
                "robinhood.oauth-access-token".to_owned(),
                "test-secret".to_owned(),
            )]),
            _ => BTreeMap::new(),
        };
        supervisor.activate_app(&staging, credentials).unwrap();
    }

    fn stage_checked_in_app(release: &Path, app_id: &str) {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("apps")
            .join(app_id);
        std::fs::create_dir_all(release).unwrap();
        for name in ["app.json", "schema.sql", "actions.js", "view.js"] {
            std::fs::copy(source.join(name), release.join(name)).unwrap();
        }
        let descriptor: Value =
            serde_json::from_slice(&std::fs::read(source.join("app.json")).unwrap()).unwrap();
        for resource in descriptor["resources"]
            .as_object()
            .into_iter()
            .flat_map(|resources| resources.values())
        {
            let path = resource["path"].as_str().unwrap();
            let destination = release.join(path);
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(source.join(path), destination).unwrap();
        }
    }
}
