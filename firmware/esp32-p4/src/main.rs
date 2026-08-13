use core::time::Duration;
use std::time::Instant;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::handle::RawHandle;
use esp_idf_svc::netif::{EspNetif, NetifStack};
use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition};
use esp_idf_svc::wifi::{AuthMethod, ClientConfiguration, Configuration, WifiDriver};
use pocket_pi_tools::PlatformTools;
use pocketjs_esp32p4_ppa::RenderTargetState;

mod agentos_main;
mod app_services;
mod backend;
mod device_state;
mod storage;
mod transport;

use device_state::{SettingsProjection, WifiNetworkProjection};

const BOARD_NAME: &str = "Waveshare ESP32-P4-WIFI6-Touch-LCD-5";
const PANEL_WIDTH: u32 = 720;
const PANEL_HEIGHT: u32 = 1280;
const WIFI_NVS_NAMESPACE: &str = "pocket_pi";
const WIFI_NVS_SSID_KEY: &str = "wifi_ssid";
const WIFI_NVS_PASSWORD_KEY: &str = "wifi_pass";
const AGENTOS_LAUNCHER_STACK_BYTES: u32 = 4 * 1024;
const AGENTOS_TASK_STACK_BYTES: u32 = 64 * 1024;

fn delay_current_task(delay: Duration) {
    let ticks = esp_idf_svc::hal::delay::TickType::from(delay)
        .ticks()
        .max(1);
    unsafe { esp_idf_svc::sys::vTaskDelay(ticks) };
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let mut task = core::ptr::null_mut();
    let result = unsafe {
        esp_idf_svc::sys::xTaskCreatePinnedToCoreWithCaps(
            Some(agentos_launcher_task),
            c"agentos-launch".as_ptr(),
            AGENTOS_LAUNCHER_STACK_BYTES,
            core::ptr::null_mut(),
            esp_idf_svc::sys::ESP_TASK_MAIN_PRIO,
            &mut task,
            0,
            esp_idf_svc::sys::MALLOC_CAP_SPIRAM | esp_idf_svc::sys::MALLOC_CAP_8BIT,
        )
    };
    if result != 1 || task.is_null() {
        anyhow::bail!("could not create AgentOS launcher task (result={result})")
    }
    log::info!("AgentOS runtime launcher queued");
    Ok(())
}

unsafe extern "C" fn agentos_launcher_task(_argument: *mut core::ffi::c_void) {
    // Let ESP-IDF delete its entry task first. That releases enough contiguous
    // internal RAM for the large runtime stack required by QuickJS. The tiny
    // launcher itself lives in PSRAM and never performs flash I/O.
    delay_current_task(Duration::from_millis(100));
    let mut task = core::ptr::null_mut();
    let result = esp_idf_svc::sys::xTaskCreatePinnedToCoreWithCaps(
        Some(agentos_task),
        c"agentos".as_ptr(),
        AGENTOS_TASK_STACK_BYTES,
        core::ptr::null_mut(),
        esp_idf_svc::sys::ESP_TASK_MAIN_PRIO,
        &mut task,
        0,
        esp_idf_svc::sys::MALLOC_CAP_INTERNAL | esp_idf_svc::sys::MALLOC_CAP_8BIT,
    );
    if result == 1 && !task.is_null() {
        log::info!(
            "AgentOS runtime started on a {} KiB internal stack",
            AGENTOS_TASK_STACK_BYTES / 1024
        );
    } else {
        log::error!("could not create AgentOS runtime task (result={result})");
    }
    loop {
        delay_current_task(Duration::from_secs(10));
    }
}

unsafe extern "C" fn agentos_task(_argument: *mut core::ffi::c_void) {
    if let Err(error) = agentos_main::run() {
        log::error!("AgentOS runtime stopped: {error:#}");
    }
    loop {
        delay_current_task(Duration::from_secs(10));
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
            "memoryTelemetry":"boot projection only"
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
                delay_current_task(Duration::from_millis(750));
                unsafe { esp_idf_svc::sys::esp_restart() }
            })
            .map_err(|error| format!("schedule reboot: {error}"))?;
        Ok(serde_json::json!({"status":"scheduled","delayMs":750}))
    }
}

fn init_wifi(
    provisioned_ssid: Option<&str>,
    provisioned_password: Option<&str>,
) -> anyhow::Result<WifiConnection> {
    let peripherals = Peripherals::take()?;
    let system_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let mut driver = WifiDriver::new(peripherals.modem, system_loop, Some(nvs.clone()))?;
    driver.set_configuration(&Configuration::Client(ClientConfiguration::default()))?;
    driver.start()?;
    driver.stop()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while driver.is_started()? {
        if Instant::now() >= deadline {
            anyhow::bail!("C6 Wi-Fi driver did not stop before lwIP attach")
        }
        delay_current_task(Duration::from_millis(25));
    }
    let sta_netif = EspNetif::new(NetifStack::Sta)?;
    esp_result("esp_netif_attach_wifi_station", unsafe {
        esp_idf_svc::sys::esp_netif_attach_wifi_station(sta_netif.handle())
    })?;
    esp_result("esp_wifi_set_default_wifi_sta_handlers", unsafe {
        esp_idf_svc::sys::esp_wifi_set_default_wifi_sta_handlers()
    })?;
    let mut wifi = WifiConnection {
        driver,
        sta_netif,
        nvs,
        pending: None,
    };
    match load_wifi_credentials(wifi.nvs.clone(), provisioned_ssid, provisioned_password) {
        Ok((ssid, password)) => {
            if let Err(error) = wifi.begin_connect(&ssid, &password, false) {
                log::warn!("saved Wi-Fi connection could not start: {error:#}");
            }
        }
        Err(error) => log::warn!("Wi-Fi is not configured: {error:#}"),
    }
    Ok(wifi)
}

struct WifiConnection {
    driver: WifiDriver<'static>,
    sta_netif: EspNetif,
    nvs: EspDefaultNvsPartition,
    pending: Option<PendingWifiConnect>,
}

struct PendingWifiConnect {
    ssid: String,
    password: String,
    started_at: Instant,
    persist_on_success: bool,
}

impl WifiConnection {
    fn scan(&mut self) -> anyhow::Result<Vec<WifiNetworkProjection>> {
        if !self.driver.is_started()? {
            self.driver.start()?;
        }
        let (access_points, _) = self.driver.scan_n::<16>()?;
        let mut networks = access_points
            .into_iter()
            .filter(|access_point| !access_point.ssid.is_empty())
            .map(|access_point| WifiNetworkProjection {
                ssid: access_point.ssid.as_str().to_owned(),
                rssi_dbm: access_point.signal_strength as i16,
                secured: access_point.auth_method != Some(AuthMethod::None),
            })
            .collect::<Vec<_>>();
        networks.sort_by_key(|network| core::cmp::Reverse(network.rssi_dbm));
        networks.dedup_by(|left, right| left.ssid == right.ssid);
        networks.truncate(5);
        Ok(networks)
    }

    fn begin_connect(
        &mut self,
        ssid: &str,
        password: &str,
        persist_on_success: bool,
    ) -> anyhow::Result<()> {
        validate_wifi_ssid(ssid)?;
        if !password.is_empty() {
            validate_wifi_password(password)?;
        }
        if self.driver.is_started()? {
            // A timed-out ESP-Hosted association may still be in the remote
            // driver's connecting state even though is_connected() is false.
            // Clear that state before every explicit attempt or retry.
            let _ = self.driver.disconnect();
            delay_current_task(Duration::from_millis(50));
        }
        self.driver
            .set_configuration(&Configuration::Client(ClientConfiguration {
                ssid: ssid.try_into()?,
                bssid: None,
                auth_method: if password.is_empty() {
                    AuthMethod::None
                } else {
                    AuthMethod::WPA2Personal
                },
                password: password.try_into()?,
                channel: None,
                ..Default::default()
            }))?;
        if !self.driver.is_started()? {
            self.driver.start()?;
        }
        esp_result("disable Wi-Fi modem power save", unsafe {
            esp_idf_svc::sys::esp_wifi_set_ps(esp_idf_svc::sys::wifi_ps_type_t_WIFI_PS_NONE)
        })?;
        self.driver.connect()?;
        self.pending = Some(PendingWifiConnect {
            ssid: ssid.to_owned(),
            password: password.to_owned(),
            started_at: Instant::now(),
            persist_on_success,
        });
        Ok(())
    }

    fn poll_connect(&mut self) -> Option<anyhow::Result<()>> {
        let pending = self.pending.as_ref()?;
        match (self.driver.is_connected(), self.sta_netif.is_up()) {
            (Ok(true), Ok(true)) => {
                let pending = self.pending.take().expect("pending Wi-Fi connect");
                if pending.persist_on_success {
                    let result = (|| {
                        let storage = EspDefaultNvs::new(
                            self.nvs.clone(),
                            WIFI_NVS_NAMESPACE,
                            true,
                        )?;
                        storage.set_str(WIFI_NVS_SSID_KEY, &pending.ssid)?;
                        storage.set_str(WIFI_NVS_PASSWORD_KEY, &pending.password)?;
                        Ok::<(), anyhow::Error>(())
                    })();
                    return Some(result);
                }
                Some(Ok(()))
            }
            (Err(error), _) => {
                self.pending = None;
                Some(Err(error.into()))
            }
            (_, Err(error)) => {
                self.pending = None;
                Some(Err(error.into()))
            }
            _ if pending.started_at.elapsed() >= Duration::from_secs(15) => {
                let _ = self.driver.disconnect();
                self.pending = None;
                Some(Err(anyhow::anyhow!("Wi-Fi association or DHCP timed out")))
            }
            _ => None,
        }
    }

    fn is_connecting(&self) -> bool {
        self.pending.is_some()
    }

    fn is_connected(&self) -> bool {
        self.driver.is_connected().unwrap_or(false) && self.sta_netif.is_up().unwrap_or(false)
    }

    fn forget(&mut self) -> anyhow::Result<()> {
        self.pending = None;
        if self.driver.is_connected()? {
            self.driver.disconnect()?;
        }
        let storage = EspDefaultNvs::new(self.nvs.clone(), WIFI_NVS_NAMESPACE, true)?;
        storage.remove(WIFI_NVS_SSID_KEY)?;
        storage.remove(WIFI_NVS_PASSWORD_KEY)?;
        Ok(())
    }

    fn projection(&self, status: impl Into<String>) -> SettingsProjection {
        let mut projection = SettingsProjection {
            firmware_version: env!("CARGO_PKG_VERSION").into(),
            workspace_free_bytes: storage::workspace_free_bytes().ok(),
            ..Default::default()
        };
        projection.wifi.status = status.into();
        let mut access_point = unsafe { core::mem::zeroed::<esp_idf_svc::sys::wifi_ap_record_t>() };
        if unsafe { esp_idf_svc::sys::esp_wifi_sta_get_ap_info(&mut access_point) }
            == esp_idf_svc::sys::ESP_OK
        {
            let end = access_point
                .ssid
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(access_point.ssid.len());
            projection.wifi.connected_ssid =
                Some(String::from_utf8_lossy(&access_point.ssid[..end]).into_owned());
            projection.wifi.rssi_dbm = Some(access_point.rssi as i16);
            projection.wifi.ip_address = self
                .sta_netif
                .get_ip_info()
                .ok()
                .map(|info| info.ip.to_string());
        }
        projection
    }
}

fn load_wifi_credentials(
    partition: EspDefaultNvsPartition,
    provisioned_ssid: Option<&str>,
    provisioned_password: Option<&str>,
) -> anyhow::Result<(String, String)> {
    let storage = EspDefaultNvs::new(partition, WIFI_NVS_NAMESPACE, true)?;
    if let (Some(ssid), Some(password)) = (provisioned_ssid, provisioned_password) {
        validate_wifi_ssid(ssid)?;
        validate_wifi_password(password)?;
        storage.set_str(WIFI_NVS_SSID_KEY, ssid)?;
        storage.set_str(WIFI_NVS_PASSWORD_KEY, password)?;
        return Ok((ssid.to_owned(), password.to_owned()));
    }
    if provisioned_ssid.is_some() || provisioned_password.is_some() {
        anyhow::bail!("Wi-Fi SSID and password must be supplied together")
    }
    let mut ssid_buf = [0u8; 33];
    let mut password_buf = [0u8; 64];
    let ssid = storage.get_str(WIFI_NVS_SSID_KEY, &mut ssid_buf)?;
    let password = storage.get_str(WIFI_NVS_PASSWORD_KEY, &mut password_buf)?;
    if let (Some(ssid), Some(password)) = (ssid, password) {
        validate_wifi_ssid(ssid)?;
        validate_wifi_password(password)?;
        return Ok((ssid.to_owned(), password.to_owned()));
    }
    anyhow::bail!("Wi-Fi credentials are not configured")
}

fn validate_wifi_ssid(ssid: &str) -> anyhow::Result<()> {
    if ssid.is_empty() || ssid.len() > 32 {
        anyhow::bail!("Wi-Fi SSID must contain 1 to 32 bytes")
    }
    Ok(())
}

fn validate_wifi_password(password: &str) -> anyhow::Result<()> {
    if password.is_empty() {
        return Ok(());
    }
    if !(8..=63).contains(&password.len()) {
        anyhow::bail!("Wi-Fi password must contain 8 to 63 bytes")
    }
    if !password.is_ascii() {
        anyhow::bail!("Wi-Fi password must be ASCII")
    }
    Ok(())
}

struct DisplayProbe {
    panel: esp_idf_svc::sys::esp_lcd_panel_handle_t,
    _io: esp_idf_svc::sys::esp_lcd_panel_io_handle_t,
    touch: esp_idf_svc::sys::esp_lcd_touch_handle_t,
    framebuffers: [*mut u16; 3],
    render_states: [RenderTargetState; 3],
    next_framebuffer: usize,
}

impl DisplayProbe {
    fn read_touch(&mut self) -> Option<(u16, u16)> {
        let mut x = 0u16;
        let mut y = 0u16;
        unsafe { esp_idf_svc::sys::pi_p4_touch_read(self.touch, &mut x, &mut y).then_some((x, y)) }
    }
}

fn esp_result(operation: &str, code: esp_idf_svc::sys::esp_err_t) -> anyhow::Result<()> {
    if code == esp_idf_svc::sys::ESP_OK {
        Ok(())
    } else {
        anyhow::bail!("{operation} returned ESP-IDF error 0x{code:x}")
    }
}
