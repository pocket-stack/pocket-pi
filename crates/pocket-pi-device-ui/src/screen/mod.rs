#![allow(
    clippy::if_same_then_else,
    clippy::manual_div_ceil,
    clippy::too_many_arguments
)]

mod font;
mod workspace_browser;

use std::collections::VecDeque;

use crate::model::{
    AgentState, DeviceState, ModelBackendSettings, ModelSettings, ScheduleProjection,
    SettingsCommand, SettingsProjection, UartProvider, WifiNetworkProjection, WirelessProvider,
};
use pocketjs_core::{spec, Ui};
use workspace_browser::{format_size, format_timestamp, WorkspaceBrowser};

pub fn load_fonts(ui: &mut Ui) -> bool {
    font::load(ui)
}

const PANEL_WIDTH: u16 = 720;
const PANEL_HEIGHT: u16 = 1280;
const HEADER_HEIGHT: i16 = 112;
const BOTTOM_BAR_Y: i16 = 1172;
const FILE_ROW_START_Y: i16 = 190;
const FILE_ROW_HEIGHT: i16 = 108;
const FILE_VISIBLE_ROWS: usize = 8;
const VIEWER_VISIBLE_LINES: usize = 39;
const MESSAGE_VISIBLE_LINES: usize = 39;
const MAX_CHAT_TURNS: usize = 10;
const CHAT_VISIBLE_TURNS: usize = 2;
const WIFI_ROW_START_Y: u16 = 330;
const WIFI_ROW_HEIGHT: u16 = 92;
const WIFI_VISIBLE_ROWS: usize = 5;
const COMPOSE_Y: u16 = 1070;
const MAX_PROMPT_BYTES: usize = 256;
const PENDING_ASSISTANT: &str = "THINKING...";

// PocketJS draw-list colors are packed ABGR, not ARGB.
const UI_ACCENT_GREEN: u32 = 0xff3b_d158; // RGB #58D13B
const UI_LOSS_RED: u32 = 0xff44_44ef; // RGB #EF4444

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScreenView {
    Chat,
    Files,
    Settings,
    Viewer,
    MessageReader,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScreenInteraction {
    None,
    Redraw,
    SubmitPrompt(String),
    Settings(SettingsCommand),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeyboardMode {
    Letters,
    Numbers,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum KeyboardPurpose {
    Prompt,
    WifiPassword { ssid: String },
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemTelemetry {
    pub psram_used_percent: u8,
    pub psram_free_bytes: usize,
    pub cpu_percent: Option<u8>,
    pub ui_fps_tenths: u16,
    pub lcd_refresh_hz: u16,
}

#[derive(Clone, Debug)]
struct BackendProjection {
    model: String,
    link: String,
    auth: String,
}

impl Default for BackendProjection {
    fn default() -> Self {
        Self {
            model: "CODEX".to_owned(),
            link: "UART / MAC".to_owned(),
            auth: "CODING PLAN".to_owned(),
        }
    }
}

impl BackendProjection {
    fn from_settings(settings: &ModelSettings) -> Self {
        match &settings.backend {
            ModelBackendSettings::Uart { provider } => match provider {
                UartProvider::Codex => Self::default(),
                UartProvider::ClaudeCode => Self {
                    model: "CLAUDE CODE".to_owned(),
                    link: "UART / MAC".to_owned(),
                    auth: "CLAUDE LOGIN".to_owned(),
                },
            },
            ModelBackendSettings::Wireless { provider } => Self {
                model: match provider {
                    WirelessProvider::OpenAi => "OPENAI API",
                    WirelessProvider::OpenRouter => "OPENROUTER",
                    WirelessProvider::Anthropic => "ANTHROPIC API",
                }
                .to_owned(),
                link: "WIFI / DIRECT".to_owned(),
                auth: "API KEY / RAM".to_owned(),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChatTurn {
    pub user: String,
    pub assistant: String,
    pending: bool,
}

#[derive(Debug)]
pub struct ChatProjection {
    turns: VecDeque<ChatTurn>,
}

#[derive(Debug)]
struct MessageReader {
    author: &'static str,
    lines: Vec<String>,
    line_offset: usize,
}

impl ChatProjection {
    pub fn new(user: impl Into<String>, assistant: impl Into<String>) -> Self {
        let mut turns = VecDeque::new();
        turns.push_back(ChatTurn {
            user: user.into(),
            assistant: assistant.into(),
            pending: false,
        });
        Self { turns }
    }

    pub fn push_turn(&mut self, user: impl Into<String>, assistant: impl Into<String>) {
        self.turns.push_back(ChatTurn {
            user: user.into(),
            assistant: assistant.into(),
            pending: false,
        });
        while self.turns.len() > MAX_CHAT_TURNS {
            self.turns.pop_front();
        }
    }

    pub fn set_latest_assistant(&mut self, assistant: impl Into<String>) {
        if let Some(turn) = self.turns.back_mut() {
            turn.assistant = assistant.into();
            turn.pending = false;
        }
    }

    pub fn push_pending(&mut self, user: impl Into<String>) {
        let user = user.into();
        if self
            .turns
            .back()
            .is_some_and(|turn| turn.pending && turn.user == user)
        {
            return;
        }
        self.turns.push_back(ChatTurn {
            user,
            assistant: PENDING_ASSISTANT.to_owned(),
            pending: true,
        });
        while self.turns.len() > MAX_CHAT_TURNS {
            self.turns.pop_front();
        }
    }

    pub fn append_model_delta(&mut self, delta: &str) -> bool {
        let Some(turn) = self.turns.back_mut().filter(|turn| turn.pending) else {
            return false;
        };
        if turn.assistant == PENDING_ASSISTANT {
            turn.assistant.clear();
        }
        turn.assistant.push_str(delta);
        true
    }

    pub fn finish_pending(&mut self) {
        if let Some(turn) = self.turns.back_mut().filter(|turn| turn.pending) {
            if turn.assistant.is_empty() || turn.assistant == PENDING_ASSISTANT {
                turn.assistant = "TURN COMPLETE".to_owned();
            }
            turn.pending = false;
        }
    }

    pub fn fail_pending(&mut self, error: impl Into<String>) {
        let error = error.into();
        if let Some(turn) = self.turns.back_mut() {
            if turn.pending {
                turn.assistant = error;
                turn.pending = false;
                return;
            }
        }
        self.push_turn("SYSTEM", error);
    }
}

#[derive(Debug)]
pub struct ScreenState {
    pub view: ScreenView,
    pub browser: WorkspaceBrowser,
    pub telemetry: SystemTelemetry,
    schedule: ScheduleProjection,
    chat_scroll: usize,
    wifi_scroll: usize,
    keyboard_open: bool,
    keyboard_mode: KeyboardMode,
    keyboard_uppercase: bool,
    keyboard_purpose: KeyboardPurpose,
    keyboard_input: String,
    pressed_key: Option<String>,
    backend: BackendProjection,
    settings: SettingsProjection,
    message_reader: Option<MessageReader>,
}

impl ScreenState {
    pub fn new(workspace_root: &str) -> Self {
        Self {
            view: ScreenView::Chat,
            browser: WorkspaceBrowser::new(workspace_root),
            telemetry: SystemTelemetry::default(),
            schedule: ScheduleProjection::default(),
            chat_scroll: 0,
            wifi_scroll: 0,
            keyboard_open: false,
            keyboard_mode: KeyboardMode::Letters,
            keyboard_uppercase: false,
            keyboard_purpose: KeyboardPurpose::Prompt,
            keyboard_input: String::new(),
            pressed_key: None,
            backend: BackendProjection::default(),
            settings: SettingsProjection::default(),
            message_reader: None,
        }
    }

    pub fn set_telemetry(&mut self, telemetry: SystemTelemetry) {
        self.telemetry = telemetry;
    }

    pub fn set_schedule(&mut self, schedule: ScheduleProjection) {
        self.schedule = schedule;
    }

    pub fn set_model_backend(&mut self, settings: &ModelSettings) {
        self.backend = BackendProjection::from_settings(settings);
    }

    pub fn set_backend_status(&mut self, model: &str, link: &str, auth: &str) {
        self.backend = BackendProjection {
            model: model.to_owned(),
            link: link.to_owned(),
            auth: auth.to_owned(),
        };
    }

    pub fn set_settings(&mut self, settings: SettingsProjection) {
        self.wifi_scroll = self.wifi_scroll.min(
            settings
                .wifi
                .networks
                .len()
                .saturating_sub(WIFI_VISIBLE_ROWS),
        );
        self.settings = settings;
    }

    pub fn handle_touch_release(&mut self) -> bool {
        self.pressed_key.take().is_some() && self.keyboard_open
    }

    pub fn refresh_workspace(&mut self) {
        self.browser.refresh();
    }

    pub fn show_latest_chat(&mut self) {
        self.chat_scroll = 0;
    }

    pub fn handle_tap(
        &mut self,
        x: u16,
        y: u16,
        chat: &ChatProjection,
        ui: &Ui,
    ) -> ScreenInteraction {
        if self.keyboard_open {
            return self.handle_keyboard_tap(x, y);
        }
        if !matches!(self.view, ScreenView::Viewer | ScreenView::MessageReader)
            && y as i16 >= BOTTOM_BAR_Y
        {
            let next = match x {
                0..=239 => ScreenView::Chat,
                240..=479 => {
                    self.browser.refresh();
                    ScreenView::Files
                }
                _ => ScreenView::Settings,
            };
            let changed = self.view != next;
            self.view = next;
            self.keyboard_open = false;
            self.pressed_key = None;
            return if changed {
                ScreenInteraction::Redraw
            } else {
                ScreenInteraction::None
            };
        }
        let changed = match self.view {
            ScreenView::Chat => self.handle_chat_tap(x, y, chat, ui),
            ScreenView::Files => self.handle_files_tap(x, y),
            ScreenView::Settings => return self.handle_settings_tap(x, y),
            ScreenView::Viewer => self.handle_viewer_tap(x, y),
            ScreenView::MessageReader => self.handle_message_reader_tap(x, y),
        };
        if changed {
            ScreenInteraction::Redraw
        } else {
            ScreenInteraction::None
        }
    }

    fn handle_chat_tap(&mut self, x: u16, y: u16, chat: &ChatProjection, ui: &Ui) -> bool {
        if (24..=696).contains(&x) && (COMPOSE_Y..=1150).contains(&y) {
            self.keyboard_open = true;
            self.keyboard_purpose = KeyboardPurpose::Prompt;
            self.keyboard_uppercase = false;
            self.keyboard_input.clear();
            self.pressed_key = None;
            return true;
        }
        if x >= 620 && (140..=272).contains(&y) {
            self.chat_scroll = self.chat_scroll.saturating_add(CHAT_VISIBLE_TURNS);
            return true;
        }
        if x >= 620 && (610..=790).contains(&y) {
            let before = self.chat_scroll;
            self.chat_scroll = self.chat_scroll.saturating_sub(CHAT_VISIBLE_TURNS);
            return before != self.chat_scroll;
        }
        if (24..610).contains(&x) {
            let len = chat.turns.len();
            let scroll = self.chat_scroll.min(len.saturating_sub(CHAT_VISIBLE_TURNS));
            let end = len.saturating_sub(scroll);
            let start = end.saturating_sub(CHAT_VISIBLE_TURNS);
            for (row, turn) in chat.turns.iter().skip(start).take(end - start).enumerate() {
                let top = 140 + row as u16 * 320;
                if (top..top + 150).contains(&y) {
                    self.open_message(ui, "YOU", &turn.user);
                    return true;
                }
                if (top + 150..top + 298).contains(&y) {
                    self.open_message(ui, "PI", &turn.assistant);
                    return true;
                }
            }
        }
        false
    }

    fn open_message(&mut self, ui: &Ui, author: &'static str, text: &str) {
        self.message_reader = Some(MessageReader {
            author,
            lines: font::wrap_text(ui, text, 32, font::TextStyle::Body),
            line_offset: 0,
        });
        self.view = ScreenView::MessageReader;
    }

    fn handle_message_reader_tap(&mut self, x: u16, y: u16) -> bool {
        if x < 104 && y < HEADER_HEIGHT as u16 {
            self.message_reader = None;
            self.view = ScreenView::Chat;
            return true;
        }
        let Some(reader) = self.message_reader.as_mut() else {
            self.view = ScreenView::Chat;
            return true;
        };
        if x >= 620 && (170..=340).contains(&y) {
            let before = reader.line_offset;
            reader.line_offset = reader.line_offset.saturating_sub(18);
            return before != reader.line_offset;
        }
        if x >= 620 && (920..=1100).contains(&y) {
            let max_scroll = reader.lines.len().saturating_sub(MESSAGE_VISIBLE_LINES);
            let before = reader.line_offset;
            reader.line_offset = (reader.line_offset + 18).min(max_scroll);
            return before != reader.line_offset;
        }
        false
    }

    fn handle_keyboard_tap(&mut self, x: u16, y: u16) -> ScreenInteraction {
        let max_input_bytes = match &self.keyboard_purpose {
            KeyboardPurpose::Prompt => MAX_PROMPT_BYTES,
            KeyboardPurpose::WifiPassword { .. } => 63,
        };
        if (24..=696).contains(&x) && (1164..=1279).contains(&y) {
            self.keyboard_open = false;
            self.keyboard_input.clear();
            self.keyboard_uppercase = false;
            self.pressed_key = None;
            return ScreenInteraction::Redraw;
        }
        if (548..=680).contains(&x) && (360..=416).contains(&y) {
            self.pressed_key = Some("CLEAR".to_owned());
            self.keyboard_input.clear();
            return ScreenInteraction::Redraw;
        }

        let rows = match self.keyboard_mode {
            KeyboardMode::Letters => ["qwertyuiop", "asdfghjkl", "zxcvbnm"],
            KeyboardMode::Numbers => ["1234567890", "-/:;()$&@", ".,?!'\"+"],
        };
        let character = if (488..=608).contains(&y) {
            row_character(x, 24, 60, 8, rows[0])
        } else if (628..=748).contains(&y) {
            row_character(x, 31, 66, 8, rows[1])
        } else if (768..=888).contains(&y) {
            row_character(x, 24, 72, 8, rows[2])
        } else {
            None
        };
        if let Some(character) = character {
            self.pressed_key = Some(character.to_ascii_uppercase().to_string());
            if self.keyboard_input.len() < max_input_bytes {
                self.keyboard_input.push(
                    if self.keyboard_mode == KeyboardMode::Letters && self.keyboard_uppercase {
                        character.to_ascii_uppercase()
                    } else {
                        character
                    },
                );
            }
            return ScreenInteraction::Redraw;
        }
        if (592..=696).contains(&x) && (768..=888).contains(&y) {
            self.pressed_key = Some("DEL".to_owned());
            self.keyboard_input.pop();
            return ScreenInteraction::Redraw;
        }
        if (908..=1064).contains(&y) {
            match x {
                24..=116 => {
                    self.pressed_key = Some(
                        if self.keyboard_mode == KeyboardMode::Letters {
                            "123"
                        } else {
                            "ABC"
                        }
                        .to_owned(),
                    );
                    self.keyboard_mode = match self.keyboard_mode {
                        KeyboardMode::Letters => KeyboardMode::Numbers,
                        KeyboardMode::Numbers => KeyboardMode::Letters,
                    };
                    self.keyboard_uppercase = false;
                    return ScreenInteraction::Redraw;
                }
                124..=424 => {
                    self.pressed_key = Some("SPACE".to_owned());
                    if !self.keyboard_input.is_empty()
                        && self.keyboard_input.len() < max_input_bytes
                    {
                        self.keyboard_input.push(' ');
                    }
                    return ScreenInteraction::Redraw;
                }
                432..=500 => {
                    if self.keyboard_mode == KeyboardMode::Letters {
                        self.keyboard_uppercase = !self.keyboard_uppercase;
                        self.pressed_key = Some("SHIFT".to_owned());
                        return ScreenInteraction::Redraw;
                    }
                    self.pressed_key = Some(".".to_owned());
                    if self.keyboard_input.len() < max_input_bytes {
                        self.keyboard_input.push('.');
                    }
                    return ScreenInteraction::Redraw;
                }
                508..=576 => {
                    if self.keyboard_mode == KeyboardMode::Letters {
                        self.keyboard_uppercase = !self.keyboard_uppercase;
                        self.pressed_key = Some("SHIFT".to_owned());
                        return ScreenInteraction::Redraw;
                    }
                    self.pressed_key = Some("?".to_owned());
                    if self.keyboard_input.len() < max_input_bytes {
                        self.keyboard_input.push('?');
                    }
                    return ScreenInteraction::Redraw;
                }
                584..=696 => {
                    let value = self.keyboard_input.trim().to_owned();
                    if value.is_empty() {
                        return ScreenInteraction::None;
                    }
                    let purpose = self.keyboard_purpose.clone();
                    self.keyboard_input.clear();
                    self.keyboard_open = false;
                    self.pressed_key = None;
                    self.keyboard_mode = KeyboardMode::Letters;
                    self.keyboard_uppercase = false;
                    return match purpose {
                        KeyboardPurpose::Prompt => ScreenInteraction::SubmitPrompt(value),
                        KeyboardPurpose::WifiPassword { ssid } => {
                            ScreenInteraction::Settings(SettingsCommand::ConnectWifi {
                                ssid,
                                password: value,
                            })
                        }
                    };
                }
                _ => {}
            }
        }
        ScreenInteraction::None
    }

    fn handle_settings_tap(&mut self, x: u16, y: u16) -> ScreenInteraction {
        if (480..=696).contains(&x) && (142..=218).contains(&y) {
            return ScreenInteraction::Settings(SettingsCommand::ScanWifi);
        }
        if x >= 620 && (330..=462).contains(&y) {
            let before = self.wifi_scroll;
            self.wifi_scroll = self.wifi_scroll.saturating_sub(WIFI_VISIBLE_ROWS - 1);
            return if before == self.wifi_scroll {
                ScreenInteraction::None
            } else {
                ScreenInteraction::Redraw
            };
        }
        if x >= 620 && (650..=782).contains(&y) {
            let max_scroll = self
                .settings
                .wifi
                .networks
                .len()
                .saturating_sub(WIFI_VISIBLE_ROWS);
            let before = self.wifi_scroll;
            self.wifi_scroll = (self.wifi_scroll + WIFI_VISIBLE_ROWS - 1).min(max_scroll);
            return if before == self.wifi_scroll {
                ScreenInteraction::None
            } else {
                ScreenInteraction::Redraw
            };
        }
        if x < 610 && (WIFI_ROW_START_Y..790).contains(&y) {
            let row = ((y - WIFI_ROW_START_Y) / WIFI_ROW_HEIGHT) as usize;
            let row_top = WIFI_ROW_START_Y + row as u16 * WIFI_ROW_HEIGHT;
            if y >= row_top + 84 {
                return ScreenInteraction::None;
            }
            if let Some(network) = self.settings.wifi.networks.get(self.wifi_scroll + row) {
                if !network.secured {
                    return ScreenInteraction::Settings(SettingsCommand::ConnectWifi {
                        ssid: network.ssid.clone(),
                        password: String::new(),
                    });
                }
                self.keyboard_open = true;
                self.keyboard_mode = KeyboardMode::Letters;
                self.keyboard_uppercase = false;
                self.keyboard_purpose = KeyboardPurpose::WifiPassword {
                    ssid: network.ssid.clone(),
                };
                self.keyboard_input.clear();
                self.pressed_key = None;
                return ScreenInteraction::Redraw;
            }
        }
        if (24..=340).contains(&x) && (1010..=1090).contains(&y) {
            return ScreenInteraction::Settings(SettingsCommand::ForgetWifi);
        }
        if (356..=696).contains(&x) && (1010..=1090).contains(&y) {
            return ScreenInteraction::Settings(SettingsCommand::Restart);
        }
        ScreenInteraction::None
    }

    fn handle_files_tap(&mut self, x: u16, y: u16) -> bool {
        if x < 96 && y < HEADER_HEIGHT as u16 && self.browser.can_go_up() {
            self.browser.go_up();
            return true;
        }
        if x >= 620 && (170..=340).contains(&y) {
            self.browser.scroll_list(-4, FILE_VISIBLE_ROWS);
            return true;
        }
        if x >= 620 && (920..=1100).contains(&y) {
            self.browser.scroll_list(4, FILE_VISIBLE_ROWS);
            return true;
        }
        if x < 610 && y as i16 >= FILE_ROW_START_Y {
            let row = ((y as i16 - FILE_ROW_START_Y) / FILE_ROW_HEIGHT) as usize;
            if row < FILE_VISIBLE_ROWS && self.browser.activate_visible_row(row) {
                if self.browser.open_file.is_some() {
                    self.view = ScreenView::Viewer;
                }
                return true;
            }
        }
        false
    }

    fn handle_viewer_tap(&mut self, x: u16, y: u16) -> bool {
        if x < 104 && y < HEADER_HEIGHT as u16 {
            self.browser.close_file();
            self.view = ScreenView::Files;
            return true;
        }
        if x >= 620 && (170..=340).contains(&y) {
            self.browser.scroll_file(-18, VIEWER_VISIBLE_LINES);
            return true;
        }
        if x >= 620 && (920..=1100).contains(&y) {
            self.browser.scroll_file(18, VIEWER_VISIBLE_LINES);
            return true;
        }
        false
    }

    pub fn draw_list(&self, ui: &Ui, state: &DeviceState, chat: &ChatProjection) -> Vec<u32> {
        match self.view {
            _ if self.keyboard_open => keyboard_draw_list(
                ui,
                state,
                &self.keyboard_input,
                self.keyboard_mode,
                self.keyboard_uppercase,
                self.pressed_key.as_deref(),
                &self.keyboard_purpose,
                self.telemetry,
            ),
            ScreenView::Chat => chat_draw_list(
                ui,
                state,
                chat,
                self.chat_scroll,
                &self.schedule,
                self.telemetry,
            ),
            ScreenView::Files => files_draw_list(ui, state, &self.browser, self.telemetry),
            ScreenView::Settings => settings_draw_list(
                ui,
                state,
                &self.settings,
                self.wifi_scroll,
                &self.backend,
                self.telemetry,
            ),
            ScreenView::Viewer => viewer_draw_list(ui, state, &self.browser, self.telemetry),
            ScreenView::MessageReader => {
                message_reader_draw_list(ui, state, self.message_reader.as_ref(), self.telemetry)
            }
        }
    }
}

fn base_words(ui: &Ui, state: &DeviceState, title: &str, telemetry: SystemTelemetry) -> Vec<u32> {
    let mut words = Vec::new();
    rect(&mut words, 0, 0, PANEL_WIDTH, PANEL_HEIGHT, 0xfff1_f5f9);
    rect(
        &mut words,
        0,
        0,
        PANEL_WIDTH,
        HEADER_HEIGHT as u16,
        0xff0f_172a,
    );
    status_header(
        ui,
        &mut words,
        state,
        title,
        telemetry,
        0xffff_ffff,
        0xffcb_d5e1,
        0xff94_a3b8,
    );
    words
}

#[allow(clippy::too_many_arguments)]
fn status_header(
    ui: &Ui,
    words: &mut Vec<u32>,
    state: &DeviceState,
    title: &str,
    telemetry: SystemTelemetry,
    title_color: u32,
    primary_status_color: u32,
    secondary_status_color: u32,
) {
    const RIGHT_EDGE: i16 = 696;
    rect(words, 24, 38, 36, 36, agent_state_color(state.agent));
    push_title(ui, words, title, 78, 40, 22, title_color);
    let cpu = telemetry
        .cpu_percent
        .map(|value| format!("{value:02}%"))
        .unwrap_or_else(|| "--".to_owned());
    let free_megabytes = telemetry.psram_free_bytes as f32 / (1024.0 * 1024.0);
    let memory = format!(
        "PSRAM {:02}%  FREE {free_megabytes:.1}M",
        telemetry.psram_used_percent
    );
    let runtime = format!(
        "CPU {cpu}  UI {}.{}FPS  LCD {}HZ",
        telemetry.ui_fps_tenths / 10,
        telemetry.ui_fps_tenths % 10,
        telemetry.lcd_refresh_hz,
    );
    push_text_right(ui, words, &memory, RIGHT_EDGE, 25, primary_status_color);
    push_text_right(ui, words, &runtime, RIGHT_EDGE, 61, secondary_status_color);
}

fn chat_draw_list(
    ui: &Ui,
    state: &DeviceState,
    chat: &ChatProjection,
    scroll: usize,
    schedule: &ScheduleProjection,
    telemetry: SystemTelemetry,
) -> Vec<u32> {
    let mut words = base_words(ui, state, "ESP32 PI AGENT", telemetry);
    let len = chat.turns.len();
    let max_scroll = len.saturating_sub(CHAT_VISIBLE_TURNS);
    let scroll = scroll.min(max_scroll);
    let end = len.saturating_sub(scroll);
    let start = end.saturating_sub(CHAT_VISIBLE_TURNS);
    for (row, turn) in chat.turns.iter().skip(start).take(end - start).enumerate() {
        let y = 140 + row as i16 * 320;
        rect(&mut words, 24, y, 584, 298, 0xffff_ffff);
        rect(&mut words, 40, y + 18, 82, 34, 0xffdb_eafe);
        push_text_bold(ui, &mut words, "YOU", 54, y + 27, 8, 0xff1d_4ed8);
        push_text_limited(ui, &mut words, &turn.user, 42, y + 70, 34, 4, 0xff0f_172a);
        rect(&mut words, 40, y + 160, 62, 34, 0xffdc_fce7);
        push_text_bold(ui, &mut words, "PI", 56, y + 169, 8, 0xff04_7a55);
        push_text_limited(
            ui,
            &mut words,
            &turn.assistant,
            42,
            y + 210,
            34,
            4,
            0xff0f_172a,
        );
    }
    chat_scroll_buttons(ui, &mut words);
    schedule_panel(ui, &mut words, schedule);
    compose_button(ui, &mut words);
    bottom_bar(ui, &mut words, ScreenView::Chat);
    words
}

fn schedule_panel(ui: &Ui, words: &mut Vec<u32>, schedule: &ScheduleProjection) {
    rect(words, 24, 798, 672, 252, 0xffe2_e8f0);
    push_text_bold(ui, words, "NEXT WAKE", 44, 822, 28, 0xff33_4155);
    if let (Some(name), Some(seconds)) = (&schedule.name, schedule.next_in_seconds) {
        let remaining = if seconds < 60 {
            "IN <1 MIN".to_owned()
        } else if seconds < 3600 {
            format!("IN {} MIN", (seconds + 59) / 60)
        } else {
            format!("IN {}H {}M", seconds / 3600, (seconds % 3600) / 60)
        };
        let cadence = schedule
            .every_minutes
            .map(|minutes| format!("EVERY {minutes}M  "))
            .unwrap_or_default();
        let time = format!("{name}  {cadence}{remaining}");
        push_text_bold(ui, words, &time, 44, 870, 30, 0xff0f_172a);
        rect(words, 42, 914, 636, 112, 0xffff_ffff);
        push_text_limited(ui, words, &schedule.prompt, 58, 934, 36, 4, 0xff33_4155);
    } else {
        push_text_bold(ui, words, "NO WAKE SCHEDULED", 44, 884, 34, 0xff64_748b);
        push_text_limited(
            ui,
            words,
            "ASK PI TO CREATE ONE WITH SCHEDULE.SET",
            44,
            938,
            38,
            3,
            0xff64_748b,
        );
    }
}

fn compose_button(ui: &Ui, words: &mut Vec<u32>) {
    rect(words, 24, COMPOSE_Y as i16, 672, 80, 0xff25_63eb);
    push_text_bold(
        ui,
        words,
        "TYPE A MESSAGE",
        238,
        COMPOSE_Y as i16 + 27,
        18,
        0xffff_ffff,
    );
}

fn keyboard_draw_list(
    ui: &Ui,
    state: &DeviceState,
    input: &str,
    mode: KeyboardMode,
    uppercase: bool,
    pressed_key: Option<&str>,
    purpose: &KeyboardPurpose,
    telemetry: SystemTelemetry,
) -> Vec<u32> {
    let title = match purpose {
        KeyboardPurpose::Prompt => "NEW MESSAGE",
        KeyboardPurpose::WifiPassword { .. } => "WIFI PASSWORD",
    };
    let mut words = base_words(ui, state, title, telemetry);
    let display = match purpose {
        KeyboardPurpose::Prompt => prompt_tail(input, MAX_PROMPT_BYTES).to_owned(),
        KeyboardPurpose::WifiPassword { .. } => "*".repeat(input.len()),
    };
    rect(&mut words, 24, 132, 672, 300, 0xffff_ffff);
    push_text_limited(
        ui,
        &mut words,
        &display,
        42,
        154,
        29,
        9,
        if input.is_empty() {
            0xff94_a3b8
        } else {
            0xff0f_172a
        },
    );
    if input.is_empty() {
        push_text_limited(
            ui,
            &mut words,
            match purpose {
                KeyboardPurpose::Prompt => "TYPE YOUR MESSAGE...",
                KeyboardPurpose::WifiPassword { .. } => "ENTER NETWORK PASSWORD...",
            },
            42,
            154,
            29,
            2,
            0xff94_a3b8,
        );
    }
    draw_key(
        ui,
        &mut words,
        "CLEAR",
        548,
        360,
        132,
        56,
        0xffe2_e8f0,
        pressed_key == Some("CLEAR"),
    );
    push_text(
        ui,
        &mut words,
        &format!(
            "{} / {} ASCII BYTES",
            input.len(),
            match purpose {
                KeyboardPurpose::Prompt => MAX_PROMPT_BYTES,
                KeyboardPurpose::WifiPassword { .. } => 63,
            }
        ),
        30,
        442,
        34,
        0xff64_748b,
    );

    let rows = match mode {
        KeyboardMode::Letters => ["qwertyuiop", "asdfghjkl", "zxcvbnm"],
        KeyboardMode::Numbers => ["1234567890", "-/:;()$&@", ".,?!'\"+"],
    };
    draw_character_row(ui, &mut words, rows[0], 24, 488, 60, 8, pressed_key);
    draw_character_row(ui, &mut words, rows[1], 31, 628, 66, 8, pressed_key);
    draw_character_row(ui, &mut words, rows[2], 24, 768, 72, 8, pressed_key);
    draw_key(
        ui,
        &mut words,
        "DEL",
        592,
        768,
        104,
        120,
        0xffe2_e8f0,
        pressed_key == Some("DEL"),
    );

    draw_key(
        ui,
        &mut words,
        if mode == KeyboardMode::Letters {
            "123"
        } else {
            "ABC"
        },
        24,
        908,
        92,
        156,
        0xffe2_e8f0,
        pressed_key
            == Some(if mode == KeyboardMode::Letters {
                "123"
            } else {
                "ABC"
            }),
    );
    draw_key(
        ui,
        &mut words,
        "SPACE",
        124,
        908,
        300,
        156,
        0xffe2_e8f0,
        pressed_key == Some("SPACE"),
    );
    if mode == KeyboardMode::Letters {
        draw_key(
            ui,
            &mut words,
            "SHIFT",
            432,
            908,
            144,
            156,
            if uppercase { 0xffbf_dbfe } else { 0xffe2_e8f0 },
            pressed_key == Some("SHIFT"),
        );
    } else {
        draw_key(
            ui,
            &mut words,
            ".",
            432,
            908,
            68,
            156,
            0xffe2_e8f0,
            pressed_key == Some("."),
        );
        draw_key(
            ui,
            &mut words,
            "?",
            508,
            908,
            68,
            156,
            0xffe2_e8f0,
            pressed_key == Some("?"),
        );
    }
    draw_key(
        ui,
        &mut words,
        match purpose {
            KeyboardPurpose::Prompt => "SEND",
            KeyboardPurpose::WifiPassword { .. } => "JOIN",
        },
        584,
        908,
        112,
        156,
        UI_ACCENT_GREEN,
        matches!(pressed_key, Some("SEND" | "JOIN")),
    );

    draw_key(
        ui,
        &mut words,
        "CLOSE KEYBOARD",
        24,
        1172,
        672,
        84,
        0xffe2_e8f0,
        false,
    );
    words
}

fn draw_character_row(
    ui: &Ui,
    words: &mut Vec<u32>,
    characters: &str,
    start_x: i16,
    y: i16,
    key_width: u16,
    gap: i16,
    pressed_key: Option<&str>,
) {
    for (index, character) in characters.chars().enumerate() {
        let x = start_x + index as i16 * (key_width as i16 + gap);
        let label = character.to_ascii_uppercase().to_string();
        draw_key(
            ui,
            words,
            &label,
            x,
            y,
            key_width,
            120,
            0xffff_ffff,
            pressed_key == Some(label.as_str()),
        );
    }
}

fn draw_key(
    ui: &Ui,
    words: &mut Vec<u32>,
    label: &str,
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    color: u32,
    pressed: bool,
) {
    let fill = if pressed { 0xff47_3b33 } else { color };
    rect(words, x, y, width, height, fill);
    let label_width = label.len() as i16 * 12;
    let text_x = x + ((width as i16 - label_width) / 2).max(6);
    push_text_bold(
        ui,
        words,
        label,
        text_x,
        y + (height as i16 / 2) - 10,
        label.len() + 1,
        if pressed {
            0xffff_ffff
        } else if color == UI_ACCENT_GREEN {
            0xff00_0000
        } else {
            0xff0f_172a
        },
    );
}

fn row_character(x: u16, start_x: u16, key_width: u16, gap: u16, characters: &str) -> Option<char> {
    characters
        .chars()
        .enumerate()
        .find_map(|(index, character)| {
            let key_x = start_x + index as u16 * (key_width + gap);
            (x >= key_x && x < key_x + key_width).then_some(character)
        })
}

fn prompt_tail(input: &str, max_bytes: usize) -> &str {
    if input.len() <= max_bytes {
        return input;
    }
    let mut start = input.len() - max_bytes;
    while !input.is_char_boundary(start) {
        start += 1;
    }
    &input[start..]
}

fn settings_draw_list(
    ui: &Ui,
    state: &DeviceState,
    settings: &SettingsProjection,
    wifi_scroll: usize,
    backend: &BackendProjection,
    telemetry: SystemTelemetry,
) -> Vec<u32> {
    let mut words = base_words(ui, state, "SETTINGS", telemetry);
    rect(&mut words, 24, 132, 672, 154, 0xffff_ffff);
    push_text_bold(ui, &mut words, "WI-FI", 44, 154, 12, 0xff0f_172a);
    let network = settings
        .wifi
        .connected_ssid
        .as_deref()
        .unwrap_or("NOT CONNECTED");
    push_text_bold(ui, &mut words, network, 44, 198, 30, 0xff25_63eb);
    let detail = match (&settings.wifi.ip_address, settings.wifi.rssi_dbm) {
        (Some(ip), Some(rssi)) => format!("IP {ip}  RSSI {rssi} DBM"),
        _ if !settings.wifi.status.is_empty() => settings.wifi.status.clone(),
        _ => "SCAN AND SELECT A NETWORK".to_owned(),
    };
    push_text(ui, &mut words, &detail, 44, 244, 42, 0xff64_748b);
    rect(&mut words, 480, 146, 196, 72, 0xff25_63eb);
    push_text_bold(
        ui,
        &mut words,
        if settings.wifi.scanning {
            "SCANNING"
        } else {
            "SCAN"
        },
        532,
        172,
        10,
        0xffff_ffff,
    );

    push_text(
        ui,
        &mut words,
        "AVAILABLE NETWORKS",
        28,
        294,
        28,
        0xff64_748b,
    );
    if settings.wifi.networks.is_empty() {
        rect(&mut words, 24, 330, 672, 112, 0xffe2_e8f0);
        push_text_bold(
            ui,
            &mut words,
            "TAP SCAN TO FIND WI-FI",
            52,
            372,
            30,
            0xff64_748b,
        );
    } else {
        let max_scroll = settings
            .wifi
            .networks
            .len()
            .saturating_sub(WIFI_VISIBLE_ROWS);
        for (row, network) in settings
            .wifi
            .networks
            .iter()
            .skip(wifi_scroll.min(max_scroll))
            .take(WIFI_VISIBLE_ROWS)
            .enumerate()
        {
            wifi_network_row(
                ui,
                &mut words,
                network,
                WIFI_ROW_START_Y as i16 + row as i16 * WIFI_ROW_HEIGHT as i16,
            );
        }
        wifi_scroll_buttons(ui, &mut words, settings.wifi.networks.len());
    }

    rect(&mut words, 24, 806, 672, 176, 0xffff_ffff);
    push_text(ui, &mut words, "MODEL BACKEND", 44, 832, 24, 0xff64_748b);
    push_text_bold(
        ui,
        &mut words,
        &format!("{}  /  {}", backend.model, backend.link),
        44,
        874,
        40,
        0xff0f_172a,
    );
    push_text(ui, &mut words, &backend.auth, 44, 920, 34, 0xff33_4155);
    let storage = settings
        .workspace_free_bytes
        .map(format_size)
        .unwrap_or_else(|| "--".into());
    push_text(
        ui,
        &mut words,
        &format!(
            "FIRMWARE {}  WORKSPACE FREE {storage}",
            settings.firmware_version
        ),
        44,
        956,
        44,
        0xff64_748b,
    );

    rect(&mut words, 24, 1010, 316, 80, 0xffe2_e8f0);
    push_text_bold(ui, &mut words, "FORGET WI-FI", 94, 1038, 18, 0xff0f_172a);
    rect(&mut words, 356, 1010, 340, 80, 0xfffe_e2e2);
    push_text_bold(ui, &mut words, "RESTART DEVICE", 424, 1038, 20, UI_LOSS_RED);
    bottom_bar(ui, &mut words, ScreenView::Settings);
    words
}

fn wifi_network_row(ui: &Ui, words: &mut Vec<u32>, network: &WifiNetworkProjection, y: i16) {
    rect(words, 24, y, 584, 84, 0xffff_ffff);
    push_text_bold(ui, words, &network.ssid, 44, y + 24, 34, 0xff0f_172a);
    push_text(
        ui,
        words,
        &format!(
            "{} DBM  {}",
            network.rssi_dbm,
            if network.secured { "LOCK" } else { "OPEN" }
        ),
        444,
        y + 28,
        18,
        0xff64_748b,
    );
}

fn wifi_scroll_buttons(ui: &Ui, words: &mut Vec<u32>, network_count: usize) {
    let color = if network_count > WIFI_VISIBLE_ROWS {
        0xff1d_4ed8
    } else {
        0xff94_a3b8
    };
    rect(words, 628, 330, 68, 132, 0xffdb_eafe);
    push_text_bold(ui, words, "UP", 646, 384, 4, color);
    rect(words, 628, 650, 68, 132, 0xffdb_eafe);
    push_text_bold(ui, words, "DN", 646, 704, 4, color);
}

fn files_draw_list(
    ui: &Ui,
    state: &DeviceState,
    browser: &WorkspaceBrowser,
    telemetry: SystemTelemetry,
) -> Vec<u32> {
    let title = if browser.can_go_up() {
        "< WORKSPACE FILES"
    } else {
        "WORKSPACE FILES"
    };
    let mut words = base_words(ui, state, title, telemetry);
    rect(&mut words, 24, 132, 672, 48, 0xffe2_e8f0);
    push_text(
        ui,
        &mut words,
        &browser.current_path(),
        40,
        146,
        39,
        0xff33_4155,
    );
    for row in 0..FILE_VISIBLE_ROWS {
        let index = browser.list_offset + row;
        let Some(entry) = browser.entries.get(index) else {
            break;
        };
        let y = FILE_ROW_START_Y + row as i16 * FILE_ROW_HEIGHT;
        rect(&mut words, 24, y, 584, 94, 0xffff_ffff);
        rect(
            &mut words,
            42,
            y + 18,
            48,
            48,
            if entry.is_dir {
                0xffbf_dbfe
            } else {
                0xffdc_fce7
            },
        );
        push_text(
            ui,
            &mut words,
            if entry.is_dir { "D" } else { "F" },
            58,
            y + 30,
            2,
            if entry.is_dir {
                0xff1d_4ed8
            } else {
                0xff04_7a55
            },
        );
        let name = if entry.is_dir {
            format!("{}/", entry.name)
        } else {
            entry.name.clone()
        };
        push_text_bold(ui, &mut words, &name, 108, y + 14, 29, 0xff0f_172a);
        let detail = if entry.is_dir {
            entry
                .timestamp
                .map(|timestamp| format_timestamp(Some(timestamp)))
                .unwrap_or_else(|| "FOLDER".to_owned())
        } else {
            format!(
                "{}  {}",
                format_size(entry.size),
                format_timestamp(entry.timestamp)
            )
        };
        push_text(ui, &mut words, &detail, 108, y + 52, 29, 0xff64_748b);
    }
    if browser.entries.is_empty() {
        push_text(
            ui,
            &mut words,
            "THIS DIRECTORY IS EMPTY",
            64,
            280,
            34,
            0xff64_748b,
        );
    }
    if let Some(status) = &browser.status {
        rect(&mut words, 40, 1038, 560, 72, 0xfffe_e2e2);
        push_text(ui, &mut words, status, 56, 1056, 32, 0xffb9_1c1c);
    }
    scroll_buttons(ui, &mut words);
    bottom_bar(ui, &mut words, ScreenView::Files);
    words
}

fn viewer_draw_list(
    ui: &Ui,
    state: &DeviceState,
    browser: &WorkspaceBrowser,
    telemetry: SystemTelemetry,
) -> Vec<u32> {
    let mut words = base_words(ui, state, "< FILE VIEWER", telemetry);
    let Some(file) = browser.open_file.as_ref() else {
        push_text(ui, &mut words, "NO FILE OPEN", 64, 240, 30, 0xff64_748b);
        return words;
    };
    rect(&mut words, 24, 132, 584, 82, 0xffff_ffff);
    push_text_bold(
        ui,
        &mut words,
        &file.relative_path,
        40,
        146,
        32,
        0xff0f_172a,
    );
    push_text(
        ui,
        &mut words,
        &format!(
            "{}  {}",
            format_size(file.size),
            format_timestamp(file.timestamp)
        ),
        40,
        180,
        32,
        0xff64_748b,
    );
    rect(&mut words, 24, 228, 584, 900, 0xff0b_1220);
    let visible = file
        .lines
        .iter()
        .skip(file.line_offset)
        .take(VIEWER_VISIBLE_LINES)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    push_text_limited(
        ui,
        &mut words,
        &visible,
        44,
        248,
        32,
        VIEWER_VISIBLE_LINES,
        0xffe2_e8f0,
    );
    push_text(
        ui,
        &mut words,
        &format!(
            "LINES {}-{} / {}",
            file.line_offset + 1,
            (file.line_offset + VIEWER_VISIBLE_LINES).min(file.lines.len()),
            file.lines.len()
        ),
        40,
        1138,
        32,
        0xff64_748b,
    );
    scroll_buttons(ui, &mut words);
    words
}

fn message_reader_draw_list(
    ui: &Ui,
    state: &DeviceState,
    reader: Option<&MessageReader>,
    telemetry: SystemTelemetry,
) -> Vec<u32> {
    let mut words = base_words(ui, state, "< MESSAGE READER", telemetry);
    let Some(reader) = reader else {
        push_text(
            ui,
            &mut words,
            "NO MESSAGE SELECTED",
            64,
            240,
            30,
            0xff64_748b,
        );
        return words;
    };

    rect(&mut words, 24, 132, 584, 82, 0xffff_ffff);
    rect(
        &mut words,
        40,
        154,
        if reader.author == "YOU" { 82 } else { 62 },
        42,
        if reader.author == "YOU" {
            0xffdb_eafe
        } else {
            0xffdc_fce7
        },
    );
    push_text_bold(
        ui,
        &mut words,
        reader.author,
        54,
        166,
        8,
        if reader.author == "YOU" {
            0xff1d_4ed8
        } else {
            0xff04_7a55
        },
    );

    rect(&mut words, 24, 228, 584, 900, 0xffff_ffff);
    let max_scroll = reader.lines.len().saturating_sub(MESSAGE_VISIBLE_LINES);
    let offset = reader.line_offset.min(max_scroll);
    let visible = reader
        .lines
        .iter()
        .skip(offset)
        .take(MESSAGE_VISIBLE_LINES)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    push_text_limited(
        ui,
        &mut words,
        &visible,
        44,
        248,
        32,
        MESSAGE_VISIBLE_LINES,
        0xff0f_172a,
    );
    push_text(
        ui,
        &mut words,
        &format!(
            "LINES {}-{} / {}",
            offset + 1,
            (offset + MESSAGE_VISIBLE_LINES).min(reader.lines.len()),
            reader.lines.len()
        ),
        40,
        1138,
        32,
        0xff64_748b,
    );
    scroll_buttons(ui, &mut words);
    words
}

fn bottom_bar(ui: &Ui, words: &mut Vec<u32>, active: ScreenView) {
    rect(words, 0, BOTTOM_BAR_Y, PANEL_WIDTH, 108, 0xff0f_172a);
    for (x, label, view) in [
        (10, "CHAT", ScreenView::Chat),
        (250, "FILES", ScreenView::Files),
        (490, "SETTINGS", ScreenView::Settings),
    ] {
        rect(
            words,
            x,
            BOTTOM_BAR_Y + 16,
            220,
            76,
            if active == view {
                0xff25_63eb
            } else {
                0xff1e_293b
            },
        );
        let label_x = x + (220 - label.len() as i16 * 12) / 2;
        push_text_bold(ui, words, label, label_x, BOTTOM_BAR_Y + 44, 8, 0xffff_ffff);
    }
}

fn chat_scroll_buttons(ui: &Ui, words: &mut Vec<u32>) {
    // Align the top edge with the first chat card at y=140.
    rect(words, 628, 140, 68, 132, 0xffdb_eafe);
    push_text_bold(ui, words, "UP", 646, 194, 4, 0xff1d_4ed8);
    rect(words, 628, 624, 68, 132, 0xffdb_eafe);
    push_text_bold(ui, words, "DN", 646, 678, 4, 0xff1d_4ed8);
}

fn scroll_buttons(ui: &Ui, words: &mut Vec<u32>) {
    rect(words, 628, 190, 68, 132, 0xffdb_eafe);
    push_text_bold(ui, words, "UP", 646, 244, 4, 0xff1d_4ed8);
    rect(words, 628, 940, 68, 132, 0xffdb_eafe);
    push_text_bold(ui, words, "DN", 646, 994, 4, 0xff1d_4ed8);
}

fn rect(words: &mut Vec<u32>, x: i16, y: i16, width: u16, height: u16, color: u32) {
    words.extend_from_slice(&[spec::draw_op::RECT, xy(x, y), wh(width, height), color]);
}

fn push_text(
    ui: &Ui,
    words: &mut Vec<u32>,
    text: &str,
    x: i16,
    y: i16,
    max_columns: usize,
    color: u32,
) {
    push_text_limited(ui, words, text, x, y, max_columns, 20, color);
}

fn push_text_right(ui: &Ui, words: &mut Vec<u32>, text: &str, right: i16, y: i16, color: u32) {
    let width = font::text_width(ui, text, font::TextStyle::Body);
    push_text(ui, words, text, right - width, y, text.len(), color);
}

fn push_text_bold(
    ui: &Ui,
    words: &mut Vec<u32>,
    text: &str,
    x: i16,
    y: i16,
    max_columns: usize,
    color: u32,
) {
    font::append_text(
        ui,
        words,
        text,
        x,
        y,
        max_columns,
        20,
        color,
        font::TextStyle::Bold,
    );
}

fn push_title(
    ui: &Ui,
    words: &mut Vec<u32>,
    text: &str,
    x: i16,
    y: i16,
    max_columns: usize,
    color: u32,
) {
    font::append_text(
        ui,
        words,
        text,
        x,
        y,
        max_columns,
        1,
        color,
        font::TextStyle::Title,
    );
}

fn push_text_limited(
    ui: &Ui,
    words: &mut Vec<u32>,
    text: &str,
    x: i16,
    y: i16,
    max_columns: usize,
    max_rows: usize,
    color: u32,
) {
    font::append_text(
        ui,
        words,
        text,
        x,
        y,
        max_columns,
        max_rows,
        color,
        font::TextStyle::Body,
    );
}

const fn agent_state_color(state: AgentState) -> u32 {
    match state {
        AgentState::Stopped => 0xff64_748b,
        AgentState::Starting | AgentState::WaitingForAuth => 0xfff5_9e0b,
        AgentState::Idle | AgentState::Thinking | AgentState::Acting => 0xff10_b981,
        AgentState::NetworkBlocked | AgentState::Faulted => UI_LOSS_RED,
    }
}

const fn xy(x: i16, y: i16) -> u32 {
    x as u16 as u32 | ((y as u16 as u32) << 16)
}

const fn wh(width: u16, height: u16) -> u32 {
    width as u32 | ((height as u32) << 16)
}
