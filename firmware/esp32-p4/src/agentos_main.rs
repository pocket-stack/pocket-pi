use core::time::Duration;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use pocket_pi_agentos::{
    AppServiceHost, AppSupervisor, RoutedToolHost, DATA_ACTION_STACK_BYTES,
};
use pocket_pi_embedded::{AgentEvent, ModelBackend, ToolHost};
use pocket_pi_protocols::model::ModelBackendSettings;
use pocket_pi_tools::CoreToolHost;
use pocketjs_esp32p4_ppa::{EspIdfPpaOps, RenderTargetState, Renderer};
use serde_json::{json, Value};

use super::{
    app_services::EspAppServices,
    backend,
    device_state::{SettingsProjection, WifiSettingsProjection},
    esp_result, init_wifi, storage, transport, DisplayProbe, EspPlatform,
    BOARD_NAME, PANEL_HEIGHT, PANEL_WIDTH,
};

#[derive(Clone)]
struct Message {
    role: &'static str,
    text: String,
}

pub fn run() -> anyhow::Result<()> {
    let _workspace = storage::mount_workspace()?;

    let uart = Arc::new(
        transport::UartLineTransport::new()
            .map_err(|error| anyhow::anyhow!("initialize UART transport: {error}"))?,
    );
    let runtime_config =
        match transport::request_runtime_config(uart.as_ref(), Duration::from_secs(5)) {
            Ok(config) => config,
            Err(error) => {
                log::warn!("No UART runtime config received: {error}");
                transport::RuntimeConfig::default()
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
            wifi.projection(if wifi.is_connecting() {
                "CONNECTING..."
            } else if wifi.is_connected() {
                "CONNECTED"
            } else {
                "NOT CONNECTED"
            })
        })
        .unwrap_or_else(|| SettingsProjection {
            firmware_version: env!("CARGO_PKG_VERSION").into(),
            workspace_free_bytes: storage::workspace_free_bytes().ok(),
            wifi: WifiSettingsProjection {
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
            let transport: Arc<dyn transport::LineTransport> = uart;
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
    let services: Arc<dyn AppServiceHost> = Arc::new(EspAppServices::new(
        network_ready.clone(),
        runtime_config.exa_api_key,
        runtime_config.robinhood_access_token,
    ));
    let mut supervisor = with_data_action_pthread_config(|| {
        AppSupervisor::new(storage::WORKSPACE_ROOT, services)
    })?;
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
        "systemPrompt":"You are Pi Agent, the first-class system App in Pocket Pi AgentOS on an ESP32-P4. You can manage the top-level /workspace and use installed App tools. Use /workspace for durable memory, notes, plans, and artifacts; read and update relevant files when continuity matters. Be concise. Robinhood data is owned by the Robinhood App; Exa research history is owned by the Exa App."
    });
    supervisor.boot_agent(&config.to_string(), backend, Arc::new(routed_tools))?;

    let mut messages = vec![Message {
        role: "assistant",
        text: "Pocket Pi AgentOS is starting. Robinhood and Exa are installed Apps.".into(),
    }];
    let mut agent_status = "STARTING";
    let mut busy = false;
    let mut initial_prompt = runtime_config.initial_prompt;
    let initial_prompt_not_before =
        Instant::now() + Duration::from_secs(runtime_config.initial_prompt_delay_seconds);
    let mut touch_was_down = false;
    let mut redraw = true;
    let mut projection_dirty = true;
    let mut pending_ui_task: Option<String> = None;
    let mut last_tick = Instant::now();
    let mut last_runtime_pump = Instant::now();
    let mut last_projection_refresh = Instant::now();
    let mut last_heartbeat = Instant::now();
    let mut last_wifi_poll = Instant::now();
    let mut last_wifi_connected = wifi.as_ref().is_some_and(|wifi| wifi.is_connected());

    loop {
        while let Ok(request) = app_rx.try_recv() {
            request.handle(&mut supervisor);
            redraw = true;
            projection_dirty = true;
        }

        if let Some(display) = display.as_mut() {
            if let Some((x, y)) = display.read_touch() {
                if !touch_was_down {
                    supervisor.pointer_down(x, y)?;
                    let action = supervisor.tap(x, y)?;
                    match action.get("type").and_then(Value::as_str) {
                        Some("navigate") => {
                            if let Some(app) = action.get("app").and_then(Value::as_str) {
                                supervisor.open(app)?;
                                projection_dirty = true;
                            }
                        }
                        Some("submitPrompt") => {
                            if let Some(prompt) = action.get("prompt").and_then(Value::as_str) {
                                submit_prompt(
                                    prompt.to_owned(),
                                    &supervisor,
                                    &mut messages,
                                    &mut busy,
                                    &mut agent_status,
                                );
                                projection_dirty = true;
                            }
                        }
                        Some("invokeTask") => {
                            if let Some(task) = action.get("task").and_then(Value::as_str) {
                                pending_ui_task = Some(task.to_owned());
                            }
                        }
                        Some("settings") => {
                            match action.get("command").and_then(Value::as_str) {
                                Some("scan") => match wifi.as_mut() {
                                    Some(wifi) => {
                                        settings.wifi.scanning = true;
                                        settings.wifi.status = "SCANNING...".into();
                                        match wifi.scan() {
                                            Ok(networks) => {
                                                settings = wifi.projection("");
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
                                Some("connect") => {
                                    let ssid =
                                        action.get("ssid").and_then(Value::as_str).unwrap_or("");
                                    let password = action
                                        .get("password")
                                        .and_then(Value::as_str)
                                        .unwrap_or("");
                                    match wifi.as_mut() {
                                        Some(wifi) => match wifi.begin_connect(ssid, password, true) {
                                            Ok(()) => {
                                                settings = wifi.projection("CONNECTING...");
                                                last_wifi_poll = Instant::now();
                                            }
                                            Err(error) => {
                                                settings.wifi.status =
                                                    format!("CONNECT FAILED: {error}")
                                            }
                                        },
                                        None => {
                                            settings.wifi.status = "WI-FI DRIVER UNAVAILABLE".into()
                                        }
                                    }
                                }
                                Some("forget") => match wifi.as_mut() {
                                    Some(wifi) => match wifi.forget() {
                                        Ok(()) => settings = wifi.projection("NETWORK FORGOTTEN"),
                                        Err(error) => {
                                            settings.wifi.status = format!("FORGET FAILED: {error}")
                                        }
                                    },
                                    None => {
                                        settings.wifi.status = "WI-FI DRIVER UNAVAILABLE".into()
                                    }
                                },
                                Some("restart") => unsafe {
                                    esp_idf_svc::sys::esp_restart();
                                },
                                _ => {}
                            }
                            settings.workspace_free_bytes = storage::workspace_free_bytes().ok();
                            projection_dirty = true;
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
                        settings = wifi.projection("CONNECTED");
                        projection_dirty = true;
                        redraw = true;
                        log::info!("Wi-Fi association and DHCP completed");
                    }
                    Some(Err(error)) => {
                        settings = wifi.projection(format!("CONNECT FAILED: {error}"));
                        projection_dirty = true;
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
                    settings = wifi.projection(if connected {
                        "CONNECTED"
                    } else {
                        "NOT CONNECTED"
                    });
                    settings.wifi.networks = networks;
                    projection_dirty = true;
                    redraw = true;
                }
                log::info!(
                    "Wi-Fi link state changed: {}",
                    if connected { "connected" } else { "disconnected" }
                );
            }
        }

        if last_tick.elapsed() >= Duration::from_secs(1) {
            if !busy && agent_status == "IDLE" && initial_prompt.is_none() {
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
                    supervisor.poll_due_tasks()
                };
                for (task, result) in &app_results {
                    log::info!("AppTask {task}: {}", result.text);
                }
                if !app_results.is_empty() {
                    redraw = true;
                }
            }
            last_tick = Instant::now();
        }

        if last_projection_refresh.elapsed() >= Duration::from_secs(5) {
            if supervisor.active_id() == pocket_pi_agentos::ROOT_APP_ID {
                projection_dirty = true;
                redraw = true;
            }
            last_projection_refresh = Instant::now();
        }

        if projection_dirty {
            let projection = root_projection(
                &messages,
                agent_status,
                provider,
                &resolved_model,
                &settings,
                native_tools.as_ref(),
            );
            supervisor.update_root(&projection)?;
            projection_dirty = false;
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
                projection_dirty = true;
            }
            last_runtime_pump = Instant::now();
        }

        if agent_status == "IDLE"
            && !busy
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
                projection_dirty = true;
            }
        }

        if redraw {
            if let Some(display) = display.as_mut() {
                display.render_agentos(&mut renderer, &mut ppa, &supervisor)?;
            }
            redraw = false;
        }

        // A UI-requested task runs only after its loading state has reached the
        // panel. This preserves immediate touch feedback even when HTTPS is slow.
        if !busy {
            if let Some(task) = pending_ui_task.take() {
                let started = Instant::now();
                let result = supervisor.invoke_active_task(&task, &Value::Null);
                log::info!(
                    "UI AppTask {} finished in {}ms: {}",
                    task,
                    started.elapsed().as_millis(),
                    result.text
                );
                supervisor.frame()?;
                redraw = true;
            }
        }

        if last_heartbeat.elapsed() >= Duration::from_secs(5) {
            log::info!(
                "heartbeat app_data_action={} agent={} app={}",
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
        // AgentOS is a native FreeRTOS task, not a pthread. The firmware uses
        // a 100 Hz tick, so one tick polls touch every 10 ms while still
        // giving CPU0's idle task a watchdog-feeding scheduling window.
        unsafe { esp_idf_svc::sys::vTaskDelay(1) };
    }
}

fn with_data_action_pthread_config<T>(
    action: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    // std::thread maps to ESP-IDF pthread. Its default stack allocation is
    // internal RAM, where a QuickJS Data Action Guest cannot fit. Configure
    // this worker family to use byte-addressable PSRAM. The Data Action thread
    // must pass the allocation caps to its on-demand NET child; the platform
    // default is restored before unrelated System App threads are created.
    let default = unsafe { esp_idf_svc::sys::esp_pthread_get_default_config() };
    let mut config = default;
    config.stack_size = DATA_ACTION_STACK_BYTES;
    config.inherit_cfg = true;
    config.pin_to_core = 1;
    config.stack_alloc_caps =
        esp_idf_svc::sys::MALLOC_CAP_SPIRAM | esp_idf_svc::sys::MALLOC_CAP_8BIT;
    let configured = unsafe { esp_idf_svc::sys::esp_pthread_set_cfg(&config) };
    if configured != esp_idf_svc::sys::ESP_OK {
        anyhow::bail!("configure App Data Action pthread: ESP-IDF error 0x{configured:x}");
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

fn root_projection(
    messages: &[Message],
    agent_status: &str,
    provider: &str,
    model: &str,
    settings: &SettingsProjection,
    tools: &CoreToolHost,
) -> Value {
    let schedule = tools.schedule_projection();
    let schedule_text = match schedule.next_in_seconds {
        Some(seconds) => schedule.every_minutes.map_or_else(
            || format!("in {seconds}s"),
            |minutes| format!("in {seconds}s · every {minutes}m"),
        ),
        None => "not scheduled".to_owned(),
    };
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
