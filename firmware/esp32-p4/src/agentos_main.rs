use core::time::Duration;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context as _;
use base64::prelude::{Engine as _, BASE64_STANDARD};
use embedded_svc::http::{Headers as _, Method};
use embedded_svc::io::{Read as _, Write as _};
use esp_idf_svc::http::server::{Configuration as ServerConfiguration, EspHttpServer};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use pocket_pi_agentos::{
    stage_pocketapp_bytes, system_app_bundle, AppDescriptor, AppServiceHost, AppSupervisor,
    InstalledAppIndex, RoutedToolHost, StagedApp, ACTION_STACK_BYTES, MAX_POCKETAPP_BYTES,
};
use pocket_pi_embedded::{AgentEvent, ModelBackend, ToolHost, MODEL_WORKER_STACK_BYTES};
use pocket_pi_protocols::model::ModelBackendSettings;
use pocket_pi_tools::CoreToolHost;
use pocketjs_esp32p4_ppa::{EspIdfPpaOps, RenderTargetState, Renderer};
use serde_json::{json, Value};

use super::{
    app_services::EspAppServices,
    backend,
    device_state::{SettingsFacts, WifiSettingsFacts},
    esp_result, init_wifi, storage, transport, transport::LineTransport as _, DisplayProbe,
    EspPlatform, BOARD_NAME, PANEL_HEIGHT, PANEL_WIDTH,
};

const AGENTOS_FREERTOS_HZ: u32 = 100;
const SYSTEM_TELEMETRY_INTERVAL: Duration = Duration::from_secs(10);
const SYSTEM_TELEMETRY_CORES: u64 = 2;
const UART_INSTALL_BEGIN: &str = "PPI-INSTALL-BEGIN:";
const UART_INSTALL_CHUNK: &str = "PPI-INSTALL-CHUNK:";
const UART_INSTALL_READY: &str = "PPI-INSTALL-READY";
const UART_INSTALL_ACK: &str = "PPI-INSTALL-ACK";
const UART_INSTALL_UPLOADED: &str = "PPI-INSTALL-UPLOADED";
const UART_INSTALL_ERROR: &str = "PPI-INSTALL-ERROR:";
const _: () = assert!(esp_idf_svc::sys::configTICK_RATE_HZ == AGENTOS_FREERTOS_HZ);
const _: () = assert!(esp_idf_svc::sys::configNUM_CORES as u64 == SYSTEM_TELEMETRY_CORES);

#[derive(Clone)]
struct Message {
    role: &'static str,
    text: String,
}

struct InstallUi {
    state: &'static str,
    descriptor: AppDescriptor,
    error: Option<String>,
}

struct SystemTelemetry {
    idle_tasks: [esp_idf_svc::sys::TaskHandle_t; 2],
    previous: Option<([u32; 2], i64)>,
    facts: Option<SystemTelemetryFacts>,
}

struct SystemTelemetryFacts {
    cpu_percent: Option<u8>,
    psram_used_percent: u8,
    psram_free_bytes: usize,
}

impl SystemTelemetry {
    fn new() -> Self {
        let idle_tasks = unsafe {
            [
                esp_idf_svc::sys::xTaskGetIdleTaskHandleForCore(0),
                esp_idf_svc::sys::xTaskGetIdleTaskHandleForCore(1),
            ]
        };
        Self {
            idle_tasks,
            previous: None,
            facts: None,
        }
    }

    fn update(&mut self, visible: bool) {
        if !visible {
            self.previous = None;
            return;
        }
        let now = unsafe { esp_idf_svc::sys::esp_timer_get_time() };
        let idle = idle_run_times(self.idle_tasks);
        let Some((previous_idle, previous_time)) = self.previous else {
            self.previous = Some((idle, now));
            self.facts = Some(telemetry_facts(None));
            return;
        };
        let elapsed = now.saturating_sub(previous_time) as u64;
        if elapsed < SYSTEM_TELEMETRY_INTERVAL.as_micros() as u64 {
            return;
        }
        let idle_elapsed = idle
            .iter()
            .zip(previous_idle)
            .map(|(current, previous)| current.wrapping_sub(previous) as u64)
            .sum::<u64>();
        let capacity = elapsed.saturating_mul(SYSTEM_TELEMETRY_CORES);
        let cpu = (capacity != 0).then(|| {
            100u64.saturating_sub(idle_elapsed.saturating_mul(100) / capacity)
                .min(100) as u8
        });
        self.previous = Some((idle, now));
        self.facts = Some(telemetry_facts(cpu));
    }
}

fn idle_run_times(tasks: [esp_idf_svc::sys::TaskHandle_t; 2]) -> [u32; 2] {
    tasks.map(|task| unsafe {
        let mut status = core::mem::zeroed::<esp_idf_svc::sys::TaskStatus_t>();
        esp_idf_svc::sys::vTaskGetInfo(
            task,
            &mut status,
            0,
            esp_idf_svc::sys::eTaskState_eRunning,
        );
        status.ulRunTimeCounter
    })
}

fn telemetry_facts(cpu_percent: Option<u8>) -> SystemTelemetryFacts {
    let total = unsafe {
        esp_idf_svc::sys::heap_caps_get_total_size(esp_idf_svc::sys::MALLOC_CAP_SPIRAM)
    };
    let free = unsafe {
        esp_idf_svc::sys::heap_caps_get_free_size(esp_idf_svc::sys::MALLOC_CAP_SPIRAM)
    };
    let psram_used_percent = if total == 0 {
        0
    } else {
        (total.saturating_sub(free).saturating_mul(100) / total).min(100) as u8
    };
    SystemTelemetryFacts {
        cpu_percent,
        psram_used_percent,
        psram_free_bytes: free,
    }
}

pub fn run() -> anyhow::Result<()> {
    let _workspace = storage::mount_workspace()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let uart = Arc::new(
        transport::UartLineTransport::new()
            .map_err(|error| anyhow::anyhow!("initialize UART transport: {error}"))?,
    );
    let runtime_config = match transport::load_runtime_config(nvs.clone())
        .map_err(anyhow::Error::msg)?
    {
        Some(config) => {
            log::info!("loaded wireless model configuration from NVS");
            config
        }
        None => {
            let config = transport::request_runtime_config(uart.as_ref(), Duration::from_secs(20))
                .map_err(|error| anyhow::anyhow!(
                    "Pocket Pi is not provisioned; run tools/uart-provision.py: {error}"
                ))?;
            if matches!(config.model.backend, ModelBackendSettings::Wireless { .. }) {
                transport::persist_runtime_config(nvs.clone(), &config)
                    .map_err(anyhow::Error::msg)?;
                uart.write_line("PPI-CONFIG-STORED");
                log::info!("stored wireless model configuration in NVS");
            }
            config
        }
    };
    if let Some(seconds) = runtime_config.unix_time_seconds {
        let time = esp_idf_svc::sys::timeval {
            tv_sec: seconds as i64,
            tv_usec: 0,
        };
        if unsafe { esp_idf_svc::sys::settimeofday(&time, core::ptr::null()) } == 0 {
            log::info!("clock seeded by UART bridge");
        }
    }

    let mut wifi = match init_wifi(
        nvs.clone(),
        runtime_config.wifi_ssid.as_deref(),
        runtime_config.wifi_password.as_deref(),
    ) {
        Ok(wifi) => {
            log::info!("C6-SDIO Wi-Fi and lwIP netif active");
            Some(wifi)
        }
        Err(error) => {
            log::error!("C6-SDIO Wi-Fi radio probe failed: {error:#}");
            None
        }
    };
    let mut settings = wifi
        .as_ref()
        .map(|wifi| {
            wifi.facts(if wifi.is_connecting() {
                "CONNECTING..."
            } else if wifi.is_connected() {
                "CONNECTED"
            } else {
                "NOT CONNECTED"
            })
        })
        .unwrap_or_else(|| SettingsFacts {
            firmware_version: env!("CARGO_PKG_VERSION").into(),
            workspace_free_bytes: storage::workspace_free_bytes().ok(),
            wifi: WifiSettingsFacts {
                status: "WI-FI DRIVER UNAVAILABLE".into(),
                ..Default::default()
            },
        });
    let _sntp = if wifi.is_some() {
        esp_idf_svc::sntp::EspSntp::new_default().ok()
    } else {
        None
    };

    let model_settings = runtime_config.model.clone();
    let wireless_model = matches!(
        model_settings.backend,
        ModelBackendSettings::Wireless { .. }
    );
    let provider = match model_settings.backend {
        ModelBackendSettings::Uart { .. } => "uart",
        ModelBackendSettings::Wireless { provider } => provider.id(),
    };
    let resolved_model = model_settings
        .resolved_model()
        .unwrap_or_else(|_| "unknown".into());
    let uart_poc = matches!(model_settings.backend, ModelBackendSettings::Uart { .. });
    let backend: Arc<dyn ModelBackend> = match model_settings.backend {
        ModelBackendSettings::Uart { .. } => {
            let transport: Arc<dyn transport::LineTransport> = uart.clone();
            Arc::new(backend::UartBackend::new(transport))
        }
        ModelBackendSettings::Wireless { provider } => Arc::new(
            backend::WirelessBackend::new(
                provider,
                runtime_config
                    .model_api_key
                    .ok_or_else(|| anyhow::anyhow!("wireless backend is missing API key"))?,
            )
            .map_err(anyhow::Error::msg)?,
        ),
    };

    let network_ready = Arc::new(AtomicBool::new(
        wifi.as_ref().is_some_and(|wifi| wifi.is_connected()),
    ));
    let catalog = InstalledAppIndex::load(
        std::path::Path::new(storage::WORKSPACE_ROOT),
        system_app_bundle(),
    )?;
    let services: Arc<dyn AppServiceHost> = Arc::new(EspAppServices::new(
        network_ready.clone(),
        catalog.clone(),
        Some(nvs),
    ));
    let mut supervisor = with_psram_pthread_config(ACTION_STACK_BYTES, || {
        AppSupervisor::new(storage::WORKSPACE_ROOT, catalog, services)
    })?;
    let mut telemetry = SystemTelemetry::new();
    supervisor.frame()?;

    let mut renderer = pocketjs_esp32p4_ppa::Renderer::new(Default::default())
        .ok_or_else(|| anyhow::anyhow!("invalid PocketJS renderer configuration"))?;
    let mut ppa = EspIdfPpaOps::new()
        .map_err(|error| anyhow::anyhow!("initialize PocketJS PPA: ESP-IDF error 0x{error:x}"))?;
    log::info!("Pocket Pi AgentOS hardware boot: {BOARD_NAME}");
    let mut display = match init_display(&mut renderer, &mut ppa, &supervisor) {
        Ok(display) => {
            log::info!("MIPI-DSI panel active with PocketJS App View");
            Some(display)
        }
        Err(error) => {
            log::error!("MIPI-DSI panel failed: {error:#}");
            None
        }
    };
    let native_tools = Arc::new(CoreToolHost::new(
        storage::WORKSPACE_ROOT,
        Arc::new(EspPlatform),
    ));
    let native: Arc<dyn ToolHost> = native_tools.clone();
    let (routed_tools, app_rx) = RoutedToolHost::new(native, supervisor.catalog().clone());
    let config = json!({
        "provider":provider,
        "model":resolved_model,
        "thinkingLevel":model_settings.thinking_level.id(),
        "systemPrompt":"You are Pi Agent, the first-class system App in Pocket Pi AgentOS on an ESP32-P4. You can manage the top-level /workspace and use installed App tools. Use /workspace for durable memory, notes, plans, and artifacts; read and update relevant files when continuity matters. Installed Apps own their Data, Actions, and Views. Be concise."
    });
    with_psram_pthread_config(MODEL_WORKER_STACK_BYTES, || {
        supervisor
            .boot_agent(&config.to_string(), backend, Arc::new(routed_tools))
            .map_err(anyhow::Error::msg)
    })?;
    let install_root = prepare_install_root()?;
    let (install_tx, install_rx) = mpsc::sync_channel(1);
    let install_slot = Arc::new(AtomicBool::new(false));
    let _install_server = start_install_server(
        install_root.clone(),
        install_tx.clone(),
        install_slot.clone(),
    )?;

    let mut messages = vec![Message {
        role: "assistant",
        text: "Pocket Pi AgentOS is starting.".into(),
    }];
    let mut agent_status = "STARTING";
    let mut busy = false;
    let mut initial_prompt = runtime_config.initial_prompt;
    let initial_prompt_not_before =
        Instant::now() + Duration::from_secs(runtime_config.initial_prompt_delay_seconds);
    let mut touch_was_down = false;
    let mut redraw = true;
    let mut system_dirty = true;
    let mut pending_ui_action: Option<(String, Value)> = None;
    let mut pending_install: Option<StagedApp> = None;
    let mut install_ui: Option<InstallUi> = None;
    let mut install_requested = false;
    let mut pending_uninstall: Option<String> = None;
    let mut uninstall_error: Option<String> = None;
    let mut last_tick = Instant::now();
    let mut last_runtime_pump = Instant::now();
    let mut last_system_refresh = Instant::now();
    let mut last_heartbeat = Instant::now();
    let mut last_wifi_poll = Instant::now();
    let mut last_uart_install_poll = Instant::now();
    let mut last_wifi_connected = wifi.as_ref().is_some_and(|wifi| wifi.is_connected());

    loop {
        if last_uart_install_poll.elapsed() >= Duration::from_millis(250)
            && pending_install.is_none()
            && pending_uninstall.is_none()
            && !busy
            && !supervisor.services_busy()
        {
            last_uart_install_poll = Instant::now();
            if let Ok(begin) = uart.read_frame(UART_INSTALL_BEGIN, Duration::from_millis(1)) {
                receive_uart_install(
                    uart.as_ref(),
                    &begin,
                    &install_root,
                    &install_tx,
                    install_slot.as_ref(),
                );
            }
        }
        if pending_install.is_none()
            && pending_uninstall.is_none()
            && !busy
            && !supervisor.services_busy()
        {
            if let Ok(staged) = install_rx.try_recv() {
                install_ui = Some(InstallUi {
                    state: "review",
                    descriptor: staged.descriptor.clone(),
                    error: None,
                });
                pending_install = Some(staged);
                supervisor.open(pocket_pi_agentos::ROOT_APP_ID)?;
                system_dirty = true;
                redraw = true;
            }
        }
        while let Ok(request) = app_rx.try_recv() {
            request.handle(&mut supervisor);
            redraw = true;
            system_dirty = true;
        }

        if let Some(display) = display.as_mut() {
            if let Some((x, y)) = display.read_touch() {
                if !touch_was_down {
                    supervisor.pointer_down(x, y)?;
                    let action = supervisor.tap(x, y)?;
                    match action.get("type").and_then(Value::as_str) {
                        Some("action") => {
                            if let Some(name) = action.get("action").and_then(Value::as_str) {
                                pending_ui_action = Some((
                                    name.to_owned(),
                                    action.get("args").cloned().unwrap_or(Value::Null),
                                ));
                            }
                        }
                        Some("command") => {
                            let args = action.get("args").unwrap_or(&Value::Null);
                            match action.get("command").and_then(Value::as_str) {
                                Some("apps.open") => {
                                    if let Some(app) = args.get("app").and_then(Value::as_str) {
                                        supervisor.open(app)?;
                                    }
                                }
                                Some("agent.submit") => {
                                    if let Some(prompt) = args.get("prompt").and_then(Value::as_str) {
                                        submit_prompt(
                                            prompt.to_owned(),
                                            &supervisor,
                                            &mut messages,
                                            &mut busy,
                                            &mut agent_status,
                                        );
                                    }
                                }
                                Some("apps.uninstall") => {
                                    if let Some(app_id) = args.get("app").and_then(Value::as_str) {
                                        if busy || supervisor.services_busy() {
                                            uninstall_error = Some("APP SERVICES ARE BUSY".into());
                                        } else {
                                            pending_uninstall = Some(app_id.to_owned());
                                            uninstall_error = None;
                                        }
                                    }
                                }
                                Some("apps.install")
                                    if install_ui.as_ref().is_some_and(|ui| ui.state == "review") =>
                                {
                                    if let Some(ui) = &mut install_ui {
                                        ui.state = "installing";
                                    }
                                    install_requested = true;
                                }
                                Some("apps.dismissInstall") => {
                                    if let Some(staged) = pending_install.take() {
                                        if let Some(path) = staged.release_dir.parent() {
                                            let _ = std::fs::remove_dir_all(path);
                                        }
                                    }
                                    install_ui = None;
                                    install_slot.store(false, Ordering::Release);
                                }
                                Some("device.wifi.scan") => match wifi.as_mut() {
                                    Some(wifi) => {
                                        settings.wifi.scanning = true;
                                        settings.wifi.status = "SCANNING...".into();
                                        match wifi.scan() {
                                            Ok(networks) => {
                                                settings = wifi.facts("");
                                                settings.wifi.networks = networks;
                                            }
                                            Err(error) => {
                                                settings.wifi.status =
                                                    format!("SCAN FAILED: {error}");
                                            }
                                        }
                                        settings.wifi.scanning = false;
                                    }
                                    None => {
                                        settings.wifi.status = "WI-FI DRIVER UNAVAILABLE".into()
                                    }
                                },
                                Some("device.wifi.connect") => {
                                    let ssid = args.get("ssid").and_then(Value::as_str).unwrap_or("");
                                    let password = args
                                        .get("password")
                                        .and_then(Value::as_str)
                                        .unwrap_or("");
                                    match wifi.as_mut() {
                                        Some(wifi) => {
                                            match wifi.begin_connect(ssid, password, true) {
                                                Ok(()) => {
                                                    settings = wifi.facts("CONNECTING...");
                                                    last_wifi_poll = Instant::now();
                                                }
                                                Err(error) => {
                                                    settings.wifi.status =
                                                        format!("CONNECT FAILED: {error}")
                                                }
                                            }
                                        }
                                        None => {
                                            settings.wifi.status = "WI-FI DRIVER UNAVAILABLE".into()
                                        }
                                    }
                                }
                                Some("device.wifi.forget") => match wifi.as_mut() {
                                    Some(wifi) => match wifi.forget() {
                                        Ok(()) => settings = wifi.facts("NETWORK FORGOTTEN"),
                                        Err(error) => {
                                            settings.wifi.status = format!("FORGET FAILED: {error}")
                                        }
                                    },
                                    None => {
                                        settings.wifi.status = "WI-FI DRIVER UNAVAILABLE".into()
                                    }
                                },
                                Some("device.restart") => unsafe {
                                    esp_idf_svc::sys::esp_restart();
                                },
                                _ => {}
                            }
                            settings.workspace_free_bytes = storage::workspace_free_bytes().ok();
                            system_dirty = true;
                        }
                        _ => {}
                    }
                    redraw = true;
                }
                touch_was_down = true;
            } else {
                if touch_was_down {
                    supervisor.pointer_up()?;
                    redraw = true;
                }
                touch_was_down = false;
            }
        }

        if last_wifi_poll.elapsed() >= Duration::from_millis(500) {
            last_wifi_poll = Instant::now();
            if let Some(wifi) = wifi.as_mut() {
                match wifi.poll_connect() {
                    Some(Ok(())) => {
                        settings = wifi.facts("CONNECTED");
                        system_dirty = true;
                        redraw = true;
                        log::info!("Wi-Fi association and DHCP completed");
                    }
                    Some(Err(error)) => {
                        settings = wifi.facts(format!("CONNECT FAILED: {error}"));
                        system_dirty = true;
                        redraw = true;
                        log::warn!("Wi-Fi connection attempt failed: {error:#}");
                    }
                    None => {}
                }
            }
            let connected = wifi.as_ref().is_some_and(|wifi| wifi.is_connected());
            network_ready.store(connected, Ordering::Release);
            if connected != last_wifi_connected {
                last_wifi_connected = connected;
                if let Some(wifi) = wifi.as_ref().filter(|wifi| !wifi.is_connecting()) {
                    let networks = core::mem::take(&mut settings.wifi.networks);
                    settings = wifi.facts(if connected {
                        "CONNECTED"
                    } else {
                        "NOT CONNECTED"
                    });
                    settings.wifi.networks = networks;
                    system_dirty = true;
                    redraw = true;
                }
                log::info!(
                    "Wi-Fi link state changed: {}",
                    if connected {
                        "connected"
                    } else {
                        "disconnected"
                    }
                );
            }
        }

        if last_tick.elapsed() >= Duration::from_secs(1) {
            if install_ui.is_none()
                && pending_uninstall.is_none()
                && !busy
                && agent_status == "IDLE"
                && initial_prompt.is_none()
            {
                if let Some(wake) = native_tools.claim_due() {
                    submit_prompt(
                        wake.prompt,
                        &supervisor,
                        &mut messages,
                        &mut busy,
                        &mut agent_status,
                    );
                }
                // The ESP32 v1 runtime has one App/SQLite execution owner.
                // App schedules wait for an active Agent turn to complete so
                // they cannot overlap an App Tool or its transaction.
                let app_results = if busy {
                    Vec::new()
                } else {
                    supervisor.poll_due_actions()
                };
                for (action, result) in &app_results {
                    log::info!("App Action {action}: {}", result.text);
                }
                if !app_results.is_empty() {
                    redraw = true;
                }
            }
            last_tick = Instant::now();
        }

        if last_system_refresh.elapsed() >= Duration::from_secs(5) {
            if supervisor.active_id() == pocket_pi_agentos::ROOT_APP_ID {
                telemetry.update(supervisor.system_telemetry_visible()?);
                system_dirty = true;
                redraw = true;
            } else {
                telemetry.update(false);
            }
            last_system_refresh = Instant::now();
        }

        if system_dirty {
            let facts = system_facts(
                &messages,
                agent_status,
                provider,
                &resolved_model,
                &settings,
                native_tools.as_ref(),
                supervisor.catalog(),
                install_ui.as_ref(),
                pending_uninstall.as_deref(),
                uninstall_error.as_deref(),
                telemetry.facts.as_ref(),
            );
            supervisor.update_system(&facts)?;
            system_dirty = false;
        }
        if supervisor.active_projection_is_stale() {
            // A background App commit is itself a redraw cause. The check is
            // one atomic revision comparison; SQLite is read only after the
            // active View consumes the invalidation below.
            redraw = true;
        }
        // Touch sampling stays at the display cadence, while entering QuickJS
        // is event-driven and capped at 20 Hz. An idle system therefore leaves
        // most CPU0 time to FreeRTOS and navigation is not queued behind
        // redundant Agent/App tick calls.
        if redraw || last_runtime_pump.elapsed() >= Duration::from_millis(50) {
            for event in supervisor.frame_render(redraw)? {
                match event {
                    AgentEvent::Ready => {
                        log::info!("Pi Agent System App ready with App Tool registry");
                        if uart_poc {
                            unsafe {
                                esp_idf_svc::sys::esp_log_level_set(
                                    c"*".as_ptr(),
                                    esp_idf_svc::sys::esp_log_level_t_ESP_LOG_WARN,
                                );
                                // Keep App transport stage boundaries visible on UART without
                                // restoring noisy global INFO logging or exposing response data.
                                esp_idf_svc::sys::esp_log_level_set(
                                    c"pocket_pi_p4::app_services".as_ptr(),
                                    esp_idf_svc::sys::esp_log_level_t_ESP_LOG_INFO,
                                );
                            }
                        }
                        agent_status = "IDLE";
                        messages[0].text = "ESP32-P4 Pi Agent is ready.".into();
                    }
                    AgentEvent::ResponseText(text) => {
                        if let Some(message) = messages.last_mut() {
                            message.text.push_str(&text);
                        }
                    }
                    AgentEvent::Done => {
                        log::info!("Pi Agent turn completed");
                        agent_status = "IDLE";
                        busy = false;
                    }
                    AgentEvent::Failed(error) => {
                        log::error!("Pi Agent failed: {error}");
                        if let Some(message) = messages.last_mut() {
                            message.text = format!("Agent failed: {error}");
                        }
                        agent_status = "FAULTED";
                        busy = false;
                    }
                }
                redraw = true;
                system_dirty = true;
            }
            last_runtime_pump = Instant::now();
        }

        if agent_status == "IDLE"
            && !busy
            && install_ui.is_none()
            && pending_uninstall.is_none()
            && initial_prompt.is_some()
            && Instant::now() >= initial_prompt_not_before
            && (!wireless_model || network_ready.load(Ordering::Acquire))
        {
            if let Some(prompt) = initial_prompt.take() {
                submit_prompt(
                    prompt,
                    &supervisor,
                    &mut messages,
                    &mut busy,
                    &mut agent_status,
                );
                redraw = true;
                system_dirty = true;
            }
        }

        if redraw {
            if let Some(display) = display.as_mut() {
                display.render_agentos(&mut renderer, &mut ppa, &supervisor)?;
            }
            redraw = false;
        }

        // The loading facts is rendered before activation enters QuickJS,
        // SQLite and flash. Touch and prompts remain locked by InstallScreen.
        if install_requested {
            install_requested = false;
            if let Some(staged) = pending_install.take() {
                let cleanup = staged
                    .release_dir
                    .parent()
                    .map(std::path::Path::to_path_buf);
                let result = with_psram_pthread_config(ACTION_STACK_BYTES, || {
                    supervisor.activate_app(&staged.release_dir, staged.credentials)
                });
                match &result {
                    Ok(descriptor) => {
                        log::info!(
                            "App install succeeded: {} {}",
                            descriptor.id,
                            descriptor.version
                        )
                    }
                    Err(error) => log::error!("App install failed: {error:#}"),
                }
                if let Some(path) = cleanup {
                    let _ = std::fs::remove_dir_all(path);
                }
                if let Some(ui) = &mut install_ui {
                    match result {
                        Ok(_) => ui.state = "success",
                        Err(error) => {
                            ui.state = "failed";
                            ui.error = Some(format!("{error:#}"));
                        }
                    }
                }
                system_dirty = true;
                redraw = true;
            }
        }

        if let Some(app_id) = pending_uninstall.take() {
            match supervisor.uninstall_app(&app_id) {
                Ok(_) => log::info!("App uninstall succeeded: {app_id}"),
                Err(error) => {
                    log::error!("App uninstall failed: {error:#}");
                    uninstall_error = Some(format!("{error:#}"));
                }
            }
            system_dirty = true;
            redraw = true;
        }

        // A UI-requested Action runs only after its loading state has reached the
        // panel. This preserves immediate touch feedback even when HTTPS is slow.
        if install_ui.is_none() && pending_uninstall.is_none() && !busy {
            if let Some((action, args)) = pending_ui_action.take() {
                let started = Instant::now();
                let result = supervisor.invoke_active_action(&action, &args);
                log::info!(
                    "UI App Action {} finished in {}ms: {}",
                    action,
                    started.elapsed().as_millis(),
                    result.text
                );
                supervisor.frame()?;
                redraw = true;
            }
        }

        if last_heartbeat.elapsed() >= Duration::from_secs(5) {
            log::info!(
                "heartbeat app_action={} agent={} app={}",
                if supervisor.services_busy() {
                    "running"
                } else {
                    "idle"
                },
                agent_status,
                supervisor.active_id(),
            );
            last_heartbeat = Instant::now();
        }
        // AgentOS is a native FreeRTOS task, not a pthread. With a 100 Hz tick,
        // delaying one tick keeps the loop at scheduler granularity and gives
        // CPU0's idle task a watchdog-feeding scheduling opportunity.
        unsafe { esp_idf_svc::sys::vTaskDelay(1) };
    }
}

fn prepare_install_root() -> anyhow::Result<PathBuf> {
    let install_root = Path::new(storage::WORKSPACE_ROOT).join(".system/install");
    if let Err(error) = std::fs::remove_dir_all(&install_root) {
        if error.kind() != std::io::ErrorKind::NotFound {
            return Err(error).context("clear stale App install staging");
        }
    }
    std::fs::create_dir_all(&install_root)?;
    Ok(install_root)
}

fn start_install_server(
    install_root: PathBuf,
    tx: SyncSender<StagedApp>,
    slot: Arc<AtomicBool>,
) -> anyhow::Result<EspHttpServer<'static>> {
    let mut server = EspHttpServer::new(&ServerConfiguration {
        stack_size: 12 * 1024,
        ..Default::default()
    })?;
    server.fn_handler::<anyhow::Error, _>("/", Method::Get, |request| {
        request.into_ok_response()?.write_all(
            b"<form><h1>Pocket Pi App Install</h1><input type=file id=f><button type=button onclick=send()>Upload</button><script>function send(){fetch('/install',{method:'POST',body:f.files[0]}).then(r=>r.text()).then(alert)}</script></form>",
        )?;
        Ok(())
    })?;

    let handler_slot = slot.clone();
    server.fn_handler::<anyhow::Error, _>("/install", Method::Post, move |mut request| {
        if handler_slot
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            request
                .into_status_response(409)?
                .write_all(b"another install is pending")?;
            return Ok(());
        }
        let job = install_job(&install_root);
        let result = (|| -> anyhow::Result<StagedApp> {
            let length = usize::try_from(
                request
                    .content_len()
                    .ok_or_else(|| anyhow::anyhow!("missing Content-Length"))?,
            )
            .context("Content-Length is too large")?;
            anyhow::ensure!(length <= MAX_POCKETAPP_BYTES, "package exceeds 2 MiB");
            let mut bytes = vec![0; length];
            request.read_exact(&mut bytes)?;
            std::fs::create_dir_all(&job)?;
            stage_pocketapp_bytes(&bytes, &job.join("release"))
        })();
        match result {
            Ok(staged) => match tx.try_send(staged) {
                Ok(()) => {
                    request
                        .into_status_response(202)?
                        .write_all(b"uploaded; confirm installation on Pocket Pi")?;
                }
                Err(
                    mpsc::TrySendError::Full(staged) | mpsc::TrySendError::Disconnected(staged),
                ) => {
                    if let Some(path) = staged.release_dir.parent() {
                        let _ = std::fs::remove_dir_all(path);
                    }
                    handler_slot.store(false, Ordering::Release);
                    request
                        .into_status_response(409)?
                        .write_all(b"installer is busy")?;
                }
            },
            Err(error) => {
                let _ = std::fs::remove_dir_all(&job);
                handler_slot.store(false, Ordering::Release);
                request
                    .into_status_response(400)?
                    .write_all(format!("invalid package: {error:#}").as_bytes())?;
            }
        }
        Ok(())
    })?;
    log::info!("App installer listening on http://<device-ip>/");
    Ok(server)
}

fn receive_uart_install(
    transport: &dyn transport::LineTransport,
    begin: &str,
    install_root: &Path,
    tx: &SyncSender<StagedApp>,
    slot: &AtomicBool,
) {
    if slot
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        transport.write_line(&format!("{UART_INSTALL_ERROR}installer is busy"));
        return;
    }

    let job = install_job(install_root);
    let result = (|| -> anyhow::Result<()> {
        let length = begin
            .strip_prefix(UART_INSTALL_BEGIN)
            .context("invalid upload header")?
            .parse::<usize>()
            .context("invalid package length")?;
        anyhow::ensure!(length > 0, "package is empty");
        anyhow::ensure!(length <= MAX_POCKETAPP_BYTES, "package exceeds 2 MiB");

        let mut bytes = Vec::with_capacity(length);
        transport.write_line(UART_INSTALL_READY);
        while bytes.len() < length {
            let frame = transport
                .read_frame(UART_INSTALL_CHUNK, Duration::from_secs(10))
                .map_err(anyhow::Error::msg)?;
            let payload = frame
                .strip_prefix(UART_INSTALL_CHUNK)
                .context("invalid chunk header")?;
            let chunk = BASE64_STANDARD
                .decode(payload)
                .context("invalid chunk encoding")?;
            anyhow::ensure!(!chunk.is_empty(), "empty chunk");
            anyhow::ensure!(
                bytes.len() + chunk.len() <= length,
                "package exceeds declared length"
            );
            bytes.extend_from_slice(&chunk);
            transport.write_line(UART_INSTALL_ACK);
        }

        std::fs::create_dir_all(&job)?;
        let staged = stage_pocketapp_bytes(&bytes, &job.join("release"))?;
        tx.try_send(staged)
            .map_err(|_| anyhow::anyhow!("installer is busy"))?;
        Ok(())
    })();

    match result {
        Ok(()) => transport.write_line(UART_INSTALL_UPLOADED),
        Err(error) => {
            let _ = std::fs::remove_dir_all(&job);
            slot.store(false, Ordering::Release);
            let message = format!("{error:#}").replace(['\r', '\n'], " ");
            transport.write_line(&format!("{UART_INSTALL_ERROR}{message}"));
        }
    }
}

fn install_job(install_root: &Path) -> PathBuf {
    install_root.join(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
            .to_string(),
    )
}

fn with_psram_pthread_config<T>(
    stack_size: usize,
    action: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    // Large persistent workers use PSRAM stacks. App Action passes this setting
    // to its NET child; the platform default is restored after each spawn.
    let default = unsafe { esp_idf_svc::sys::esp_pthread_get_default_config() };
    let mut config = default;
    config.stack_size = stack_size;
    config.inherit_cfg = true;
    config.pin_to_core = 1;
    config.stack_alloc_caps =
        esp_idf_svc::sys::MALLOC_CAP_SPIRAM | esp_idf_svc::sys::MALLOC_CAP_8BIT;
    let configured = unsafe { esp_idf_svc::sys::esp_pthread_set_cfg(&config) };
    if configured != esp_idf_svc::sys::ESP_OK {
        anyhow::bail!("configure PSRAM pthread: ESP-IDF error 0x{configured:x}");
    }
    let result = action();
    let restored = unsafe { esp_idf_svc::sys::esp_pthread_set_cfg(&default) };
    if restored != esp_idf_svc::sys::ESP_OK {
        anyhow::bail!("restore pthread defaults: ESP-IDF error 0x{restored:x}");
    }
    result
}

fn submit_prompt(
    prompt: String,
    supervisor: &AppSupervisor,
    messages: &mut Vec<Message>,
    busy: &mut bool,
    agent_status: &mut &'static str,
) {
    if *busy || prompt.trim().is_empty() {
        return;
    }
    messages.push(Message {
        role: "user",
        text: prompt.clone(),
    });
    messages.push(Message {
        role: "assistant",
        text: String::new(),
    });
    *busy = true;
    *agent_status = "THINKING";
    if let Err(error) = supervisor.prompt_agent(&prompt) {
        messages.last_mut().unwrap().text = format!("Agent is unavailable: {error:#}");
        *busy = false;
        *agent_status = "FAULTED";
    }
}

fn system_facts(
    messages: &[Message],
    agent_status: &str,
    provider: &str,
    model: &str,
    settings: &SettingsFacts,
    tools: &CoreToolHost,
    catalog: &InstalledAppIndex,
    install: Option<&InstallUi>,
    uninstalling_app: Option<&str>,
    uninstall_error: Option<&str>,
    telemetry: Option<&SystemTelemetryFacts>,
) -> Value {
    let schedule = tools.schedule_projection();
    let schedule_text = match schedule.next_in_seconds {
        Some(seconds) => schedule.every_minutes.map_or_else(
            || format!("in {seconds}s"),
            |minutes| format!("in {seconds}s · every {minutes}m"),
        ),
        None => "not scheduled".to_owned(),
    };
    let install = install.map(|install| {
        let network = install
            .descriptor
            .native_services
            .http
            .iter()
            .flat_map(|policy| policy.urls.clone())
            .chain(
                install
                    .descriptor
                    .native_services
                    .mcp
                    .iter()
                    .map(|policy| policy.url.clone()),
            )
            .collect::<Vec<_>>();
        let credentials = install
            .descriptor
            .native_services
            .http
            .iter()
            .filter_map(|policy| policy.credential.as_ref())
            .chain(
                install
                    .descriptor
                    .native_services
                    .mcp
                    .iter()
                    .map(|policy| &policy.credential),
            )
            .map(|credential| credential.id.clone())
            .collect::<Vec<_>>();
        json!({
            "state":install.state,
            "name":install.descriptor.title,
            "version":install.descriptor.version,
            "tools":install.descriptor.tools.len(),
            "network":network,
            "credentials":credentials,
            "schedules":install.descriptor.schedules.len(),
            "error":install.error,
        })
    });
    json!({
        "agent":agent_status,
        "model":format!("{provider} / {model}"),
        "messages":messages.iter().map(|message| json!({"role":message.role,"text":message.text})).collect::<Vec<_>>(),
        "schedule":{
            "name":schedule.name,
            "prompt":schedule.prompt,
            "next":schedule_text,
            "everyMinutes":schedule.every_minutes,
        },
        "apps":catalog.descriptors().into_iter().filter(|app| app.id != pocket_pi_agentos::ROOT_APP_ID).map(|app| json!({
            "id":app.id,
            "title":app.title,
            "description":app.description,
            "scheduleEveryMinutes":app.schedules.first().map(|schedule| schedule.every_minutes),
        })).collect::<Vec<_>>(),
        "install":install,
        "uninstallingApp":uninstalling_app,
        "uninstallError":uninstall_error,
        "settings":{
            "wifi":{
                "connectedSsid":settings.wifi.connected_ssid,
                "ipAddress":settings.wifi.ip_address,
                "rssiDbm":settings.wifi.rssi_dbm,
                "scanning":settings.wifi.scanning,
                "networks":settings.wifi.networks.iter().map(|network| json!({
                    "ssid":network.ssid,
                    "rssiDbm":network.rssi_dbm,
                    "secured":network.secured,
                })).collect::<Vec<_>>(),
                "status":settings.wifi.status,
            },
            "firmwareVersion":settings.firmware_version,
            "workspaceFree":settings.workspace_free_bytes.map(|bytes| format!("{} KB", bytes / 1024)),
            "telemetry":telemetry.map(|telemetry| json!({
                "cpuPercent":telemetry.cpu_percent,
                "psramUsedPercent":telemetry.psram_used_percent,
                "psramFree":format!("{:.1} MB", telemetry.psram_free_bytes as f32 / (1024.0 * 1024.0)),
            })),
        },
    })
}

fn init_display(
    renderer: &mut Renderer,
    ppa: &mut EspIdfPpaOps,
    supervisor: &AppSupervisor,
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
        let mut render_states = [
            RenderTargetState::new(),
            RenderTargetState::new(),
            RenderTargetState::new(),
        ];
        supervisor.with_ui(|ui| {
            let words = ui.draw().words.clone();
            renderer
                .render_incremental(
                    &mut render_states[0],
                    ui,
                    &words,
                    pixels,
                    PANEL_WIDTH,
                    PANEL_HEIGHT,
                    ppa,
                )
                .ok_or_else(|| anyhow::anyhow!("PocketJS rejected the App framebuffer geometry"))
        })?;

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
        esp_result("esp_lcd_panel_draw_bitmap", {
            esp_idf_svc::sys::esp_lcd_panel_draw_bitmap(
                panel,
                0,
                0,
                PANEL_WIDTH as i32,
                PANEL_HEIGHT as i32,
                framebuffers[0],
            )
        })?;
        esp_result(
            "bsp_display_backlight_on",
            esp_idf_svc::sys::bsp_display_backlight_on(),
        )?;
        esp_result(
            "pi_p4_touch_new",
            esp_idf_svc::sys::pi_p4_touch_new(&mut touch),
        )?;

        Ok(DisplayProbe {
            panel,
            _io: io,
            touch,
            framebuffers: framebuffers.map(|framebuffer| framebuffer.cast()),
            render_states,
            next_framebuffer: 1,
        })
    }
}

impl DisplayProbe {
    fn render_agentos(
        &mut self,
        renderer: &mut Renderer,
        ppa: &mut EspIdfPpaOps,
        supervisor: &AppSupervisor,
    ) -> anyhow::Result<()> {
        let framebuffer = self.framebuffers[self.next_framebuffer];
        let pixels = unsafe {
            core::slice::from_raw_parts_mut(
                framebuffer,
                PANEL_WIDTH as usize * PANEL_HEIGHT as usize,
            )
        };
        supervisor.with_ui(|ui| {
            let words = ui.draw().words.clone();
            renderer
                .render_incremental(
                    &mut self.render_states[self.next_framebuffer],
                    ui,
                    &words,
                    pixels,
                    PANEL_WIDTH,
                    PANEL_HEIGHT,
                    ppa,
                )
                .ok_or_else(|| anyhow::anyhow!("PocketJS rejected the App framebuffer geometry"))
        })?;
        esp_result("esp_lcd_panel_draw_bitmap", unsafe {
            esp_idf_svc::sys::esp_lcd_panel_draw_bitmap(
                self.panel,
                0,
                0,
                PANEL_WIDTH as i32,
                PANEL_HEIGHT as i32,
                framebuffer.cast(),
            )
        })?;
        self.next_framebuffer = (self.next_framebuffer + 1) % self.framebuffers.len();
        Ok(())
    }
}
