use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use pocket3d::gpu::{Gpu, OffscreenTarget};
use pocket_pi_device_ui::{
    load_fonts, AgentState, ChatProjection, DeviceState, ModelBackendSettings, ModelSettings,
    ScreenInteraction, ScreenState, ScreenView, SettingsCommand, SettingsProjection,
    SystemTelemetry, UartProvider, WifiNetworkProjection,
};
use pocket_pi_embedded::{spawn_agent_worker, AgentEvent};
use pocket_pi_tools::{CoreToolHost, PlatformTools};
use pocket_ui_wgpu::UiRenderer;
use pocketjs_core::Ui;
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
    view: ScreenView,
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
    let mut view = ScreenView::Chat;
    let mut backend = std::env::var("POCKET_PI_BACKEND").unwrap_or_else(|_| "codex".into());
    let mut model = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--screenshot" => screenshot = Some(PathBuf::from(next(&mut args, "--screenshot")?)),
            "--prompt" => prompt = Some(next(&mut args, "--prompt")?),
            "--workspace" => workspace = PathBuf::from(next(&mut args, "--workspace")?),
            "--view" => {
                view = match next(&mut args, "--view")?.as_str() {
                    "chat" => ScreenView::Chat,
                    "workspace" | "files" => ScreenView::Files,
                    "settings" => ScreenView::Settings,
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
        std::fs::write(notes, "Physical ESP32 and simulator share one device UI.\n")?;
    }
    Ok(())
}

fn new_ui() -> Result<Ui> {
    let mut ui = Ui::new();
    ui.set_viewport(PANEL_WIDTH as f32, PANEL_HEIGHT as f32);
    if !load_fonts(&mut ui) {
        return Err(anyhow!("PocketJS rejected the shared Inter font atlases"));
    }
    Ok(ui)
}

struct SimPlatform;

impl PlatformTools for SimPlatform {
    fn device_status(&self) -> serde_json::Value {
        serde_json::json!({
            "status":"ok",
            "board":"esp32-p4-sim",
            "piHarness":"pi-agent-core",
            "jsRuntime":"QuickJS via PocketJS host",
            "simulated":true
        })
    }

    fn wifi_status(&self) -> serde_json::Value {
        serde_json::json!({
            "status":"connected",
            "ssid":"macOS host network",
            "simulated":true
        })
    }

    fn reboot(&self) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"status":"scheduled","simulated":true}))
    }
}

struct Product {
    chat: ChatProjection,
    screen: ScreenState,
    device: DeviceState,
    prompt_tx: Sender<String>,
    agent_rx: Receiver<AgentEvent>,
    tools: Arc<CoreToolHost>,
    settings: SettingsProjection,
    last_schedule_poll: Instant,
    busy: bool,
    dirty: bool,
}

impl Product {
    fn new(workspace: PathBuf, backend: BackendChoice, view: ScreenView) -> Result<Self> {
        let local_codex = matches!(backend, BackendChoice::Codex { .. });
        let model_settings = settings_for(&backend);
        let tools = Arc::new(CoreToolHost::new(workspace.clone(), Arc::new(SimPlatform)));
        let config = backend.agent_config();
        let backend = backend.build();
        let (prompt_tx, agent_rx) =
            spawn_agent_worker(config, backend, tools.clone(), None).map_err(anyhow::Error::msg)?;
        let workspace = workspace
            .to_str()
            .ok_or_else(|| anyhow!("workspace path is not valid UTF-8"))?;
        let mut screen = ScreenState::new(workspace);
        let settings = SettingsProjection {
            wifi: pocket_pi_device_ui::WifiSettingsProjection {
                connected_ssid: Some("macOS host network".into()),
                ip_address: Some("192.0.2.2".into()),
                rssi_dbm: Some(-42),
                status: "SIMULATED ESP32 WI-FI".into(),
                ..Default::default()
            },
            firmware_version: env!("CARGO_PKG_VERSION").into(),
            workspace_free_bytes: None,
        };
        screen.view = view;
        screen.set_model_backend(&model_settings);
        if local_codex {
            screen.set_backend_status("CODEX", "LOCAL / MAC", "CODING PLAN");
        }
        screen.set_telemetry(SystemTelemetry {
            psram_used_percent: 25,
            psram_free_bytes: 24 * 1024 * 1024,
            cpu_percent: None,
            ui_fps_tenths: 0,
            lcd_refresh_hz: 32,
        });
        screen.refresh_workspace();
        screen.set_settings(settings.clone());
        Ok(Self {
            chat: ChatProjection::new("TYPE A MESSAGE", "ESP32-P4 PI AGENT SIMULATOR READY."),
            screen,
            device: DeviceState {
                agent: AgentState::Idle,
            },
            prompt_tx,
            agent_rx,
            tools,
            settings,
            last_schedule_poll: Instant::now(),
            busy: false,
            dirty: true,
        })
    }

    fn send_prompt(&mut self, prompt: String) {
        if self.busy || prompt.trim().is_empty() {
            return;
        }
        self.chat.push_pending(prompt.clone());
        self.screen.show_latest_chat();
        self.device.agent = AgentState::Thinking;
        self.busy = true;
        if self.prompt_tx.send(prompt).is_err() {
            self.chat.fail_pending("AGENT WORKER IS NOT AVAILABLE");
            self.device.agent = AgentState::Faulted;
            self.busy = false;
        }
        self.dirty = true;
    }

    fn tap(&mut self, x: u16, y: u16, ui: &Ui) {
        match self.screen.handle_tap(x, y, &self.chat, ui) {
            ScreenInteraction::None => {}
            ScreenInteraction::Redraw => self.dirty = true,
            ScreenInteraction::SubmitPrompt(prompt) => self.send_prompt(prompt),
            ScreenInteraction::Settings(command) => self.handle_settings(command),
        }
    }

    fn handle_settings(&mut self, command: SettingsCommand) {
        match command {
            SettingsCommand::ScanWifi => {
                self.settings.wifi.networks = vec![
                    WifiNetworkProjection {
                        ssid: "POCKET-PI-LAB".into(),
                        rssi_dbm: -38,
                        secured: true,
                    },
                    WifiNetworkProjection {
                        ssid: "PHONE-HOTSPOT".into(),
                        rssi_dbm: -56,
                        secured: true,
                    },
                    WifiNetworkProjection {
                        ssid: "GUEST".into(),
                        rssi_dbm: -71,
                        secured: false,
                    },
                    WifiNetworkProjection {
                        ssid: "OFFICE".into(),
                        rssi_dbm: -74,
                        secured: true,
                    },
                    WifiNetworkProjection {
                        ssid: "CAFE".into(),
                        rssi_dbm: -78,
                        secured: false,
                    },
                    WifiNetworkProjection {
                        ssid: "PHONE-2".into(),
                        rssi_dbm: -82,
                        secured: true,
                    },
                ];
                self.settings.wifi.status.clear();
            }
            SettingsCommand::ConnectWifi { ssid, password } => {
                if !password.is_empty() && !(8..=63).contains(&password.len()) {
                    self.settings.wifi.status = "PASSWORD MUST BE 8-63 BYTES".into();
                } else {
                    self.settings.wifi.connected_ssid = Some(ssid);
                    self.settings.wifi.ip_address = Some("192.0.2.2".into());
                    self.settings.wifi.rssi_dbm = Some(-40);
                    self.settings.wifi.status = "CONNECTED (SIMULATED)".into();
                }
            }
            SettingsCommand::ForgetWifi => {
                self.settings.wifi.connected_ssid = None;
                self.settings.wifi.ip_address = None;
                self.settings.wifi.rssi_dbm = None;
                self.settings.wifi.status = "NETWORK FORGOTTEN (SIMULATED)".into();
            }
            SettingsCommand::Restart => {
                self.settings.wifi.status = "RESTART REQUESTED (SIMULATED)".into();
            }
        }
        self.screen.set_settings(self.settings.clone());
        self.dirty = true;
    }

    fn release_touch(&mut self) {
        self.dirty |= self.screen.handle_touch_release();
    }

    fn poll_agent(&mut self) {
        while let Ok(event) = self.agent_rx.try_recv() {
            match event {
                AgentEvent::Ready => {
                    self.device.agent = AgentState::Idle;
                }
                AgentEvent::Delta(delta) => {
                    if self.chat.append_model_delta(&delta) {
                        self.screen.show_latest_chat();
                    }
                }
                AgentEvent::Done => {
                    self.chat.finish_pending();
                    self.screen.refresh_workspace();
                    self.device.agent = AgentState::Idle;
                    self.busy = false;
                }
                AgentEvent::Failed(error) => {
                    self.chat.fail_pending(format!("AGENT FAILED: {error}"));
                    self.device.agent = AgentState::Faulted;
                    self.busy = false;
                }
            }
            self.dirty = true;
        }
        if self.last_schedule_poll.elapsed() >= Duration::from_secs(1) {
            let schedule = self.tools.schedule_projection();
            self.screen
                .set_schedule(pocket_pi_device_ui::ScheduleProjection {
                    name: schedule.name,
                    prompt: schedule.prompt,
                    next_in_seconds: schedule.next_in_seconds,
                    every_minutes: schedule.every_minutes,
                });
            if !self.busy {
                if let Some(wake) = self.tools.claim_due() {
                    self.send_prompt(wake.prompt);
                }
            }
            self.last_schedule_poll = Instant::now();
            self.dirty = true;
        }
    }

    fn words(&self, ui: &Ui) -> Vec<u32> {
        self.screen.draw_list(ui, &self.device, &self.chat)
    }
}

fn settings_for(backend: &BackendChoice) -> ModelSettings {
    let backend = match backend {
        BackendChoice::Wireless { provider, .. } => ModelBackendSettings::Wireless {
            provider: *provider,
        },
        BackendChoice::Codex { .. } => ModelBackendSettings::Uart {
            provider: UartProvider::Codex,
        },
    };
    ModelSettings {
        backend,
        model: None,
    }
}

fn headless(
    output: PathBuf,
    workspace: PathBuf,
    prompt: Option<String>,
    view: ScreenView,
    backend: BackendChoice,
) -> Result<()> {
    let ui = new_ui()?;
    let mut product = Product::new(workspace, backend, view)?;
    let wait_for_turn = prompt.is_some();
    if let Some(prompt) = prompt {
        product.send_prompt(prompt);
    }
    for _ in 0..7_500 {
        product.poll_agent();
        if !wait_for_turn || !product.busy {
            break;
        }
        std::thread::sleep(Duration::from_millis(16));
    }

    let gpu = Gpu::new_headless()?;
    let target = OffscreenTarget::new(&gpu, PANEL_WIDTH, PANEL_HEIGHT);
    let mut renderer = UiRenderer::new(&gpu, pocket3d::gpu::OFFSCREEN_FORMAT);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    let words = product.words(&ui);
    renderer.render_words(
        &gpu,
        &ui,
        &words,
        &mut encoder,
        &target.view,
        (PANEL_WIDTH, PANEL_HEIGHT),
        wgpu::LoadOp::Clear(wgpu::Color::BLACK),
    )?;
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
    ui: Ui,
    product: Product,
    cursor: (u16, u16),
    touch_down: bool,
    frames: u32,
    fps_started: Instant,
}

struct WindowApp {
    workspace: PathBuf,
    initial_prompt: Option<String>,
    initial_view: ScreenView,
    backend: Option<BackendChoice>,
    state: Option<WindowState>,
    error: Option<anyhow::Error>,
}

fn windowed(
    workspace: PathBuf,
    prompt: Option<String>,
    view: ScreenView,
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
        let ui = new_ui()?;
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Pocket Pi — ESP32-P4 Simulator")
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
        let mut product = Product::new(self.workspace.clone(), backend, self.initial_view)?;
        if let Some(prompt) = self.initial_prompt.take() {
            product.send_prompt(prompt);
        }
        Ok(WindowState {
            window,
            gpu,
            surface,
            config,
            renderer,
            ui,
            product,
            cursor: (0, 0),
            touch_down: false,
            frames: 0,
            fps_started: Instant::now(),
        })
    }

    fn redraw(state: &mut WindowState) -> Result<()> {
        state.product.poll_agent();
        state.frames += 1;
        let elapsed = state.fps_started.elapsed();
        if elapsed >= Duration::from_secs(1) {
            state.product.screen.telemetry.ui_fps_tenths =
                ((state.frames as f32 / elapsed.as_secs_f32()) * 10.0).round() as u16;
            state.frames = 0;
            state.fps_started = Instant::now();
            state.product.dirty = true;
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
        let scale = state.config.width as f32 / PANEL_WIDTH as f32;
        let words = state.product.words(&state.ui);
        state.renderer.render_words_scaled(
            &state.gpu,
            &state.ui,
            &words,
            &mut encoder,
            &view,
            (state.config.width, state.config.height),
            scale,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        )?;
        state.gpu.queue.submit([encoder.finish()]);
        state.window.pre_present_notify();
        frame.present();
        state.product.dirty = false;
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
                state.product.tap(state.cursor.0, state.cursor.1, &state.ui);
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Released,
                ..
            } => {
                state.touch_down = false;
                state.product.release_touch();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape))
                {
                    event_loop.exit();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottom_navigation_uses_the_physical_screen_hit_map() {
        let temp = tempfile::tempdir().unwrap();
        let ui = new_ui().unwrap();
        let mut product = Product::new(
            temp.path().to_owned(),
            BackendChoice::Codex { model: None },
            ScreenView::Chat,
        )
        .unwrap();

        product.tap(360, 1220, &ui);

        assert_eq!(product.screen.view, ScreenView::Files);
        product.screen.view = ScreenView::Chat;
        product.tap(650, 1220, &ui);
        assert_eq!(product.screen.view, ScreenView::Settings);
    }

    #[test]
    fn settings_scan_and_wifi_password_use_the_shared_touch_keyboard() {
        let temp = tempfile::tempdir().unwrap();
        let ui = new_ui().unwrap();
        let mut product = Product::new(
            temp.path().to_owned(),
            BackendChoice::Codex { model: None },
            ScreenView::Settings,
        )
        .unwrap();

        product.tap(580, 180, &ui);
        product.tap(120, 350, &ui);
        for _ in 0..8 {
            product.tap(54, 548, &ui);
        }
        product.tap(640, 986, &ui);

        assert_eq!(
            product.settings.wifi.connected_ssid.as_deref(),
            Some("POCKET-PI-LAB")
        );
        assert!(!product.screen.handle_touch_release());
    }

    #[test]
    fn settings_wifi_list_scrolls_to_later_networks() {
        let temp = tempfile::tempdir().unwrap();
        let ui = new_ui().unwrap();
        let mut product = Product::new(
            temp.path().to_owned(),
            BackendChoice::Codex { model: None },
            ScreenView::Settings,
        )
        .unwrap();

        product.tap(580, 180, &ui);
        assert!(product.settings.wifi.status.is_empty());
        product.tap(660, 710, &ui);
        product.tap(120, 626, &ui);

        assert_eq!(
            product.settings.wifi.connected_ssid.as_deref(),
            Some("CAFE")
        );
    }

    #[test]
    fn mouse_coordinates_drive_the_same_keyboard_as_touch() {
        let temp = tempfile::tempdir().unwrap();
        let ui = new_ui().unwrap();
        let mut screen = ScreenState::new(temp.path().to_str().unwrap());
        let chat = ChatProjection::new("YOU", "PI");

        assert_eq!(
            screen.handle_tap(360, 1100, &chat, &ui),
            ScreenInteraction::Redraw
        );
        assert_eq!(
            screen.handle_tap(54, 548, &chat, &ui),
            ScreenInteraction::Redraw
        );
        assert_eq!(
            screen.handle_tap(640, 986, &chat, &ui),
            ScreenInteraction::SubmitPrompt("q".into())
        );
    }

    #[test]
    fn touch_keyboard_can_type_uppercase_wifi_passwords() {
        let temp = tempfile::tempdir().unwrap();
        let ui = new_ui().unwrap();
        let mut screen = ScreenState::new(temp.path().to_str().unwrap());
        let chat = ChatProjection::new("YOU", "PI");

        assert_eq!(
            screen.handle_tap(360, 1100, &chat, &ui),
            ScreenInteraction::Redraw
        );
        assert_eq!(
            screen.handle_tap(450, 986, &chat, &ui),
            ScreenInteraction::Redraw
        );
        assert_eq!(
            screen.handle_tap(54, 548, &chat, &ui),
            ScreenInteraction::Redraw
        );
        assert_eq!(
            screen.handle_tap(640, 986, &chat, &ui),
            ScreenInteraction::SubmitPrompt("Q".into())
        );
    }

    #[test]
    fn workspace_row_opens_the_physical_file_viewer() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("memory.md"), "shared-ui").unwrap();
        let ui = new_ui().unwrap();
        let mut product = Product::new(
            temp.path().to_owned(),
            BackendChoice::Codex { model: None },
            ScreenView::Chat,
        )
        .unwrap();

        product.tap(360, 1220, &ui);
        product.tap(120, 220, &ui);

        assert_eq!(product.screen.view, ScreenView::Viewer);
    }
}
