use core::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::WifiDriver;
use pocket_pi_agentos::{AppSupervisor, Viewport};
use pocket_pi_esp32_common::{
    delay_current_task, esp_result, DeviceHost, DisplayHost, WifiConnection,
};
use pocketjs_esp32p4_ppa::{EspIdfPpaOps, RenderTargetState, Renderer};

const DISPLAY_VIEWPORT: Viewport = Viewport::new(720, 1280);
const AGENTOS_LAUNCHER_STACK_BYTES: u32 = 4 * 1024;
const AGENTOS_TASK_STACK_BYTES: u32 = 64 * 1024;

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
    if let Err(error) = pocket_pi_esp32_common::run::<P4Host>() {
        log::error!("AgentOS runtime stopped: {error:#}");
    }
    loop {
        delay_current_task(Duration::from_secs(10));
    }
}

struct P4Host;

impl DeviceHost for P4Host {
    const BOARD_ID: &'static str = "esp32-p4";
    const BOARD_NAME: &'static str = "Waveshare ESP32-P4-WIFI6-Touch-LCD-5";
    const FIRMWARE_VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const VIEWPORT: Viewport = DISPLAY_VIEWPORT;
    const SHOW_MODEL_PROGRESS: bool = true;
    const MODEL_WORKER_CORE: i32 = 1;

    fn init_wifi(
        nvs: EspDefaultNvsPartition,
        provisioned_ssid: Option<&str>,
        provisioned_password: Option<&str>,
    ) -> anyhow::Result<WifiConnection> {
        let peripherals = Peripherals::take()?;
        let system_loop = EspSystemEventLoop::take()?;
        let driver = WifiDriver::new(peripherals.modem, system_loop, Some(nvs.clone()))?;
        WifiConnection::attach(
            driver,
            nvs,
            provisioned_ssid,
            provisioned_password,
            Self::FIRMWARE_VERSION,
        )
    }

    fn init_display(supervisor: &AppSupervisor) -> anyhow::Result<Box<dyn DisplayHost>> {
        Ok(Box::new(P4Display::new(supervisor)?))
    }
}

struct P4Display {
    renderer: Renderer,
    ppa: EspIdfPpaOps,
    panel: esp_idf_svc::sys::esp_lcd_panel_handle_t,
    _io: esp_idf_svc::sys::esp_lcd_panel_io_handle_t,
    touch: esp_idf_svc::sys::esp_lcd_touch_handle_t,
    framebuffers: [*mut u16; 3],
    render_states: [RenderTargetState; 3],
    next_framebuffer: usize,
}

impl P4Display {
    fn new(supervisor: &AppSupervisor) -> anyhow::Result<Self> {
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

            let mut renderer = Renderer::new(Default::default())
                .ok_or_else(|| anyhow::anyhow!("invalid PocketJS renderer configuration"))?;
            let mut ppa = EspIdfPpaOps::new().map_err(|error| {
                anyhow::anyhow!("initialize PocketJS PPA: ESP-IDF error 0x{error:x}")
            })?;
            let mut render_states = [
                RenderTargetState::new(),
                RenderTargetState::new(),
                RenderTargetState::new(),
            ];
            let pixels = core::slice::from_raw_parts_mut(
                framebuffers[0].cast::<u16>(),
                DISPLAY_VIEWPORT.width as usize * DISPLAY_VIEWPORT.height as usize,
            );
            render_ui(
                supervisor,
                &mut renderer,
                &mut ppa,
                &mut render_states[0],
                pixels,
            )?;

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
            draw_frame(panel, framebuffers[0])?;
            esp_result(
                "bsp_display_backlight_on",
                esp_idf_svc::sys::bsp_display_backlight_on(),
            )?;
            esp_result(
                "pi_p4_touch_new",
                esp_idf_svc::sys::pi_p4_touch_new(&mut touch),
            )?;

            Ok(Self {
                renderer,
                ppa,
                panel,
                _io: io,
                touch,
                framebuffers: framebuffers.map(|framebuffer| framebuffer.cast()),
                render_states,
                next_framebuffer: 1,
            })
        }
    }
}

impl DisplayHost for P4Display {
    fn read_touch(&mut self) -> Option<(u16, u16)> {
        let mut x = 0u16;
        let mut y = 0u16;
        unsafe { esp_idf_svc::sys::pi_p4_touch_read(self.touch, &mut x, &mut y).then_some((x, y)) }
    }

    fn render(&mut self, supervisor: &AppSupervisor) -> anyhow::Result<()> {
        let index = self.next_framebuffer;
        let pixels = unsafe {
            core::slice::from_raw_parts_mut(
                self.framebuffers[index],
                DISPLAY_VIEWPORT.width as usize * DISPLAY_VIEWPORT.height as usize,
            )
        };
        render_ui(
            supervisor,
            &mut self.renderer,
            &mut self.ppa,
            &mut self.render_states[index],
            pixels,
        )?;
        draw_frame(self.panel, self.framebuffers[index].cast())?;
        self.next_framebuffer = (index + 1) % self.framebuffers.len();
        Ok(())
    }
}

fn render_ui(
    supervisor: &AppSupervisor,
    renderer: &mut Renderer,
    ppa: &mut EspIdfPpaOps,
    state: &mut RenderTargetState,
    pixels: &mut [u16],
) -> anyhow::Result<()> {
    supervisor.with_ui(|ui| {
        let words = ui.draw().words.clone();
        renderer
            .render_incremental(
                state,
                ui,
                &words,
                pixels,
                DISPLAY_VIEWPORT.width,
                DISPLAY_VIEWPORT.height,
                ppa,
            )
            .ok_or_else(|| anyhow::anyhow!("PocketJS rejected the App framebuffer geometry"))
            .map(|_| ())
    })
}

fn draw_frame(
    panel: esp_idf_svc::sys::esp_lcd_panel_handle_t,
    framebuffer: *mut core::ffi::c_void,
) -> anyhow::Result<()> {
    esp_result("esp_lcd_panel_draw_bitmap", unsafe {
        esp_idf_svc::sys::esp_lcd_panel_draw_bitmap(
            panel,
            0,
            0,
            DISPLAY_VIEWPORT.width as i32,
            DISPLAY_VIEWPORT.height as i32,
            framebuffer,
        )
    })
}
