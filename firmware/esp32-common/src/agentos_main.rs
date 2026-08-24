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
use serde_json::{json, Value};

use crate::{
    app_services::EspAppServices,
    backend,
    device_state::{SettingsFacts, WifiSettingsFacts},
    storage, transport,
    transport::LineTransport as _,
    DeviceHost, EspPlatform,
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

struct AgentTurnDiagnostics {
    id: u64,
    started: Instant,
    response_bytes: usize,
}

struct InstallUi {
    state: &'static str,
    descriptor: AppDescriptor,
    update: bool,
    current_version: Option<String>,
    current_schema_version: Option<u32>,
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
            100u64
                .saturating_sub(idle_elapsed.saturating_mul(100) / capacity)
                .min(100) as u8
        });
        self.previous = Some((idle, now));
        self.facts = Some(telemetry_facts(cpu));
    }
}

fn idle_run_times(tasks: [esp_idf_svc::sys::TaskHandle_t; 2]) -> [u32; 2] {
    tasks.map(|task| unsafe {
        let mut status = core::mem::zeroed::<esp_idf_svc::sys::TaskStatus_t>();
        esp_idf_svc::sys::vTaskGetInfo(task, &mut status, 0, esp_idf_svc::sys::eTaskState_eRunning);
        status.ulRunTimeCounter
    })
}

fn telemetry_facts(cpu_percent: Option<u8>) -> SystemTelemetryFacts {
    let total =
        unsafe { esp_idf_svc::sys::heap_caps_get_total_size(esp_idf_svc::sys::MALLOC_CAP_SPIRAM) };
    let free =
        unsafe { esp_idf_svc::sys::heap_caps_get_free_size(esp_idf_svc::sys::MALLOC_CAP_SPIRAM) };
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

fn log_runtime_checkpoint(scope: &str, phase: &str, started: &Instant) {
    let internal_caps =
        esp_idf_svc::sys::MALLOC_CAP_INTERNAL | esp_idf_svc::sys::MALLOC_CAP_8BIT;
    let internal_free =
        unsafe { esp_idf_svc::sys::heap_caps_get_free_size(internal_caps) };
    let internal_largest =
        unsafe { esp_idf_svc::sys::heap_caps_get_largest_free_block(internal_caps) };
    let internal_min =
        unsafe { esp_idf_svc::sys::heap_caps_get_minimum_free_size(internal_caps) };
    let psram_free = unsafe {
        esp_idf_svc::sys::heap_caps_get_free_size(esp_idf_svc::sys::MALLOC_CAP_SPIRAM)
    };
    let psram_largest = unsafe {
        esp_idf_svc::sys::heap_caps_get_largest_free_block(esp_idf_svc::sys::MALLOC_CAP_SPIRAM)
    };
    let psram_min = unsafe {
        esp_idf_svc::sys::heap_caps_get_minimum_free_size(esp_idf_svc::sys::MALLOC_CAP_SPIRAM)
    };
    let stack_high_water_words =
        unsafe { esp_idf_svc::sys::uxTaskGetStackHighWaterMark2(core::ptr::null_mut()) };
    log::info!(
        "diag runtime scope={scope} phase={phase} elapsed_ms={} internal_free={internal_free} internal_largest={internal_largest} internal_min={internal_min} psram_free={psram_free} psram_largest={psram_largest} psram_min={psram_min} stack_high_water_words={stack_high_water_words}",
        started.elapsed().as_millis()
    );
}

pub fn run<H: DeviceHost>() -> anyhow::Result<()> {
    let boot_started = Instant::now();
    log_runtime_checkpoint("boot", "run_start", &boot_started);
    let _workspace = storage::mount_workspace()?;
    log_runtime_checkpoint("boot", "workspace_mounted", &boot_started);
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
                .map_err(|error| {
                    anyhow::anyhow!(
                        "Pocket Pi is not provisioned; run tools/uart-provision.py: {error}"
                    )
                })?;
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

    let mut wifi = match H::init_wifi(
        nvs.clone(),
        runtime_config.wifi_ssid.as_deref(),
        runtime_config.wifi_password.as_deref(),
    ) {
        Ok(wifi) => {
            log::info!("Wi-Fi and lwIP netif active");
            Some(wifi)
        }
        Err(error) => {
            log::error!("Wi-Fi radio probe failed: {error:#}");
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
            firmware_version: H::FIRMWARE_VERSION.into(),
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
                H::SHOW_MODEL_PROGRESS,
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
    let mut supervisor = with_psram_pthread_config(ACTION_STACK_BYTES, 1, || {
        AppSupervisor::new(storage::WORKSPACE_ROOT, H::VIEWPORT, catalog, services)
    })?;
    let mut telemetry = SystemTelemetry::new();
    supervisor.frame()?;

    let native_tools = Arc::new(CoreToolHost::new(
        storage::WORKSPACE_ROOT,
        EspPlatform::new(H::BOARD_ID),
    ));
    let native: Arc<dyn ToolHost> = native_tools.clone();
    let (routed_tools, agent_rx) = RoutedToolHost::new(native, supervisor.catalog().clone());
    let config = json!({
        "provider":provider,
        "model":resolved_model,
        "thinkingLevel":model_settings.thinking_level.id(),
        "systemPrompt":format!("You are Pi Agent, the first-class system App in Pocket Pi AgentOS on {}. You can manage the top-level /workspace and use installed App tools. Use /workspace for durable memory, notes, plans, and artifacts; read and update relevant files when continuity matters. Installed Apps own their Data, Actions, and Views. To iterate an installed ordinary App, call app.checkout, edit only its returned checkout with the normal file tools, update app.json version, then call app.submit after all edits are complete; it ends the turn and opens physical confirmation. Change schemaVersion and add the matching numbered migration only when the SQLite schema changes. Be concise.", H::BOARD_NAME)
    });
    with_psram_pthread_config(MODEL_WORKER_STACK_BYTES, H::MODEL_WORKER_CORE, || {
        supervisor
            .boot_agent(&config.to_string(), backend, Arc::new(routed_tools))
            .map_err(anyhow::Error::msg)
    })?;
    log_runtime_checkpoint("boot", "agent_booted", &boot_started);

    log::info!("Pocket Pi AgentOS hardware boot: {}", H::BOARD_NAME);
    let mut display = match H::init_display(&supervisor) {
        Ok(display) => {
            log::info!("display active with PocketJS App View");
            Some(display)
        }
        Err(error) => {
            log::error!("display failed: {error:#}");
            None
        }
    };
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
    let mut pending_prompt = None;
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
    let mut post_install_started: Option<Instant> = None;
    let mut pending_uninstall: Option<String> = None;
    let mut uninstall_error: Option<String> = None;
    let mut last_tick = Instant::now();
    let mut last_runtime_pump = Instant::now();
    let mut last_system_refresh = Instant::now();
    let mut last_heartbeat = Instant::now();
    let mut last_wifi_poll = Instant::now();
    let mut last_uart_install_poll = Instant::now();
    let mut last_wifi_connected = wifi.as_ref().is_some_and(|wifi| wifi.is_connected());
    let mut next_agent_turn_id = 1u64;
    let mut active_agent_turn: Option<AgentTurnDiagnostics> = None;

    log_runtime_checkpoint("boot", "event_loop_start", &boot_started);

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
                match supervisor.review_app(&staged) {
                    Ok(review) => {
                        install_ui = Some(InstallUi {
                            state: "review",
                            descriptor: staged.descriptor.clone(),
                            update: review.update,
                            current_version: review.current_version,
                            current_schema_version: review.current_schema_version,
                            error: None,
                        });
                        pending_install = Some(staged);
                    }
                    Err(error) => {
                        let current = supervisor.catalog().descriptor(&staged.descriptor.id);
                        if let Some(path) = staged.release_dir.parent() {
                            let _ = std::fs::remove_dir_all(path);
                        }
                        install_ui = Some(InstallUi {
                            state: "failed",
                            update: current.is_some(),
                            descriptor: staged.descriptor,
                            current_version: current.as_ref().map(|app| app.version.clone()),
                            current_schema_version: current.map(|app| app.schema_version),
                            error: Some(format!("{error:#}")),
                        });
                    }
                }
                supervisor.open(pocket_pi_agentos::ROOT_APP_ID)?;
                system_dirty = true;
                redraw = true;
            }
        }
        while let Ok(request) = agent_rx.try_recv() {
            request.handle(&mut supervisor, |supervisor, path| {
                let submit_started = Instant::now();
                log::info!(
                    "diag update.host phase=submit_start path_bytes={}",
                    path.len()
                );
                log_runtime_checkpoint("update.host", "submit_start", &submit_started);
                anyhow::ensure!(pending_install.is_none(), "another install is pending");
                anyhow::ensure!(pending_uninstall.is_none(), "an App uninstall is pending");
                anyhow::ensure!(!supervisor.services_busy(), "App services are busy");
                anyhow::ensure!(
                    install_slot
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok(),
                    "another install is pending"
                );
                let result = (|| -> anyhow::Result<Value> {
                    let (staged, review) =
                        supervisor.submit_app_checkout(path, &install_root)?;
                    log::info!(
                        "diag update.host phase=submit_staged app={} version={} update={}",
                        staged.descriptor.id,
                        staged.descriptor.version,
                        review.update
                    );
                    log_runtime_checkpoint("update.host", "submit_staged", &submit_started);
                    supervisor
                        .open(pocket_pi_agentos::ROOT_APP_ID)
                        .expect("resident Pi Agent remains available");
                    let descriptor = staged.descriptor.clone();
                    install_ui = Some(InstallUi {
                        state: "review",
                        descriptor: descriptor.clone(),
                        update: review.update,
                        current_version: review.current_version,
                        current_schema_version: review.current_schema_version,
                        error: None,
                    });
                    pending_install = Some(staged);
                    Ok(json!({
                        "status":"pending_confirmation",
                        "app":descriptor.id,
                        "version":descriptor.version
                    }))
                })();
                if result.is_err() {
                    install_slot.store(false, Ordering::Release);
                }
                log::info!(
                    "diag update.host phase=submit_done elapsed_ms={} error={}",
                    submit_started.elapsed().as_millis(),
                    result.is_err()
                );
                log_runtime_checkpoint("update.host", "submit_done", &submit_started);
                result
            });
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
                                    if let Some(prompt) = args.get("prompt").and_then(Value::as_str)
                                    {
                                        queue_prompt(
                                            prompt.to_owned(),
                                            &mut messages,
                                            &mut busy,
                                            &mut agent_status,
                                            &mut pending_prompt,
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
                                    if install_ui
                                        .as_ref()
                                        .is_some_and(|ui| ui.state == "review") =>
                                {
                                    if let Some(ui) = &mut install_ui {
                                        let confirmation_started = Instant::now();
                                        log::info!(
                                            "diag update.host phase=confirmed app={} version={} update={}",
                                            ui.descriptor.id,
                                            ui.descriptor.version,
                                            ui.update
                                        );
                                        log_runtime_checkpoint(
                                            "update.host",
                                            "confirmed",
                                            &confirmation_started,
                                        );
                                        ui.state = "installing";
                                    }
                                    install_requested = true;
                                }
                                Some("apps.dismissInstall") => {
                                    if let Some(staged) = pending_install.take() {
                                        supervisor.record_app_dismissal(
                                            &staged.descriptor,
                                            install_ui.as_ref().is_some_and(|ui| ui.update),
                                        );
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
                                    let ssid =
                                        args.get("ssid").and_then(Value::as_str).unwrap_or("");
                                    let password =
                                        args.get("password").and_then(Value::as_str).unwrap_or("");
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
                    queue_prompt(
                        wake.prompt,
                        &mut messages,
                        &mut busy,
                        &mut agent_status,
                        &mut pending_prompt,
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

        if !busy && last_system_refresh.elapsed() >= Duration::from_secs(5) {
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
            if let Some(started) = post_install_started.as_ref() {
                log_runtime_checkpoint("update.host", "system_update_start", started);
            }
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
            if let Some(started) = post_install_started.as_ref() {
                log_runtime_checkpoint("update.host", "system_update_done", started);
            }
            system_dirty = false;
            redraw = true;
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
            if let Some(started) = post_install_started.as_ref() {
                log_runtime_checkpoint("update.host", "frame_render_start", started);
            }
            for event in supervisor.frame_render(redraw)? {
                let show_event = match &event {
                    AgentEvent::ResponseText(_) => H::SHOW_MODEL_PROGRESS,
                    _ => true,
                };
                match event {
                    AgentEvent::Ready => {
                        log::info!("Pi Agent System App ready with App Tool registry");
                        log_runtime_checkpoint("agent.turn", "ready", &boot_started);
                        if uart_poc {
                            unsafe {
                                esp_idf_svc::sys::esp_log_level_set(
                                    c"*".as_ptr(),
                                    esp_idf_svc::sys::esp_log_level_t_ESP_LOG_WARN,
                                );
                                // Log App transport stage boundaries without exposing response data.
                                esp_idf_svc::sys::esp_log_level_set(
                                    c"pocket_pi_esp32_common::app_services".as_ptr(),
                                    esp_idf_svc::sys::esp_log_level_t_ESP_LOG_INFO,
                                );
                            }
                        }
                        agent_status = "IDLE";
                        messages[0].text = format!("{} Pi Agent is ready.", H::BOARD_ID);
                    }
                    AgentEvent::ResponseText(text) => {
                        if let Some(turn) = active_agent_turn.as_mut() {
                            turn.response_bytes = turn.response_bytes.saturating_add(text.len());
                        }
                        if let Some(message) = messages.last_mut() {
                            message.text.push_str(&text);
                        }
                    }
                    AgentEvent::Done => {
                        log::info!("Pi Agent turn completed");
                        if let Some(turn) = active_agent_turn.take() {
                            log::info!(
                                "diag agent.turn phase=done turn_id={} elapsed_ms={} response_bytes={}",
                                turn.id,
                                turn.started.elapsed().as_millis(),
                                turn.response_bytes
                            );
                            log_runtime_checkpoint("agent.turn", "done", &turn.started);
                        } else {
                            log::warn!("diag agent.turn phase=done_without_active_turn");
                        }
                        agent_status = "IDLE";
                        busy = false;
                    }
                    AgentEvent::Failed(error) => {
                        log::error!("Pi Agent failed: {error}");
                        if let Some(turn) = active_agent_turn.take() {
                            log::warn!(
                                "diag agent.turn phase=failed turn_id={} elapsed_ms={} response_bytes={} error_bytes={}",
                                turn.id,
                                turn.started.elapsed().as_millis(),
                                turn.response_bytes,
                                error.len()
                            );
                            log_runtime_checkpoint("agent.turn", "failed", &turn.started);
                        } else {
                            log::warn!(
                                "diag agent.turn phase=failed_without_active_turn error_bytes={}",
                                error.len()
                            );
                        }
                        if let Some(message) = messages.last_mut() {
                            message.text = format!("Agent failed: {error}");
                        }
                        agent_status = "FAULTED";
                        busy = false;
                    }
                }
                if show_event {
                    system_dirty = true;
                }
            }
            if let Some(started) = post_install_started.as_ref() {
                log_runtime_checkpoint("update.host", "frame_render_done", started);
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
                queue_prompt(
                    prompt,
                    &mut messages,
                    &mut busy,
                    &mut agent_status,
                    &mut pending_prompt,
                );
                redraw = true;
                system_dirty = true;
            }
        }

        if redraw {
            if let Some(started) = post_install_started.as_ref() {
                log_runtime_checkpoint("update.host", "display_render_start", started);
            }
            if let Some(display) = display.as_mut() {
                display.render(&supervisor)?;
            }
            if let Some(started) = post_install_started.as_ref() {
                log_runtime_checkpoint("update.host", "display_render_done", started);
            }
            redraw = false;
            post_install_started = None;
        }

        if !touch_was_down && !system_dirty {
            if let Some(prompt) = pending_prompt.take() {
                let turn = AgentTurnDiagnostics {
                    id: next_agent_turn_id,
                    started: Instant::now(),
                    response_bytes: 0,
                };
                next_agent_turn_id = next_agent_turn_id.wrapping_add(1).max(1);
                log::info!(
                    "diag agent.turn phase=dispatch_start turn_id={} prompt_bytes={}",
                    turn.id,
                    prompt.len()
                );
                log_runtime_checkpoint("agent.turn", "dispatch_start", &turn.started);
                active_agent_turn = Some(turn);
                if let Err(error) = supervisor.prompt_agent(&prompt) {
                    if let Some(turn) = active_agent_turn.take() {
                        log::warn!(
                            "diag agent.turn phase=dispatch_failed turn_id={} elapsed_ms={} error_bytes={}",
                            turn.id,
                            turn.started.elapsed().as_millis(),
                            error.to_string().len()
                        );
                        log_runtime_checkpoint("agent.turn", "dispatch_failed", &turn.started);
                    }
                    messages.last_mut().unwrap().text = format!("Agent is unavailable: {error:#}");
                    busy = false;
                    agent_status = "FAULTED";
                    system_dirty = true;
                    redraw = true;
                } else if let Some(turn) = active_agent_turn.as_ref() {
                    log::info!(
                        "diag agent.turn phase=dispatched turn_id={} elapsed_ms={}",
                        turn.id,
                        turn.started.elapsed().as_millis()
                    );
                    log_runtime_checkpoint("agent.turn", "dispatched", &turn.started);
                }
            }
        }

        // The loading facts is rendered before activation enters QuickJS,
        // SQLite and flash. Touch and prompts remain locked by InstallScreen.
        if install_requested {
            install_requested = false;
            if let Some(staged) = pending_install.take() {
                let operation_started = Instant::now();
                let operation = if install_ui.as_ref().is_some_and(|ui| ui.update) {
                    "update"
                } else {
                    "install"
                };
                log::info!(
                    "diag update.host phase=apply_start operation={operation} app={} version={}",
                    staged.descriptor.id,
                    staged.descriptor.version
                );
                log_runtime_checkpoint("update.host", "apply_start", &operation_started);
                let cleanup = staged
                    .release_dir
                    .parent()
                    .map(std::path::Path::to_path_buf);
                let result = with_psram_pthread_config(ACTION_STACK_BYTES, 1, || {
                    supervisor.apply_app(&staged.release_dir, staged.credentials)
                });
                log::info!(
                    "diag update.host phase=apply_done operation={operation} elapsed_ms={} error={}",
                    operation_started.elapsed().as_millis(),
                    result.is_err()
                );
                log_runtime_checkpoint("update.host", "apply_done", &operation_started);
                match &result {
                    Ok(descriptor) => {
                        log::info!(
                            "App {operation} succeeded: {} {}",
                            descriptor.id,
                            descriptor.version
                        )
                    }
                    Err(error) => log::error!("App {operation} failed: {error:#}"),
                }
                if let Some(path) = cleanup {
                    log::info!("diag update.host phase=staging_cleanup_start");
                    let _ = std::fs::remove_dir_all(path);
                    log::info!(
                        "diag update.host phase=staging_cleanup_done elapsed_ms={}",
                        operation_started.elapsed().as_millis()
                    );
                    log_runtime_checkpoint(
                        "update.host",
                        "staging_cleanup_done",
                        &operation_started,
                    );
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
                log_runtime_checkpoint("update.host", "result_published", &operation_started);
                post_install_started = Some(operation_started);
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
            b"<form><h1>Pocket Pi App Package</h1><input type=file id=f><button type=button onclick=send()>Upload</button><script>function send(){fetch('/install',{method:'POST',body:f.files[0]}).then(r=>r.text()).then(alert)}</script></form>",
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
                        .write_all(b"uploaded; confirm on Pocket Pi")?;
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
    core: i32,
    action: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    // Large persistent workers use PSRAM stacks. App Action passes this setting
    // to its NET child; the caller's configuration is restored after each spawn.
    let default = unsafe { esp_idf_svc::sys::esp_pthread_get_default_config() };
    let mut previous = default;
    let current = unsafe { esp_idf_svc::sys::esp_pthread_get_cfg(&mut previous) };
    if current == esp_idf_svc::sys::ESP_ERR_NOT_FOUND {
        previous = default;
    } else if current != esp_idf_svc::sys::ESP_OK {
        anyhow::bail!("read pthread configuration: ESP-IDF error 0x{current:x}");
    }
    let mut config = previous;
    config.stack_size = stack_size;
    config.inherit_cfg = true;
    config.pin_to_core = core;
    config.stack_alloc_caps =
        esp_idf_svc::sys::MALLOC_CAP_SPIRAM | esp_idf_svc::sys::MALLOC_CAP_8BIT;
    let configured = unsafe { esp_idf_svc::sys::esp_pthread_set_cfg(&config) };
    if configured != esp_idf_svc::sys::ESP_OK {
        anyhow::bail!("configure PSRAM pthread: ESP-IDF error 0x{configured:x}");
    }
    let result = action();
    let restored = unsafe { esp_idf_svc::sys::esp_pthread_set_cfg(&previous) };
    if restored != esp_idf_svc::sys::ESP_OK {
        anyhow::bail!("restore pthread configuration: ESP-IDF error 0x{restored:x}");
    }
    result
}

fn queue_prompt(
    prompt: String,
    messages: &mut Vec<Message>,
    busy: &mut bool,
    agent_status: &mut &'static str,
    pending_prompt: &mut Option<String>,
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
    *pending_prompt = Some(prompt);
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
            "update":install.update,
            "name":install.descriptor.title,
            "version":install.descriptor.version,
            "currentVersion":install.current_version,
            "schemaVersion":install.descriptor.schema_version,
            "currentSchemaVersion":install.current_schema_version,
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
