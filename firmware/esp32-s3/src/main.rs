use core::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::WifiDriver;
use pocket_pi_agentos::{AppSupervisor, Viewport};
use pocket_pi_esp32_common::{
    delay_current_task, esp_result, DeviceHost, DisplayHost, WifiConnection,
};
use pocketjs_core::damage::DamageRect;
use pocketjs_core::raster::render_scaled_rgb565_window_over;

const LOGICAL_WIDTH: usize = 480;
const LOGICAL_HEIGHT: usize = 800;
const PHYSICAL_WIDTH: usize = 800;
const RENDER_STRIPE_HEIGHT: usize = 64;
const DISPLAY_VIEWPORT: Viewport = Viewport::new(LOGICAL_WIDTH as u32, LOGICAL_HEIGHT as u32);
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
    if let Err(error) = pocket_pi_esp32_common::run::<S3Host>() {
        log::error!("AgentOS runtime stopped: {error:#}");
    }
    loop {
        delay_current_task(Duration::from_secs(10));
    }
}

struct S3Host;

impl DeviceHost for S3Host {
    const BOARD_ID: &'static str = "esp32-s3";
    const BOARD_NAME: &'static str = "Waveshare ESP32-S3-Touch-LCD-4.3";
    const FIRMWARE_VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const VIEWPORT: Viewport = DISPLAY_VIEWPORT;

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
        Ok(Box::new(S3Display::new(supervisor)?))
    }
}

struct S3Display {
    touch: esp_idf_svc::sys::esp_lcd_touch_handle_t,
    framebuffer: *mut u16,
    stripe: *mut u16,
}

impl S3Display {
    fn new(supervisor: &AppSupervisor) -> anyhow::Result<Self> {
        unsafe {
            let mut panel = core::ptr::null_mut();
            let mut touch = core::ptr::null_mut();
            let mut framebuffer = core::ptr::null_mut();
            esp_result(
                "pi_s3_board_init",
                esp_idf_svc::sys::pi_s3_board_init(&mut panel, &mut touch, &mut framebuffer),
            )?;

            let stripe = esp_idf_svc::sys::heap_caps_malloc(
                LOGICAL_WIDTH * RENDER_STRIPE_HEIGHT * core::mem::size_of::<u16>(),
                esp_idf_svc::sys::MALLOC_CAP_SPIRAM | esp_idf_svc::sys::MALLOC_CAP_8BIT,
            )
            .cast::<u16>();
            if stripe.is_null() {
                anyhow::bail!("could not allocate the RGB565 render stripe in PSRAM")
            }

            let mut display = Self {
                touch,
                framebuffer,
                stripe,
            };
            display.render(supervisor)?;
            esp_result("pi_s3_backlight_on", esp_idf_svc::sys::pi_s3_backlight_on())?;
            Ok(display)
        }
    }
}

impl DisplayHost for S3Display {
    fn read_touch(&mut self) -> Option<(u16, u16)> {
        let mut physical_x = 0u16;
        let mut physical_y = 0u16;
        let touched = unsafe {
            esp_idf_svc::sys::pi_s3_touch_read(self.touch, &mut physical_x, &mut physical_y)
        };
        touched.then(|| physical_to_logical(physical_x, physical_y))
    }

    fn render(&mut self, supervisor: &AppSupervisor) -> anyhow::Result<()> {
        let framebuffer = self.framebuffer;
        let stripe = self.stripe;
        supervisor.with_ui(|ui| {
            let words = ui.draw().words.clone();
            let framebuffer = unsafe {
                core::slice::from_raw_parts_mut(framebuffer, PHYSICAL_WIDTH * LOGICAL_WIDTH)
            };
            for y in (0..LOGICAL_HEIGHT).step_by(RENDER_STRIPE_HEIGHT) {
                let height = RENDER_STRIPE_HEIGHT.min(LOGICAL_HEIGHT - y);
                let stripe =
                    unsafe { core::slice::from_raw_parts_mut(stripe, LOGICAL_WIDTH * height) };
                stripe.fill(0);
                render_scaled_rgb565_window_over(
                    ui,
                    &words,
                    stripe,
                    1,
                    DamageRect::new(0, y as i32, LOGICAL_WIDTH as i32, (y + height) as i32),
                );
                for (row_y, row) in stripe.chunks_exact(LOGICAL_WIDTH).enumerate() {
                    let physical_x = PHYSICAL_WIDTH - 1 - (y + row_y);
                    for (physical_y, pixel) in row.iter().enumerate() {
                        framebuffer[physical_y * PHYSICAL_WIDTH + physical_x] = *pixel;
                    }
                }
            }
            Ok::<(), anyhow::Error>(())
        })
    }
}

impl Drop for S3Display {
    fn drop(&mut self) {
        unsafe { esp_idf_svc::sys::heap_caps_free(self.stripe.cast()) };
    }
}

fn physical_to_logical(x: u16, y: u16) -> (u16, u16) {
    (
        y.min((LOGICAL_WIDTH - 1) as u16),
        (PHYSICAL_WIDTH - 1) as u16 - x.min((PHYSICAL_WIDTH - 1) as u16),
    )
}
