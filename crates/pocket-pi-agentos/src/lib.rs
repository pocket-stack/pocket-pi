//! Pocket Pi's App runtime: bounded PocketJS Guest caches, app-owned FS/SQLite
//! state, namespaced Agent tools and native AppTask wakes.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
const BUILTIN_RELEASE: &str = "builtin-v1";
pub const POCKETJS_REVISION: &str = "e12cf12f82cc60b636368119d49a06eb9ed2a3d5";
const VIEWPORT: (f32, f32) = (720.0, 1280.0);
pub const MAX_POCKETAPP_BYTES: usize = 2 * 1024 * 1024;
const VIEW_RUNTIME_LIMIT: usize = 3;
const DATA_RUNTIME_LIMIT: usize = 3;

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
// Data Action execution and native transport all consume this same deadline.
pub const APP_ACTION_TIMEOUT: Duration = Duration::from_secs(80);

fn new_action_deadline() -> Instant {
    Instant::now() + APP_ACTION_TIMEOUT
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppDescriptor {
    pub id: String,
    #[serde(skip)]
    pub title: String,
    pub description: String,
    pub version: String,
    #[serde(default)]
    pub data_version: u32,
    #[serde(default)]
    pub tool_namespace: String,
    #[serde(default)]
    pub tools: Vec<Value>,
    #[serde(default)]
    pub provider_operations: Vec<String>,
    #[serde(default)]
    pub tasks: Vec<String>,
    #[serde(default)]
    pub schedules: Vec<AppSchedule>,
    #[serde(default)]
    pub native_services: NativeServices,
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
        let allowed = BTreeSet::from([
            "agent-app.json",
            "pocket.json",
            "plan.json",
            "app.js",
            "app.pak",
            "data-action.js",
            "credentials.json",
        ]);
        let required = BTreeSet::from([
            "agent-app.json",
            "pocket.json",
            "plan.json",
            "app.js",
            "app.pak",
        ])
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
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
            let name = std::str::from_utf8(&header[..name_end])
                .ok()
                .filter(|name| !name.is_empty() && !name.contains('/') && !name.contains('\\'))
                .ok_or_else(|| anyhow!("App package contains an invalid path"))?
                .to_owned();
            anyhow::ensure!(
                allowed.contains(name.as_str()),
                "unexpected App package file: {name}"
            );
            anyhow::ensure!(
                seen.insert(name.clone()),
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
                let mut file = std::fs::File::create(staging_dir.join(&name))?;
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
        anyhow::ensure!(
            required.is_subset(&seen),
            "App package is missing required files"
        );
        let descriptor_id: String =
            serde_json::from_slice::<Value>(&std::fs::read(staging_dir.join("agent-app.json"))?)?
                ["id"]
                .as_str()
                .ok_or_else(|| anyhow!("agent-app.json is missing id"))?
                .to_owned();
        let app = load_release(staging_dir, &descriptor_id, false)?;
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
    pub pocket_json: &'static str,
    pub js: &'static str,
    pub agent_js: &'static str,
    pub pak: &'static [u8],
}

impl SystemAppBundle {
    pub const fn new(
        descriptor_json: &'static str,
        pocket_json: &'static str,
        js: &'static str,
        agent_js: &'static str,
        pak: &'static [u8],
    ) -> Self {
        Self {
            descriptor_json,
            pocket_json,
            js,
            agent_js,
            pak,
        }
    }
}

pub const fn system_app_bundle() -> SystemAppBundle {
    SystemAppBundle::new(
        include_str!("../../../apps/pi-agent/agent-app.json"),
        include_str!("../../../apps/pi-agent/pocket.json"),
        include_str!("../../../apps/pi-agent/dist/app.js"),
        include_str!("../../../apps/pi-agent/dist/agent.js"),
        include_bytes!("../../../apps/pi-agent/dist/app.pak"),
    )
}

#[derive(Clone, Debug)]
struct InstalledApp {
    descriptor: AppDescriptor,
    manifest: Value,
    release_dir: PathBuf,
}

#[derive(Clone, Debug)]
pub struct InstalledAppIndex {
    inner: Arc<RwLock<InstalledApps>>,
}

#[derive(Debug)]
struct InstalledApps {
    apps: BTreeMap<String, InstalledApp>,
    tool_owner: BTreeMap<String, String>,
}

impl InstalledAppIndex {
    pub fn load(workspace: &Path, system: SystemAppBundle) -> Result<Self> {
        seed_system_release(workspace, system)?;
        let system_release = workspace.join("data/view/releases").join(BUILTIN_RELEASE);
        let mut apps = BTreeMap::new();
        let root = load_release(&system_release, ROOT_APP_ID, true)?;
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
                let current_path = entry.path().join("current");
                let release = match std::fs::read_to_string(&current_path) {
                    Ok(value) => value.trim().to_owned(),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => {
                        return Err(error)
                            .with_context(|| format!("read {}", current_path.display()))
                    }
                };
                ensure_safe_component(&release, "release id")?;
                let installed = load_release(
                    &entry.path().join("releases").join(&release),
                    &app_id,
                    false,
                )?;
                anyhow::ensure!(
                    apps.insert(app_id.clone(), installed).is_none(),
                    "duplicate installed App id: {app_id}"
                );
            }
        }

        let mut tool_owner = BTreeMap::new();
        for app in apps.values() {
            validate_app_tools(app, &tool_owner)?;
            add_app_tools(app, &mut tool_owner);
        }
        Ok(Self {
            inner: Arc::new(RwLock::new(InstalledApps { apps, tool_owner })),
        })
    }

    fn tool_definitions(&self) -> Vec<Value> {
        self.inner
            .read()
            .expect("installed App index lock")
            .apps
            .values()
            .flat_map(|app| app.descriptor.tools.clone())
            .collect()
    }

    fn app_for_tool(&self, name: &str) -> Option<String> {
        self.inner
            .read()
            .expect("installed App index lock")
            .tool_owner
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
        validate_app_tools(app, &index.tool_owner)?;
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
        add_app_tools(&app, &mut index.tool_owner);
        index.apps.insert(app.descriptor.id.clone(), app);
    }
}

fn validate_app_tools(app: &InstalledApp, owners: &BTreeMap<String, String>) -> Result<()> {
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
        anyhow::ensure!(!owners.contains_key(name), "duplicate App tool: {name}");
    }
    Ok(())
}

fn add_app_tools(app: &InstalledApp, owners: &mut BTreeMap<String, String>) {
    for tool in &app.descriptor.tools {
        let name = tool["name"]
            .as_str()
            .expect("validated App tool name")
            .to_owned();
        owners.insert(name, app.descriptor.id.clone());
    }
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
        "credentials.json ids do not match agent-app.json"
    );
    Ok(())
}

fn load_release(release_dir: &Path, expected_id: &str, system: bool) -> Result<InstalledApp> {
    for required in [
        "agent-app.json",
        "pocket.json",
        "plan.json",
        "app.js",
        "app.pak",
    ] {
        anyhow::ensure!(
            release_dir.join(required).is_file(),
            "App {expected_id} release is missing {required}"
        );
    }
    anyhow::ensure!(
        release_dir.join("agent.js").is_file() == system,
        if system {
            "System App is missing agent.js"
        } else {
            "ordinary App may not contain agent.js"
        }
    );

    let mut descriptor: AppDescriptor = serde_json::from_slice(
        &std::fs::read(release_dir.join("agent-app.json"))
            .context("read installed agent-app.json")?,
    )
    .context("parse installed agent-app.json")?;
    anyhow::ensure!(descriptor.id == expected_id, "installed App id mismatch");
    ensure_safe_component(&descriptor.id, "App id")?;
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(release_dir.join("pocket.json")).context("read installed pocket.json")?,
    )
    .context("parse installed pocket.json")?;
    anyhow::ensure!(
        manifest.get("pocket").and_then(Value::as_u64) == Some(2),
        "App requires pocket.json v2"
    );
    anyhow::ensure!(
        manifest.get("name").and_then(Value::as_str) == Some(descriptor.id.as_str()),
        "pocket.json name does not match App id"
    );
    descriptor.title = manifest
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.is_empty())
        .ok_or_else(|| anyhow!("App {} pocket.json is missing title", descriptor.id))?
        .to_owned();
    anyhow::ensure!(
        manifest.get("version").and_then(Value::as_str) == Some(descriptor.version.as_str()),
        "App {} version differs between agent-app.json and pocket.json",
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
    for schedule in &descriptor.schedules {
        anyhow::ensure!(
            descriptor.tasks.iter().any(|task| task == &schedule.task),
            "{}.{} schedule references missing task {}",
            descriptor.id,
            schedule.id,
            schedule.task
        );
    }
    anyhow::ensure!(
        (descriptor.tools.is_empty() && descriptor.tasks.is_empty())
            || release_dir.join("data-action.js").is_file(),
        "App {} tools and tasks require data-action.js",
        descriptor.id
    );
    for capability in manifest
        .pointer("/engine/capabilities/requires")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        anyhow::ensure!(
            matches!(capability, "data.fs" | "data.sqlite" | "net.http"),
            "unsupported App capability: {capability}"
        );
    }
    let plan: Value = serde_json::from_slice(
        &std::fs::read(release_dir.join("plan.json")).context("read installed plan.json")?,
    )
    .context("parse installed plan.json")?;
    anyhow::ensure!(
        plan.get("runtime").and_then(Value::as_str) == Some("pocket-pi-agentos")
            && plan.get("pocketjsRevision").and_then(Value::as_str) == Some(POCKETJS_REVISION)
            && plan.get("app").and_then(Value::as_str) == Some(descriptor.id.as_str()),
        "App {} plan.json does not target this runtime",
        descriptor.id
    );
    Ok(InstalledApp {
        descriptor,
        manifest,
        release_dir: release_dir.to_owned(),
    })
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

/// The native transport/security boundary used by App bundles. Implementations
/// own TLS, credentials and MCP sessions; Apps own operation selection,
/// normalization, SQLite and View behavior.
pub trait AppServiceHost: Send + Sync {
    /// Execute one policy-checked synchronous service call without outliving
    /// the App Data Action's absolute deadline.
    fn call(
        &self,
        app_id: &str,
        service: &str,
        operation: &str,
        args: &Value,
        deadline: Instant,
    ) -> Result<Value, String>;

    /// Execute one policy-checked PocketJS HTTP request. This is called only
    /// from the native NET worker, never from the QuickJS/App Data thread.
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

    fn remove_credentials(&self, _ids: &[String]) -> std::result::Result<(), String> {
        Ok(())
    }
}

/// One SQLite module instance per App. View and background Data Action guests
/// share this owner, so ESP32's `unix-none` VFS never has two independent
/// connections racing on the same file. Network waits never hold this mutex;
/// only bounded SQLite operations and transactions do.
type SharedDb = Arc<Mutex<DbModule>>;
type AppRevision = Arc<AtomicU32>;

const DATA_ACTION_QUEUE: usize = 8;
const NET_COMPLETION_QUEUE: usize = 2;
const NET_WORKER_STACK_BYTES: usize = 96 * 1024;
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
    deadline: Instant,
    response: Option<mpsc::Sender<ToolResult>>,
}

#[derive(Clone)]
struct DataAppConfig {
    app_id: String,
    source_path: PathBuf,
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
            NetFailure::new("unavailable", "HTTP request has no active App Data Action")
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
        .ok_or_else(|| NetFailure::new("timeout", "App Data Action deadline expired"))?;
    Ok(remaining.as_millis().clamp(1, u128::from(u32::MAX)) as u32)
}

struct DataActionRuntime {
    guest: Guest,
    net: Option<NetSurface<AppNetTransport>>,
    action_deadline: Rc<Cell<Option<Instant>>>,
    _database: SharedDb,
    _revision: AppRevision,
}

impl DataActionRuntime {
    fn load(config: &DataAppConfig, services: Arc<dyn AppServiceHost>) -> Result<Self> {
        let guest = new_app_guest().context("create App Data Action Guest")?;
        mount_shared_db(&guest, config.database.clone())?;
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
            net,
            action_deadline,
            _database: config.database.clone(),
            _revision: config.revision.clone(),
        })
    }

    fn invoke(&self, request: &DataActionRequest) -> Result<ToolResult> {
        anyhow::ensure!(
            Instant::now() < request.deadline,
            "{} timed out before its Data Action started",
            request.name
        );
        self.action_deadline.set(Some(request.deadline));
        let result = if let Some(net) = &self.net {
            self.invoke_net(request, net)
        } else {
            let method = match request.kind {
                DataActionKind::Task => "invokeTask",
                DataActionKind::Tool => "invokeTool",
            };
            let line: Result<String> = self.guest.with(|ctx| {
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
            });
            line.and_then(|line| parse_data_result(&line))
        };
        self.action_deadline.set(None);
        result
    }

    fn invoke_net(
        &self,
        request: &DataActionRequest,
        net: &NetSurface<AppNetTransport>,
    ) -> Result<ToolResult> {
        let method = match request.kind {
            DataActionKind::Task => "beginInvokeTask",
            DataActionKind::Tool => "beginInvokeTool",
        };
        self.guest.with(|ctx| -> Result<()> {
            let data: Object = ctx.globals().get("PocketPiData")?;
            let function: Function = data.get(method)?;
            function.call::<_, ()>((request.name.clone(), request.args.to_string()))?;
            Ok(())
        })?;
        loop {
            net.begin_tick();
            let line = self.guest.with(|ctx| -> Result<Option<String>> {
                let data: Object = ctx.globals().get("PocketPiData")?;
                let tick: Function = data.get("tick")?;
                tick.call::<_, ()>(())?;
                let poll: Function = data.get("pollResult")?;
                Ok(poll.call::<_, Option<String>>(())?)
            })?;
            self.guest.drain_jobs();
            if let Some(line) = line {
                return parse_data_result(&line);
            }
            anyhow::ensure!(
                Instant::now() < request.deadline,
                "PocketPiData.{method} timed out"
            );
            yield_scheduler_tick();
        }
    }
}

fn parse_data_result(line: &str) -> Result<ToolResult> {
    let value: Value = serde_json::from_str(line).context("parse Data Action result")?;
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

struct AppDataRunner {
    tx: mpsc::SyncSender<DataActionRequest>,
    configs: Arc<Mutex<BTreeMap<String, DataAppConfig>>>,
    next_run_id: AtomicU32,
    busy: Arc<AtomicBool>,
}

impl AppDataRunner {
    fn start(configs: Vec<DataAppConfig>, services: Arc<dyn AppServiceHost>) -> Result<Self> {
        let (tx, rx) = mpsc::sync_channel::<DataActionRequest>(DATA_ACTION_QUEUE);
        let configs = Arc::new(Mutex::new(
            configs
                .into_iter()
                .map(|config| (config.app_id.clone(), config))
                .collect::<BTreeMap<_, _>>(),
        ));
        let worker_configs = configs.clone();
        let busy = Arc::new(AtomicBool::new(false));
        let worker_busy = busy.clone();
        std::thread::Builder::new()
            .name("app-data".to_owned())
            .stack_size(DATA_ACTION_STACK_BYTES)
            .spawn(move || {
                let mut runtimes = Vec::<(String, DataActionRuntime)>::new();
                while let Ok(request) = rx.recv() {
                    worker_busy.store(true, Ordering::Release);
                    let result = (|| -> Result<ToolResult> {
                        let runtime = match take_runtime(&mut runtimes, &request.app_id) {
                            Some(runtime) => runtime,
                            None => {
                                make_runtime_room(&mut runtimes, DATA_RUNTIME_LIMIT, None);
                                let config = worker_configs
                                    .lock()
                                    .map_err(|_| {
                                        anyhow!("App Data Action config lock was poisoned")
                                    })?
                                    .get(&request.app_id)
                                    .cloned()
                                    .ok_or_else(|| {
                                        anyhow!("{} has no Data Action", request.app_id)
                                    })?;
                                DataActionRuntime::load(&config, services.clone())?
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
                                "App Data Action run={} {}.{} failed: {}",
                                request.run_id,
                                request.app_id,
                                request.name,
                                result.text
                            );
                            result
                        }
                        Ok(result) => result,
                        Err(error) => {
                            log::error!(
                                "App Data Action run={} {}.{} crashed: {error:#}",
                                request.run_id,
                                request.app_id,
                                request.name
                            );
                            tool_error(format!("{}: {error:#}", request.name))
                        }
                    };
                    if let Some(response) = request.response {
                        let _ = response.send(result);
                    }
                    worker_busy.store(false, Ordering::Release);
                }
            })
            .context("start App Data Action runner")?;
        Ok(Self {
            tx,
            configs,
            next_run_id: AtomicU32::new(1),
            busy,
        })
    }

    fn enqueue(
        &self,
        app_id: &str,
        kind: DataActionKind,
        name: &str,
        args: Value,
        deadline: Instant,
        response: Option<mpsc::Sender<ToolResult>>,
    ) -> Result<u64> {
        let run_id = u64::from(self.next_run_id.fetch_add(1, Ordering::Relaxed));
        self.tx
            .try_send(DataActionRequest {
                run_id,
                app_id: app_id.to_owned(),
                kind,
                name: name.to_owned(),
                args,
                deadline,
                response,
            })
            .map_err(|error| anyhow!("queue App Data Action: {error}"))?;
        Ok(run_id)
    }

    fn busy(&self) -> bool {
        self.busy.load(Ordering::Acquire)
    }

    fn register(&self, config: DataAppConfig) -> Result<()> {
        let mut configs = self
            .configs
            .lock()
            .map_err(|_| anyhow!("App Data Action config lock was poisoned"))?;
        anyhow::ensure!(
            !configs.contains_key(&config.app_id),
            "App {} Data Action is already registered",
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
}

struct AppRuntime {
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

impl AppRuntime {
    fn load(
        app: &InstalledApp,
        fs_root: &Path,
        tmp_root: &Path,
        db: SharedDb,
        revision: AppRevision,
    ) -> Result<Self> {
        std::fs::create_dir_all(fs_root)?;
        let guest = new_app_guest().context("create App View Guest")?;
        let surface = UiSurface::new(VIEWPORT);
        let pak = std::fs::read(app.release_dir.join("app.pak")).context("read App pak")?;
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

        let source =
            std::fs::read_to_string(app.release_dir.join("app.js")).context("read App bundle")?;
        eval_bundle(&guest, &app.descriptor.id, &source)?;
        anyhow::ensure!(
            guest.has_frame(),
            "{} bundle installed no frame()",
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
        // committed Data Action made that revision stale.
        self.call_method("tick", ()).map(|_: String| ())?;
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

    fn update(&self, projection: &Value) -> Result<()> {
        self.call_method("update", (projection.to_string(),))
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
        self.call_optional_method("pointerDown", (x as i32, y as i32))
    }

    fn pointer_up(&self) -> Result<()> {
        self.call_optional_method("pointerUp", ())
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

    fn call_optional_method<A>(&self, name: &str, args: A) -> Result<()>
    where
        A: for<'js> pocket_mod::qjs::function::IntoArgs<'js>,
    {
        self.guest.with(|ctx| {
            let app: Object = ctx
                .globals()
                .get("PocketPiApp")
                .map_err(|error| anyhow!("PocketPiApp missing: {error}"))?;
            let Some(function) = app.get::<_, Function>(name).ok() else {
                return Ok(());
            };
            function
                .call::<_, String>(args)
                .catch(&ctx)
                .map_err(|error| anyhow!("PocketPiApp.{name}: {error}"))?;
            Ok(())
        })
    }
}

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
                let result = module.lock().map_or(1, |mut db| db.exec(handle, &sql));
                yield_after_db_call();
                result
            })?,
        )?;
        let module = db.clone();
        ns.set(
            "query",
            Function::new(
                ctx.clone(),
                move |handle: i32, sql: String, args: String| -> String {
                    let result = module.lock().map_or_else(
                        |_| json!({"error":"App database owner is unavailable"}).to_string(),
                        |mut db| db.query(handle, &sql, &args),
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
                        .ok_or_else(|| "App service call has no active Data Action".to_owned())
                        .and_then(|deadline| {
                            if Instant::now() >= deadline {
                                Err("App Data Action deadline expired".to_owned())
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
    data_runner: AppDataRunner,
    databases: BTreeMap<String, SharedDb>,
    revisions: BTreeMap<String, AppRevision>,
    /// The Pi Agent System App is booted once and remains resident for the
    /// entire supervisor lifetime. Foreground navigation never replaces it.
    system: AppRuntime,
    agent: Option<GuestAgent>,
    agent_tools: Option<Arc<dyn ToolHost>>,
    // Least recently used first. Only the selected View advances.
    runtimes: Vec<(String, AppRuntime)>,
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
        let schedules = AppScheduleStore::load(&workspace, &catalog)?;
        let mut databases = BTreeMap::new();
        let mut revisions = BTreeMap::new();
        for descriptor in catalog.descriptors() {
            let (_, db_root, _) = data_paths(&workspace, &descriptor.id);
            std::fs::create_dir_all(&db_root)?;
            reset_development_database(&workspace, &descriptor, &db_root)?;
            databases.insert(
                descriptor.id.clone(),
                Arc::new(Mutex::new(DbModule::new(DbStorage::Dir(db_root)))),
            );
            revisions.insert(descriptor.id.clone(), Arc::new(AtomicU32::new(0)));
        }
        let data_configs = catalog
            .apps()
            .into_iter()
            .filter_map(|app| {
                let descriptor = &app.descriptor;
                let source_path = app.release_dir.join("data-action.js");
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
                    net: app
                        .manifest
                        .pointer("/engine/capabilities/requires")
                        .and_then(Value::as_array)
                        .is_some_and(|capabilities| {
                            capabilities.iter().any(|value| value == "net.http")
                        }),
                })
            })
            .collect();
        let data_runner = AppDataRunner::start(data_configs, services.clone())?;
        log::info!("loading Pi Agent System App");
        let system = load_runtime(&workspace, &catalog, &databases, &revisions, ROOT_APP_ID)?;
        log::info!("Pi Agent System App loaded");
        Ok(Self {
            workspace,
            catalog,
            services,
            data_runner,
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
        self.services.busy() || self.data_runner.busy()
    }

    pub fn activate_new_app(
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
            &std::fs::read(staging_release.join("agent-app.json"))
                .context("read staged agent-app.json")?,
        )
        .context("parse staged agent-app.json")?;
        ensure_safe_component(&descriptor.id, "App id")?;
        anyhow::ensure!(
            descriptor.id != ROOT_APP_ID,
            "cannot install the System App"
        );
        ensure_safe_component(&descriptor.version, "release id")?;
        let app_root = self.workspace.join("apps").join(&descriptor.id);
        anyhow::ensure!(
            !app_root.exists(),
            "App {} is already installed",
            descriptor.id
        );

        let credential_ids = credentials.keys().cloned().collect::<Vec<_>>();
        let result =
            self.activate_new_app_inner(staging_release, descriptor, credentials, &app_root);
        if result.is_err() {
            if let Some(app_id) = app_root.file_name().and_then(|value| value.to_str()) {
                self.data_runner.unregister(app_id);
            }
            let _ = self.services.remove_credentials(&credential_ids);
            let _ = std::fs::remove_dir_all(&app_root);
        }
        result
    }

    fn activate_new_app_inner(
        &mut self,
        staging_release: &Path,
        descriptor: AppDescriptor,
        credentials: BTreeMap<String, String>,
        app_root: &Path,
    ) -> Result<AppDescriptor> {
        let mut app = load_release(staging_release, &descriptor.id, false)?;
        self.catalog.validate_insert(&app)?;

        let (fs_root, db_root, tmp_root) = data_paths(&self.workspace, &descriptor.id);
        std::fs::create_dir_all(&db_root)?;
        std::fs::create_dir_all(&tmp_root)?;
        reset_development_database(&self.workspace, &app.descriptor, &db_root)?;
        let database = Arc::new(Mutex::new(DbModule::new(DbStorage::Dir(db_root))));
        let revision = Arc::new(AtomicU32::new(0));
        make_runtime_room(
            &mut self.runtimes,
            VIEW_RUNTIME_LIMIT,
            self.active_app.as_deref(),
        );
        let runtime = AppRuntime::load(
            &app,
            &fs_root,
            &tmp_root,
            database.clone(),
            revision.clone(),
        )?;
        let net = app
            .manifest
            .pointer("/engine/capabilities/requires")
            .and_then(Value::as_array)
            .is_some_and(|capabilities| capabilities.iter().any(|value| value == "net.http"));
        let staged_data = staging_release.join("data-action.js");
        if staged_data.is_file() {
            let candidate = DataAppConfig {
                app_id: descriptor.id.clone(),
                source_path: staged_data,
                database: database.clone(),
                revision: revision.clone(),
                net,
            };
            DataActionRuntime::load(&candidate, self.services.clone())?;
        }

        let schedules = new_schedules(&app.descriptor);
        let schedule_path = app_root.join("data/.system/schedules.json");
        atomic_write(&schedule_path, &serde_json::to_vec(&schedules)?)?;

        let release_dir = app_root.join("releases").join(&descriptor.version);
        std::fs::create_dir_all(release_dir.parent().expect("release parent"))?;
        anyhow::ensure!(!release_dir.exists(), "App release already exists");
        std::fs::rename(staging_release, &release_dir).context("move staged App release")?;
        app.release_dir = release_dir.clone();

        let has_data_action = release_dir.join("data-action.js").is_file();
        if has_data_action {
            self.data_runner.register(DataAppConfig {
                app_id: descriptor.id.clone(),
                source_path: release_dir.join("data-action.js"),
                database: database.clone(),
                revision: revision.clone(),
                net,
            })?;
        }
        self.services
            .store_credentials(&credentials)
            .map_err(anyhow::Error::msg)?;
        let prior_tools = self.agent_tools.as_ref().map(|tools| tools.definitions());
        if let (Some(agent), Some(prior)) = (&self.agent, &prior_tools) {
            let mut next = prior.clone();
            next.extend(app.descriptor.tools.clone());
            agent
                .replace_tools(&self.system.guest, next)
                .map_err(anyhow::Error::msg)?;
        }
        if let Err(error) = atomic_write(&app_root.join("current"), descriptor.version.as_bytes()) {
            self.data_runner.unregister(&descriptor.id);
            if let (Some(agent), Some(prior)) = (&self.agent, prior_tools) {
                let _ = agent.replace_tools(&self.system.guest, prior);
            }
            return Err(error).context("activate App release");
        }

        self.catalog.insert_validated(app);
        self.databases.insert(descriptor.id.clone(), database);
        self.revisions.insert(descriptor.id.clone(), revision);
        self.runtimes.push((descriptor.id.clone(), runtime));
        self.schedules
            .register(descriptor.id.clone(), schedule_path, schedules);
        Ok(descriptor)
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
        let agent_source = std::fs::read_to_string(release.join("agent.js"))
            .context("read Pi Agent System App loop bundle")?;
        self.agent = Some(
            GuestAgent::mount_source(
                &self.system.guest,
                config_json,
                backend,
                tools.clone(),
                &agent_source,
            )
            .map_err(|error| anyhow!(error))?,
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

    pub fn update_root(&self, projection: &Value) -> Result<()> {
        self.system.update(projection)
    }

    pub fn tap(&self, x: u16, y: u16) -> Result<Value> {
        self.active().tap(x, y)
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
        let Some(app_id) = self.catalog.app_for_tool(name) else {
            let _ = response.send(tool_error(format!("unknown App tool: {name}")));
            return;
        };
        let args = serde_json::from_str(args_json).unwrap_or(Value::Null);
        match self.data_runner.enqueue(
            &app_id,
            DataActionKind::Tool,
            name,
            args,
            deadline,
            Some(response.clone()),
        ) {
            Ok(_) => {}
            Err(error) => {
                let _ = response.send(tool_error(format!("{name}: {error:#}")));
            }
        }
    }

    /// Runs a task requested by the currently visible App. The host calls this
    /// after presenting the App's immediate pressed/loading state so slow
    /// native services never hide touch feedback.
    pub fn invoke_active_task(&mut self, name: &str, args: &Value) -> ToolResult {
        let app_id = self.active_id().to_owned();
        match self.data_runner.enqueue(
            &app_id,
            DataActionKind::Task,
            name,
            args.clone(),
            new_action_deadline(),
            None,
        ) {
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
                new_action_deadline(),
                None,
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

    fn active(&self) -> &AppRuntime {
        self.active_app
            .as_deref()
            .and_then(|app_id| self.cached_view(app_id))
            .unwrap_or(&self.system)
    }

    fn cached_view(&self, app_id: &str) -> Option<&AppRuntime> {
        self.runtimes
            .iter()
            .find(|(cached_id, _)| cached_id == app_id)
            .map(|(_, runtime)| runtime)
    }
}

fn load_runtime(
    workspace: &Path,
    catalog: &InstalledAppIndex,
    databases: &BTreeMap<String, SharedDb>,
    revisions: &BTreeMap<String, AppRevision>,
    app_id: &str,
) -> Result<AppRuntime> {
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
    AppRuntime::load(&app, &fs_root, &tmp_root, db, revision)
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

fn seed_system_release(workspace: &Path, bundle: SystemAppBundle) -> Result<()> {
    let release_dir = workspace.join("data/view/releases").join(BUILTIN_RELEASE);
    std::fs::create_dir_all(&release_dir)?;
    atomic_write(&release_dir.join("app.js"), bundle.js.as_bytes())?;
    atomic_write(&release_dir.join("agent.js"), bundle.agent_js.as_bytes())?;
    atomic_write(&release_dir.join("app.pak"), bundle.pak)?;
    atomic_write(
        &release_dir.join("agent-app.json"),
        bundle.descriptor_json.as_bytes(),
    )?;
    atomic_write(
        &release_dir.join("pocket.json"),
        bundle.pocket_json.as_bytes(),
    )?;
    let manifest: Value = serde_json::from_str(bundle.pocket_json)?;
    let modules = manifest
        .pointer("/engine/capabilities/requires")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    atomic_write(
        &release_dir.join("plan.json"),
        &serde_json::to_vec_pretty(&json!({
            "runtime":"pocket-pi-agentos",
            "pocketjsRevision":POCKETJS_REVISION,
            "app":ROOT_APP_ID,
            "modules":modules
        }))?,
    )?;
    atomic_write(
        &workspace.join("data/view/current"),
        BUILTIN_RELEASE.as_bytes(),
    )?;
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
        if self.catalog.app_for_tool(name).is_none() {
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
                    return tool_error("App Data Action response channel closed");
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
    fn load(workspace: &Path, catalog: &InstalledAppIndex) -> Result<Self> {
        // AppTask declarations travel with the App release, while their
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
        item.next_run_at = next_schedule_run(item.next_run_at, item.every_seconds, now);
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

    fn register(&mut self, app_id: String, path: PathBuf, schedules: Vec<StoredSchedule>) {
        self.paths.insert(app_id, path);
        self.schedules.extend(schedules);
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
                task: declaration.task.clone(),
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

    struct NoTools;

    impl ToolHost for NoTools {
        fn definitions(&self) -> Vec<Value> {
            Vec::new()
        }

        fn execute(&self, _call_id: &str, name: &str, _args_json: &str) -> ToolResult {
            tool_error(format!("unexpected native Tool: {name}"))
        }
    }

    #[test]
    fn catalog_uses_each_apps_declared_tool_namespace() {
        let temp = tempfile::tempdir().unwrap();
        install_fixture(
            temp.path(),
            "search",
            r#"{"id":"search","description":"Research","version":"1","toolNamespace":"research","tools":[{"name":"research.query","parameters":{"type":"object"}}],"tasks":[],"schedules":[]}"#,
        );
        let catalog = InstalledAppIndex::load(temp.path(), system_app_bundle()).unwrap();

        assert_eq!(
            catalog.app_for_tool("research.query"),
            Some("search".into())
        );
        assert_eq!(catalog.descriptors().len(), 2);
    }

    #[test]
    fn catalog_rejects_a_tool_outside_its_declared_namespace() {
        let temp = tempfile::tempdir().unwrap();
        install_fixture(
            temp.path(),
            "search",
            r#"{"id":"search","description":"Research","version":"1","tools":[{"name":"other.query","parameters":{"type":"object"}}],"tasks":[],"schedules":[]}"#,
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
            r#"{"id":"search","description":"Research","version":"1","tools":[],"tasks":[],"schedules":[],"nativeServices":{"http":[{"method":"POST","urls":["https://example.com/search"],"allowedRequestHeaders":["content-type"],"credential":{"id":"search.api-key","header":"x-api-key"}}],"mcp":[]}}"#,
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
            id: "search".into(),
            title: "Search".into(),
            description: "Research".into(),
            version: "1".into(),
            data_version: 0,
            tool_namespace: "search".into(),
            tools: Vec::new(),
            provider_operations: Vec::new(),
            tasks: Vec::new(),
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
    fn app_tool_request_carries_the_single_80_second_deadline() {
        assert_eq!(APP_ACTION_TIMEOUT, Duration::from_secs(80));
        let temp = tempfile::tempdir().unwrap();
        install_fixture(
            temp.path(),
            "search",
            r#"{"id":"search","description":"Research","version":"1","toolNamespace":"research","tools":[{"name":"research.query","parameters":{"type":"object"}}],"tasks":[],"schedules":[]}"#,
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
    fn data_version_resets_only_that_apps_database_once() {
        let temp = tempfile::tempdir().unwrap();
        let db_root = temp.path().join("apps/notes/data");
        std::fs::create_dir_all(&db_root).unwrap();
        let database = db_root.join("notes.sqlite");
        std::fs::write(&database, "old-schema").unwrap();
        let mut descriptor = AppDescriptor {
            id: "notes".into(),
            title: "Notes".into(),
            description: "Notes App".into(),
            version: "1.0.0".into(),
            data_version: 3,
            tool_namespace: "notes".into(),
            tools: Vec::new(),
            provider_operations: Vec::new(),
            tasks: Vec::new(),
            schedules: Vec::new(),
            native_services: NativeServices::default(),
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
    fn failed_candidate_never_creates_current() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("device");
        let staging = temp.path().join("staged-release");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(
            staging.join("agent-app.json"),
            r#"{"id":"broken","description":"Broken","version":"1","tools":[],"tasks":[],"schedules":[]}"#,
        )
        .unwrap();
        std::fs::write(
            staging.join("pocket.json"),
            r#"{"pocket":2,"name":"broken","title":"Broken","version":"1","engine":{"capabilities":{"requires":[]}}}"#,
        )
        .unwrap();
        std::fs::write(
            staging.join("plan.json"),
            json!({
                "runtime":"pocket-pi-agentos",
                "pocketjsRevision":POCKETJS_REVISION,
                "app":"broken",
                "modules":[]
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(staging.join("app.js"), "").unwrap();
        std::fs::write(staging.join("app.pak"), []).unwrap();

        let index = InstalledAppIndex::load(&workspace, system_app_bundle()).unwrap();
        let mut supervisor = AppSupervisor::new(&workspace, index, Arc::new(NoServices)).unwrap();
        let error = supervisor
            .activate_new_app(&staging, BTreeMap::new())
            .unwrap_err();
        assert!(error.to_string().contains("frame") || error.to_string().contains("PocketPiApp"));
        assert!(!workspace.join("apps/broken/current").exists());
        assert!(!workspace.join("apps/broken").exists());
    }

    #[test]
    fn installing_app_keeps_root_foreground_until_user_opens_it() {
        let temp = tempfile::tempdir().unwrap();
        let staging = temp.path().join("staged-exa");
        stage_checked_in_app(&staging, "exa");
        let catalog = InstalledAppIndex::load(temp.path(), system_app_bundle()).unwrap();
        let mut supervisor =
            AppSupervisor::new(temp.path(), catalog, Arc::new(NoServices)).unwrap();

        let installed = supervisor
            .activate_new_app(&staging, BTreeMap::new())
            .unwrap();

        assert_eq!(installed.id, "exa");
        assert_eq!(supervisor.active_id(), ROOT_APP_ID);
        assert!(supervisor.catalog().app("exa").is_some());
        assert!(supervisor.cached_view("exa").is_some());

        supervisor.open("exa").unwrap();
        assert_eq!(supervisor.active_id(), "exa");
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
    fn data_actions_keep_the_three_most_recent_runtimes() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("data-action.js");
        std::fs::write(
            &source,
            r#"
const nativeDb = globalThis.db;
const handle = nativeDb.open("cache");
if (handle < 0) throw new Error("open cache.sqlite");
if (nativeDb.exec(handle, `
  CREATE TABLE IF NOT EXISTS loads(value INTEGER NOT NULL);
  INSERT INTO loads(value) SELECT 0 WHERE NOT EXISTS (SELECT 1 FROM loads);
  UPDATE loads SET value=value+1;
`) !== 0) throw new Error(nativeDb.lastError(handle));
globalThis.PocketPiData = {
  invokeTask() { return JSON.stringify({text:"ok", isError:false}); },
  invokeTool() { return JSON.stringify({text:"ok", isError:false}); },
};
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
                DataAppConfig {
                    app_id: app_id.into(),
                    source_path: source.clone(),
                    database,
                    revision: Arc::new(AtomicU32::new(0)),
                    net: false,
                }
            })
            .collect();
        let runner = AppDataRunner::start(configs, Arc::new(NoServices)).unwrap();

        for app_id in ["one", "two", "three", "one", "four", "two"] {
            let (response, rx) = mpsc::channel();
            runner
                .enqueue(
                    app_id,
                    DataActionKind::Task,
                    "run",
                    Value::Null,
                    new_action_deadline(),
                    Some(response),
                )
                .unwrap();
            assert!(!rx.recv_timeout(Duration::from_secs(2)).unwrap().is_error);
        }

        let load_count = |app_id: &str| {
            let mut database = databases[app_id].lock().unwrap();
            let handle = database.open("cache");
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

    fn install_view_fixture(workspace: &Path, app_id: &str) {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/exa");
        let release = workspace.join("apps").join(app_id).join("releases/r1");
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(
            release.join("agent-app.json"),
            json!({
                "id":app_id,
                "title":app_id,
                "description":"View cache fixture",
                "version":"1",
                "tools":[],
                "tasks":[],
                "schedules":[]
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            release.join("pocket.json"),
            json!({
                "pocket":2,
                "name":app_id,
                "title":app_id,
                "version":"1",
                "engine":{"capabilities":{"requires":[]}}
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            release.join("plan.json"),
            json!({
                "runtime":"pocket-pi-agentos",
                "pocketjsRevision":POCKETJS_REVISION,
                "app":app_id,
                "modules":[]
            })
            .to_string(),
        )
        .unwrap();
        std::fs::copy(source.join("dist/app.js"), release.join("app.js")).unwrap();
        std::fs::copy(source.join("dist/app.pak"), release.join("app.pak")).unwrap();
        std::fs::write(workspace.join("apps").join(app_id).join("current"), "r1").unwrap();
    }

    fn install_fixture(workspace: &Path, app_id: &str, descriptor: &str) {
        let release = workspace.join("apps").join(app_id).join("releases/r1");
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(release.join("agent-app.json"), descriptor).unwrap();
        std::fs::write(
            release.join("pocket.json"),
            json!({
                "pocket":2,
                "name":app_id,
                "title":"Fixture",
                "version":"1",
                "engine":{"capabilities":{"requires":[]}}
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            release.join("plan.json"),
            json!({
                "runtime":"pocket-pi-agentos",
                "pocketjsRevision":POCKETJS_REVISION,
                "app":app_id,
                "modules":[]
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(release.join("app.js"), "").unwrap();
        std::fs::write(release.join("app.pak"), []).unwrap();
        let descriptor: Value = serde_json::from_str(descriptor).unwrap();
        if descriptor["tools"]
            .as_array()
            .is_some_and(|tools| !tools.is_empty())
            || descriptor["tasks"]
                .as_array()
                .is_some_and(|tasks| !tasks.is_empty())
        {
            std::fs::write(release.join("data-action.js"), "").unwrap();
        }
        std::fs::write(workspace.join("apps").join(app_id).join("current"), "r1").unwrap();
    }

    fn install_checked_in_app(workspace: &Path, app_id: &str) {
        let release = workspace.join("apps").join(app_id).join("releases/r1");
        stage_checked_in_app(&release, app_id);
        std::fs::write(workspace.join("apps").join(app_id).join("current"), "r1").unwrap();
    }

    fn stage_checked_in_app(release: &Path, app_id: &str) {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("apps")
            .join(app_id);
        std::fs::create_dir_all(release).unwrap();
        for (from, to) in [
            ("agent-app.json", "agent-app.json"),
            ("pocket.json", "pocket.json"),
            ("dist/app.js", "app.js"),
            ("dist/app.pak", "app.pak"),
            ("dist/data-action.js", "data-action.js"),
        ] {
            std::fs::copy(source.join(from), release.join(to)).unwrap();
        }
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(source.join("pocket.json")).unwrap()).unwrap();
        std::fs::write(
            release.join("plan.json"),
            serde_json::to_vec(&json!({
                "runtime":"pocket-pi-agentos",
                "pocketjsRevision":POCKETJS_REVISION,
                "app":app_id,
                "modules":manifest.pointer("/engine/capabilities/requires").unwrap()
            }))
            .unwrap(),
        )
        .unwrap();
    }
}
