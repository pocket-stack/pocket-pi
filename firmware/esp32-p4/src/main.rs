use core::time::Duration;

use pocket_pi_embedded_core::DeviceState;
use pocketjs_core::Ui;
use pocketjs_esp32p4_ppa::{Renderer, RendererConfig};

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let mut state = DeviceState::default();
    state.begin_boot();

    // The board BSP will replace this placeholder viewport after the panel is
    // identified. Constructing both objects here keeps the actual PocketJS P4
    // renderer in the cross-compiled firmware instead of using a mock UI path.
    let mut ui = Ui::new();
    ui.set_viewport(1.0, 1.0);
    let renderer = Renderer::new(RendererConfig::default())
        .ok_or_else(|| anyhow::anyhow!("invalid PocketJS renderer configuration"))?;

    let memory = memory_snapshot();
    log::info!("Pocket Pi ESP32-P4 boot probe");
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
    log::warn!(
        "board profile is not selected; Wi-Fi coprocessor and display are intentionally disabled"
    );

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
