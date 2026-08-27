use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context as _, Result};
use pocket3d::gpu::{Gpu, OffscreenTarget};
use pocket_pi_agentos::{
    stage_pocketapp_bytes, system_app_bundle, AgentToolRequest, AppDescriptor, AppServiceHost,
    AppSupervisor, HttpRequest, InstalledAppIndex, NetFailure, RoutedToolHost, StagedApp,
    TransportCompletion, Viewport, MAX_POCKETAPP_BYTES, ROOT_APP_ID,
};
use pocket_pi_embedded::{AgentEvent, ToolHost};
use pocket_pi_tools::{CoreToolHost, PlatformTools};
use pocket_ui_wgpu::UiRenderer;
use serde_json::{json, Value};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

mod backend;

use backend::{BackendChoice, DemoKind};

const DEFAULT_VIEWPORT: Viewport = Viewport::new(720, 1280);

struct Args {
    screenshot: Option<PathBuf>,
    prompt: Option<String>,
    workspace: PathBuf,
    app: String,
    root_tap: Option<(u16, u16)>,
    viewport: Viewport,
    backend: BackendChoice,
    demo: Option<DemoKind>,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = parse_args()?;
    match args.demo {
        Some(DemoKind::AppIteration) => prepare_app_iteration_demo(&args.workspace)?,
        Some(DemoKind::AppAuthoring) => prepare_app_authoring_demo(&args.workspace)?,
        None => prepare_workspace(&args.workspace)?,
    }
    if let Some(path) = args.screenshot {
        headless(
            path,
            args.workspace,
            args.prompt,
            args.app,
            args.root_tap,
            args.viewport,
            args.backend,
        )
    } else {
        windowed(
            args.workspace,
            args.prompt,
            args.app,
            args.root_tap,
            args.viewport,
            args.backend,
            args.demo,
        )
    }
}

fn parse_args() -> Result<Args> {
    let raw_args = std::env::args().skip(1).collect::<Vec<_>>();
    if matches!(
        raw_args.as_slice(),
        [flag, name] if flag == "--demo" && matches!(name.as_str(), "app-iteration" | "app-authoring")
    ) {
        let kind = if raw_args[1] == "app-iteration" {
            DemoKind::AppIteration
        } else {
            DemoKind::AppAuthoring
        };
        let name = raw_args[1].clone();
        return Ok(Args {
            screenshot: None,
            prompt: None,
            workspace: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/esp32-sim/demos")
                .join(name),
            app: if matches!(kind, DemoKind::AppIteration) {
                "demo".into()
            } else {
                ROOT_APP_ID.into()
            },
            root_tap: None,
            viewport: Viewport::new(480, 800),
            backend: BackendChoice::Demo(kind),
            demo: Some(kind),
        });
    }
    let mut screenshot = None;
    let mut prompt = None;
    let mut workspace =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/esp32-sim/workspace");
    let mut app = ROOT_APP_ID.to_owned();
    let mut root_tap = None;
    let mut viewport = DEFAULT_VIEWPORT;
    let mut backend = std::env::var("POCKET_PI_BACKEND").unwrap_or_else(|_| "codex".into());
    let mut model = None;
    let mut args = raw_args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--screenshot" => screenshot = Some(PathBuf::from(next(&mut args, "--screenshot")?)),
            "--prompt" => prompt = Some(next(&mut args, "--prompt")?),
            "--workspace" => workspace = PathBuf::from(next(&mut args, "--workspace")?),
            "--app" => {
                app = match next(&mut args, &argument)?.as_str() {
                    "pi-agent" => ROOT_APP_ID.to_owned(),
                    value => value.to_owned(),
                }
            }
            "--viewport" => viewport = parse_viewport(&next(&mut args, "--viewport")?)?,
            "--backend" => backend = next(&mut args, "--backend")?,
            "--model" => model = Some(next(&mut args, "--model")?),
            "--tap" => {
                let value = next(&mut args, "--tap")?;
                let (x, y) = value
                    .split_once(',')
                    .ok_or_else(|| anyhow!("--tap expects x,y"))?;
                root_tap = Some((x.parse()?, y.parse()?));
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }
    Ok(Args {
        screenshot,
        prompt,
        workspace,
        app,
        root_tap,
        viewport,
        backend: BackendChoice::from_name(&backend, model).map_err(anyhow::Error::msg)?,
        demo: None,
    })
}

fn next(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String> {
    args.next().ok_or_else(|| anyhow!("{name} needs a value"))
}

fn parse_viewport(value: &str) -> Result<Viewport> {
    let (width, height) = value
        .split_once('x')
        .ok_or_else(|| anyhow!("--viewport expects WIDTHxHEIGHT"))?;
    let viewport = Viewport::new(width.parse()?, height.parse()?);
    anyhow::ensure!(
        viewport.width > 0 && viewport.height > 0,
        "viewport dimensions must be positive"
    );
    Ok(viewport)
}

fn prepare_workspace(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let memory = root.join("memory.md");
    if !memory.exists() {
        std::fs::write(
            memory,
            "# Pocket Pi memory\n\nThis directory simulates the ESP32 LittleFS workspace.\n",
        )?;
    }
    let notes = root.join("notes.txt");
    if !notes.exists() {
        std::fs::write(notes, "Pi Agent owns this top-level workspace.\n")?;
    }
    Ok(())
}

fn prepare_app_iteration_demo(workspace: &Path) -> Result<()> {
    if workspace.exists() {
        std::fs::remove_dir_all(workspace)?;
    }
    prepare_workspace(workspace)?;
    let staging = workspace.join(".demo-seed");
    std::fs::create_dir_all(&staging)?;
    for (name, source) in [
        ("app.json", include_str!("../demo/app-iteration/app.json")),
        (
            "schema.sql",
            include_str!("../demo/app-iteration/schema.sql"),
        ),
        (
            "actions.js",
            include_str!("../demo/app-iteration/actions.js"),
        ),
        ("view.js", include_str!("../demo/app-iteration/view.js")),
    ] {
        std::fs::write(staging.join(name), source)?;
    }
    let catalog = InstalledAppIndex::load(workspace, system_app_bundle())?;
    let mut supervisor = AppSupervisor::new(
        workspace,
        Viewport::new(480, 800),
        catalog,
        Arc::new(SimAppServices),
    )?;
    supervisor.apply_app(&staging, BTreeMap::new())?;
    Ok(())
}

fn prepare_app_authoring_demo(workspace: &Path) -> Result<()> {
    if workspace.exists() {
        std::fs::remove_dir_all(workspace)?;
    }
    prepare_workspace(workspace)
}

struct SimPlatform;

impl PlatformTools for SimPlatform {
    fn device_status(&self) -> Value {
        json!({
            "status":"ok",
            "board":"esp32-sim",
            "agentOs":true,
            "jsRuntime":"QuickJS via PocketJS",
            "simulated":true
        })
    }

    fn wifi_status(&self) -> Value {
        json!({"status":"connected","ssid":"macOS host network","simulated":true})
    }

    fn reboot(&self) -> Result<Value, String> {
        Ok(json!({"status":"scheduled","simulated":true}))
    }
}

/// Deterministic native fixtures keep the simulator useful without credentials.
/// The App code, SQLite writes and View are exactly the same releases as the board.
struct SimAppServices;

impl AppServiceHost for SimAppServices {
    fn call(
        &self,
        app_id: &str,
        service: &str,
        operation: &str,
        args: &Value,
        deadline: Instant,
    ) -> Result<Value, String> {
        if Instant::now() >= deadline {
            return Err("App Action deadline expired".into());
        }
        let tool_name = args.get("name").and_then(Value::as_str).unwrap_or("");
        let tool_args = args.get("arguments").unwrap_or(&Value::Null);
        if app_id == "robinhood"
            && std::env::var("POCKET_PI_SIM_ROBINHOOD_FAIL").as_deref() == Ok("1")
        {
            return Err("simulated Robinhood service outage".into());
        }
        if (app_id, service, operation) == ("robinhood", "mcp.client", "callTools") {
            let calls = args
                .get("calls")
                .and_then(Value::as_array)
                .ok_or_else(|| "simulated callTools requires calls".to_owned())?;
            let results = calls
                .iter()
                .map(|call| {
                    let name = call.get("name").and_then(Value::as_str).unwrap_or("");
                    let single = json!({
                        "name":name,
                        "arguments":call.get("arguments").unwrap_or(&Value::Null)
                    });
                    match self.call(app_id, service, "callTool", &single, deadline) {
                        Ok(value) => json!({"name":name,"ok":true,"value":value}),
                        Err(error) => json!({"name":name,"ok":false,"error":error}),
                    }
                })
                .collect::<Vec<_>>();
            return Ok(json!({"results":results}));
        }
        match (app_id, service, operation, tool_name) {
            ("robinhood", "mcp.client", "callTool", "get_accounts") => Ok(json!({
                "accounts":[
                    {"account_number":"SIM-AGENT-001","status":"active","type":"cash","agentic_allowed":true},
                    {"account_number":"SIM-IRA-002","status":"active","type":"traditional_ira"},
                    {"account_number":"SIM-JOINT-003","status":"active","type":"joint"}
                ]
            })),
            ("robinhood", "mcp.client", "callTool", "get_portfolio") => {
                let account = tool_args
                    .get("account_number")
                    .and_then(Value::as_str)
                    .unwrap_or("SIM-AGENT-001");
                let (equity, cash, buying_power, day_pnl, week_pnl) = match account {
                    "SIM-IRA-002" => ("84220.18", "4120.40", "4120.40", "-182.14", "1250.42"),
                    "SIM-JOINT-003" => ("31008.75", "5280.05", "10560.10", "205.80", "935.22"),
                    _ => ("15320.42", "1280.20", "2560.40", "128.35", "412.80"),
                };
                Ok(
                    json!({"account_number":account,"equity":equity,"cash":cash,"buying_power":buying_power,"day_pnl":day_pnl,"week_pnl":week_pnl}),
                )
            }
            ("robinhood", "mcp.client", "callTool", "get_equity_positions") => Ok(json!({
                "positions":[
                    {"symbol":"NVDA","quantity":"8","average_buy_price":"712.48","market_value":"7344.00"},
                    {"symbol":"AAPL","quantity":"12","average_buy_price":"218.32","market_value":"2784.00"},
                    {"symbol":"MSFT","quantity":"6","average_buy_price":"405.15","market_value":"2586.00"},
                    {"symbol":"META","quantity":"3","average_buy_price":"502.10","market_value":"1581.00"},
                    {"symbol":"AMZN","quantity":"5","average_buy_price":"191.25","market_value":"1012.50"},
                    {"symbol":"GOOGL","quantity":"4","average_buy_price":"174.82","market_value":"724.00"},
                    {"symbol":"TSLA","quantity":"2","average_buy_price":"332.50","market_value":"690.00"},
                    {"symbol":"AMD","quantity":"3","average_buy_price":"168.40","market_value":"522.00"},
                    {"symbol":"PLTR","quantity":"4","average_buy_price":"112.20","market_value":"468.00"},
                    {"symbol":"VTI","quantity":"2","average_buy_price":"289.75","market_value":"602.00"}
                ]
            })),
            ("robinhood", "mcp.client", "callTool", "get_equity_orders") => Ok(json!({"orders":[
                {"symbol":"NVDA","side":"buy","state":"filled","type":"limit","executed_quantity":"2","average_price":"918.00","created_at":"2026-08-08T09:42:00Z"},
                {"symbol":"AAPL","side":"sell","state":"filled","type":"market","executed_quantity":"4","average_price":"232.00","created_at":"2026-08-07T15:14:00Z"},
                {"symbol":"MSFT","side":"buy","state":"filled","type":"market","executed_quantity":"1","average_price":"431.00","created_at":"2026-08-06T18:28:00Z"},
                {"symbol":"META","side":"sell","state":"filled","type":"limit","executed_quantity":"1","average_price":"527.00","created_at":"2026-08-05T11:20:00Z"},
                {"symbol":"AMZN","side":"buy","state":"filled","type":"limit","executed_quantity":"3","average_price":"202.50","created_at":"2026-08-04T10:05:00Z"},
                {"symbol":"GOOGL","side":"buy","state":"cancelled","type":"limit","quantity":"2","price":"179.00","created_at":"2026-08-03T14:01:00Z"},
                {"symbol":"TSLA","side":"sell","state":"filled","type":"market","executed_quantity":"2","average_price":"345.00","created_at":"2026-08-02T16:32:00Z"},
                {"symbol":"AMD","side":"buy","state":"filled","type":"limit","executed_quantity":"3","average_price":"174.00","created_at":"2026-08-01T12:18:00Z"},
                {"symbol":"PLTR","side":"buy","state":"filled","type":"market","executed_quantity":"4","average_price":"117.00","created_at":"2026-07-31T17:11:00Z"}
            ]})),
            ("robinhood", "mcp.client", "callTool", "get_realized_pnl") => {
                Ok(json!({"realized_pnl":"682.15"}))
            }
            ("robinhood", "mcp.client", "callTool", name) => {
                Ok(json!({"operation":name,"simulated":true,"args":tool_args}))
            }
            _ => Err(format!(
                "unsupported simulated service call: {app_id}/{service}/{operation}"
            )),
        }
    }

    fn http(
        &self,
        app_id: &str,
        request: HttpRequest,
        deadline: Instant,
    ) -> std::result::Result<TransportCompletion, NetFailure> {
        if Instant::now() >= deadline {
            return Err(NetFailure::new("timeout", "App Action deadline expired"));
        }
        if app_id != "exa" || request.method != "POST" {
            return Err(NetFailure::new(
                "invalid_request",
                "simulator denied HTTP request",
            ));
        }
        let body: Value = serde_json::from_slice(&request.body)
            .map_err(|error| NetFailure::new("invalid_request", error.to_string()))?;
        let value = match request.url.as_str() {
            "https://api.exa.ai/search" => {
                let query = body
                    .get("query")
                    .and_then(Value::as_str)
                    .unwrap_or("Pocket Pi");
                json!({"results":[
                    {"title":format!("Research result for {query}"),"url":"https://example.com/research"},
                    {"title":"PocketJS runtime notes","url":"https://pocketjs.dev/docs/concepts/"}
                ]})
            }
            "https://api.exa.ai/contents" => json!({
                "results":[{"title":"Simulated Exa document","url":"https://example.com/research","text":"Local simulator fixture"}]
            }),
            _ => {
                return Err(NetFailure::new(
                    "invalid_request",
                    "simulator denied HTTP URL",
                ))
            }
        };
        Ok(TransportCompletion::Done {
            handle: request.handle,
            status: 200,
            url: request.url,
            headers: BTreeMap::from([("content-type".into(), "application/json".into())]),
            body: serde_json::to_vec(&value)
                .map_err(|error| NetFailure::new("other", error.to_string()))?,
        })
    }

    fn store_credentials(&self, _credentials: &BTreeMap<String, String>) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Clone)]
struct Message {
    role: &'static str,
    text: String,
}

struct Product {
    messages: Vec<Message>,
    agent_status: &'static str,
    model_label: String,
    native_tools: Arc<CoreToolHost>,
    agent_rx: Receiver<AgentToolRequest>,
    supervisor: AppSupervisor,
    last_schedule_poll: Instant,
    busy: bool,
    system_dirty: bool,
    pending_ui_action: Option<(String, Value)>,
    wifi_connected: Option<String>,
    wifi_networks: Vec<(&'static str, i32, bool)>,
    wifi_status: String,
    install_rx: Receiver<StagedApp>,
    install_root: PathBuf,
    install_slot: Arc<AtomicBool>,
    pending_install: Option<StagedApp>,
    install_ui: Option<InstallUi>,
    install_requested: bool,
    pending_uninstall: Option<String>,
    uninstall_error: Option<String>,
}

struct InstallUi {
    state: &'static str,
    descriptor: AppDescriptor,
    update: bool,
    current_version: Option<String>,
    current_schema_version: Option<u32>,
    error: Option<String>,
}

impl Product {
    fn new(
        workspace: PathBuf,
        viewport: Viewport,
        backend: BackendChoice,
        app: &str,
    ) -> Result<Self> {
        let model_label = match &backend {
            BackendChoice::Wireless {
                provider, model, ..
            } => {
                format!("{} / {model}", provider.id())
            }
            BackendChoice::Codex { model } => {
                format!("Codex / {}", model.as_deref().unwrap_or("coding-plan"))
            }
            BackendChoice::Demo(_) => "DEMO REPLAY".into(),
        };
        let services: Arc<dyn AppServiceHost> = Arc::new(SimAppServices);
        let catalog = InstalledAppIndex::load(&workspace, system_app_bundle())?;
        let mut supervisor = AppSupervisor::new(workspace.clone(), viewport, catalog, services)?;
        supervisor.open(app)?;

        let native_tools = Arc::new(CoreToolHost::new(workspace.clone(), Arc::new(SimPlatform)));
        let catalog = supervisor.catalog().clone();
        let native: Arc<dyn ToolHost> = native_tools.clone();
        let (routed, agent_rx) = RoutedToolHost::new(native, catalog);
        let config = backend.agent_config();
        let model = backend.build();
        supervisor.boot_agent(&config, model, Arc::new(routed))?;
        let (install_rx, install_slot) = start_install_server(&workspace)?;
        let install_root = workspace.join(".system/install");

        Ok(Self {
            messages: vec![Message {
                role: "assistant",
                text: "Pocket Pi AgentOS is ready.".into(),
            }],
            agent_status: "IDLE",
            model_label,
            native_tools,
            agent_rx,
            supervisor,
            last_schedule_poll: Instant::now(),
            busy: false,
            system_dirty: true,
            pending_ui_action: None,
            wifi_connected: Some("macOS host network".into()),
            wifi_networks: Vec::new(),
            wifi_status: "SIMULATED NETWORK READY".into(),
            install_rx,
            install_root,
            install_slot,
            pending_install: None,
            install_ui: None,
            install_requested: false,
            pending_uninstall: None,
            uninstall_error: None,
        })
    }

    fn send_prompt(&mut self, prompt: String) {
        if self.busy || prompt.trim().is_empty() {
            return;
        }
        self.messages.push(Message {
            role: "user",
            text: prompt.clone(),
        });
        self.messages.push(Message {
            role: "assistant",
            text: String::new(),
        });
        self.agent_status = "THINKING";
        self.busy = true;
        if let Err(error) = self.supervisor.prompt_agent(&prompt) {
            self.messages.last_mut().unwrap().text = format!("Agent is unavailable: {error:#}");
            self.agent_status = "FAULTED";
            self.busy = false;
        }
        self.system_dirty = true;
    }

    fn tap(&mut self, x: u16, y: u16) -> Result<()> {
        let action = self.supervisor.tap(x, y)?;
        match action.get("type").and_then(Value::as_str) {
            Some("action") => {
                if let Some(name) = action.get("action").and_then(Value::as_str) {
                    self.pending_ui_action = Some((
                        name.to_owned(),
                        action.get("args").cloned().unwrap_or(Value::Null),
                    ));
                }
            }
            Some("command") => {
                let args = action.get("args").unwrap_or(&Value::Null);
                match action.get("command").and_then(Value::as_str) {
                    Some("apps.open") => {
                        if let Some(app) = args.get("app").and_then(Value::as_str) {
                            self.supervisor.open(app)?;
                        }
                    }
                    Some("agent.submit") => {
                        if let Some(prompt) = args.get("prompt").and_then(Value::as_str) {
                            self.send_prompt(prompt.to_owned());
                        }
                    }
                    Some("apps.uninstall") => {
                        if let Some(app_id) = args.get("app").and_then(Value::as_str) {
                            if self.busy || self.supervisor.services_busy() {
                                self.uninstall_error = Some("APP SERVICES ARE BUSY".into());
                            } else {
                                self.pending_uninstall = Some(app_id.to_owned());
                                self.uninstall_error = None;
                            }
                        }
                    }
                    Some("apps.install")
                        if self
                            .install_ui
                            .as_ref()
                            .is_some_and(|ui| ui.state == "review") =>
                    {
                        if let Some(ui) = &mut self.install_ui {
                            ui.state = "installing";
                        }
                        self.install_requested = true;
                    }
                    Some("apps.dismissInstall") => {
                        if let Some(staged) = self.pending_install.take() {
                            self.supervisor.record_app_dismissal(
                                &staged.descriptor,
                                self.install_ui.as_ref().is_some_and(|ui| ui.update),
                            );
                            if let Some(path) = staged.release_dir.parent() {
                                let _ = std::fs::remove_dir_all(path);
                            }
                        }
                        self.install_ui = None;
                        self.install_slot.store(false, Ordering::Release);
                    }
                    Some("device.wifi.scan") => {
                        self.wifi_networks = vec![
                            ("PocketPi Lab", -42, true),
                            ("Studio Guest", -61, false),
                            ("ESP32 Testbench", -73, true),
                        ];
                        self.wifi_status = "3 NETWORKS FOUND".into();
                        self.system_dirty = true;
                    }
                    Some("device.wifi.connect") => {
                        if let Some(ssid) = args.get("ssid").and_then(Value::as_str) {
                            self.wifi_connected = Some(ssid.to_owned());
                            self.wifi_status = format!("CONNECTED TO {ssid}");
                            self.system_dirty = true;
                        }
                    }
                    Some("device.wifi.forget") => {
                        self.wifi_connected = None;
                        self.wifi_status = "WI-FI CREDENTIALS FORGOTTEN".into();
                        self.system_dirty = true;
                    }
                    Some("device.restart") => {
                        self.wifi_status = "SIMULATED RESTART REQUESTED".into();
                        self.system_dirty = true;
                    }
                    _ => {}
                }
                self.system_dirty = true;
            }
            _ => {}
        }
        Ok(())
    }

    fn pointer_down(&mut self, x: u16, y: u16) -> Result<()> {
        self.supervisor.pointer_down(x, y)?;
        self.tap(x, y)
    }

    fn pointer_up(&mut self) -> Result<()> {
        self.supervisor.pointer_up()
    }

    fn run_pending_ui_action(&mut self) {
        let Some((action, args)) = self.pending_ui_action.take() else {
            return;
        };
        let started = Instant::now();
        let result = self.supervisor.invoke_active_action(&action, &args);
        log::info!(
            "UI App Action {} finished in {}ms: {}",
            action,
            started.elapsed().as_millis(),
            result.text
        );
        self.system_dirty = true;
    }

    fn run_pending_install(&mut self) {
        if !self.install_requested || self.busy {
            return;
        }
        self.install_requested = false;
        let Some(staged) = self.pending_install.take() else {
            return;
        };
        let cleanup = staged.release_dir.parent().map(Path::to_path_buf);
        let result = self
            .supervisor
            .apply_app(&staged.release_dir, staged.credentials);
        if let Some(path) = cleanup {
            let _ = std::fs::remove_dir_all(path);
        }
        if let Some(ui) = &mut self.install_ui {
            match result {
                Ok(_) => ui.state = "success",
                Err(error) => {
                    ui.state = "failed";
                    ui.error = Some(format!("{error:#}"));
                }
            }
        }
        self.system_dirty = true;
    }

    fn run_pending_uninstall(&mut self) {
        let Some(app_id) = self.pending_uninstall.take() else {
            return;
        };
        if let Err(error) = self.supervisor.uninstall_app(&app_id) {
            self.uninstall_error = Some(format!("{error:#}"));
        }
        self.system_dirty = true;
    }

    fn poll(&mut self) -> Result<()> {
        if self.pending_install.is_none()
            && self.pending_uninstall.is_none()
            && !self.busy
            && !self.supervisor.services_busy()
        {
            if let Ok(staged) = self.install_rx.try_recv() {
                match self.supervisor.review_app(&staged) {
                    Ok(review) => {
                        self.install_ui = Some(InstallUi {
                            state: "review",
                            descriptor: staged.descriptor.clone(),
                            update: review.update,
                            current_version: review.current_version,
                            current_schema_version: review.current_schema_version,
                            error: None,
                        });
                        self.pending_install = Some(staged);
                    }
                    Err(error) => {
                        let current = self.supervisor.catalog().descriptor(&staged.descriptor.id);
                        if let Some(path) = staged.release_dir.parent() {
                            let _ = std::fs::remove_dir_all(path);
                        }
                        self.install_ui = Some(InstallUi {
                            state: "failed",
                            update: current.is_some(),
                            descriptor: staged.descriptor,
                            current_version: current.as_ref().map(|app| app.version.clone()),
                            current_schema_version: current.map(|app| app.schema_version),
                            error: Some(format!("{error:#}")),
                        });
                    }
                }
                self.supervisor.open(ROOT_APP_ID)?;
                self.system_dirty = true;
            }
        }
        while let Ok(request) = self.agent_rx.try_recv() {
            let pending_install = &mut self.pending_install;
            let install_ui = &mut self.install_ui;
            let pending_uninstall = &self.pending_uninstall;
            let install_root = &self.install_root;
            let install_slot = &self.install_slot;
            request.handle(&mut self.supervisor, |supervisor, path| {
                anyhow::ensure!(pending_install.is_none(), "another install is pending");
                anyhow::ensure!(pending_uninstall.is_none(), "an App uninstall is pending");
                anyhow::ensure!(!supervisor.services_busy(), "App services are busy");
                anyhow::ensure!(
                    install_slot
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok(),
                    "another install is pending"
                );
                let result = (|| -> Result<Value> {
                    let (staged, review) = supervisor.submit_app_checkout(path, install_root)?;
                    supervisor
                        .open(ROOT_APP_ID)
                        .expect("resident Pi Agent remains available");
                    let descriptor = staged.descriptor.clone();
                    *install_ui = Some(InstallUi {
                        state: "review",
                        descriptor: descriptor.clone(),
                        update: review.update,
                        current_version: review.current_version,
                        current_schema_version: review.current_schema_version,
                        error: None,
                    });
                    *pending_install = Some(staged);
                    Ok(json!({
                        "status":"pending_confirmation",
                        "app":descriptor.id,
                        "version":descriptor.version
                    }))
                })();
                if result.is_err() {
                    install_slot.store(false, Ordering::Release);
                }
                result
            });
            self.system_dirty = true;
        }
        if self.last_schedule_poll.elapsed() >= Duration::from_secs(1) {
            if self.install_ui.is_none() && self.pending_uninstall.is_none() && !self.busy {
                if let Some(wake) = self.native_tools.claim_due() {
                    self.send_prompt(wake.prompt);
                }
                for (action, result) in self.supervisor.poll_due_actions() {
                    log::info!("App Action {action}: {}", result.text);
                }
            }
            self.last_schedule_poll = Instant::now();
            self.system_dirty = true;
        }

        if self.system_dirty {
            self.supervisor.update_system(&self.system_facts())?;
            self.system_dirty = false;
        }
        // frame() always advances the Pi Agent System App, even while another
        // App owns the visible View. Model/tool work itself remains off-thread.
        for event in self.supervisor.frame()? {
            match event {
                AgentEvent::Ready => self.agent_status = "IDLE",
                AgentEvent::ResponseText(text) => {
                    if let Some(message) = self.messages.last_mut() {
                        message.text.push_str(&text);
                    }
                }
                AgentEvent::Done => {
                    self.agent_status = "IDLE";
                    self.busy = false;
                }
                AgentEvent::Failed(error) => {
                    if let Some(message) = self.messages.last_mut() {
                        message.text = format!("Agent failed: {error}");
                    }
                    self.agent_status = "FAULTED";
                    self.busy = false;
                }
            }
            self.system_dirty = true;
        }
        Ok(())
    }

    fn system_facts(&self) -> Value {
        let schedule = self.native_tools.schedule_projection();
        let schedule_next = match schedule.next_in_seconds {
            Some(seconds) => schedule.every_minutes.map_or_else(
                || format!("in {seconds}s"),
                |minutes| format!("in {seconds}s · every {minutes}m"),
            ),
            None => "not scheduled".to_owned(),
        };
        let install = self.install_ui.as_ref().map(|install| {
            let network = install
                .descriptor
                .native_services
                .http
                .iter()
                .flat_map(|policy| policy.urls.clone())
                .chain(
                    install
                        .descriptor
                        .native_services
                        .mcp
                        .iter()
                        .map(|policy| policy.url.clone()),
                )
                .collect::<Vec<_>>();
            let credentials = install
                .descriptor
                .native_services
                .http
                .iter()
                .filter_map(|policy| policy.credential.as_ref())
                .chain(
                    install
                        .descriptor
                        .native_services
                        .mcp
                        .iter()
                        .map(|policy| &policy.credential),
                )
                .map(|credential| credential.id.clone())
                .collect::<Vec<_>>();
            json!({
                "state":install.state,
                "update":install.update,
                "name":install.descriptor.title,
                "version":install.descriptor.version,
                "currentVersion":install.current_version,
                "schemaVersion":install.descriptor.schema_version,
                "currentSchemaVersion":install.current_schema_version,
                "tools":install.descriptor.tools.len(),
                "network":network,
                "credentials":credentials,
                "schedules":install.descriptor.schedules.len(),
                "error":install.error,
            })
        });
        json!({
            "agent":self.agent_status,
            "model":self.model_label,
            "messages":self.messages.iter().map(|message| json!({"role":message.role,"text":message.text})).collect::<Vec<_>>(),
            "schedule":{
                "name":schedule.name,
                "prompt":schedule.prompt,
                "next":schedule_next,
                "everyMinutes":schedule.every_minutes,
            },
            "apps":self.supervisor.catalog().descriptors().into_iter().filter(|app| app.id != ROOT_APP_ID).map(|app| json!({
                "id":app.id,
                "title":app.title,
                "description":app.description,
                "scheduleEveryMinutes":app.schedules.first().map(|schedule| schedule.every_minutes),
            })).collect::<Vec<_>>(),
            "install":install,
            "uninstallingApp":self.pending_uninstall,
            "uninstallError":self.uninstall_error,
            "settings":{
                "wifi":{
                    "connectedSsid":self.wifi_connected,
                    "ipAddress":if self.wifi_connected.is_some() { Some("192.168.4.20") } else { None },
                    "rssiDbm":if self.wifi_connected.is_some() { Some(-42) } else { None },
                    "scanning":false,
                    "networks":self.wifi_networks.iter().map(|(ssid,rssi,secured)| json!({
                        "ssid":ssid,"rssiDbm":rssi,"secured":secured
                    })).collect::<Vec<_>>(),
                    "status":self.wifi_status,
                },
                "firmwareVersion":env!("CARGO_PKG_VERSION"),
                "workspaceFree":"SIMULATED 24 MB",
            }
        })
    }
}

fn start_install_server(workspace: &Path) -> Result<(Receiver<StagedApp>, Arc<AtomicBool>)> {
    let server = tiny_http::Server::http("0.0.0.0:8080")
        .map_err(|error| anyhow!("start App install server: {error}"))?;
    let temp_root = workspace.join(".system/install");
    if let Err(error) = std::fs::remove_dir_all(&temp_root) {
        if error.kind() != std::io::ErrorKind::NotFound {
            return Err(error).context("clear stale App install staging");
        }
    }
    std::fs::create_dir_all(&temp_root)?;
    let (tx, rx) = mpsc::sync_channel(1);
    let slot = Arc::new(AtomicBool::new(false));
    let worker_slot = slot.clone();
    std::thread::Builder::new()
        .name("app-install-http".into())
        .spawn(move || {
            for mut request in server.incoming_requests() {
                if request.method() == &tiny_http::Method::Get {
                    let page = "<form method=post action=/install enctype=application/octet-stream><h1>Pocket Pi App Package</h1><input type=file id=f><button type=button onclick=send()>Upload</button><script>function send(){fetch('/install',{method:'POST',body:f.files[0]}).then(r=>r.text()).then(alert)}</script></form>";
                    let _ = request.respond(tiny_http::Response::from_string(page));
                    continue;
                }
                if request.method() != &tiny_http::Method::Post || request.url() != "/install" {
                    let _ = request.respond(tiny_http::Response::from_string("not found").with_status_code(404));
                    continue;
                }
                if worker_slot
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
                {
                    let _ = request.respond(tiny_http::Response::from_string("another install is pending").with_status_code(409));
                    continue;
                }
                let job = temp_root.join(format!(
                    "{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|duration| duration.as_nanos())
                        .unwrap_or_default()
                ));
                let result = (|| -> Result<StagedApp> {
                    let length = request.body_length().ok_or_else(|| anyhow!("missing Content-Length"))?;
                    anyhow::ensure!(length <= MAX_POCKETAPP_BYTES, "package exceeds 2 MiB");
                    std::fs::create_dir_all(&job)?;
                    let mut bytes = vec![0; length];
                    request.as_reader().read_exact(&mut bytes)?;
                    stage_pocketapp_bytes(&bytes, &job.join("release"))
                })();
                match result {
                    Ok(staged) => {
                        match tx.try_send(staged) {
                            Ok(()) => {
                                let _ = request.respond(tiny_http::Response::from_string("uploaded; confirm on Pocket Pi").with_status_code(202));
                            }
                            Err(mpsc::TrySendError::Full(staged)
                            | mpsc::TrySendError::Disconnected(staged)) => {
                                if let Some(path) = staged.release_dir.parent() {
                                    let _ = std::fs::remove_dir_all(path);
                                }
                                worker_slot.store(false, Ordering::Release);
                                let _ = request.respond(tiny_http::Response::from_string("installer is busy").with_status_code(409));
                            }
                        }
                    }
                    Err(error) => {
                        let _ = std::fs::remove_dir_all(&job);
                        worker_slot.store(false, Ordering::Release);
                        let _ = request.respond(tiny_http::Response::from_string(format!("invalid package: {error:#}")).with_status_code(400));
                    }
                }
            }
        })?;
    log::info!("App installer available at http://127.0.0.1:8080");
    Ok((rx, slot))
}

fn headless(
    output: PathBuf,
    workspace: PathBuf,
    prompt: Option<String>,
    app: String,
    root_tap: Option<(u16, u16)>,
    viewport: Viewport,
    backend: BackendChoice,
) -> Result<()> {
    let mut product = Product::new(workspace, viewport, backend, &app)?;
    if let Some((x, y)) = root_tap {
        product.tap(x, y)?;
        product.run_pending_ui_action();
    }
    let wait_for_turn = prompt.is_some();
    if let Some(prompt) = prompt {
        product.send_prompt(prompt);
    }
    let mut settled_frames = 0;
    for frame in 0..7_500 {
        product.poll()?;
        product.run_pending_install();
        product.run_pending_uninstall();
        // Give PocketJS a few ticks to settle reactive insertions and layout
        // before taking a deterministic screenshot.
        if (!wait_for_turn || !product.busy) && !product.supervisor.services_busy() {
            settled_frames += 1;
        } else {
            settled_frames = 0;
        }
        if frame >= 2 && settled_frames >= 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(16));
    }

    let gpu = Gpu::new_headless()?;
    let target = OffscreenTarget::new(&gpu, viewport.width, viewport.height);
    let mut renderer = UiRenderer::new(&gpu, pocket3d::gpu::OFFSCREEN_FORMAT);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    product.supervisor.with_ui(|ui| {
        let words = ui.draw().words.clone();
        renderer.render_words(
            &gpu,
            ui,
            &words,
            &mut encoder,
            &target.view,
            (viewport.width, viewport.height),
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        )
    })?;
    gpu.queue.submit([encoder.finish()]);
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    target.save_png(&gpu, &output)?;
    println!("wrote {}", output.display());
    Ok(())
}

enum DemoStep {
    Pause(Duration),
    MoveTo((u16, u16), Duration),
    Click((u16, u16)),
    SendPrompt(&'static str),
    WaitFor(DemoCondition),
}

enum DemoCondition {
    InstallState(&'static str),
    InstallDismissed,
    ActiveApp(&'static str),
    AgentIdle,
    ServicesIdle,
}

struct DemoPlayback {
    steps: VecDeque<DemoStep>,
    started: Instant,
    cursor_origin: (u16, u16),
    pressed: bool,
}

impl DemoPlayback {
    fn new(kind: DemoKind) -> Self {
        let mut steps = match kind {
            DemoKind::AppIteration => vec![
                DemoStep::Pause(Duration::from_millis(4_000)),
                DemoStep::MoveTo((28, 42), Duration::from_millis(450)),
                DemoStep::Click((28, 42)),
                DemoStep::Pause(Duration::from_millis(350)),
                DemoStep::SendPrompt(
                    "Change the button to store updated instead of clicked, and make the Built by Pi badge green.",
                ),
            ],
            DemoKind::AppAuthoring => vec![
                DemoStep::Pause(Duration::from_millis(4_000)),
                DemoStep::SendPrompt(
                    "Create a Todo List App where I can create, edit, and complete tasks.",
                ),
            ],
        };
        steps.extend([
            DemoStep::Pause(Duration::from_millis(600)),
            DemoStep::WaitFor(DemoCondition::InstallState("review")),
            DemoStep::Pause(Duration::from_millis(250)),
            DemoStep::MoveTo((240, 722), Duration::from_millis(450)),
            DemoStep::Click((240, 722)),
            DemoStep::WaitFor(DemoCondition::InstallState("success")),
            DemoStep::Pause(Duration::from_millis(350)),
            DemoStep::MoveTo((240, 722), Duration::from_millis(250)),
            DemoStep::Click((240, 722)),
            DemoStep::WaitFor(DemoCondition::InstallDismissed),
        ]);
        match kind {
            DemoKind::AppIteration => steps.extend([
                DemoStep::Pause(Duration::from_secs(5)),
                DemoStep::MoveTo((300, 760), Duration::from_millis(450)),
                DemoStep::Click((300, 760)),
                DemoStep::Pause(Duration::from_millis(350)),
                DemoStep::MoveTo((240, 170), Duration::from_millis(450)),
                DemoStep::Click((240, 170)),
                DemoStep::WaitFor(DemoCondition::ActiveApp("demo")),
                DemoStep::Pause(Duration::from_millis(900)),
                DemoStep::MoveTo((240, 744), Duration::from_millis(500)),
                DemoStep::Click((240, 744)),
                DemoStep::WaitFor(DemoCondition::ServicesIdle),
                DemoStep::Pause(Duration::from_millis(1_800)),
            ]),
            DemoKind::AppAuthoring => steps.extend([
                DemoStep::Pause(Duration::from_secs(1)),
                DemoStep::SendPrompt(
                    "Create a todo called Review Pocket Pi demo, then rename it to Ship the Todo App and mark it complete.",
                ),
                DemoStep::WaitFor(DemoCondition::AgentIdle),
                DemoStep::Pause(Duration::from_secs(2)),
                DemoStep::MoveTo((300, 760), Duration::from_millis(450)),
                DemoStep::Click((300, 760)),
                DemoStep::Pause(Duration::from_millis(350)),
                DemoStep::MoveTo((240, 170), Duration::from_millis(450)),
                DemoStep::Click((240, 170)),
                DemoStep::WaitFor(DemoCondition::ActiveApp("todo")),
                DemoStep::Pause(Duration::from_millis(1_800)),
            ]),
        }
        Self {
            steps: VecDeque::from(steps),
            started: Instant::now(),
            cursor_origin: (240, 400),
            pressed: false,
        }
    }

    fn advance(&mut self, state: &mut WindowState) -> Result<()> {
        let Some(step) = self.steps.front() else {
            return Ok(());
        };
        let elapsed = self.started.elapsed();
        let complete = match step {
            DemoStep::Pause(duration) => elapsed >= *duration,
            DemoStep::MoveTo(target, duration) => {
                let progress = (elapsed.as_secs_f32() / duration.as_secs_f32()).min(1.0);
                let ease = progress * progress * (3.0 - 2.0 * progress);
                let x = self.cursor_origin.0 as f32
                    + (target.0 as f32 - self.cursor_origin.0 as f32) * ease;
                let y = self.cursor_origin.1 as f32
                    + (target.1 as f32 - self.cursor_origin.1 as f32) * ease;
                set_demo_cursor(state, (x.round() as u16, y.round() as u16));
                progress >= 1.0
            }
            DemoStep::Click(target) => {
                set_demo_cursor(state, *target);
                if !self.pressed {
                    state.touch_down = true;
                    state.product.pointer_down(target.0, target.1)?;
                    self.pressed = true;
                    false
                } else if elapsed >= Duration::from_millis(140) {
                    state.product.pointer_up()?;
                    state.touch_down = false;
                    self.pressed = false;
                    true
                } else {
                    false
                }
            }
            DemoStep::SendPrompt(prompt) => {
                state.product.send_prompt((*prompt).into());
                true
            }
            DemoStep::WaitFor(condition) => match condition {
                DemoCondition::InstallState(expected) => state
                    .product
                    .install_ui
                    .as_ref()
                    .is_some_and(|install| install.state == *expected),
                DemoCondition::InstallDismissed => state.product.install_ui.is_none(),
                DemoCondition::ActiveApp(expected) => {
                    state.product.supervisor.active_id() == *expected
                }
                DemoCondition::AgentIdle => !state.product.busy,
                DemoCondition::ServicesIdle => !state.product.supervisor.services_busy(),
            },
        };
        if complete {
            self.steps.pop_front();
            self.started = Instant::now();
            self.cursor_origin = state.cursor;
        }
        Ok(())
    }
}

fn set_demo_cursor(state: &mut WindowState, cursor: (u16, u16)) {
    state.cursor = cursor;
    let x = cursor.0 as f64 * state.config.width as f64 / state.viewport.width as f64;
    let y = cursor.1 as f64 * state.config.height as f64 / state.viewport.height as f64;
    let _ = state
        .window
        .set_cursor_position(winit::dpi::PhysicalPosition::new(x, y));
}

struct WindowState {
    window: Arc<Window>,
    gpu: Gpu,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: UiRenderer,
    product: Product,
    viewport: Viewport,
    cursor: (u16, u16),
    touch_down: bool,
    demo: Option<DemoPlayback>,
}

struct WindowApp {
    workspace: PathBuf,
    initial_prompt: Option<String>,
    initial_app: String,
    initial_root_tap: Option<(u16, u16)>,
    viewport: Viewport,
    backend: Option<BackendChoice>,
    demo: Option<DemoKind>,
    state: Option<WindowState>,
    error: Option<anyhow::Error>,
}

fn windowed(
    workspace: PathBuf,
    prompt: Option<String>,
    app: String,
    root_tap: Option<(u16, u16)>,
    viewport: Viewport,
    backend: BackendChoice,
    demo: Option<DemoKind>,
) -> Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = WindowApp {
        workspace,
        initial_prompt: prompt,
        initial_app: app,
        initial_root_tap: root_tap,
        viewport,
        backend: Some(backend),
        demo,
        state: None,
        error: None,
    };
    event_loop.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

impl WindowApp {
    fn init(&mut self, event_loop: &ActiveEventLoop) -> Result<WindowState> {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title(format!(
                        "Pocket Pi AgentOS Simulator — {}×{}",
                        self.viewport.width, self.viewport.height
                    ))
                    .with_inner_size(winit::dpi::LogicalSize::new(
                        self.viewport.width.div_ceil(2),
                        self.viewport.height.div_ceil(2),
                    ))
                    .with_resizable(false),
            )?,
        );
        let instance = Gpu::new_instance();
        let surface = instance.create_surface(window.clone())?;
        let gpu = Gpu::from_instance_for_surface(instance, &surface)?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&gpu.adapter, size.width, size.height)
            .ok_or_else(|| anyhow!("window surface is unsupported"))?;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&gpu.device, &config);
        let renderer = UiRenderer::new(&gpu, config.format);
        let backend = self
            .backend
            .take()
            .ok_or_else(|| anyhow!("simulator backend was already consumed"))?;
        let mut product = Product::new(
            self.workspace.clone(),
            self.viewport,
            backend,
            &self.initial_app,
        )?;
        if let Some((x, y)) = self.initial_root_tap.take() {
            product.tap(x, y)?;
        }
        if let Some(prompt) = self.initial_prompt.take() {
            product.send_prompt(prompt);
        }
        Ok(WindowState {
            window,
            gpu,
            surface,
            config,
            renderer,
            product,
            viewport: self.viewport,
            cursor: if self.demo.is_some() {
                (240, 400)
            } else {
                (0, 0)
            },
            touch_down: false,
            demo: self.demo.map(DemoPlayback::new),
        })
    }

    fn redraw(state: &mut WindowState) -> Result<()> {
        state.product.poll()?;
        if let Some(mut demo) = state.demo.take() {
            demo.advance(state)?;
            state.demo = Some(demo);
        }
        let frame = match state.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                state.surface.configure(&state.gpu.device, &state.config);
                return Ok(());
            }
            Err(error) => return Err(anyhow!("surface: {error}")),
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = state.gpu.device.create_command_encoder(&Default::default());
        let scale = state.config.width as f32 / state.viewport.width as f32;
        state.product.supervisor.with_ui(|ui| {
            let words = ui.draw().words.clone();
            state.renderer.render_words_scaled(
                &state.gpu,
                ui,
                &words,
                &mut encoder,
                &view,
                (state.config.width, state.config.height),
                scale,
                wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            )
        })?;
        state.gpu.queue.submit([encoder.finish()]);
        state.window.pre_present_notify();
        frame.present();
        state.product.run_pending_ui_action();
        state.product.run_pending_install();
        state.product.run_pending_uninstall();
        state.window.request_redraw();
        Ok(())
    }

    fn update_cursor(state: &mut WindowState, x: f64, y: f64) {
        let x = x * state.viewport.width as f64 / state.config.width as f64;
        let y = y * state.viewport.height as f64 / state.config.height as f64;
        state.cursor = (
            x.clamp(0.0, (state.viewport.width - 1) as f64) as u16,
            y.clamp(0.0, (state.viewport.height - 1) as f64) as u16,
        );
    }
}

impl ApplicationHandler for WindowApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        match self.init(event_loop) {
            Ok(state) => {
                state.window.request_redraw();
                self.state = Some(state);
            }
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::CursorMoved { position, .. } => {
                Self::update_cursor(state, position.x, position.y);
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                ..
            } if !state.touch_down => {
                state.touch_down = true;
                if let Err(error) = state.product.pointer_down(state.cursor.0, state.cursor.1) {
                    self.error = Some(error);
                    event_loop.exit();
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Released,
                ..
            } => {
                state.touch_down = false;
                if let Err(error) = state.product.pointer_up() {
                    self.error = Some(error);
                    event_loop.exit();
                }
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape)) =>
            {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = Self::redraw(state) {
                    self.error = Some(error);
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocket_pi_embedded::ToolResult;

    #[test]
    fn viewport_argument_requires_positive_width_and_height() {
        assert_eq!(parse_viewport("800x480").unwrap(), Viewport::new(800, 480));
        for value in ["800", "0x480", "800x0", "widextall"] {
            assert!(parse_viewport(value).is_err(), "accepted {value}");
        }
    }

    fn init_logs() {
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .is_test(true)
            .try_init();
    }

    struct FailingRobinhood;

    impl AppServiceHost for FailingRobinhood {
        fn call(
            &self,
            _app_id: &str,
            service: &str,
            operation: &str,
            _args: &Value,
            _deadline: Instant,
        ) -> Result<Value, String> {
            let _ = (service, operation);
            Err("simulated Robinhood outage".into())
        }

        fn store_credentials(&self, _credentials: &BTreeMap<String, String>) -> Result<(), String> {
            Ok(())
        }
    }

    struct NoTools;

    impl ToolHost for NoTools {
        fn definitions(&self) -> Vec<Value> {
            Vec::new()
        }

        fn execute(&self, _call_id: &str, name: &str, _args_json: &str) -> ToolResult {
            ToolResult {
                text: format!("unexpected Tool {name}"),
                is_error: true,
                ..ToolResult::default()
            }
        }
    }

    fn route_tool(supervisor: &mut AppSupervisor, name: &str, args_json: &str) -> ToolResult {
        let (tools, requests) =
            RoutedToolHost::new(Arc::new(NoTools), supervisor.catalog().clone());
        let name = name.to_owned();
        let args_json = args_json.to_owned();
        let call = std::thread::spawn(move || tools.execute("test", &name, &args_json));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !call.is_finished() {
            while let Ok(request) = requests.try_recv() {
                request.handle(supervisor, |_, _| anyhow::bail!("unexpected app.submit"));
            }
            supervisor.frame_render(true).unwrap();
            assert!(Instant::now() < deadline, "timed out routing App Tool");
            std::thread::sleep(Duration::from_millis(2));
        }
        call.join().unwrap()
    }

    fn wait_for(mut ready: impl FnMut() -> bool) {
        for _ in 0..100 {
            if ready() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("timed out waiting for background App Action");
    }

    fn try_query_app_database(
        workspace: &Path,
        app_id: &str,
        database_name: &str,
        sql: &str,
    ) -> Option<Vec<Value>> {
        let mut database = pocket_db::DbModule::new(pocket_db::Storage::Dir(
            workspace.join("apps").join(app_id).join("data"),
        ));
        let handle = database.open(database_name);
        if handle < 0 {
            return None;
        }
        let value: Value = serde_json::from_str(&database.query(handle, sql, "[]")).ok()?;
        database.close(handle);
        if value.get("error").is_some() {
            return None;
        }
        value["rows"].as_array().cloned()
    }

    fn query_app_database(
        workspace: &Path,
        app_id: &str,
        database_name: &str,
        sql: &str,
    ) -> Vec<Value> {
        try_query_app_database(workspace, app_id, database_name, sql)
            .unwrap_or_else(|| panic!("query {app_id}/{database_name}: {sql}"))
    }

    fn test_supervisor(workspace: &Path, services: Arc<dyn AppServiceHost>) -> AppSupervisor {
        let catalog = InstalledAppIndex::load(workspace, system_app_bundle()).unwrap();
        let mut supervisor =
            AppSupervisor::new(workspace, DEFAULT_VIEWPORT, catalog, services).unwrap();
        for app_id in ["robinhood", "exa"] {
            let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join("apps")
                .join(app_id);
            let staging = workspace.join(format!(".staged-{app_id}"));
            std::fs::create_dir_all(&staging).unwrap();
            for name in ["app.json", "schema.sql", "actions.js", "view.js"] {
                std::fs::copy(source.join(name), staging.join(name)).unwrap();
            }
            let descriptor: Value =
                serde_json::from_slice(&std::fs::read(source.join("app.json")).unwrap()).unwrap();
            for resource in descriptor["resources"]
                .as_object()
                .into_iter()
                .flat_map(|resources| resources.values())
            {
                let path = resource["path"].as_str().unwrap();
                let destination = staging.join(path);
                std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
                std::fs::copy(source.join(path), destination).unwrap();
            }
            let credentials = match app_id {
                "robinhood" => BTreeMap::from([(
                    "robinhood.oauth-access-token".to_owned(),
                    "test-secret".to_owned(),
                )]),
                "exa" => BTreeMap::from([("exa.api-key".to_owned(), "test-secret".to_owned())]),
                _ => unreachable!(),
            };
            supervisor.apply_app(&staging, credentials).unwrap();
        }
        supervisor
    }

    fn checked_in_package(app_id: &str, output: &Path) {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("apps")
            .join(app_id);
        let file = std::fs::File::create(output).unwrap();
        let mut archive = tar::Builder::new(file);
        for name in ["app.json", "schema.sql", "actions.js", "view.js"] {
            archive
                .append_path_with_name(source.join(name), name)
                .unwrap();
        }
        let credentials = serde_json::to_vec(&json!({"exa.api-key":"test-secret"})).unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_size(credentials.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        archive
            .append_data(&mut header, "credentials.json", credentials.as_slice())
            .unwrap();
        archive.finish().unwrap();
    }

    #[test]
    fn fresh_device_installs_new_app_routes_its_tool_and_restores_after_restart() {
        init_logs();
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let package = temp.path().join("exa.pocketapp");
        checked_in_package("exa", &package);
        let staged =
            pocket_pi_agentos::stage_pocketapp(&package, &temp.path().join("staged")).unwrap();

        let index = InstalledAppIndex::load(&workspace, system_app_bundle()).unwrap();
        let mut supervisor = AppSupervisor::new(
            &workspace,
            DEFAULT_VIEWPORT,
            index,
            Arc::new(SimAppServices),
        )
        .unwrap();
        assert!(supervisor.catalog().descriptor("exa").is_none());
        supervisor
            .apply_app(&staged.release_dir, staged.credentials)
            .unwrap();
        assert!(supervisor.catalog().descriptor("exa").is_some());
        let result = route_tool(
            &mut supervisor,
            "research.search",
            r#"{"query":"freshly installed App"}"#,
        );
        assert!(!result.is_error, "{}", result.text);
        assert_eq!(
            query_app_database(
                &workspace,
                "exa",
                "exa",
                "SELECT status,result_count FROM searches ORDER BY id DESC LIMIT 1",
            ),
            vec![json!(["ok", 2])]
        );
        assert!(!workspace.join("apps/exa/release/credentials.json").exists());

        drop(supervisor);
        let restored = InstalledAppIndex::load(&workspace, system_app_bundle()).unwrap();
        assert!(restored
            .descriptor("exa")
            .unwrap()
            .tools
            .iter()
            .any(|tool| { tool.get("name").and_then(Value::as_str) == Some("research.search") }));
    }

    #[test]
    fn exa_tool_writes_app_owned_sqlite() {
        init_logs();
        let temp = tempfile::tempdir().unwrap();
        let mut supervisor = test_supervisor(temp.path(), Arc::new(SimAppServices));
        let result = route_tool(
            &mut supervisor,
            "research.search",
            r#"{"query":"NVIDIA FY2026 annual report 10-K revenue data center guidance","numResults":5,"includeDomains":["investor.nvidia.com","sec.gov"]}"#,
        );
        assert!(!result.is_error, "{}", result.text);
        assert!(temp.path().join("apps/exa/data/exa.sqlite").exists());
        let rows = query_app_database(
            temp.path(),
            "exa",
            "exa",
            "SELECT status,result_count FROM searches ORDER BY id DESC LIMIT 1",
        );
        assert_eq!(rows, vec![json!(["ok", 2])]);
    }

    #[test]
    fn exa_write_removes_rows_older_than_seven_days() {
        init_logs();
        let temp = tempfile::tempdir().unwrap();
        let mut supervisor = test_supervisor(temp.path(), Arc::new(SimAppServices));
        let first = route_tool(
            &mut supervisor,
            "research.search",
            r#"{"query":"expired search"}"#,
        );
        assert!(!first.is_error, "{}", first.text);

        let mut database =
            pocket_db::DbModule::new(pocket_db::Storage::Dir(temp.path().join("apps/exa/data")));
        let handle = database.open("exa");
        assert!(handle >= 0);
        assert_eq!(
            database.exec(handle, "UPDATE searches SET searched_at=0;"),
            0,
            "{}",
            database.last_error(handle)
        );
        database.close(handle);

        let second = route_tool(
            &mut supervisor,
            "research.search",
            r#"{"query":"new search"}"#,
        );
        assert!(!second.is_error, "{}", second.text);
        let rows = query_app_database(
            temp.path(),
            "exa",
            "exa",
            "SELECT id,query FROM searches ORDER BY id",
        );
        assert_eq!(rows, vec![json!([2, "new search"])]);
    }

    #[test]
    fn robinhood_refresh_failure_records_no_view_data() {
        init_logs();
        let temp = tempfile::tempdir().unwrap();
        let mut supervisor = test_supervisor(temp.path(), Arc::new(FailingRobinhood));
        supervisor.open("robinhood").unwrap();

        let failed = supervisor.invoke_active_action("refreshPortfolio", &Value::Null);
        assert!(!failed.is_error);
        wait_for(|| {
            try_query_app_database(
                temp.path(),
                "robinhood",
                "robinhood",
                "SELECT status FROM refresh_runs ORDER BY id DESC LIMIT 1",
            )
            .is_some_and(|rows| rows.first() == Some(&json!(["failed"])))
        });
        supervisor.frame_render(true).unwrap();

        let rows = query_app_database(
            temp.path(),
            "robinhood",
            "robinhood",
            "SELECT COUNT(*) FROM total_value",
        );
        assert_eq!(rows, vec![json!([0])]);
    }

    #[test]
    fn robinhood_refresh_writes_the_fixed_view_projection() {
        init_logs();
        let temp = tempfile::tempdir().unwrap();
        let mut supervisor = test_supervisor(temp.path(), Arc::new(SimAppServices));
        supervisor.open("robinhood").unwrap();
        let refreshed = route_tool(&mut supervisor, "robinhood.refresh_portfolio", "{}");
        assert!(!refreshed.is_error, "{}", refreshed.text);
        supervisor.frame_render(true).unwrap();

        let rows = query_app_database(
            temp.path(),
            "robinhood",
            "robinhood",
            "SELECT status,operation_count,success_count FROM refresh_runs ORDER BY id DESC LIMIT 1",
        );
        assert_eq!(rows, vec![json!(["succeeded", 16, 16])]);
        let rows = query_app_database(
            temp.path(),
            "robinhood",
            "robinhood",
            "SELECT (SELECT COUNT(*) FROM accounts),(SELECT COUNT(*) FROM portfolio_current),(SELECT COUNT(*) FROM total_value)",
        );
        assert_eq!(rows, vec![json!([3, 3, 3])]);
    }
}
