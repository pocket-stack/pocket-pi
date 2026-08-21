use core::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::WifiDriver;
use pocket_pi_agentos::{AppSupervisor, Viewport};
use pocket_pi_esp32_common::{
    delay_current_task, esp_result, DeviceHost, DisplayHost, WifiConnection,
};
use pocketjs_core::damage::{DamagePolicy, DamageRect, DamageTarget, DamageTracker};
use pocketjs_core::raster::render_scaled_rgb565_window_over;
use std::time::Instant;

const LOGICAL_WIDTH: usize = 480;
const LOGICAL_HEIGHT: usize = 800;
const PHYSICAL_WIDTH: usize = 800;
const PHYSICAL_HEIGHT: usize = 480;
const RENDER_STRIPE_HEIGHT: usize = 8;
const ROTATION_TILE_SIZE: usize = 8;
const DISPLAY_VIEWPORT: Viewport = Viewport::new(LOGICAL_WIDTH as u32, LOGICAL_HEIGHT as u32);
const AGENTOS_LAUNCHER_STACK_BYTES: u32 = 4 * 1024;
const AGENTOS_TASK_STACK_BYTES: u32 = 48 * 1024;
const CPU_CYCLES_PER_MICROSECOND: u32 = 240;
const SCAN_LOG_INTERVAL: Duration = Duration::from_secs(30);

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    esp_result("pi_s3_flash_dispatcher_init", unsafe {
        esp_idf_svc::sys::pi_s3_flash_dispatcher_init()
    })?;

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
    const SHOW_MODEL_PROGRESS: bool = false;

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
    panel: esp_idf_svc::sys::esp_lcd_panel_handle_t,
    touch: esp_idf_svc::sys::esp_lcd_touch_handle_t,
    framebuffers: [*mut u16; 2],
    damage: [DamageTracker; 2],
    next_framebuffer: usize,
    render_buffer: *mut u16,
    touch_down: bool,
    touch_started: Option<Instant>,
    last_scan_log: Instant,
}

impl S3Display {
    fn new(supervisor: &AppSupervisor) -> anyhow::Result<Self> {
        unsafe {
            let mut panel = core::ptr::null_mut();
            let mut touch = core::ptr::null_mut();
            let mut framebuffer_0 = core::ptr::null_mut();
            let mut framebuffer_1 = core::ptr::null_mut();
            esp_result(
                "pi_s3_board_init",
                esp_idf_svc::sys::pi_s3_board_init(
                    &mut panel,
                    &mut touch,
                    &mut framebuffer_0,
                    &mut framebuffer_1,
                ),
            )?;

            let internal_caps =
                esp_idf_svc::sys::MALLOC_CAP_INTERNAL | esp_idf_svc::sys::MALLOC_CAP_8BIT;
            let largest_internal_before =
                esp_idf_svc::sys::heap_caps_get_largest_free_block(internal_caps);
            let render_buffer = esp_idf_svc::sys::heap_caps_malloc(
                LOGICAL_WIDTH * RENDER_STRIPE_HEIGHT * core::mem::size_of::<u16>(),
                internal_caps,
            )
            .cast::<u16>();
            if render_buffer.is_null() {
                anyhow::bail!("could not allocate the RGB565 render stripe in internal RAM")
            }
            log::info!(
                "S3 render stripe={}x{} internal largest_before={} largest_after={}",
                LOGICAL_WIDTH,
                RENDER_STRIPE_HEIGHT,
                largest_internal_before,
                esp_idf_svc::sys::heap_caps_get_largest_free_block(internal_caps),
            );

            let mut display = Self {
                panel,
                touch,
                framebuffers: [framebuffer_0, framebuffer_1],
                damage: [DamageTracker::new(), DamageTracker::new()],
                next_framebuffer: 1,
                render_buffer,
                touch_down: false,
                touch_started: None,
                last_scan_log: Instant::now(),
            };
            display.render(supervisor)?;
            core::ptr::copy_nonoverlapping(
                framebuffer_1,
                framebuffer_0,
                PHYSICAL_WIDTH * PHYSICAL_HEIGHT,
            );
            supervisor.with_ui(|ui| {
                let words = ui.draw().words.clone();
                display.damage[0].commit(ui, &words, damage_target());
            });
            esp_result("pi_s3_backlight_on", esp_idf_svc::sys::pi_s3_backlight_on())?;
            Ok(display)
        }
    }
}

impl DisplayHost for S3Display {
    fn read_touch(&mut self) -> Option<(u16, u16)> {
        if self.last_scan_log.elapsed() >= SCAN_LOG_INTERVAL {
            let mut stats = unsafe { core::mem::zeroed() };
            if unsafe { esp_idf_svc::sys::pi_s3_take_scan_stats(&mut stats) } {
                let internal_caps =
                    esp_idf_svc::sys::MALLOC_CAP_INTERNAL | esp_idf_svc::sys::MALLOC_CAP_8BIT;
                log::info!(
                    "S3 scan vsync={} frames={} max_vsync={}us max_frame={}us internal_free={} internal_largest={} agentos_stack_free={} psram_free={} psram_largest={}",
                    stats.vsync_count,
                    stats.frame_count,
                    stats.max_vsync_cycles / CPU_CYCLES_PER_MICROSECOND,
                    stats.max_frame_cycles / CPU_CYCLES_PER_MICROSECOND,
                    unsafe { esp_idf_svc::sys::heap_caps_get_free_size(internal_caps) },
                    unsafe {
                        esp_idf_svc::sys::heap_caps_get_largest_free_block(internal_caps)
                    },
                    unsafe {
                        esp_idf_svc::sys::uxTaskGetStackHighWaterMark2(core::ptr::null_mut())
                    },
                    unsafe {
                        esp_idf_svc::sys::heap_caps_get_free_size(
                            esp_idf_svc::sys::MALLOC_CAP_SPIRAM,
                        )
                    },
                    unsafe {
                        esp_idf_svc::sys::heap_caps_get_largest_free_block(
                            esp_idf_svc::sys::MALLOC_CAP_SPIRAM,
                        )
                    },
                );
            }
            self.last_scan_log = Instant::now();
        }
        let mut physical_x = 0u16;
        let mut physical_y = 0u16;
        let touched = unsafe {
            esp_idf_svc::sys::pi_s3_touch_read(self.touch, &mut physical_x, &mut physical_y)
        };
        if touched && !self.touch_down {
            self.touch_started = Some(Instant::now());
        }
        self.touch_down = touched;
        touched.then(|| physical_to_logical(physical_x, physical_y))
    }

    fn render(&mut self, supervisor: &AppSupervisor) -> anyhow::Result<()> {
        let render_started = Instant::now();
        let before_firmware_ms = self
            .touch_started
            .map(|started| render_started.duration_since(started).as_millis());
        let index = self.next_framebuffer;
        let framebuffer = self.framebuffers[index];
        let render_buffer = self.render_buffer;
        let mut plan_ms = 0;
        let mut raster_ms = 0;
        let mut rotate_ms = 0;
        let mut damage_regions = 0;
        let mut damage_pixels = 0;
        let mut full_redraw = false;
        let changed = supervisor.with_ui(|ui| {
            let words = ui.draw().words.clone();
            let plan_started = Instant::now();
            let damage = self.damage[index]
                .prepare(ui, &words, damage_target())
                .and_then(|plan| plan.with_policy(DamagePolicy::default()))
                .map_err(|error| anyhow::anyhow!("could not plan S3 display damage: {error:?}"))?;
            plan_ms = plan_started.elapsed().as_millis();
            damage_regions = damage.region_count();
            damage_pixels = damage.area();
            full_redraw = damage.is_full_redraw();
            if damage.is_empty() {
                self.damage[index].commit(ui, &words, damage_target());
                return Ok(false);
            }
            let framebuffer = unsafe {
                core::slice::from_raw_parts_mut(framebuffer, PHYSICAL_WIDTH * PHYSICAL_HEIGHT)
            };
            for region in damage.regions() {
                let x = region.x0 as usize;
                let width = (region.x1 - region.x0) as usize;
                for y in (region.y0 as usize..region.y1 as usize).step_by(RENDER_STRIPE_HEIGHT) {
                    let height = RENDER_STRIPE_HEIGHT.min(region.y1 as usize - y);
                    let render_buffer =
                        unsafe { core::slice::from_raw_parts_mut(render_buffer, width * height) };
                    render_buffer.fill(0);
                    let raster_started = Instant::now();
                    render_scaled_rgb565_window_over(
                        ui,
                        &words,
                        render_buffer,
                        1,
                        DamageRect::new(
                            x as i32,
                            y as i32,
                            (x + width) as i32,
                            (y + height) as i32,
                        ),
                    );
                    raster_ms += raster_started.elapsed().as_millis();
                    let rotate_started = Instant::now();
                    rotate_region(render_buffer, width, height, x, y, framebuffer);
                    rotate_ms += rotate_started.elapsed().as_millis();
                }
            }
            self.damage[index].commit(ui, &words, damage_target());
            Ok::<_, anyhow::Error>(true)
        })?;
        let present_ms = if changed {
            let present_started = Instant::now();
            esp_result("present S3 framebuffer", unsafe {
                esp_idf_svc::sys::pi_s3_present(self.panel, framebuffer)
            })?;
            self.next_framebuffer = 1 - index;
            present_started.elapsed().as_millis()
        } else {
            0
        };
        if let Some(touch_started) = self.touch_started.take() {
            log::info!(
                "S3 input before_firmware={}ms regions={} pixels={} full={} plan={}ms raster={}ms rotate={}ms present={}ms total={}ms touch_to_frame={}ms changed={changed}",
                before_firmware_ms.unwrap_or_default(),
                damage_regions,
                damage_pixels,
                full_redraw,
                plan_ms,
                raster_ms,
                rotate_ms,
                present_ms,
                render_started.elapsed().as_millis(),
                touch_started.elapsed().as_millis(),
            );
        } else {
            log::info!(
                "S3 frame regions={} pixels={} full={} plan={}ms raster={}ms rotate={}ms present={}ms total={}ms changed={changed}",
                damage_regions,
                damage_pixels,
                full_redraw,
                plan_ms,
                raster_ms,
                rotate_ms,
                present_ms,
                render_started.elapsed().as_millis(),
            );
        }
        Ok(())
    }
}

fn rotate_region(
    source: &[u16],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    framebuffer: &mut [u16],
) {
    let mut tile = [0u16; ROTATION_TILE_SIZE * ROTATION_TILE_SIZE];
    for tile_y in (0..height).step_by(ROTATION_TILE_SIZE) {
        let tile_height = ROTATION_TILE_SIZE.min(height - tile_y);
        for tile_x in (0..width).step_by(ROTATION_TILE_SIZE) {
            let tile_width = ROTATION_TILE_SIZE.min(width - tile_x);
            for row in 0..tile_height {
                let source_start = (tile_y + row) * width + tile_x;
                let tile_start = row * ROTATION_TILE_SIZE;
                tile[tile_start..tile_start + tile_width]
                    .copy_from_slice(&source[source_start..source_start + tile_width]);
            }
            for column in 0..tile_width {
                let physical_y = x + tile_x + column;
                let physical_x = PHYSICAL_WIDTH - 1 - (y + tile_y);
                let destination = physical_y * PHYSICAL_WIDTH + physical_x;
                for row in 0..tile_height {
                    framebuffer[destination - row] = tile[row * ROTATION_TILE_SIZE + column];
                }
            }
        }
    }
}

fn damage_target() -> DamageTarget {
    DamageTarget::new(LOGICAL_WIDTH as u32, LOGICAL_HEIGHT as u32, 1, 0)
}

impl Drop for S3Display {
    fn drop(&mut self) {
        unsafe { esp_idf_svc::sys::heap_caps_free(self.render_buffer.cast()) };
    }
}

fn physical_to_logical(x: u16, y: u16) -> (u16, u16) {
    (
        y.min((LOGICAL_WIDTH - 1) as u16),
        (PHYSICAL_WIDTH - 1) as u16 - x.min((PHYSICAL_WIDTH - 1) as u16),
    )
}
