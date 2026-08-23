use core::time::Duration;
use std::sync::Arc;

use esp_idf_svc::nvs::EspDefaultNvsPartition;
use pocket_pi_agentos::{AppSupervisor, Viewport};
use pocket_pi_tools::PlatformTools;

mod agentos_main;
mod app_services;
mod backend;
pub mod device_state;
pub mod storage;
mod transport;
mod wifi;

pub use wifi::WifiConnection;

pub const DEVICE_NVS_NAMESPACE: &str = "pocket_pi";

pub trait DisplayHost {
    fn read_touch(&mut self) -> Option<(u16, u16)>;
    fn render(&mut self, supervisor: &AppSupervisor) -> anyhow::Result<()>;
}

pub trait DeviceHost {
    const BOARD_ID: &'static str;
    const BOARD_NAME: &'static str;
    const FIRMWARE_VERSION: &'static str;
    const VIEWPORT: Viewport;
    const SHOW_MODEL_PROGRESS: bool;
    const MODEL_WORKER_CORE: i32;

    fn init_wifi(
        nvs: EspDefaultNvsPartition,
        provisioned_ssid: Option<&str>,
        provisioned_password: Option<&str>,
    ) -> anyhow::Result<WifiConnection>;

    fn init_display(supervisor: &AppSupervisor) -> anyhow::Result<Box<dyn DisplayHost>>;
}

pub fn run<H: DeviceHost>() -> anyhow::Result<()> {
    agentos_main::run::<H>()
}

pub fn delay_current_task(delay: Duration) {
    let ticks = esp_idf_svc::hal::delay::TickType::from(delay)
        .ticks()
        .max(1);
    unsafe { esp_idf_svc::sys::vTaskDelay(ticks) };
}

pub struct EspPlatform {
    board_id: &'static str,
}

impl EspPlatform {
    fn new(board_id: &'static str) -> Arc<Self> {
        Arc::new(Self { board_id })
    }
}

impl PlatformTools for EspPlatform {
    fn device_status(&self) -> serde_json::Value {
        serde_json::json!({
            "status":"ok",
            "board":self.board_id,
            "piHarness":"pi-agent-core",
            "jsRuntime":"QuickJS via PocketJS host",
            "memoryTelemetry":"boot facts only"
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
        serde_json::json!({
            "status":"connected",
            "ssid":String::from_utf8_lossy(&access_point.ssid[..ssid_end]),
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

pub fn esp_result(operation: &str, code: esp_idf_svc::sys::esp_err_t) -> anyhow::Result<()> {
    if code == esp_idf_svc::sys::ESP_OK {
        Ok(())
    } else {
        anyhow::bail!("{operation} returned ESP-IDF error 0x{code:x}")
    }
}
