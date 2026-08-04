use core::time::Duration;

use pocket_pi_embedded_core::DeviceState;
use pocketjs_core::{spec, Ui};
use pocketjs_esp32p4_ppa::{PpaOps, Rect, Renderer, RendererConfig, SrmTransform};

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
    let _display = match init_display_probe(&mut renderer, &ui) {
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
    _framebuffer: *mut u16,
}

fn init_display_probe(renderer: &mut Renderer, ui: &Ui) -> anyhow::Result<DisplayProbe> {
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
        let words = boot_draw_list();
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

fn boot_draw_list() -> Vec<u32> {
    let mut words = Vec::new();
    let mut rect = |x, y, width, height, color| {
        words.extend_from_slice(&[spec::draw_op::RECT, xy(x, y), wh(width, height), color]);
    };

    // A credential-free dashboard skeleton. It proves the exact PocketJS
    // DrawList -> RGB565 -> native DSI framebuffer route before QuickJS and
    // the compiled JSX dashboard are loaded.
    rect(0, 0, 720, 1280, 0xfff1_f5f9);
    rect(0, 0, 720, 128, 0xff0f_172a);
    rect(36, 42, 36, 36, 0xff10_b981);
    rect(532, 48, 28, 28, 0xff10_b981);
    rect(584, 48, 28, 28, 0xfff5_9e0b);
    rect(636, 48, 28, 28, 0xffef_4444);
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
