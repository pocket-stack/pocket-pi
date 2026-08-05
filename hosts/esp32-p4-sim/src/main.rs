use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use pocket3d::gpu::{Gpu, OffscreenTarget};
use pocket_mod::Guest;
use pocket_pi_app_core::{
    decode_command, encode_snapshot, AppCommand, AppSnapshot, FileEntry, OpenFile, Role,
    SystemState, Turn, View,
};
use pocket_pi_embedded::{PiEmbedded, ToolHost, ToolResult};
use pocket_ui_wgpu::{UiRenderer, UiSurface};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

mod backend;

use backend::BackendChoice;

const UI_WIDTH: u32 = 360;
const UI_HEIGHT: u32 = 640;
const RASTER_DENSITY: u32 = 2;
const ANALOG_CENTER: u32 = 0x8080;

const BTN_UP: u32 = 0x0010;
const BTN_RIGHT: u32 = 0x0020;
const BTN_DOWN: u32 = 0x0040;
const BTN_LEFT: u32 = 0x0080;
const BTN_LTRIGGER: u32 = 0x0100;
const BTN_RTRIGGER: u32 = 0x0200;
const BTN_CIRCLE: u32 = 0x2000;

struct Args {
    screenshot: Option<PathBuf>,
    prompt: Option<String>,
    workspace: PathBuf,
    view: View,
    backend: BackendChoice,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = parse_args()?;
    prepare_workspace(&args.workspace)?;
    if let Some(path) = args.screenshot {
        headless(path, args.workspace, args.prompt, args.view, args.backend)
    } else {
        windowed(args.workspace, args.prompt, args.view, args.backend)
    }
}

fn parse_args() -> Result<Args> {
    let mut screenshot = None;
    let mut prompt = None;
    let mut workspace =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/esp32-p4-sim/workspace");
    let mut view = View::Chat;
    let mut backend = std::env::var("POCKET_PI_BACKEND").unwrap_or_else(|_| "scripted".into());
    let mut model = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--screenshot" => screenshot = Some(PathBuf::from(next(&mut args, "--screenshot")?)),
            "--prompt" => prompt = Some(next(&mut args, "--prompt")?),
            "--workspace" => workspace = PathBuf::from(next(&mut args, "--workspace")?),
            "--view" => {
                view = match next(&mut args, "--view")?.as_str() {
                    "chat" => View::Chat,
                    "workspace" => View::Workspace,
                    value => return Err(anyhow!("unknown view: {value}")),
                }
            }
            "--backend" => backend = next(&mut args, "--backend")?,
            "--model" => model = Some(next(&mut args, "--model")?),
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }
    Ok(Args {
        screenshot,
        prompt,
        workspace,
        view,
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
        std::fs::write(notes, "Chat and workspace share one PocketJS UI bundle.\n")?;
    }
    Ok(())
}

fn artifact(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts/ui")
        .join(name)
}

fn boot_ui() -> Result<(Guest, UiSurface)> {
    let js = std::fs::read_to_string(artifact("agent-shell.js"))
        .context("missing agent-shell.js; run `cargo xtask build esp32-p4-sim`")?;
    let pak = std::fs::read(artifact("agent-shell.pak"))
        .context("missing agent-shell.pak; run `cargo xtask build esp32-p4-sim`")?;
    let surface = UiSurface::new_with_density((UI_WIDTH as f32, UI_HEIGHT as f32), RASTER_DENSITY);
    surface.set_svc_allowlist(["pocket-pi"]);
    surface.feed_pak(&pak);
    let guest = Guest::new()?;
    surface.mount(&guest)?;
    guest.eval("agent-shell", &js)?;
    Ok((guest, surface))
}

enum AgentEvent {
    Delta(String),
    Done,
    Failed(String),
}

struct NoTools;

impl ToolHost for NoTools {
    fn execute(&self, _call_id: &str, name: &str, _args_json: &str) -> ToolResult {
        ToolResult {
            text: format!("tool not configured in simulator: {name}"),
            is_error: true,
            terminate: false,
        }
    }
}

fn spawn_agent(backend: BackendChoice) -> (Sender<String>, Receiver<AgentEvent>) {
    let (prompt_tx, prompt_rx) = mpsc::channel::<String>();
    let (event_tx, event_rx) = mpsc::channel::<AgentEvent>();
    let config = backend.agent_config();
    let backend = backend.build();
    std::thread::spawn(move || {
        let delta_tx = event_tx.clone();
        let runtime = PiEmbedded::new(
            &config,
            backend,
            Arc::new(NoTools),
            Arc::new(move |delta| {
                delta_tx.send(AgentEvent::Delta(delta)).ok();
            }),
        );
        let runtime = match runtime {
            Ok(runtime) => runtime,
            Err(error) => {
                event_tx.send(AgentEvent::Failed(error)).ok();
                return;
            }
        };
        for prompt in prompt_rx {
            let result = runtime.prompt(&prompt).and_then(|()| runtime.pump());
            match result {
                Ok(_) => event_tx.send(AgentEvent::Done).ok(),
                Err(error) => event_tx.send(AgentEvent::Failed(error)).ok(),
            };
        }
    });
    (prompt_tx, event_rx)
}

struct Product {
    workspace: PathBuf,
    snapshot: AppSnapshot,
    prompt_tx: Sender<String>,
    agent_rx: Receiver<AgentEvent>,
    next_turn_id: u64,
    dirty: bool,
}

impl Product {
    fn new(workspace: PathBuf, backend: BackendChoice) -> Result<Self> {
        let backend_label = backend.label();
        let (prompt_tx, agent_rx) = spawn_agent(backend);
        let mut product = Self {
            workspace,
            snapshot: AppSnapshot {
                active_view: View::Chat,
                system: SystemState {
                    backend: backend_label,
                    network: "ONLINE".into(),
                    free_ram_kib: 24 * 1024,
                    fps: 60,
                },
                ..AppSnapshot::default()
            },
            prompt_tx,
            agent_rx,
            next_turn_id: 1,
            dirty: true,
        };
        let ready_turn_id = product.take_turn_id();
        product.snapshot.chat.turns.push(Turn {
            id: ready_turn_id,
            role: Role::Assistant,
            text: "ESP32-P4 embedded profile simulator is ready.".into(),
            streaming: false,
        });
        product.refresh_workspace()?;
        Ok(product)
    }

    fn take_turn_id(&mut self) -> u64 {
        let id = self.next_turn_id;
        self.next_turn_id = self.next_turn_id.saturating_add(1);
        id
    }

    fn refresh_workspace(&mut self) -> Result<()> {
        let mut entries = Vec::new();
        for item in std::fs::read_dir(&self.workspace)? {
            let item = item?;
            let metadata = item.metadata()?;
            if !metadata.is_file() {
                continue;
            }
            entries.push(FileEntry {
                name: item.file_name().to_string_lossy().into_owned(),
                size: metadata.len(),
                modified_unix_seconds: metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs())
                    .unwrap_or_default(),
            });
        }
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        self.snapshot.workspace.path = "/workspace".into();
        self.snapshot.workspace.entries = entries;
        self.changed();
        Ok(())
    }

    fn changed(&mut self) {
        self.snapshot.changed();
        self.dirty = true;
    }

    fn send_prompt(&mut self, text: String) {
        if self.snapshot.chat.busy || text.trim().is_empty() {
            return;
        }
        let user_id = self.take_turn_id();
        let assistant_id = self.take_turn_id();
        self.snapshot.chat.turns.push(Turn {
            id: user_id,
            role: Role::User,
            text: text.clone(),
            streaming: false,
        });
        self.snapshot.chat.turns.push(Turn {
            id: assistant_id,
            role: Role::Assistant,
            text: String::new(),
            streaming: true,
        });
        if self.snapshot.chat.turns.len() > 10 {
            let remove = self.snapshot.chat.turns.len() - 10;
            self.snapshot.chat.turns.drain(0..remove);
        }
        self.snapshot.chat.busy = true;
        self.prompt_tx.send(text).ok();
        self.changed();
    }

    fn handle(&mut self, command: AppCommand) -> Result<()> {
        match command {
            AppCommand::SwitchView { view } => self.snapshot.active_view = view,
            AppCommand::SendPrompt { text } => {
                self.send_prompt(text);
                return Ok(());
            }
            AppCommand::OpenPath { name } => {
                if name.contains('/') || name.contains('\\') {
                    return Ok(());
                }
                let path = self.workspace.join(&name);
                if path.is_file() {
                    self.snapshot.workspace.open_file = Some(OpenFile {
                        name,
                        content: std::fs::read_to_string(path)
                            .unwrap_or_else(|_| "<binary file>".into()),
                    });
                }
            }
            AppCommand::CloseFile => self.snapshot.workspace.open_file = None,
        }
        self.changed();
        Ok(())
    }

    fn poll_agent(&mut self) {
        while let Ok(event) = self.agent_rx.try_recv() {
            match event {
                AgentEvent::Delta(delta) => {
                    if let Some(turn) = self.snapshot.chat.turns.last_mut() {
                        turn.text.push_str(&delta);
                    }
                }
                AgentEvent::Done => {
                    self.snapshot.chat.busy = false;
                    if let Some(turn) = self.snapshot.chat.turns.last_mut() {
                        turn.streaming = false;
                    }
                }
                AgentEvent::Failed(error) => {
                    self.snapshot.chat.busy = false;
                    if let Some(turn) = self.snapshot.chat.turns.last_mut() {
                        turn.streaming = false;
                        turn.text = format!("Agent failed: {error}");
                    }
                }
            }
            self.changed();
        }
    }

    fn exchange(&mut self, surface: &UiSurface) {
        for line in surface.svc_drain() {
            match decode_command(&line) {
                Ok(command) => {
                    self.handle(command).ok();
                }
                Err(error) => log::warn!("invalid UI command: {error}"),
            }
        }
        self.poll_agent();
        if self.dirty {
            if let Ok(message) = encode_snapshot(&self.snapshot) {
                surface.svc_push(message);
            }
            self.dirty = false;
        }
    }
}

fn headless(
    output: PathBuf,
    workspace: PathBuf,
    prompt: Option<String>,
    view: View,
    backend: BackendChoice,
) -> Result<()> {
    let (guest, surface) = boot_ui()?;
    let mut product = Product::new(workspace, backend)?;
    product.snapshot.active_view = view;
    product.changed();
    let wait_for_turn = prompt.is_some();
    if let Some(prompt) = prompt {
        product.send_prompt(prompt);
    }
    let gpu = Gpu::new_headless()?;
    let target = OffscreenTarget::new(&gpu, UI_WIDTH * RASTER_DENSITY, UI_HEIGHT * RASTER_DENSITY);
    let mut renderer = UiRenderer::new(&gpu, pocket3d::gpu::OFFSCREEN_FORMAT);
    for frame in 0..7_500 {
        product.exchange(&surface);
        guest.frame(0)?;
        surface.tick();
        if frame >= 30 && (!wait_for_turn || !product.snapshot.chat.busy) {
            break;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    surface.with_ui(|ui| {
        let words = ui.draw().words.clone();
        renderer.render_words_scaled(
            &gpu,
            ui,
            &words,
            &mut encoder,
            &target.view,
            (UI_WIDTH * RASTER_DENSITY, UI_HEIGHT * RASTER_DENSITY),
            RASTER_DENSITY as f32,
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
    guest: Guest,
    ui: UiSurface,
    product: Product,
    buttons: u32,
    cursor: (u32, u32),
    touch_down: bool,
    frames: u32,
    fps_started: Instant,
}

struct WindowApp {
    workspace: PathBuf,
    initial_prompt: Option<String>,
    initial_view: View,
    backend: Option<BackendChoice>,
    state: Option<WindowState>,
    error: Option<anyhow::Error>,
}

fn windowed(
    workspace: PathBuf,
    prompt: Option<String>,
    view: View,
    backend: BackendChoice,
) -> Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = WindowApp {
        workspace,
        initial_prompt: prompt,
        initial_view: view,
        backend: Some(backend),
        state: None,
        error: None,
    };
    event_loop.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

impl WindowApp {
    fn init(&mut self, event_loop: &ActiveEventLoop) -> Result<WindowState> {
        let (guest, ui) = boot_ui()?;
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Pocket Pi — ESP32-P4 Simulator")
                    .with_inner_size(winit::dpi::LogicalSize::new(UI_WIDTH, UI_HEIGHT))
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
        let mut product = Product::new(self.workspace.clone(), backend)?;
        product.snapshot.active_view = self.initial_view.clone();
        product.changed();
        if let Some(prompt) = self.initial_prompt.take() {
            product.send_prompt(prompt);
        }
        Ok(WindowState {
            window,
            gpu,
            surface,
            config,
            renderer,
            guest,
            ui,
            product,
            buttons: 0,
            cursor: (0, 0),
            touch_down: false,
            frames: 0,
            fps_started: Instant::now(),
        })
    }

    fn redraw(state: &mut WindowState) -> Result<()> {
        state.product.exchange(&state.ui);
        let touches = if state.touch_down {
            vec![pack_touch(state.cursor.0, state.cursor.1)]
        } else {
            Vec::new()
        };
        state
            .guest
            .frame_with_touches(state.buttons, ANALOG_CENTER, &touches)?;
        state.ui.tick();

        state.frames += 1;
        let elapsed = state.fps_started.elapsed();
        if elapsed >= Duration::from_secs(1) {
            state.product.snapshot.system.fps =
                (state.frames as f32 / elapsed.as_secs_f32()).round() as u16;
            state.product.changed();
            state.frames = 0;
            state.fps_started = Instant::now();
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
        let scale = state.config.width as f32 / UI_WIDTH as f32;
        state.ui.with_ui(|ui| {
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
        state.window.request_redraw();
        Ok(())
    }
}

impl ApplicationHandler for WindowApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
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
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::CursorMoved { position, .. } => {
                let scale = state.config.width as f64 / UI_WIDTH as f64;
                state.cursor = (
                    (position.x / scale).clamp(0.0, (UI_WIDTH - 1) as f64) as u32,
                    (position.y / scale).clamp(0.0, (UI_HEIGHT - 1) as f64) as u32,
                );
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: element,
                ..
            } => state.touch_down = element == ElementState::Pressed,
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if code == KeyCode::Escape {
                        event_loop.exit();
                        return;
                    }
                    if let Some(button) = button_for(code) {
                        if event.state == ElementState::Pressed {
                            state.buttons |= button;
                        } else {
                            state.buttons &= !button;
                        }
                    }
                }
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

fn pack_touch(x: u32, y: u32) -> u32 {
    0x8000_0000 | ((y & 0x3ff) << 10) | (x & 0x3ff)
}

fn button_for(code: KeyCode) -> Option<u32> {
    Some(match code {
        KeyCode::ArrowUp => BTN_UP,
        KeyCode::ArrowRight => BTN_RIGHT,
        KeyCode::ArrowDown => BTN_DOWN,
        KeyCode::ArrowLeft => BTN_LEFT,
        KeyCode::KeyQ => BTN_LTRIGGER,
        KeyCode::KeyW => BTN_RTRIGGER,
        KeyCode::Enter | KeyCode::Space => BTN_CIRCLE,
        _ => return None,
    })
}
