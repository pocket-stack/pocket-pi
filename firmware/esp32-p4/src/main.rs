use core::time::Duration;
use std::io::Read;
use std::sync::{mpsc, Arc};
use std::time::Instant;

use embedded_svc::http::{client::Client as HttpClient, Method};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::handle::RawHandle;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use esp_idf_svc::netif::{EspNetif, NetifStack};
use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition};
use esp_idf_svc::wifi::{AuthMethod, ClientConfiguration, Configuration, WifiDriver};
use pocket_pi_device_ui::{
    load_fonts, AgentState, ChatProjection, DeviceState, ScreenInteraction, ScreenState,
    SystemTelemetry,
};
use pocket_pi_embedded::{ModelBackend, PiEmbedded};
use pocket_pi_tools::{CoreToolHost, PlatformTools};
use pocketjs_core::Ui;
use pocketjs_esp32p4_ppa::{PpaOps, Rect, Renderer, RendererConfig, SrmTransform};

mod storage;

const BOARD_NAME: &str = "Waveshare ESP32-P4-WIFI6-Touch-LCD-5";
const PANEL_WIDTH: u32 = 720;
const PANEL_HEIGHT: u32 = 1280;
const WIFI_SSID_PRIMARY: &str = "<Y/OUR SPACE>_5g";
const WIFI_SSID_FALLBACK: &str = "<Y/OUR SPACE>_2.4G";
const WIFI_NVS_NAMESPACE: &str = "pocket_pi";
const WIFI_NVS_PASSWORD_KEY: &str = "wifi_pass";
const WIFI_PROVISION_PREFIX: &str = "PPI-WIFI-PASS:";

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let mut ui = Ui::new();
    ui.set_viewport(PANEL_WIDTH as f32, PANEL_HEIGHT as f32);
    if !load_fonts(&mut ui) {
        anyhow::bail!("PocketJS rejected the shared Inter font atlases")
    }
    let _workspace = storage::mount_workspace()?;

    let mut device = DeviceState {
        agent: AgentState::Starting,
    };
    let mut chat = ChatProjection::new("TYPE A MESSAGE", "BOOTING PI AGENT...");
    let mut screen = ScreenState::new(storage::WORKSPACE_ROOT);
    screen.refresh_workspace();
    screen.set_telemetry(system_telemetry(0));

    let tools = Arc::new(CoreToolHost::new(
        storage::WORKSPACE_ROOT,
        Arc::new(EspPlatform),
    ));
    let (prompt_tx, agent_rx) = spawn_agent(Arc::clone(&tools));
    device.agent = AgentState::Idle;
    chat.set_latest_assistant("ESP32-P4 PI AGENT READY.");

    let mut renderer = Renderer::new(RendererConfig::default())
        .ok_or_else(|| anyhow::anyhow!("invalid PocketJS renderer configuration"))?;
    log::info!("Pocket Pi ESP32-P4 hardware probe: {BOARD_NAME}");
    log::info!(
        "PocketJS shared device UI ready: viewport={:?} scale={}",
        ui.viewport(),
        renderer.config().scale,
    );

    let mut display = match init_display_probe(&mut renderer, &ui, &device, &chat, &screen) {
        Ok(display) => {
            log::info!("MIPI-DSI panel active");
            Some(display)
        }
        Err(error) => {
            log::error!("MIPI-DSI panel failed: {error:#}");
            None
        }
    };

    let wifi = match init_wifi() {
        Ok(wifi) => {
            log::info!("C6-SDIO Wi-Fi and lwIP netif active");
            Some(wifi)
        }
        Err(error) => {
            log::error!("C6-SDIO Wi-Fi radio probe failed: {error:#}");
            None
        }
    };
    log::info!(
        "network targets staged: openai={} codex_plan={}",
        pocket_pi_protocols::model::OPENAI_API_BASE_URL,
        pocket_pi_protocols::model::CODEX_BACKEND_BASE_URL,
    );
    if wifi.is_some() {
        match probe_https_origins() {
            Ok(reachability) => log::info!("HTTPS connectivity probe completed: {reachability:?}"),
            Err(error) => log::error!("HTTPS connectivity probe failed: {error:#}"),
        }
    }

    let mut touch_was_down = false;
    let mut redraw = true;
    let mut last_telemetry = Instant::now();
    let mut last_heartbeat = Instant::now();
    loop {
        while let Ok(event) = agent_rx.try_recv() {
            match event {
                AgentEvent::Delta(delta) => {
                    if chat.append_model_delta(&delta) {
                        screen.show_latest_chat();
                    }
                }
                AgentEvent::Done => {
                    chat.finish_pending();
                    screen.refresh_workspace();
                    device.agent = AgentState::Idle;
                }
                AgentEvent::Failed(error) => {
                    chat.fail_pending(format!("AGENT FAILED: {error}"));
                    device.agent = AgentState::Faulted;
                }
            }
            redraw = true;
        }

        if let Some(display) = display.as_mut() {
            if let Some((x, y)) = display.read_touch() {
                if !touch_was_down {
                    match screen.handle_tap(x, y, &chat, &ui) {
                        ScreenInteraction::None => {}
                        ScreenInteraction::Redraw => redraw = true,
                        ScreenInteraction::SubmitPrompt(prompt) => {
                            chat.push_pending(prompt.clone());
                            screen.show_latest_chat();
                            device.agent = AgentState::Thinking;
                            if prompt_tx.send(prompt).is_err() {
                                chat.fail_pending("AGENT WORKER IS NOT AVAILABLE");
                                device.agent = AgentState::Faulted;
                            }
                            redraw = true;
                        }
                    }
                }
                touch_was_down = true;
            } else {
                if touch_was_down && screen.handle_touch_release() {
                    redraw = true;
                }
                touch_was_down = false;
            }
        }

        if last_telemetry.elapsed() >= Duration::from_secs(2) {
            screen.set_telemetry(system_telemetry(0));
            let schedule = tools.schedule_projection();
            screen.set_schedule(pocket_pi_device_ui::ScheduleProjection {
                name: schedule.name,
                prompt: schedule.prompt,
                next_in_seconds: schedule.next_in_seconds,
                every_minutes: schedule.every_minutes,
            });
            if device.agent == AgentState::Idle {
                if let Some(wake) = tools.claim_due() {
                    chat.push_pending(wake.prompt.clone());
                    screen.show_latest_chat();
                    device.agent = AgentState::Thinking;
                    if prompt_tx.send(wake.prompt).is_err() {
                        chat.fail_pending("AGENT WORKER IS NOT AVAILABLE");
                        device.agent = AgentState::Faulted;
                    }
                }
            }
            redraw = true;
            last_telemetry = Instant::now();
        }
        if redraw {
            if let Some(display) = display.as_mut() {
                display.render(&mut renderer, &ui, &device, &chat, &screen)?;
            }
            redraw = false;
        }
        if last_heartbeat.elapsed() >= Duration::from_secs(5) {
            let memory = memory_snapshot();
            log::info!(
                "heartbeat heap={} psram_free={} agent={:?}",
                memory.free_heap,
                memory.psram_free,
                device.agent,
            );
            last_heartbeat = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}

enum AgentEvent {
    Delta(String),
    Done,
    Failed(String),
}

fn spawn_agent(tools: Arc<CoreToolHost>) -> (mpsc::Sender<String>, mpsc::Receiver<AgentEvent>) {
    let (prompt_tx, prompt_rx) = mpsc::channel::<String>();
    let (event_tx, event_rx) = mpsc::channel::<AgentEvent>();
    std::thread::spawn(move || {
        let delta_tx = event_tx.clone();
        let runtime = PiEmbedded::new(
            r#"{"provider":"offline","model":"esp32-p4","systemPrompt":"You are Pocket Pi."}"#,
            Arc::new(OfflineModel),
            tools,
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

struct OfflineModel;

impl ModelBackend for OfflineModel {
    fn complete(
        &self,
        _request_json: &str,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<String, String> {
        let text =
            "Embedded Pi is running. Configure a model adapter to replace this offline reply.";
        for word in text.split_inclusive(' ') {
            on_delta(word);
        }
        Ok(r#"{"text":"Embedded Pi is running. Configure a model adapter to replace this offline reply."}"#.into())
    }
}

struct EspPlatform;

impl PlatformTools for EspPlatform {
    fn device_status(&self) -> serde_json::Value {
        serde_json::json!({
            "status":"ok",
            "board":"esp32-p4",
            "piHarness":"pi-agent-core",
            "jsRuntime":"QuickJS via PocketJS host",
            "freeHeapBytes":unsafe { esp_idf_svc::sys::esp_get_free_heap_size() },
            "freePsramBytes":unsafe {
                esp_idf_svc::sys::heap_caps_get_free_size(
                    esp_idf_svc::sys::MALLOC_CAP_SPIRAM,
                )
            }
        })
    }

    fn wifi_status(&self) -> serde_json::Value {
        let mut access_point = unsafe { core::mem::zeroed::<esp_idf_svc::sys::wifi_ap_record_t>() };
        let status = unsafe { esp_idf_svc::sys::esp_wifi_sta_get_ap_info(&mut access_point) };
        if status != esp_idf_svc::sys::ESP_OK {
            return serde_json::json!({"status":"offline","espError":status});
        }
        let ssid_end = access_point
            .ssid
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(access_point.ssid.len());
        let ssid = String::from_utf8_lossy(&access_point.ssid[..ssid_end]);
        serde_json::json!({
            "status":"connected",
            "ssid":ssid,
            "rssiDbm":access_point.rssi,
            "channel":access_point.primary
        })
    }

    fn reboot(&self) -> Result<serde_json::Value, String> {
        std::thread::Builder::new()
            .name("delayed-reboot".to_owned())
            .stack_size(4 * 1024)
            .spawn(|| {
                std::thread::sleep(Duration::from_millis(750));
                unsafe { esp_idf_svc::sys::esp_restart() }
            })
            .map_err(|error| format!("schedule reboot: {error}"))?;
        Ok(serde_json::json!({"status":"scheduled","delayMs":750}))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct HttpsReachability {
    openai_api: bool,
    codex_backend: bool,
}

fn probe_https_origins() -> anyhow::Result<HttpsReachability> {
    let probes = [
        ("control", "https://api.github.com/zen", true),
        ("openai-api", "https://api.openai.com/v1/models", false),
        (
            "codex-backend",
            "https://chatgpt.com/backend-api/models",
            false,
        ),
    ];

    let mut control_ok = false;
    let mut target_ok = 0u8;
    let mut reachability = HttpsReachability::default();
    for (name, url, control) in probes {
        let configuration = HttpConfiguration {
            timeout: Some(Duration::from_secs(10)),
            crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
            ..Default::default()
        };
        let mut client = HttpClient::wrap(EspHttpConnection::new(&configuration)?);
        let request = match client.request(
            Method::Get,
            url,
            &[
                ("accept", "application/json"),
                ("user-agent", "pocket-pi-p4/0.1"),
            ],
        ) {
            Ok(request) => request,
            Err(error) => {
                log::warn!("HTTPS probe setup failed: origin={name} error={error}");
                continue;
            }
        };
        match request.submit() {
            Ok(response) => {
                let status = response.status();
                if !(100..600).contains(&status) {
                    log::warn!("HTTPS probe invalid status: origin={name} status={status}");
                    continue;
                }
                log::info!("HTTPS probe complete: origin={name} status={status}");
                if control {
                    control_ok = true;
                } else {
                    target_ok = target_ok.saturating_add(1);
                    match name {
                        "openai-api" => reachability.openai_api = true,
                        "codex-backend" => reachability.codex_backend = true,
                        _ => {}
                    }
                }
            }
            Err(error) => {
                log::warn!("HTTPS probe unavailable: origin={name} error={error}");
            }
        }
    }

    if !control_ok {
        anyhow::bail!("control HTTPS origin was unreachable")
    }
    log::info!("target HTTPS reachability: {target_ok}/2");
    Ok(reachability)
}

fn init_wifi() -> anyhow::Result<WifiConnection> {
    let peripherals = Peripherals::take()?;
    let system_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let password = load_or_provision_wifi_password(nvs.clone())?;
    // A radio scan does not need a TCP/IP netif. Keeping this as a bare
    // WifiDriver also avoids double-registering the remote STA interface: the
    // C6 firmware owns it until the IP-connectivity milestone attaches lwIP.
    let mut wifi = WifiDriver::new(peripherals.modem, system_loop.clone(), Some(nvs))?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration::default()))?;
    wifi.start()?;

    let (access_points, total_found) = wifi.scan_n::<16>()?;
    let strongest = access_points
        .iter()
        .max_by_key(|access_point| access_point.signal_strength);
    if let Some(access_point) = strongest {
        log::info!(
            "C6-SDIO scan complete: total={} retained={} strongest_rssi={}dBm channel={}",
            total_found,
            access_points.len(),
            access_point.signal_strength,
            access_point.channel,
        );
    } else {
        log::info!("C6-SDIO scan complete: no visible access points");
    }

    let primary_visible = access_points
        .iter()
        .any(|access_point| access_point.ssid.as_str() == WIFI_SSID_PRIMARY);
    let fallback_visible = access_points
        .iter()
        .any(|access_point| access_point.ssid.as_str() == WIFI_SSID_FALLBACK);
    log::info!(
        "configured Wi-Fi visibility: primary={} fallback={}",
        primary_visible,
        fallback_visible,
    );

    // Scan while no lwIP interface is attached. The generic EspWifi wrapper
    // creates both STA and AP netifs when soft-AP support is compiled in. The
    // C6 remote transport on this P4 is STA-only, and attaching both makes its
    // connect event add the same lwIP interface twice. Stop the radio, then
    // attach exactly one STA netif using the same sequence as Espressif's
    // esp_wifi_remote station example.
    wifi.stop()?;
    let stop_deadline = std::time::Instant::now() + Duration::from_secs(5);
    while wifi.is_started()? {
        if std::time::Instant::now() >= stop_deadline {
            anyhow::bail!("C6 Wi-Fi driver did not stop before lwIP attach")
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    log::info!("C6 Wi-Fi driver stopped; attaching one lwIP STA netif");
    let sta_netif = EspNetif::new(NetifStack::Sta)?;
    esp_result("esp_netif_attach_wifi_station", unsafe {
        esp_idf_svc::sys::esp_netif_attach_wifi_station(sta_netif.handle())
    })?;
    esp_result("esp_wifi_set_default_wifi_sta_handlers", unsafe {
        esp_idf_svc::sys::esp_wifi_set_default_wifi_sta_handlers()
    })?;

    let candidates = if primary_visible {
        [WIFI_SSID_PRIMARY, WIFI_SSID_FALLBACK]
    } else {
        [WIFI_SSID_FALLBACK, WIFI_SSID_PRIMARY]
    };
    let mut last_error = None;
    for (index, ssid) in candidates.into_iter().enumerate() {
        if (ssid == WIFI_SSID_PRIMARY && !primary_visible)
            || (ssid == WIFI_SSID_FALLBACK && !fallback_visible)
        {
            continue;
        }

        let configuration = Configuration::Client(ClientConfiguration {
            ssid: ssid.try_into()?,
            bssid: None,
            auth_method: AuthMethod::WPA2Personal,
            password: password.as_str().try_into()?,
            channel: None,
            ..Default::default()
        });
        wifi.set_configuration(&configuration)?;
        if !wifi.is_started()? {
            wifi.start()?;
        }

        log::info!("Wi-Fi connection attempt profile={}", index + 1);
        let attempt = (|| {
            wifi.connect()?;
            let deadline = std::time::Instant::now() + Duration::from_secs(15);
            while !wifi.is_connected()? || !sta_netif.is_up()? {
                if std::time::Instant::now() >= deadline {
                    anyhow::bail!("Wi-Fi association or DHCP timed out")
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok::<_, anyhow::Error>(())
        })();
        match attempt {
            Ok(()) => {
                let info = sta_netif.get_ip_info()?;
                log::info!(
                    "Wi-Fi connected: profile={} ip={} gateway={} dns={:?}",
                    index + 1,
                    info.ip,
                    info.subnet.gateway,
                    info.dns,
                );
                return Ok(WifiConnection {
                    _driver: wifi,
                    _sta_netif: sta_netif,
                });
            }
            Err(error) => {
                log::warn!("Wi-Fi profile {} failed: {error:#}", index + 1);
                last_error = Some(error);
                let _ = wifi.disconnect();
            }
        }
    }

    match last_error {
        Some(error) => Err(error),
        None => anyhow::bail!("neither configured Wi-Fi network is visible"),
    }
}

struct WifiConnection {
    _driver: WifiDriver<'static>,
    _sta_netif: EspNetif,
}

fn load_or_provision_wifi_password(partition: EspDefaultNvsPartition) -> anyhow::Result<String> {
    let storage = EspDefaultNvs::new(partition, WIFI_NVS_NAMESPACE, true)?;
    let mut password_buf = [0u8; 64];
    if let Some(password) = storage.get_str(WIFI_NVS_PASSWORD_KEY, &mut password_buf)? {
        validate_wifi_password(password)?;
        log::info!("Wi-Fi credential loaded from local NVS");
        return Ok(password.to_owned());
    }

    log::warn!("Wi-Fi credential missing; send the one-line USB-UART provisioning frame now");
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();
    let mut frame = Vec::with_capacity(96);
    let mut byte = [0u8; 1];
    loop {
        match stdin.read(&mut byte) {
            Ok(0) => {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Ok(_) if byte[0] == b'\n' => {}
            Ok(_) if byte[0] == b'\r' => continue,
            Ok(_) if !byte[0].is_ascii_graphic() && byte[0] != b' ' => continue,
            Ok(_) => {
                if frame.len() >= 96 {
                    frame.clear();
                    log::warn!("Discarded oversized UART provisioning frame");
                } else {
                    frame.push(byte[0]);
                }
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(error) => return Err(error.into()),
        }

        let line = core::str::from_utf8(&frame)?;
        let Some(prefix_start) = line.find(WIFI_PROVISION_PREFIX) else {
            log::warn!("Ignored non-provisioning UART input");
            frame.clear();
            continue;
        };
        let password = &line[prefix_start + WIFI_PROVISION_PREFIX.len()..];

        validate_wifi_password(password)?;
        storage.set_str(WIFI_NVS_PASSWORD_KEY, password)?;
        log::info!("Wi-Fi credential stored in local NVS (value not logged)");
        return Ok(password.to_owned());
    }
}

fn validate_wifi_password(password: &str) -> anyhow::Result<()> {
    if !(8..=63).contains(&password.len()) {
        anyhow::bail!("Wi-Fi password must contain 8 to 63 bytes")
    }
    if !password.is_ascii() {
        anyhow::bail!("Wi-Fi password must be ASCII")
    }
    Ok(())
}

#[derive(Debug)]
struct DisplayProbe {
    panel: esp_idf_svc::sys::esp_lcd_panel_handle_t,
    _io: esp_idf_svc::sys::esp_lcd_panel_io_handle_t,
    touch: esp_idf_svc::sys::esp_lcd_touch_handle_t,
    framebuffers: [*mut u16; 3],
    next_framebuffer: usize,
}

fn init_display_probe(
    renderer: &mut Renderer,
    ui: &Ui,
    device: &DeviceState,
    chat: &ChatProjection,
    screen: &ScreenState,
) -> anyhow::Result<DisplayProbe> {
    unsafe {
        let mut panel = core::ptr::null_mut();
        let mut io = core::ptr::null_mut();
        let mut touch = core::ptr::null_mut();
        let mut framebuffer_0 = core::ptr::null_mut();
        let mut framebuffer_1 = core::ptr::null_mut();
        let mut framebuffer_2 = core::ptr::null_mut();

        esp_result(
            "bsp_display_new",
            esp_idf_svc::sys::bsp_display_new(core::ptr::null(), &mut panel, &mut io),
        )?;
        esp_result(
            "esp_lcd_dpi_panel_get_frame_buffer",
            esp_idf_svc::sys::esp_lcd_dpi_panel_get_frame_buffer(
                panel,
                3,
                &mut framebuffer_0,
                &mut framebuffer_1,
                &mut framebuffer_2,
            ),
        )?;
        let framebuffers = [framebuffer_0, framebuffer_1, framebuffer_2];
        if framebuffers.iter().any(|framebuffer| framebuffer.is_null()) {
            anyhow::bail!("esp_lcd_dpi_panel_get_frame_buffer returned a null buffer")
        }

        let pixels = core::slice::from_raw_parts_mut(
            framebuffers[0].cast::<u16>(),
            PANEL_WIDTH as usize * PANEL_HEIGHT as usize,
        );
        let words = screen.draw_list(ui, device, chat);
        let mut software = SoftwareOnly;
        let stats = renderer
            .render(ui, &words, pixels, PANEL_WIDTH, PANEL_HEIGHT, &mut software)
            .ok_or_else(|| anyhow::anyhow!("PocketJS rejected the panel framebuffer geometry"))?;

        esp_result(
            "esp_lcd_dpi_panel_set_pattern",
            esp_idf_svc::sys::esp_lcd_dpi_panel_set_pattern(
                panel,
                esp_idf_svc::sys::mipi_dsi_pattern_type_t_MIPI_DSI_PATTERN_NONE,
            ),
        )?;
        esp_result(
            "esp_lcd_panel_disp_on_off",
            esp_idf_svc::sys::esp_lcd_panel_disp_on_off(panel, true),
        )?;
        esp_result(
            "esp_lcd_panel_draw_bitmap",
            esp_idf_svc::sys::esp_lcd_panel_draw_bitmap(
                panel,
                0,
                0,
                PANEL_WIDTH as i32,
                PANEL_HEIGHT as i32,
                framebuffers[0],
            ),
        )?;
        esp_result(
            "bsp_display_backlight_on",
            esp_idf_svc::sys::bsp_display_backlight_on(),
        )?;
        esp_result(
            "pi_p4_touch_new",
            esp_idf_svc::sys::pi_p4_touch_new(&mut touch),
        )?;

        log::info!(
            "PocketJS triple-buffer probe: fb0={:p} fb1={:p} fb2={:p} stats={stats:?}",
            framebuffers[0],
            framebuffers[1],
            framebuffers[2]
        );
        Ok(DisplayProbe {
            panel,
            _io: io,
            touch,
            framebuffers: framebuffers.map(|framebuffer| framebuffer.cast()),
            next_framebuffer: 1,
        })
    }
}

impl DisplayProbe {
    fn render(
        &mut self,
        renderer: &mut Renderer,
        ui: &Ui,
        device: &DeviceState,
        chat: &ChatProjection,
        screen: &ScreenState,
    ) -> anyhow::Result<()> {
        let framebuffer = self.framebuffers[self.next_framebuffer];
        let pixels = unsafe {
            core::slice::from_raw_parts_mut(
                framebuffer,
                PANEL_WIDTH as usize * PANEL_HEIGHT as usize,
            )
        };
        let words = screen.draw_list(ui, device, chat);
        let mut software = SoftwareOnly;
        renderer
            .render(ui, &words, pixels, PANEL_WIDTH, PANEL_HEIGHT, &mut software)
            .ok_or_else(|| anyhow::anyhow!("PocketJS rejected the status framebuffer geometry"))?;

        esp_result("esp_lcd_panel_draw_bitmap", unsafe {
            esp_idf_svc::sys::esp_lcd_panel_draw_bitmap(
                self.panel,
                0,
                0,
                PANEL_WIDTH as i32,
                PANEL_HEIGHT as i32,
                framebuffer.cast(),
            )
        })?;
        self.next_framebuffer = (self.next_framebuffer + 1) % self.framebuffers.len();
        Ok(())
    }

    fn read_touch(&mut self) -> Option<(u16, u16)> {
        let mut x = 0u16;
        let mut y = 0u16;
        unsafe { esp_idf_svc::sys::pi_p4_touch_read(self.touch, &mut x, &mut y).then_some((x, y)) }
    }
}

struct SoftwareOnly;

impl PpaOps for SoftwareOnly {
    fn fill_rgb565(&mut self, _: &mut [u16], _: u32, _: u32, _: Rect, _: u16) -> bool {
        false
    }

    fn blend_a8_rgb565(
        &mut self,
        _: &mut [u16],
        _: u32,
        _: u32,
        _: &[u8],
        _: Rect,
        _: [u8; 3],
        _: u8,
    ) -> bool {
        false
    }

    fn srm_psm5650_to_rgb565(
        &mut self,
        _: &mut [u16],
        _: u32,
        _: u32,
        _: &[u8],
        _: u32,
        _: u32,
        _: Rect,
        _: Rect,
        _: SrmTransform,
    ) -> bool {
        false
    }
}

fn esp_result(operation: &str, code: esp_idf_svc::sys::esp_err_t) -> anyhow::Result<()> {
    if code == esp_idf_svc::sys::ESP_OK {
        Ok(())
    } else {
        anyhow::bail!("{operation} returned ESP-IDF error 0x{code:x}")
    }
}

#[derive(Debug)]
struct MemorySnapshot {
    free_heap: u32,
    psram_total: usize,
    psram_free: usize,
}

fn memory_snapshot() -> MemorySnapshot {
    unsafe {
        MemorySnapshot {
            free_heap: esp_idf_svc::sys::esp_get_free_heap_size(),
            psram_total: esp_idf_svc::sys::heap_caps_get_total_size(
                esp_idf_svc::sys::MALLOC_CAP_SPIRAM,
            ),
            psram_free: esp_idf_svc::sys::heap_caps_get_free_size(
                esp_idf_svc::sys::MALLOC_CAP_SPIRAM,
            ),
        }
    }
}

fn system_telemetry(ui_fps: u16) -> SystemTelemetry {
    let memory = memory_snapshot();
    let used = memory.psram_total.saturating_sub(memory.psram_free);
    let used_percent = if memory.psram_total == 0 {
        0
    } else {
        ((used.saturating_mul(100)) / memory.psram_total).min(100) as u8
    };
    SystemTelemetry {
        ram_used_percent: used_percent,
        ram_free_bytes: memory.free_heap as usize,
        cpu_percent: None,
        ui_fps,
    }
}
