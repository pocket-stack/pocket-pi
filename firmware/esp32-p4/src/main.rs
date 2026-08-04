use core::time::Duration;
use std::io::Read;

use embedded_svc::http::{client::Client as HttpClient, Method};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::handle::RawHandle;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use esp_idf_svc::netif::{EspNetif, NetifStack};
use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition};
use esp_idf_svc::wifi::{AuthMethod, ClientConfiguration, Configuration, WifiDriver};
use pocket_pi_embedded_core::{AgentState, DeviceState, LinkState, SecretKind, SecretStorePolicy};
use pocketjs_core::{spec, Ui};
use pocketjs_esp32p4_ppa::{PpaOps, Rect, Renderer, RendererConfig, SrmTransform};

const BOARD_NAME: &str = "Waveshare ESP32-P4-WIFI6-Touch-LCD-5";
const PANEL_WIDTH: f32 = 720.0;
const PANEL_HEIGHT: f32 = 1280.0;
const WIFI_SSID_PRIMARY: &str = "<Y/OUR SPACE>_5g";
const WIFI_SSID_FALLBACK: &str = "<Y/OUR SPACE>_2.4G";
const WIFI_NVS_NAMESPACE: &str = "pocket_pi";
const WIFI_NVS_PASSWORD_KEY: &str = "wifi_pass";
const WIFI_PROVISION_PREFIX: &str = "PPI-WIFI-PASS:";

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let mut state = DeviceState::default();
    state.begin_boot();

    // Construct both objects here so the real PocketJS P4 renderer remains in
    // the cross-compiled firmware rather than a mock UI path. The panel BSP
    // will attach this logical viewport to the MIPI-DSI framebuffer next.
    let mut ui = Ui::new();
    ui.set_viewport(PANEL_WIDTH, PANEL_HEIGHT);
    let mut renderer = Renderer::new(RendererConfig::default())
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
    log::info!("device state: {state:?}");
    log::info!(
        "PocketJS P4 renderer ready: viewport={:?} scale={}",
        ui.viewport(),
        renderer.config().scale,
    );

    // Keep the handles alive for the process lifetime. This first display
    // milestone uses the DSI engine's vertical color-bar pattern so panel
    // timing can be verified independently of the PocketJS framebuffer path.
    let mut display = match init_display_probe(&mut renderer, &ui, &state) {
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
            state.set_network_online();
            Some(wifi)
        }
        Err(error) => {
            log::error!("C6-SDIO Wi-Fi radio probe failed: {error:#}");
            None
        }
    };
    log::info!(
        "network targets staged: codex_api={} codex_plan={} robinhood_mcp={}",
        pocket_pi_embedded_net::codex::OPENAI_API_BASE_URL,
        pocket_pi_embedded_net::codex::CODEX_BACKEND_BASE_URL,
        pocket_pi_embedded_net::robinhood::MCP_URL,
    );
    let secret_policy = SecretStorePolicy::DEVELOPMENT;
    log::warn!(
        "long-lived token persistence enabled={} (HMAC NVS activation requires explicit approval)",
        secret_policy.may_persist(SecretKind::CodexRefreshToken),
    );
    if _wifi.is_some() {
        match probe_https_origins() {
            Ok(reachability) => {
                state.codex = match (reachability.codex_backend, reachability.openai_api) {
                    (true, _) => LinkState::Online,
                    (false, true) => LinkState::Degraded,
                    (false, false) => LinkState::Offline,
                };
                state.robinhood = if reachability.robinhood {
                    LinkState::Online
                } else {
                    LinkState::Offline
                };
                state.agent = if reachability.codex_backend || reachability.openai_api {
                    AgentState::WaitingForAuth
                } else {
                    AgentState::NetworkBlocked
                };
                log::info!("HTTPS connectivity probe completed: {reachability:?}");
            }
            Err(error) => log::error!("HTTPS connectivity probe failed: {error:#}"),
        }
    }
    if let Some(display) = display.as_mut() {
        if let Err(error) = display.render_state(&mut renderer, &ui, &state) {
            log::error!("PocketJS status projection failed: {error:#}");
        }
    }

    loop {
        log::info!(
            "heartbeat heap={} psram_free={} agent={:?}",
            unsafe { esp_idf_svc::sys::esp_get_free_heap_size() },
            unsafe {
                esp_idf_svc::sys::heap_caps_get_free_size(esp_idf_svc::sys::MALLOC_CAP_SPIRAM)
            },
            state.agent,
        );
        std::thread::sleep(Duration::from_secs(5));
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct HttpsReachability {
    openai_api: bool,
    codex_backend: bool,
    robinhood: bool,
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
        (
            "robinhood-mcp",
            pocket_pi_embedded_net::robinhood::MCP_URL,
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
                        "robinhood-mcp" => reachability.robinhood = true,
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
    log::info!("target HTTPS reachability: {target_ok}/3");
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
    _framebuffer: *mut u16,
}

fn init_display_probe(
    renderer: &mut Renderer,
    ui: &Ui,
    state: &DeviceState,
) -> anyhow::Result<DisplayProbe> {
    unsafe {
        let mut panel = core::ptr::null_mut();
        let mut io = core::ptr::null_mut();
        let mut framebuffer = core::ptr::null_mut();

        esp_result(
            "bsp_display_new",
            esp_idf_svc::sys::bsp_display_new(core::ptr::null(), &mut panel, &mut io),
        )?;
        esp_result(
            "esp_lcd_dpi_panel_get_frame_buffer",
            esp_idf_svc::sys::esp_lcd_dpi_panel_get_frame_buffer(panel, 1, &mut framebuffer),
        )?;
        if framebuffer.is_null() {
            anyhow::bail!("esp_lcd_dpi_panel_get_frame_buffer returned a null buffer")
        }

        let pixels = core::slice::from_raw_parts_mut(
            framebuffer.cast::<u16>(),
            PANEL_WIDTH as usize * PANEL_HEIGHT as usize,
        );
        let mut software = SoftwareOnly;
        let words = dashboard_draw_list(state);
        let stats = renderer
            .render(
                ui,
                &words,
                pixels,
                PANEL_WIDTH as u32,
                PANEL_HEIGHT as u32,
                &mut software,
            )
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
                framebuffer,
            ),
        )?;
        esp_result(
            "bsp_display_backlight_on",
            esp_idf_svc::sys::bsp_display_backlight_on(),
        )?;

        log::info!("PocketJS framebuffer probe: address={framebuffer:p} stats={stats:?}");

        Ok(DisplayProbe {
            _panel: panel,
            _io: io,
            _framebuffer: framebuffer.cast(),
        })
    }
}

impl DisplayProbe {
    fn render_state(
        &mut self,
        renderer: &mut Renderer,
        ui: &Ui,
        state: &DeviceState,
    ) -> anyhow::Result<()> {
        let pixels = unsafe {
            core::slice::from_raw_parts_mut(
                self._framebuffer,
                PANEL_WIDTH as usize * PANEL_HEIGHT as usize,
            )
        };
        let mut software = SoftwareOnly;
        let words = dashboard_draw_list(state);
        renderer
            .render(
                ui,
                &words,
                pixels,
                PANEL_WIDTH as u32,
                PANEL_HEIGHT as u32,
                &mut software,
            )
            .ok_or_else(|| anyhow::anyhow!("PocketJS rejected the status framebuffer geometry"))?;

        esp_result("esp_lcd_panel_draw_bitmap", unsafe {
            esp_idf_svc::sys::esp_lcd_panel_draw_bitmap(
                self._panel,
                0,
                0,
                PANEL_WIDTH as i32,
                PANEL_HEIGHT as i32,
                self._framebuffer.cast(),
            )
        })
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

fn dashboard_draw_list(state: &DeviceState) -> Vec<u32> {
    let mut words = Vec::new();
    let mut rect = |x, y, width, height, color| {
        words.extend_from_slice(&[spec::draw_op::RECT, xy(x, y), wh(width, height), color]);
    };

    // A credential-free dashboard skeleton. It proves the exact PocketJS
    // DrawList -> RGB565 -> native DSI framebuffer route before QuickJS and
    // the compiled JSX dashboard are loaded.
    rect(0, 0, 720, 1280, 0xfff1_f5f9);
    rect(0, 0, 720, 128, 0xff0f_172a);
    rect(36, 42, 36, 36, agent_state_color(state.agent));
    rect(532, 48, 28, 28, link_state_color(state.wifi));
    rect(584, 48, 28, 28, link_state_color(state.codex));
    rect(636, 48, 28, 28, link_state_color(state.robinhood));
    rect(32, 164, 656, 264, 0xffff_ffff);
    rect(56, 196, 12, 196, 0xff10_b981);
    rect(96, 212, 540, 44, 0xffcbd_5e1);
    rect(96, 280, 408, 72, 0xff0f_172a);
    rect(32, 460, 656, 680, 0xffff_ffff);
    for row in 0..4 {
        let y = 504 + row * 144;
        rect(56, y, 128, 72, 0xffe2_e8f0);
        rect(220, y, 248, 48, 0xff94_a3b8);
        rect(
            532,
            y,
            116,
            48,
            if row == 2 { 0xffef_4444 } else { 0xff10_b981 },
        );
    }
    rect(32, 1172, 656, 76, 0xffe2_e8f0);
    words
}

const fn link_state_color(state: LinkState) -> u32 {
    match state {
        LinkState::Disabled => 0xff64_748b,
        LinkState::Connecting => 0xfff5_9e0b,
        LinkState::Online => 0xff10_b981,
        LinkState::Degraded => 0xfff9_7316,
        LinkState::Offline => 0xffef_4444,
    }
}

const fn agent_state_color(state: AgentState) -> u32 {
    match state {
        AgentState::Stopped => 0xff64_748b,
        AgentState::Starting | AgentState::WaitingForAuth => 0xfff5_9e0b,
        AgentState::Idle | AgentState::Thinking | AgentState::Acting => 0xff10_b981,
        AgentState::NetworkBlocked | AgentState::Faulted => 0xffef_4444,
    }
}

const fn xy(x: i16, y: i16) -> u32 {
    x as u16 as u32 | ((y as u16 as u32) << 16)
}

const fn wh(width: u16, height: u16) -> u32 {
    width as u32 | ((height as u32) << 16)
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
