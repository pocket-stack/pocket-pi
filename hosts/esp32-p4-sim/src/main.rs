use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use pocket3d::gpu::{Gpu, OffscreenTarget};
use pocket_pi_agentos::{
    AppServiceHost, AppSupervisor, AppToolRequest, HttpRequest, NetFailure, RoutedToolHost,
    TransportCompletion, EXA_APP_ID, ROOT_APP_ID,
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

use backend::BackendChoice;

const PANEL_WIDTH: u32 = 720;
const PANEL_HEIGHT: u32 = 1280;
const WINDOW_WIDTH: u32 = 360;
const WINDOW_HEIGHT: u32 = 640;

struct Args {
    screenshot: Option<PathBuf>,
    prompt: Option<String>,
    workspace: PathBuf,
    app: String,
    root_tap: Option<(u16, u16)>,
    backend: BackendChoice,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = parse_args()?;
    prepare_workspace(&args.workspace)?;
    if let Some(path) = args.screenshot {
        headless(
            path,
            args.workspace,
            args.prompt,
            args.app,
            args.root_tap,
            args.backend,
        )
    } else {
        windowed(
            args.workspace,
            args.prompt,
            args.app,
            args.root_tap,
            args.backend,
        )
    }
}

fn parse_args() -> Result<Args> {
    let mut screenshot = None;
    let mut prompt = None;
    let mut workspace =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/esp32-p4-sim/workspace");
    let mut app = ROOT_APP_ID.to_owned();
    let mut root_tap = None;
    let mut backend = std::env::var("POCKET_PI_BACKEND").unwrap_or_else(|_| "codex".into());
    let mut model = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--screenshot" => screenshot = Some(PathBuf::from(next(&mut args, "--screenshot")?)),
            "--prompt" => prompt = Some(next(&mut args, "--prompt")?),
            "--workspace" => workspace = PathBuf::from(next(&mut args, "--workspace")?),
            "--app" | "--view" => {
                app = match next(&mut args, &argument)?.as_str() {
                    "chat" | "pi-agent" => ROOT_APP_ID.to_owned(),
                    "workspace" | "files" => {
                        root_tap = Some((270, 1220));
                        ROOT_APP_ID.to_owned()
                    }
                    "apps" | "runs" => {
                        root_tap = Some((450, 1220));
                        ROOT_APP_ID.to_owned()
                    }
                    "settings" => {
                        root_tap = Some((630, 1220));
                        ROOT_APP_ID.to_owned()
                    }
                    "keyboard" => {
                        root_tap = Some((350, 1110));
                        ROOT_APP_ID.to_owned()
                    }
                    "robinhood" => "robinhood".to_owned(),
                    "exa" => EXA_APP_ID.to_owned(),
                    value => return Err(anyhow!("unknown App: {value}")),
                }
            }
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
        backend: BackendChoice::from_name(&backend, model).map_err(anyhow::Error::msg)?,
    })
}

fn next(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String> {
    args.next().ok_or_else(|| anyhow!("{name} needs a value"))
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

struct SimPlatform;

impl PlatformTools for SimPlatform {
    fn device_status(&self) -> Value {
        json!({
            "status":"ok",
            "board":"esp32-p4-sim",
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
/// The App code, SQLite writes and View are exactly the same bundles as the board.
#[derive(Default)]
struct SimAppServices;

impl AppServiceHost for SimAppServices {
    fn call(
        &self,
        app_id: &str,
        service: &str,
        operation: &str,
        args: &Value,
    ) -> Result<Value, String> {
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
                    match self.call(app_id, service, "callTool", &single) {
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
    ) -> std::result::Result<TransportCompletion, NetFailure> {
        if app_id != EXA_APP_ID || request.method != "POST" {
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
    app_rx: Receiver<AppToolRequest>,
    supervisor: AppSupervisor,
    last_schedule_poll: Instant,
    busy: bool,
    projection_dirty: bool,
    pending_ui_task: Option<String>,
    wifi_connected: Option<String>,
    wifi_networks: Vec<(&'static str, i32, bool)>,
    wifi_status: String,
}

impl Product {
    fn new(workspace: PathBuf, backend: BackendChoice, app: &str) -> Result<Self> {
        let model_label = match &backend {
            BackendChoice::Wireless {
                provider, model, ..
            } => {
                format!("{} / {model}", provider.id())
            }
            BackendChoice::Codex { model } => {
                format!("Codex / {}", model.as_deref().unwrap_or("coding-plan"))
            }
        };
        let services: Arc<dyn AppServiceHost> = Arc::new(SimAppServices::default());
        let mut supervisor = AppSupervisor::new(workspace.clone(), services)?;
        supervisor.open(app)?;

        let native_tools = Arc::new(CoreToolHost::new(workspace.clone(), Arc::new(SimPlatform)));
        let catalog = supervisor.catalog().clone();
        let native: Arc<dyn ToolHost> = native_tools.clone();
        let (routed, app_rx) = RoutedToolHost::new(native, catalog);
        let config = backend.agent_config();
        let model = backend.build();
        supervisor.boot_agent(&config, model, Arc::new(routed))?;

        Ok(Self {
            messages: vec![Message {
                role: "assistant",
                text: "Pocket Pi AgentOS is ready. Robinhood and Exa are installed Apps.".into(),
            }],
            agent_status: "IDLE",
            model_label,
            native_tools,
            app_rx,
            supervisor,
            last_schedule_poll: Instant::now(),
            busy: false,
            projection_dirty: true,
            pending_ui_task: None,
            wifi_connected: Some("macOS host network".into()),
            wifi_networks: Vec::new(),
            wifi_status: "SIMULATED NETWORK READY".into(),
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
        self.projection_dirty = true;
    }

    fn tap(&mut self, x: u16, y: u16) -> Result<()> {
        let action = self.supervisor.tap(x, y)?;
        match action.get("type").and_then(Value::as_str) {
            Some("navigate") => {
                if let Some(app) = action.get("app").and_then(Value::as_str) {
                    self.supervisor.open(app)?;
                    self.projection_dirty = true;
                }
            }
            Some("submitPrompt") => {
                if let Some(prompt) = action.get("prompt").and_then(Value::as_str) {
                    self.send_prompt(prompt.to_owned());
                }
            }
            Some("invokeTask") => {
                if let Some(task) = action.get("task").and_then(Value::as_str) {
                    self.pending_ui_task = Some(task.to_owned());
                }
            }
            Some("settings") => match action.get("command").and_then(Value::as_str) {
                Some("scan") => {
                    self.wifi_networks = vec![
                        ("PocketPi Lab", -42, true),
                        ("Studio Guest", -61, false),
                        ("ESP32 Testbench", -73, true),
                    ];
                    self.wifi_status = "3 NETWORKS FOUND".into();
                    self.projection_dirty = true;
                }
                Some("connect") => {
                    if let Some(ssid) = action.get("ssid").and_then(Value::as_str) {
                        self.wifi_connected = Some(ssid.to_owned());
                        self.wifi_status = format!("CONNECTED TO {ssid}");
                        self.projection_dirty = true;
                    }
                }
                Some("forget") => {
                    self.wifi_connected = None;
                    self.wifi_status = "WI-FI CREDENTIALS FORGOTTEN".into();
                    self.projection_dirty = true;
                }
                Some("restart") => {
                    self.wifi_status = "SIMULATED RESTART REQUESTED".into();
                    self.projection_dirty = true;
                }
                _ => {}
            },
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

    fn run_pending_ui_task(&mut self) {
        let Some(task) = self.pending_ui_task.take() else {
            return;
        };
        let started = Instant::now();
        let result = self.supervisor.invoke_active_task(&task, &Value::Null);
        log::info!(
            "UI AppTask {} finished in {}ms: {}",
            task,
            started.elapsed().as_millis(),
            result.text
        );
        self.projection_dirty = true;
    }

    fn poll(&mut self) -> Result<()> {
        while let Ok(request) = self.app_rx.try_recv() {
            request.handle(&mut self.supervisor);
            self.projection_dirty = true;
        }
        if self.last_schedule_poll.elapsed() >= Duration::from_secs(1) {
            if !self.busy {
                if let Some(wake) = self.native_tools.claim_due() {
                    self.send_prompt(wake.prompt);
                }
            }
            for (task, result) in self.supervisor.poll_due_tasks() {
                log::info!("AppTask {task}: {}", result.text);
            }
            self.last_schedule_poll = Instant::now();
            self.projection_dirty = true;
        }

        if self.projection_dirty {
            self.supervisor.update_root(&self.root_projection())?;
            self.projection_dirty = false;
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
            self.projection_dirty = true;
        }
        Ok(())
    }

    fn root_projection(&self) -> Value {
        let schedule = self.native_tools.schedule_projection();
        let schedule_next = match schedule.next_in_seconds {
            Some(seconds) => schedule.every_minutes.map_or_else(
                || format!("in {seconds}s"),
                |minutes| format!("in {seconds}s · every {minutes}m"),
            ),
            None => "not scheduled".to_owned(),
        };
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

fn headless(
    output: PathBuf,
    workspace: PathBuf,
    prompt: Option<String>,
    app: String,
    root_tap: Option<(u16, u16)>,
    backend: BackendChoice,
) -> Result<()> {
    let mut product = Product::new(workspace, backend, &app)?;
    if let Some((x, y)) = root_tap {
        product.tap(x, y)?;
        product.run_pending_ui_task();
    }
    let wait_for_turn = prompt.is_some();
    if let Some(prompt) = prompt {
        product.send_prompt(prompt);
    }
    let mut settled_frames = 0;
    for frame in 0..7_500 {
        product.poll()?;
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
    let target = OffscreenTarget::new(&gpu, PANEL_WIDTH, PANEL_HEIGHT);
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
            (PANEL_WIDTH, PANEL_HEIGHT),
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

struct WindowState {
    window: Arc<Window>,
    gpu: Gpu,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: UiRenderer,
    product: Product,
    cursor: (u16, u16),
    touch_down: bool,
}

struct WindowApp {
    workspace: PathBuf,
    initial_prompt: Option<String>,
    initial_app: String,
    initial_root_tap: Option<(u16, u16)>,
    backend: Option<BackendChoice>,
    state: Option<WindowState>,
    error: Option<anyhow::Error>,
}

fn windowed(
    workspace: PathBuf,
    prompt: Option<String>,
    app: String,
    root_tap: Option<(u16, u16)>,
    backend: BackendChoice,
) -> Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = WindowApp {
        workspace,
        initial_prompt: prompt,
        initial_app: app,
        initial_root_tap: root_tap,
        backend: Some(backend),
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
                    .with_title("Pocket Pi AgentOS — ESP32-P4 Simulator")
                    .with_inner_size(winit::dpi::LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
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
        let mut product = Product::new(self.workspace.clone(), backend, &self.initial_app)?;
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
            cursor: (0, 0),
            touch_down: false,
        })
    }

    fn redraw(state: &mut WindowState) -> Result<()> {
        state.product.poll()?;
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
        let scale = state.config.width as f32 / PANEL_WIDTH as f32;
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
        state.product.run_pending_ui_task();
        state.window.request_redraw();
        Ok(())
    }

    fn update_cursor(state: &mut WindowState, x: f64, y: f64) {
        let x = x * PANEL_WIDTH as f64 / state.config.width as f64;
        let y = y * PANEL_HEIGHT as f64 / state.config.height as f64;
        state.cursor = (
            x.clamp(0.0, (PANEL_WIDTH - 1) as f64) as u16,
            y.clamp(0.0, (PANEL_HEIGHT - 1) as f64) as u16,
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
        ) -> Result<Value, String> {
            let _ = (service, operation);
            Err("simulated Robinhood outage".into())
        }
    }

    fn wait_for(mut ready: impl FnMut() -> bool) {
        for _ in 0..100 {
            if ready() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("timed out waiting for background App Data Action");
    }

    #[test]
    fn exa_tool_writes_app_owned_sqlite() {
        init_logs();
        let temp = tempfile::tempdir().unwrap();
        let mut supervisor =
            AppSupervisor::new(temp.path(), Arc::new(SimAppServices::default())).unwrap();
        let result = supervisor.invoke_tool(
            "research.search",
            r#"{"query":"NVIDIA FY2026 annual report 10-K revenue data center guidance","numResults":5,"includeDomains":["investor.nvidia.com","sec.gov"]}"#,
        );
        assert!(!result.is_error, "{}", result.text);
        assert_eq!(result.details["status"], "queued");
        wait_for(|| {
            let storage = supervisor.invoke_tool("research.storage_status", "{}");
            storage.details["searches"] == 1
        });
        assert!(temp.path().join("apps/exa/data/exa.sqlite").exists());
        let storage = supervisor.invoke_tool("research.storage_status", "{}");
        assert!(!storage.is_error, "{}", storage.text);
        assert_eq!(storage.details["searches"], 1);
        assert_eq!(storage.details["retentionDays"], 7);
        assert_eq!(storage.details["schemaVersion"], 5);
        assert_eq!(storage.details["expectedSchemaVersion"], 5);
        assert_eq!(storage.details["latestSearch"]["status"], "ok");
        assert!(storage.text.len() < 240, "{}", storage.text);
        assert!(storage.details.get("tables").is_none());
        assert!(storage.details.get("latestSearches").is_none());
    }

    #[test]
    fn exa_write_removes_rows_older_than_seven_days() {
        init_logs();
        let temp = tempfile::tempdir().unwrap();
        let mut supervisor =
            AppSupervisor::new(temp.path(), Arc::new(SimAppServices::default())).unwrap();
        let first = supervisor.invoke_tool("research.search", r#"{"query":"old search"}"#);
        assert!(!first.is_error, "{}", first.text);
        wait_for(|| {
            supervisor
                .invoke_tool("research.storage_status", "{}")
                .details["searches"]
                == 1
        });

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

        let second = supervisor.invoke_tool("research.search", r#"{"query":"new search"}"#);
        assert!(!second.is_error, "{}", second.text);
        wait_for(|| {
            supervisor
                .invoke_tool("research.storage_status", "{}")
                .details["latestSearch"]["id"]
                == 2
        });
        let storage = supervisor.invoke_tool("research.storage_status", "{}");
        assert_eq!(storage.details["searches"], 1);
    }

    #[test]
    fn views_open_empty_databases_without_starting_data_actions() {
        init_logs();
        let temp = tempfile::tempdir().unwrap();
        let mut supervisor =
            AppSupervisor::new(temp.path(), Arc::new(SimAppServices::default())).unwrap();

        let robinhood = supervisor.invoke_tool("robinhood.storage_status", "{}");
        assert!(!robinhood.is_error, "{}", robinhood.text);
        assert!(robinhood.details["tables"].as_array().unwrap().is_empty());

        let exa = supervisor.invoke_tool("research.storage_status", "{}");
        assert!(!exa.is_error, "{}", exa.text);
        assert_eq!(exa.details["searches"], 0);
        assert_eq!(exa.details["retentionDays"], 7);
        assert_eq!(exa.details["schemaVersion"], 0);
        assert_eq!(exa.details["expectedSchemaVersion"], 5);
        assert!(exa.details["latestSearch"].is_null());
    }

    #[test]
    fn app_navigation_keeps_pi_agent_at_workspace_root() {
        init_logs();
        let temp = tempfile::tempdir().unwrap();
        let mut supervisor =
            AppSupervisor::new(temp.path(), Arc::new(SimAppServices::default())).unwrap();
        supervisor.open("robinhood").unwrap();
        assert_eq!(supervisor.active_id(), "robinhood");
        assert!(temp.path().join("data/view/current").exists());
        assert!(temp.path().join("apps/robinhood/current").exists());
    }

    #[test]
    fn robinhood_refresh_recovers_after_service_failure() {
        init_logs();
        let temp = tempfile::tempdir().unwrap();
        let mut supervisor = AppSupervisor::new(temp.path(), Arc::new(FailingRobinhood)).unwrap();
        supervisor.open("robinhood").unwrap();

        let first = supervisor.tap(650, 1220).unwrap();
        assert_eq!(first["type"], "invokeTask");
        let failed = supervisor.invoke_active_task("refreshPortfolio", &Value::Null);
        assert!(!failed.is_error);
        wait_for(|| {
            let storage = supervisor.invoke_tool("robinhood.storage_status", "{}");
            storage.details["latestRefreshes"]
                .as_array()
                .and_then(|rows| rows.first())
                .is_some_and(|row| row["status"] == "failed")
        });
        supervisor.frame().unwrap();

        let storage = supervisor.invoke_tool("robinhood.storage_status", "{}");
        assert!(!storage.is_error, "{}", storage.text);
        assert_eq!(storage.details["latestRefreshes"][0]["status"], "failed");
        let values = storage.details["tables"]
            .as_array()
            .unwrap()
            .iter()
            .find(|table| table["name"] == "total_value")
            .unwrap();
        assert_eq!(values["rowCount"], 0);

        let retry = supervisor.tap(650, 1220).unwrap();
        assert_eq!(retry["type"], "invokeTask");
    }

    #[test]
    fn robinhood_full_views_and_account_switch_render() {
        init_logs();
        let temp = tempfile::tempdir().unwrap();
        let mut supervisor =
            AppSupervisor::new(temp.path(), Arc::new(SimAppServices::default())).unwrap();
        supervisor.open("robinhood").unwrap();
        let refreshed = supervisor.invoke_tool("robinhood.refresh_portfolio", "{}");
        assert!(!refreshed.is_error, "{}", refreshed.text);
        wait_for(|| {
            let storage = supervisor.invoke_tool("robinhood.storage_status", "{}");
            storage.details["latestRefreshes"]
                .as_array()
                .and_then(|rows| rows.first())
                .is_some_and(|row| row["status"] == "succeeded")
        });
        supervisor.frame().unwrap();

        let storage = supervisor.invoke_tool("robinhood.storage_status", "{}");
        assert!(!storage.is_error, "{}", storage.text);
        assert_eq!(
            storage.details["latestRefreshes"].as_array().unwrap().len(),
            1
        );
        assert_eq!(storage.details["latestRefreshes"][0]["status"], "succeeded");
        assert_eq!(storage.details["latestRefreshes"][0]["operation_count"], 16);
        assert_eq!(storage.details["latestRefreshes"][0]["success_count"], 16);
        let tables = storage.details["tables"].as_array().unwrap();
        assert!(tables.iter().all(|table| table["name"] != "tool_events"));
        assert_eq!(
            tables
                .iter()
                .find(|table| table["name"] == "accounts")
                .unwrap()["rowCount"],
            3
        );
        assert_eq!(
            tables
                .iter()
                .find(|table| table["name"] == "portfolio_current")
                .unwrap()["rowCount"],
            3
        );
        assert_eq!(
            tables
                .iter()
                .find(|table| table["name"] == "total_value")
                .unwrap()["rowCount"],
            3
        );

        supervisor.tap(350, 140).unwrap();
        supervisor.frame().unwrap();
        assert!(supervisor.with_ui(|ui| ui.draw().words.len()) > 100);

        supervisor.tap(350, 250).unwrap();
        supervisor.frame().unwrap();
        supervisor.tap(350, 700).unwrap();
        supervisor.frame().unwrap();
        assert!(supervisor.with_ui(|ui| ui.draw().words.len()) > 100);

        supervisor.tap(80, 60).unwrap();
        supervisor.frame().unwrap();
        supervisor.tap(350, 900).unwrap();
        supervisor.frame().unwrap();
        assert!(supervisor.with_ui(|ui| ui.draw().words.len()) > 100);
    }
}
