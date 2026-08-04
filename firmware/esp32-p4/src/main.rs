use core::time::Duration;

use pocket_pi_embedded_core::DeviceState;
use pocketjs_core::Ui;
use pocketjs_esp32p4_ppa::{Renderer, RendererConfig};

const BOARD_NAME: &str = "Waveshare ESP32-P4-WIFI6-Touch-LCD-5";
const PANEL_WIDTH: f32 = 720.0;
const PANEL_HEIGHT: f32 = 1280.0;

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
    let renderer = Renderer::new(RendererConfig::default())
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
    let _display = match init_display_probe() {
        Ok(display) => {
            log::info!("MIPI-DSI panel probe active");
            Some(display)
        }
        Err(error) => {
            log::error!("MIPI-DSI panel probe failed: {error:#}");
            None
        }
    };
    log::info!(
        "network targets staged: codex_api={} codex_plan={} robinhood_mcp={}",
        pocket_pi_embedded_net::codex::OPENAI_API_BASE_URL,
        pocket_pi_embedded_net::codex::CODEX_BACKEND_BASE_URL,
        pocket_pi_embedded_net::robinhood::MCP_URL,
    );
    log::warn!("board profile selected; C6-SDIO Wi-Fi is not initialized yet");

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

#[derive(Debug)]
struct DisplayProbe {
    _panel: esp_idf_svc::sys::esp_lcd_panel_handle_t,
    _io: esp_idf_svc::sys::esp_lcd_panel_io_handle_t,
}

fn init_display_probe() -> anyhow::Result<DisplayProbe> {
    unsafe {
        let mut panel = core::ptr::null_mut();
        let mut io = core::ptr::null_mut();

        esp_result(
            "bsp_display_new",
            esp_idf_svc::sys::bsp_display_new(core::ptr::null(), &mut panel, &mut io),
        )?;
        esp_result(
            "esp_lcd_panel_disp_on_off",
            esp_idf_svc::sys::esp_lcd_panel_disp_on_off(panel, true),
        )?;
        esp_result(
            "esp_lcd_dpi_panel_set_pattern",
            esp_idf_svc::sys::esp_lcd_dpi_panel_set_pattern(
                panel,
                esp_idf_svc::sys::mipi_dsi_pattern_type_t_MIPI_DSI_PATTERN_BAR_VERTICAL,
            ),
        )?;
        esp_result(
            "bsp_display_backlight_on",
            esp_idf_svc::sys::bsp_display_backlight_on(),
        )?;

        Ok(DisplayProbe {
            _panel: panel,
            _io: io,
        })
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
