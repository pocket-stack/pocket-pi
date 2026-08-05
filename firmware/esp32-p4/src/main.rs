use core::time::Duration;
use std::io::Read;
use std::sync::{mpsc, Arc};

use embedded_svc::http::{client::Client as HttpClient, Method};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::handle::RawHandle;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use esp_idf_svc::netif::{EspNetif, NetifStack};
use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition};
use esp_idf_svc::wifi::{AuthMethod, ClientConfiguration, Configuration, WifiDriver};
use pocket_mod::Guest;
use pocket_pi_app_core::{
    decode_command, encode_snapshot, AppCommand, AppSnapshot, FileEntry, OpenFile, Role,
    SystemState, Turn,
};
use pocket_pi_embedded::{ModelBackend, PiEmbedded, ToolHost, ToolResult};
use pocket_ui_surface::UiSurface;
use pocketjs_esp32p4_ppa::{PpaOps, Rect, Renderer, RendererConfig, SrmTransform};

const BOARD_NAME: &str = "Waveshare ESP32-P4-WIFI6-Touch-LCD-5";
const PANEL_WIDTH: u32 = 720;
const PANEL_HEIGHT: u32 = 1280;
const LOGICAL_WIDTH: f32 = 360.0;
const LOGICAL_HEIGHT: f32 = 640.0;
const UI_JS: &str = include_str!("../../../artifacts/ui/agent-shell.js");
const UI_PAK: &[u8] = include_bytes!("../../../artifacts/ui/agent-shell.pak");
const WIFI_SSID_PRIMARY: &str = "<Y/OUR SPACE>_5g";
const WIFI_SSID_FALLBACK: &str = "<Y/OUR SPACE>_2.4G";
const WIFI_NVS_NAMESPACE: &str = "pocket_pi";
const WIFI_NVS_PASSWORD_KEY: &str = "wifi_pass";
const WIFI_PROVISION_PREFIX: &str = "PPI-WIFI-PASS:";
const ANALOG_CENTER: u32 = 0x8080;

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let surface = UiSurface::new_with_density((LOGICAL_WIDTH, LOGICAL_HEIGHT), 2);
    surface.set_svc_allowlist(["pocket-pi"]);
    surface.feed_pak(UI_PAK);
    let guest = Guest::new()?;
    surface.mount(&guest)?;
    guest.eval("agent-shell", UI_JS)?;

    let mut state = AppSnapshot {
        system: SystemState {
            backend: "EMBEDDED".into(),
            network: "CONNECTING".into(),
            free_ram_kib: memory_snapshot().free_heap / 1024,
            fps: 0,
        },
        ..AppSnapshot::default()
    };
    state.workspace.path = "/workspace".into();
    state.workspace.entries = vec![
        FileEntry {
            name: "memory.md".into(),
            size: 69,
            modified_unix_seconds: 0,
        },
        FileEntry {
            name: "notes.txt".into(),
            size: 48,
            modified_unix_seconds: 0,
        },
    ];
    state.chat.turns.push(Turn {
        id: 1,
        role: Role::Assistant,
        text: "Pocket Pi ESP32-P4 host is ready.".into(),
        streaming: false,
    });
    surface.svc_push(encode_snapshot(&state)?);
    guest.frame(0)?;
    surface.tick();

    let (delta_tx, delta_rx) = mpsc::channel();
    let agent = PiEmbedded::new(
        r#"{"provider":"offline","model":"esp32-p4","systemPrompt":"You are Pocket Pi."}"#,
        Arc::new(OfflineModel),
        Arc::new(NoTools),
        Arc::new(move |delta| {
            delta_tx.send(delta).ok();
        }),
    )
    .map_err(anyhow::Error::msg)?;

    let mut renderer = Renderer::new(RendererConfig {
        scale: 2,
        ..RendererConfig::default()
    })
    .ok_or_else(|| anyhow::anyhow!("invalid PocketJS renderer configuration"))?;

    let memory = memory_snapshot();
    log::info!("Pocket Pi ESP32-P4 hardware probe: {BOARD_NAME}");
    log::info!(
        "heap: free={} min_free={} internal_free={} psram_total={} psram_free={}",
        memory.free_heap,
        memory.minimum_free_heap,
        memory.internal_free,
        memory.psram_total,
        memory.psram_free,
    );
    log::info!(
        "PocketJS agent shell ready: logical={}x{} scale={}",
        LOGICAL_WIDTH,
        LOGICAL_HEIGHT,
        renderer.config().scale,
    );

    // Keep the handles alive for the process lifetime. This first display
    // milestone uses the DSI engine's vertical color-bar pattern so panel
    // timing can be verified independently of the PocketJS framebuffer path.
    let mut display = match init_display_probe(&mut renderer, &surface) {
        Ok(display) => {
            log::info!("MIPI-DSI panel probe active");
            Some(display)
        }
        Err(error) => {
            log::error!("MIPI-DSI panel probe failed: {error:#}");
            None
        }
    };

    let _wifi = match init_wifi() {
        Ok(wifi) => {
            log::info!("C6-SDIO Wi-Fi and lwIP netif active");
            state.system.network = "ONLINE".into();
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
    if _wifi.is_some() {
        match probe_https_origins() {
            Ok(reachability) => {
                state.system.backend = if reachability.codex_backend {
                    "CODEX PLAN"
                } else if reachability.openai_api {
                    "OPENAI API"
                } else {
                    "OFFLINE"
                }
                .into();
                log::info!("HTTPS connectivity probe completed: {reachability:?}");
            }
            Err(error) => log::error!("HTTPS connectivity probe failed: {error:#}"),
        }
    }
    state.changed();
    surface.svc_push(encode_snapshot(&state)?);
    guest.frame(0)?;
    surface.tick();
    if let Some(display) = display.as_mut() {
        if let Err(error) = display.render(&mut renderer, &surface) {
            log::error!("PocketJS status projection failed: {error:#}");
        }
    }

    let mut touch_down = false;
    let mut last_heartbeat = std::time::Instant::now();
    loop {
        let touches = display
            .as_mut()
            .and_then(DisplayProbe::read_touch)
            .map(|(x, y)| vec![pack_touch(x as u32 / 2, y as u32 / 2)])
            .unwrap_or_default();
        let is_touch_down = !touches.is_empty();
        guest.frame_with_touches(0, ANALOG_CENTER, &touches)?;
        surface.tick();

        let mut redraw = touch_down != is_touch_down;
        touch_down = is_touch_down;
        for line in surface.svc_drain() {
            match decode_command(&line) {
                Ok(command) => {
                    handle_command(command, &mut state, &agent, &delta_rx)?;
                    redraw = true;
                }
                Err(error) => log::warn!("invalid UI command: {error}"),
            }
        }
        if redraw {
            state.system.free_ram_kib = memory_snapshot().free_heap / 1024;
            state.changed();
            surface.svc_push(encode_snapshot(&state)?);
            guest.frame(0)?;
            surface.tick();
            if let Some(display) = display.as_mut() {
                display.render(&mut renderer, &surface)?;
            }
        }
        if last_heartbeat.elapsed() >= Duration::from_secs(5) {
            log::info!(
                "heartbeat heap={} psram_free={}",
                unsafe { esp_idf_svc::sys::esp_get_free_heap_size() },
                unsafe {
                    esp_idf_svc::sys::heap_caps_get_free_size(esp_idf_svc::sys::MALLOC_CAP_SPIRAM)
                },
            );
            last_heartbeat = std::time::Instant::now();
        }
        std::thread::sleep(Duration::from_millis(30));
    }
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
        on_delta(text);
        Ok(r#"{"text":"Embedded Pi is running. Configure a model adapter to replace this offline reply."}"#.into())
    }
}

struct NoTools;

impl ToolHost for NoTools {
    fn execute(&self, _call_id: &str, name: &str, _args_json: &str) -> ToolResult {
        ToolResult {
            text: format!("tool not configured: {name}"),
            is_error: true,
            terminate: false,
        }
    }
}

fn handle_command(
    command: AppCommand,
    state: &mut AppSnapshot,
    agent: &PiEmbedded,
    delta_rx: &mpsc::Receiver<String>,
) -> anyhow::Result<()> {
    match command {
        AppCommand::SwitchView { view } => state.active_view = view,
        AppCommand::SendPrompt { text } if !text.trim().is_empty() => {
            let next_id = state.chat.turns.last().map(|turn| turn.id + 1).unwrap_or(1);
            state.chat.busy = true;
            state.chat.turns.push(Turn {
                id: next_id,
                role: Role::User,
                text: text.clone(),
                streaming: false,
            });
            agent.prompt(&text).map_err(anyhow::Error::msg)?;
            agent.pump().map_err(anyhow::Error::msg)?;
            let reply = delta_rx.try_iter().collect::<String>();
            state.chat.turns.push(Turn {
                id: next_id + 1,
                role: Role::Assistant,
                text: reply,
                streaming: false,
            });
            state.chat.busy = false;
            if state.chat.turns.len() > 10 {
                state.chat.turns.drain(0..state.chat.turns.len() - 10);
            }
        }
        AppCommand::OpenPath { name } => {
            let content = match name.as_str() {
                "memory.md" => {
                    Some("# Pocket Pi memory\n\nThis workspace belongs to the embedded agent.")
                }
                "notes.txt" => Some("Chat and workspace use one shared PocketJS UI."),
                _ => None,
            };
            if let Some(content) = content {
                state.workspace.open_file = Some(OpenFile {
                    name,
                    content: content.into(),
                });
            }
        }
        AppCommand::CloseFile => state.workspace.open_file = None,
        AppCommand::SendPrompt { .. } => {}
    }
    Ok(())
}

fn pack_touch(x: u32, y: u32) -> u32 {
    0x8000_0000 | ((y & 0x3ff) << 10) | (x & 0x3ff)
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
    _panel: esp_idf_svc::sys::esp_lcd_panel_handle_t,
    _io: esp_idf_svc::sys::esp_lcd_panel_io_handle_t,
    _touch: esp_idf_svc::sys::esp_lcd_touch_handle_t,
    _framebuffers: [*mut u16; 3],
    next_framebuffer: usize,
}

fn init_display_probe(
    renderer: &mut Renderer,
    surface: &UiSurface,
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
        let mut software = SoftwareOnly;
        let stats = surface
            .with_ui(|ui| {
                let words = ui.draw().words.clone();
                renderer.render(ui, &words, pixels, PANEL_WIDTH, PANEL_HEIGHT, &mut software)
            })
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
            _panel: panel,
            _io: io,
            _touch: touch,
            _framebuffers: framebuffers.map(|framebuffer| framebuffer.cast()),
            next_framebuffer: 1,
        })
    }
}

impl DisplayProbe {
    fn render(&mut self, renderer: &mut Renderer, surface: &UiSurface) -> anyhow::Result<()> {
        let framebuffer = self._framebuffers[self.next_framebuffer];
        let pixels = unsafe {
            core::slice::from_raw_parts_mut(
                framebuffer,
                PANEL_WIDTH as usize * PANEL_HEIGHT as usize,
            )
        };
        let mut software = SoftwareOnly;
        surface
            .with_ui(|ui| {
                let words = ui.draw().words.clone();
                renderer.render(ui, &words, pixels, PANEL_WIDTH, PANEL_HEIGHT, &mut software)
            })
            .ok_or_else(|| anyhow::anyhow!("PocketJS rejected the status framebuffer geometry"))?;

        esp_result("esp_lcd_panel_draw_bitmap", unsafe {
            esp_idf_svc::sys::esp_lcd_panel_draw_bitmap(
                self._panel,
                0,
                0,
                PANEL_WIDTH as i32,
                PANEL_HEIGHT as i32,
                framebuffer.cast(),
            )
        })?;
        self.next_framebuffer = (self.next_framebuffer + 1) % self._framebuffers.len();
        Ok(())
    }

    fn read_touch(&mut self) -> Option<(u16, u16)> {
        let mut x = 0u16;
        let mut y = 0u16;
        unsafe { esp_idf_svc::sys::pi_p4_touch_read(self._touch, &mut x, &mut y).then_some((x, y)) }
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
    minimum_free_heap: u32,
    internal_free: usize,
    psram_total: usize,
    psram_free: usize,
}

fn memory_snapshot() -> MemorySnapshot {
    unsafe {
        MemorySnapshot {
            free_heap: esp_idf_svc::sys::esp_get_free_heap_size(),
            minimum_free_heap: esp_idf_svc::sys::esp_get_minimum_free_heap_size(),
            internal_free: esp_idf_svc::sys::heap_caps_get_free_size(
                esp_idf_svc::sys::MALLOC_CAP_INTERNAL,
            ),
            psram_total: esp_idf_svc::sys::heap_caps_get_total_size(
                esp_idf_svc::sys::MALLOC_CAP_SPIRAM,
            ),
            psram_free: esp_idf_svc::sys::heap_caps_get_free_size(
                esp_idf_svc::sys::MALLOC_CAP_SPIRAM,
            ),
        }
    }
}
