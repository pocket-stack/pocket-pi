#![allow(
    clippy::if_same_then_else,
    clippy::manual_div_ceil,
    clippy::too_many_arguments
)]

mod font;
mod workspace_browser;

use std::collections::VecDeque;

use crate::model::{
    AgentState, DeviceState, ModelBackendSettings, ModelSettings, OrderProjection,
    PortfolioCollection, PortfolioProjection, ScheduleProjection, UartProvider,
    ValueTrendProjection, WirelessProvider,
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
const ACTIVITY_VISIBLE_ROWS: usize = 8;
const POSITION_VISIBLE_ROWS: usize = 9;
const ACCOUNT_VISIBLE_ROWS: usize = 8;
const ACCOUNT_ROW_START_Y: i16 = 140;
const ACCOUNT_ROW_HEIGHT: i16 = 112;
const COMPOSE_Y: u16 = 1070;
const MAX_PROMPT_BYTES: usize = 256;
const PENDING_ASSISTANT: &str = "THINKING...";

// PocketJS draw-list colors are packed ABGR, not ARGB.
const UI_GAIN_GREEN: u32 = 0xff3b_d158; // RGB #58D13B
const UI_LOSS_RED: u32 = 0xff44_44ef; // RGB #EF4444

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScreenView {
    Chat,
    Files,
    Robinhood,
    Accounts,
    Activities,
    Positions,
    Viewer,
    MessageReader,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScreenInteraction {
    None,
    Redraw,
    SubmitPrompt(String),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UiCapabilities {
    pub portfolio: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeyboardMode {
    Letters,
    Numbers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PnlSpan {
    Day,
    Week,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemTelemetry {
    pub ram_used_percent: u8,
    pub ram_free_bytes: usize,
    pub cpu_percent: Option<u8>,
    pub ui_fps: u16,
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
            ModelBackendSettings::Wireless { provider, .. } => Self {
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

    pub fn restore<I>(&mut self, turns: I) -> usize
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let mut restored = turns
            .into_iter()
            .map(|(user, assistant)| ChatTurn {
                user,
                assistant,
                pending: false,
            })
            .collect::<VecDeque<_>>();
        while restored.len() > MAX_CHAT_TURNS {
            restored.pop_front();
        }
        let count = restored.len();
        if count > 0 {
            self.turns = restored;
        }
        count
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

    pub fn complete_turn(&mut self, user: impl Into<String>, assistant: impl Into<String>) {
        let user = user.into();
        let assistant = assistant.into();
        if let Some(turn) = self.turns.back_mut() {
            if turn.user == user && turn.pending {
                turn.assistant = assistant;
                turn.pending = false;
                return;
            }
        }
        self.push_turn(user, assistant);
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
    pub capabilities: UiCapabilities,
    schedule: ScheduleProjection,
    portfolios: PortfolioCollection,
    account_index: usize,
    account_scroll: usize,
    chat_scroll: usize,
    pnl_span: PnlSpan,
    activity_scroll: usize,
    position_scroll: usize,
    keyboard_open: bool,
    keyboard_mode: KeyboardMode,
    prompt_input: String,
    pressed_key: Option<String>,
    backend: BackendProjection,
    message_reader: Option<MessageReader>,
}

impl ScreenState {
    pub fn new(workspace_root: &str) -> Self {
        Self::with_capabilities(workspace_root, UiCapabilities::default())
    }

    pub fn with_capabilities(workspace_root: &str, capabilities: UiCapabilities) -> Self {
        Self {
            view: ScreenView::Chat,
            browser: WorkspaceBrowser::new(workspace_root),
            telemetry: SystemTelemetry::default(),
            capabilities,
            schedule: ScheduleProjection::default(),
            portfolios: PortfolioCollection::default(),
            account_index: 0,
            account_scroll: 0,
            chat_scroll: 0,
            pnl_span: PnlSpan::Week,
            activity_scroll: 0,
            position_scroll: 0,
            keyboard_open: false,
            keyboard_mode: KeyboardMode::Letters,
            prompt_input: String::new(),
            pressed_key: None,
            backend: BackendProjection::default(),
            message_reader: None,
        }
    }

    pub fn set_telemetry(&mut self, telemetry: SystemTelemetry) {
        self.telemetry = telemetry;
    }

    pub fn set_schedule(&mut self, schedule: ScheduleProjection) {
        self.schedule = schedule;
    }

    pub fn set_portfolio(&mut self, portfolios: PortfolioCollection) {
        self.portfolios = portfolios;
        self.account_index = self
            .account_index
            .min(self.portfolios.accounts.len().saturating_sub(1));
    }

    pub fn set_model_backend(&mut self, settings: &ModelSettings) {
        self.backend = BackendProjection::from_settings(settings);
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
        if self.view == ScreenView::Chat && self.keyboard_open {
            return self.handle_keyboard_tap(x, y);
        }
        if !matches!(self.view, ScreenView::Viewer | ScreenView::MessageReader)
            && y as i16 >= BOTTOM_BAR_Y
        {
            let next = match (self.capabilities.portfolio, x) {
                (true, 0..=239) | (false, 0..=359) => ScreenView::Chat,
                (true, 240..=479) | (false, _) => {
                    self.browser.refresh();
                    ScreenView::Files
                }
                (true, _) => ScreenView::Robinhood,
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
            ScreenView::Robinhood => self.handle_robinhood_tap(x, y),
            ScreenView::Accounts => self.handle_accounts_tap(x, y),
            ScreenView::Activities => self.handle_activities_tap(x, y),
            ScreenView::Positions => self.handle_positions_tap(x, y),
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
        if (24..=696).contains(&x) && (1164..=1279).contains(&y) {
            self.keyboard_open = false;
            self.pressed_key = None;
            return ScreenInteraction::Redraw;
        }
        if (548..=680).contains(&x) && (228..=284).contains(&y) {
            self.pressed_key = Some("CLEAR".to_owned());
            self.prompt_input.clear();
            return ScreenInteraction::Redraw;
        }

        let rows = match self.keyboard_mode {
            KeyboardMode::Letters => ["qwertyuiop", "asdfghjkl", "zxcvbnm"],
            KeyboardMode::Numbers => ["1234567890", "-/:;()$&@", ".,?!'\"+"],
        };
        let character = if (336..=456).contains(&y) {
            row_character(x, 24, 60, 8, rows[0])
        } else if (476..=596).contains(&y) {
            row_character(x, 31, 66, 8, rows[1])
        } else if (616..=736).contains(&y) {
            row_character(x, 24, 72, 8, rows[2])
        } else {
            None
        };
        if let Some(character) = character {
            self.pressed_key = Some(character.to_ascii_uppercase().to_string());
            if self.prompt_input.len() < MAX_PROMPT_BYTES {
                self.prompt_input.push(character);
            }
            return ScreenInteraction::Redraw;
        }
        if (592..=696).contains(&x) && (616..=736).contains(&y) {
            self.pressed_key = Some("DEL".to_owned());
            self.prompt_input.pop();
            return ScreenInteraction::Redraw;
        }
        if (756..=896).contains(&y) {
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
                    return ScreenInteraction::Redraw;
                }
                124..=424 => {
                    self.pressed_key = Some("SPACE".to_owned());
                    if !self.prompt_input.is_empty() && self.prompt_input.len() < MAX_PROMPT_BYTES {
                        self.prompt_input.push(' ');
                    }
                    return ScreenInteraction::Redraw;
                }
                432..=500 => {
                    self.pressed_key = Some(".".to_owned());
                    if self.prompt_input.len() < MAX_PROMPT_BYTES {
                        self.prompt_input.push('.');
                    }
                    return ScreenInteraction::Redraw;
                }
                508..=576 => {
                    self.pressed_key = Some("?".to_owned());
                    if self.prompt_input.len() < MAX_PROMPT_BYTES {
                        self.prompt_input.push('?');
                    }
                    return ScreenInteraction::Redraw;
                }
                584..=696 => {
                    self.pressed_key = Some("SEND".to_owned());
                    let prompt = self.prompt_input.trim().to_owned();
                    if prompt.is_empty() {
                        return ScreenInteraction::None;
                    }
                    self.prompt_input.clear();
                    self.keyboard_open = false;
                    self.pressed_key = None;
                    self.keyboard_mode = KeyboardMode::Letters;
                    return ScreenInteraction::SubmitPrompt(prompt);
                }
                _ => {}
            }
        }
        ScreenInteraction::None
    }

    fn handle_robinhood_tap(&mut self, x: u16, y: u16) -> bool {
        if (112..=166).contains(&y) {
            self.account_scroll = self.account_index.min(
                self.portfolios
                    .accounts
                    .len()
                    .saturating_sub(ACCOUNT_VISIBLE_ROWS),
            );
            self.view = ScreenView::Accounts;
            return true;
        }
        if (448..=508).contains(&y) {
            let next = if x < 136 {
                Some(PnlSpan::Day)
            } else if x < 252 {
                Some(PnlSpan::Week)
            } else {
                None
            };
            if let Some(next) = next {
                let changed = self.pnl_span != next;
                self.pnl_span = next;
                return changed;
            }
        }
        if (624..=828).contains(&y) {
            self.activity_scroll = 0;
            self.view = ScreenView::Activities;
            return true;
        }
        if (832..=1014).contains(&y) {
            self.position_scroll = 0;
            self.view = ScreenView::Positions;
            return true;
        }
        false
    }

    fn handle_accounts_tap(&mut self, x: u16, y: u16) -> bool {
        if x < 104 && y < HEADER_HEIGHT as u16 {
            self.view = ScreenView::Robinhood;
            return true;
        }
        if x >= 620 && (130..=320).contains(&y) {
            let before = self.account_scroll;
            self.account_scroll = self.account_scroll.saturating_sub(4);
            return before != self.account_scroll;
        }
        if x >= 620 && (930..=1148).contains(&y) {
            let max_scroll = self
                .portfolios
                .accounts
                .len()
                .saturating_sub(ACCOUNT_VISIBLE_ROWS);
            let before = self.account_scroll;
            self.account_scroll = (self.account_scroll + 4).min(max_scroll);
            return before != self.account_scroll;
        }
        if x < 610 && y as i16 >= ACCOUNT_ROW_START_Y {
            let row = ((y as i16 - ACCOUNT_ROW_START_Y) / ACCOUNT_ROW_HEIGHT) as usize;
            let index = self.account_scroll + row;
            if row < ACCOUNT_VISIBLE_ROWS && index < self.portfolios.accounts.len() {
                self.account_index = index;
                self.activity_scroll = 0;
                self.position_scroll = 0;
                self.view = ScreenView::Robinhood;
                return true;
            }
        }
        false
    }

    fn handle_activities_tap(&mut self, x: u16, y: u16) -> bool {
        if x < 104 && y < HEADER_HEIGHT as u16 {
            self.view = ScreenView::Robinhood;
            return true;
        }
        if x >= 620 && (130..=320).contains(&y) {
            let before = self.activity_scroll;
            self.activity_scroll = self.activity_scroll.saturating_sub(4);
            return before != self.activity_scroll;
        }
        if x >= 620 && (930..=1148).contains(&y) {
            let max_scroll = self
                .portfolios
                .account(self.account_index)
                .orders
                .len()
                .saturating_sub(ACTIVITY_VISIBLE_ROWS);
            let before = self.activity_scroll;
            self.activity_scroll = (self.activity_scroll + 4).min(max_scroll);
            return before != self.activity_scroll;
        }
        false
    }

    fn handle_positions_tap(&mut self, x: u16, y: u16) -> bool {
        if x < 104 && y < HEADER_HEIGHT as u16 {
            self.view = ScreenView::Robinhood;
            return true;
        }
        if x >= 620 && (130..=320).contains(&y) {
            let before = self.position_scroll;
            self.position_scroll = self.position_scroll.saturating_sub(4);
            return before != self.position_scroll;
        }
        if x >= 620 && (930..=1148).contains(&y) {
            let max_scroll = self
                .portfolios
                .account(self.account_index)
                .positions
                .len()
                .saturating_sub(POSITION_VISIBLE_ROWS);
            let before = self.position_scroll;
            self.position_scroll = (self.position_scroll + 4).min(max_scroll);
            return before != self.position_scroll;
        }
        false
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
            ScreenView::Chat if self.keyboard_open => keyboard_draw_list(
                ui,
                state,
                &self.prompt_input,
                self.keyboard_mode,
                self.pressed_key.as_deref(),
                &self.backend,
                self.telemetry,
            ),
            ScreenView::Chat => chat_draw_list(
                ui,
                state,
                chat,
                self.chat_scroll,
                &self.schedule,
                self.capabilities.portfolio,
                self.telemetry,
            ),
            ScreenView::Files => files_draw_list(
                ui,
                state,
                &self.browser,
                self.capabilities.portfolio,
                self.telemetry,
            ),
            ScreenView::Robinhood => robinhood_draw_list(
                ui,
                state,
                self.portfolios.account(self.account_index),
                self.account_index,
                self.portfolios.accounts.len(),
                self.pnl_span,
                self.telemetry,
            ),
            ScreenView::Accounts => accounts_draw_list(
                ui,
                state,
                &self.portfolios,
                self.account_index,
                self.account_scroll,
                self.telemetry,
            ),
            ScreenView::Activities => activities_draw_list(
                ui,
                state,
                self.portfolios.account(self.account_index),
                self.activity_scroll,
                self.telemetry,
            ),
            ScreenView::Positions => positions_draw_list(
                ui,
                state,
                self.portfolios.account(self.account_index),
                self.position_scroll,
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
    let free_megabytes = telemetry.ram_free_bytes as f32 / (1024.0 * 1024.0);
    let memory = format!(
        "RAM {:02}%  FREE {free_megabytes:.1}M",
        telemetry.ram_used_percent
    );
    let runtime = format!("CPU {cpu}  UI {}FPS  LCD32", telemetry.ui_fps);
    push_text_right(ui, words, &memory, RIGHT_EDGE, 25, primary_status_color);
    push_text_right(ui, words, &runtime, RIGHT_EDGE, 61, secondary_status_color);
}

fn chat_draw_list(
    ui: &Ui,
    state: &DeviceState,
    chat: &ChatProjection,
    scroll: usize,
    schedule: &ScheduleProjection,
    portfolio_enabled: bool,
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
    bottom_bar(ui, &mut words, ScreenView::Chat, portfolio_enabled);
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
    pressed_key: Option<&str>,
    backend: &BackendProjection,
    telemetry: SystemTelemetry,
) -> Vec<u32> {
    let mut words = base_words(ui, state, "NEW MESSAGE", telemetry);
    rect(&mut words, 24, 132, 672, 168, 0xffff_ffff);
    push_text_limited(
        ui,
        &mut words,
        prompt_tail(input, 116),
        42,
        154,
        29,
        4,
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
            "TYPE YOUR MESSAGE...",
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
        228,
        132,
        56,
        0xffe2_e8f0,
        pressed_key == Some("CLEAR"),
    );
    push_text(
        ui,
        &mut words,
        &format!("{} / {} ASCII BYTES", input.len(), MAX_PROMPT_BYTES),
        30,
        310,
        34,
        0xff64_748b,
    );

    let rows = match mode {
        KeyboardMode::Letters => ["qwertyuiop", "asdfghjkl", "zxcvbnm"],
        KeyboardMode::Numbers => ["1234567890", "-/:;()$&@", ".,?!'\"+"],
    };
    draw_character_row(ui, &mut words, rows[0], 24, 336, 60, 8, pressed_key);
    draw_character_row(ui, &mut words, rows[1], 31, 476, 66, 8, pressed_key);
    draw_character_row(ui, &mut words, rows[2], 24, 616, 72, 8, pressed_key);
    draw_key(
        ui,
        &mut words,
        "DEL",
        592,
        616,
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
        756,
        92,
        140,
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
        756,
        300,
        140,
        0xffe2_e8f0,
        pressed_key == Some("SPACE"),
    );
    draw_key(
        ui,
        &mut words,
        ".",
        432,
        756,
        68,
        140,
        0xffe2_e8f0,
        pressed_key == Some("."),
    );
    draw_key(
        ui,
        &mut words,
        "?",
        508,
        756,
        68,
        140,
        0xffe2_e8f0,
        pressed_key == Some("?"),
    );
    draw_key(
        ui,
        &mut words,
        "SEND",
        584,
        756,
        112,
        140,
        UI_GAIN_GREEN,
        pressed_key == Some("SEND"),
    );

    rect(&mut words, 24, 928, 672, 220, 0xffff_ffff);
    push_text(
        ui,
        &mut words,
        "CURRENT MODEL BACKEND",
        44,
        950,
        34,
        0xff64_748b,
    );
    push_text_bold(
        ui,
        &mut words,
        &format!("MODEL   {}", backend.model),
        44,
        990,
        34,
        0xff0f_172a,
    );
    push_text_bold(
        ui,
        &mut words,
        &format!("LINK    {}", backend.link),
        44,
        1032,
        34,
        0xff0f_172a,
    );
    push_text_bold(
        ui,
        &mut words,
        &format!("AUTH    {}", backend.auth),
        44,
        1074,
        34,
        0xff33_4155,
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
        } else if color == UI_GAIN_GREEN {
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

fn robinhood_draw_list(
    ui: &Ui,
    state: &DeviceState,
    portfolio: &PortfolioProjection,
    account_index: usize,
    account_count: usize,
    span: PnlSpan,
    telemetry: SystemTelemetry,
) -> Vec<u32> {
    let mut words = robinhood_base_words(ui, state, "ROBINHOOD", telemetry, false);
    let trend = if span == PnlSpan::Day {
        &portfolio.value_day
    } else {
        &portfolio.value_week
    };
    let accent = if trend.positive {
        UI_GAIN_GREEN
    } else {
        UI_LOSS_RED
    };
    account_switcher(ui, &mut words, portfolio, account_index, account_count);
    push_display(
        ui,
        &mut words,
        &portfolio.total_value,
        28,
        166,
        18,
        0xffff_ffff,
    );
    push_text_bold(
        ui,
        &mut words,
        &format!(
            "{}  ({})  {}",
            trend.change,
            trend.percent,
            span_label(span)
        ),
        30,
        216,
        38,
        accent,
    );
    value_chart(ui, &mut words, trend, 28, 252, 664, 160, accent);
    span_button(ui, &mut words, 28, "1D", span == PnlSpan::Day);
    span_button(ui, &mut words, 144, "1W", span == PnlSpan::Week);

    metric_card(
        ui,
        &mut words,
        24,
        508,
        210,
        "VALUE",
        &portfolio.total_value,
    );
    metric_card(ui, &mut words, 255, 508, 210, "CASH", &portfolio.cash);
    metric_card(
        ui,
        &mut words,
        486,
        508,
        210,
        "BUY POWER",
        &portfolio.buying_power,
    );

    section_title(ui, &mut words, "ACTIVITY", 28, 640, true);
    if !portfolio.orders_available {
        empty_dark_row(ui, &mut words, "ACTIVITY UNAVAILABLE", 680, 144);
    } else if portfolio.orders.is_empty() {
        empty_dark_row(ui, &mut words, "NO RECENT ACTIVITY", 680, 144);
    } else {
        for (index, order) in portfolio.orders.iter().take(2).enumerate() {
            compact_activity_row(ui, &mut words, order, 680 + index as i16 * 72);
        }
    }

    section_title(ui, &mut words, "POSITIONS", 28, 836, true);
    if !portfolio.positions_available {
        empty_dark_row(ui, &mut words, "POSITIONS UNAVAILABLE", 876, 128);
    } else if portfolio.positions.is_empty() {
        empty_dark_row(ui, &mut words, "NO OPEN POSITIONS", 876, 128);
    } else {
        for (index, position) in portfolio.positions.iter().take(2).enumerate() {
            let y = 876 + index as i16 * 64;
            push_text_bold(ui, &mut words, &position.symbol, 40, y + 8, 10, 0xffff_ffff);
            push_text(
                ui,
                &mut words,
                &format!("{} SH", position.quantity),
                172,
                y + 8,
                14,
                0xff9c_a3af,
            );
            push_text(
                ui,
                &mut words,
                &format!("AVG {}", position.average_price),
                440,
                y + 8,
                16,
                0xff9c_a3af,
            );
            rect(&mut words, 40, y + 52, 640, 1, 0xff27_2d35);
        }
    }

    realized_pnl_card(ui, &mut words, portfolio, span);
    bottom_bar(ui, &mut words, ScreenView::Robinhood, true);
    words
}

fn account_switcher(
    ui: &Ui,
    words: &mut Vec<u32>,
    portfolio: &PortfolioProjection,
    account_index: usize,
    account_count: usize,
) {
    rect(words, 24, 116, 672, 46, 0xff12_1519);
    if account_count == 0 {
        push_text_bold(ui, words, "ACCOUNT", 40, 129, 8, 0xff7d_8590);
        push_text_bold(ui, words, &portfolio.status, 156, 129, 34, UI_LOSS_RED);
        push_text_bold(ui, words, "v", 670, 129, 2, UI_GAIN_GREEN);
        return;
    }
    let label = if portfolio.account_suffix.is_empty() {
        portfolio.account_label.clone()
    } else {
        format!(
            "{}  ****{}  {}/{}",
            portfolio.account_label,
            portfolio.account_suffix,
            account_index + 1,
            account_count
        )
    };
    push_text_bold(ui, words, "ACCOUNT", 40, 129, 8, 0xff7d_8590);
    push_text_bold(
        ui,
        words,
        &label,
        156,
        129,
        30,
        if portfolio.status == "LIVE / READ ONLY" {
            0xffff_ffff
        } else {
            UI_LOSS_RED
        },
    );
    push_text_bold(ui, words, "v", 670, 129, 2, UI_GAIN_GREEN);
}

fn span_button(ui: &Ui, words: &mut Vec<u32>, x: i16, label: &str, active: bool) {
    rect(
        words,
        x,
        456,
        100,
        44,
        if active { UI_GAIN_GREEN } else { 0xff18_1c20 },
    );
    push_text_bold(
        ui,
        words,
        label,
        x + 34,
        467,
        8,
        if active { 0xff00_0000 } else { 0xff9c_a3af },
    );
}

fn robinhood_base_words(
    ui: &Ui,
    state: &DeviceState,
    title: &str,
    telemetry: SystemTelemetry,
    back: bool,
) -> Vec<u32> {
    let mut words = Vec::new();
    rect(&mut words, 0, 0, PANEL_WIDTH, PANEL_HEIGHT, 0xff00_0000);
    let header_title = if back {
        format!("< {title}")
    } else {
        title.to_owned()
    };
    status_header(
        ui,
        &mut words,
        state,
        &header_title,
        telemetry,
        0xffff_ffff,
        0xff9c_a3af,
        0xff6b_7280,
    );
    rect(&mut words, 24, 110, 672, 1, 0xff27_2d35);
    words
}

fn metric_card(
    ui: &Ui,
    words: &mut Vec<u32>,
    x: i16,
    y: i16,
    width: u16,
    label: &str,
    value: &str,
) {
    rect(words, x, y, width, 104, 0xff12_1519);
    push_text_bold(ui, words, label, x + 16, y + 16, 13, 0xff7d_8590);
    push_text_bold(ui, words, value, x + 16, y + 56, 13, 0xffff_ffff);
}

fn section_title(ui: &Ui, words: &mut Vec<u32>, label: &str, x: i16, y: i16, arrow: bool) {
    push_title(ui, words, label, x, y, 24, 0xffff_ffff);
    if arrow {
        push_text_bold(ui, words, ">", 654, y + 4, 3, UI_GAIN_GREEN);
    }
}

fn compact_activity_row(ui: &Ui, words: &mut Vec<u32>, order: &OrderProjection, y: i16) {
    push_text_bold(ui, words, &order.title, 40, y + 8, 28, 0xffff_ffff);
    push_text_bold(
        ui,
        words,
        &order.amount,
        520,
        y + 26,
        9,
        activity_color(order),
    );
    push_text(ui, words, &order.timestamp, 40, y + 38, 18, 0xff7d_8590);
    push_text(ui, words, &order.detail, 284, y + 38, 20, 0xff9c_a3af);
    rect(words, 40, y + 68, 640, 1, 0xff27_2d35);
}

fn activity_color(order: &OrderProjection) -> u32 {
    if order.side == "SELL" {
        UI_GAIN_GREEN
    } else {
        0xffff_ffff
    }
}

fn empty_dark_row(ui: &Ui, words: &mut Vec<u32>, label: &str, y: i16, height: u16) {
    rect(words, 24, y, 672, height, 0xff0d_1013);
    push_text(ui, words, label, 44, y + 34, 34, 0xff7d_8590);
}

fn realized_pnl_card(
    ui: &Ui,
    words: &mut Vec<u32>,
    portfolio: &PortfolioProjection,
    span: PnlSpan,
) {
    let pnl = if span == PnlSpan::Day {
        &portfolio.pnl_day
    } else {
        &portfolio.pnl_week
    };
    let available = if span == PnlSpan::Day {
        portfolio.pnl_day_available
    } else {
        portfolio.pnl_week_available
    };
    rect(words, 24, 1018, 672, 126, 0xff12_1519);
    push_title(ui, words, "REALIZED P&L", 42, 1036, 24, 0xffff_ffff);
    push_text(
        ui,
        words,
        &format!("EQUITIES / {}", span_label(span)),
        44,
        1080,
        22,
        0xff7d_8590,
    );
    let color = if !available {
        UI_LOSS_RED
    } else if pnl.total.starts_with("-$") {
        UI_LOSS_RED
    } else {
        UI_GAIN_GREEN
    };
    push_title(ui, words, &pnl.total, 522, 1056, 11, color);
}

fn span_label(span: PnlSpan) -> &'static str {
    match span {
        PnlSpan::Day => "TODAY",
        PnlSpan::Week => "WEEK",
    }
}

#[allow(clippy::too_many_arguments)]
fn value_chart(
    ui: &Ui,
    words: &mut Vec<u32>,
    trend: &ValueTrendProjection,
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    color: u32,
) {
    for dot in 0..42 {
        rect(
            words,
            x + dot * 16,
            y + height as i16 - 2,
            4,
            2,
            0xff4b_5563,
        );
    }
    chart_axis_labels(ui, words, &trend.x_labels, x, y + height as i16 + 8, width);
    if trend.points.len() < 2 {
        push_text(
            ui,
            words,
            "COLLECTING 5M VALUE HISTORY",
            x + 150,
            y + 78,
            30,
            0xff7d_8590,
        );
        rect(
            words,
            x + width as i16 - 7,
            y + height as i16 / 2 - 3,
            7,
            7,
            color,
        );
        return;
    }
    let min = trend.points.iter().copied().fold(f32::INFINITY, f32::min);
    let max = trend
        .points
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    let range = (max - min).max(0.01);
    let usable_height = height as f32 - 20.0;
    let mut previous = None;
    for (index, value) in trend.points.iter().enumerate() {
        let px = x as f32 + index as f32 * width as f32 / (trend.points.len() - 1) as f32;
        let py = y as f32 + 10.0 + (max - value) / range * usable_height;
        if let Some((last_x, last_y)) = previous {
            line_segment(words, last_x, last_y, px, py, 4.0, color);
            if index % 7 == 0 {
                line_segment(words, last_x, last_y, px, py, 1.25, 0xffff_ffff);
            }
        }
        previous = Some((px, py));
    }
    if let Some((last_x, last_y)) = previous {
        rect(words, last_x as i16 - 5, last_y as i16 - 5, 11, 11, color);
        rect(
            words,
            last_x as i16 - 1,
            last_y as i16 - 1,
            3,
            3,
            0xffff_ffff,
        );
    }
}

fn chart_axis_labels(ui: &Ui, words: &mut Vec<u32>, labels: &[String], x: i16, y: i16, width: u16) {
    for (index, label) in labels.iter().enumerate() {
        let label_width = font::text_width(ui, label, font::TextStyle::Body);
        let label_x = match labels.len() {
            1 => x + width as i16 - label_width,
            2 if index == 0 => x,
            2 => x + width as i16 - label_width,
            _ if index == 0 => x,
            _ if index + 1 == labels.len() => x + width as i16 - label_width,
            _ => x + width as i16 / 2 - label_width / 2,
        };
        push_text(ui, words, label, label_x, y, 8, 0xff7d_8590);
    }
}

fn line_segment(
    words: &mut Vec<u32>,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    thickness: f32,
    color: u32,
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let length = (dx * dx + dy * dy).sqrt().max(0.001);
    let nx = -dy / length * thickness * 0.5;
    let ny = dx / length * thickness * 0.5;
    let a = (x0 + nx, y0 + ny);
    let b = (x0 - nx, y0 - ny);
    let c = (x1 + nx, y1 + ny);
    let d = (x1 - nx, y1 - ny);
    tri(words, a, b, c, color);
    tri(words, b, d, c, color);
}

fn tri(words: &mut Vec<u32>, a: (f32, f32), b: (f32, f32), c: (f32, f32), color: u32) {
    words.extend_from_slice(&[
        spec::draw_op::TRI,
        xy(a.0.round() as i16, a.1.round() as i16),
        xy(b.0.round() as i16, b.1.round() as i16),
        xy(c.0.round() as i16, c.1.round() as i16),
        color,
        color,
        color,
    ]);
}

fn accounts_draw_list(
    ui: &Ui,
    state: &DeviceState,
    portfolios: &PortfolioCollection,
    selected: usize,
    scroll: usize,
    telemetry: SystemTelemetry,
) -> Vec<u32> {
    let mut words = robinhood_base_words(ui, state, "ACCOUNTS", telemetry, true);
    if portfolios.accounts.is_empty() {
        empty_dark_row(
            ui,
            &mut words,
            &portfolios.account(0).status,
            ACCOUNT_ROW_START_Y,
            150,
        );
    } else {
        let max_scroll = portfolios
            .accounts
            .len()
            .saturating_sub(ACCOUNT_VISIBLE_ROWS);
        for (row, account) in portfolios
            .accounts
            .iter()
            .skip(scroll.min(max_scroll))
            .take(ACCOUNT_VISIBLE_ROWS)
            .enumerate()
        {
            let index = scroll.min(max_scroll) + row;
            let y = ACCOUNT_ROW_START_Y + row as i16 * ACCOUNT_ROW_HEIGHT;
            let is_selected = index == selected;
            rect(
                &mut words,
                24,
                y,
                584,
                100,
                if is_selected {
                    0xff18_2018
                } else {
                    0xff0d_1013
                },
            );
            if is_selected {
                rect(&mut words, 24, y, 8, 100, UI_GAIN_GREEN);
            }
            push_title(
                ui,
                &mut words,
                &account.account_label,
                44,
                y + 14,
                18,
                0xffff_ffff,
            );
            let suffix = if account.account_suffix.is_empty() {
                "****----".to_owned()
            } else {
                format!("****{}", account.account_suffix)
            };
            push_text_bold(ui, &mut words, &suffix, 306, y + 20, 12, 0xff9c_a3af);
            push_text(
                ui,
                &mut words,
                &account.status,
                44,
                y + 62,
                30,
                if account.status == "LIVE / READ ONLY" {
                    UI_GAIN_GREEN
                } else {
                    UI_LOSS_RED
                },
            );
            if is_selected {
                push_text_bold(ui, &mut words, "SELECTED", 476, y + 62, 8, UI_GAIN_GREEN);
            }
        }
        if portfolios.accounts.len() == 1 {
            push_text(
                ui,
                &mut words,
                "ONLY ONE ACCOUNT RETURNED BY CURRENT AUTHORIZATION",
                36,
                1088,
                52,
                0xff7d_8590,
            );
        }
    }
    account_scroll_buttons(ui, &mut words, portfolios.accounts.len());
    bottom_bar(ui, &mut words, ScreenView::Robinhood, true);
    words
}

fn account_scroll_buttons(ui: &Ui, words: &mut Vec<u32>, account_count: usize) {
    let color = if account_count > ACCOUNT_VISIBLE_ROWS {
        UI_GAIN_GREEN
    } else {
        0xff4b_5563
    };
    rect(words, 628, 156, 68, 132, 0xff18_1c20);
    push_text_bold(ui, words, "UP", 646, 210, 4, color);
    rect(words, 628, 972, 68, 132, 0xff18_1c20);
    push_text_bold(ui, words, "DN", 646, 1026, 4, color);
}

fn activities_draw_list(
    ui: &Ui,
    state: &DeviceState,
    portfolio: &PortfolioProjection,
    scroll: usize,
    telemetry: SystemTelemetry,
) -> Vec<u32> {
    let mut words = robinhood_base_words(ui, state, "ACTIVITY", telemetry, true);
    if !portfolio.orders_available {
        empty_dark_row(ui, &mut words, "ACTIVITY UNAVAILABLE", 160, 150);
    } else if portfolio.orders.is_empty() {
        empty_dark_row(ui, &mut words, "NO ACTIVITY YET", 160, 150);
    } else {
        let max_scroll = portfolio.orders.len().saturating_sub(ACTIVITY_VISIBLE_ROWS);
        for (index, order) in portfolio
            .orders
            .iter()
            .skip(scroll.min(max_scroll))
            .take(ACTIVITY_VISIBLE_ROWS)
            .enumerate()
        {
            activity_history_row(ui, &mut words, order, 126 + index as i16 * 126);
        }
    }
    activity_scroll_buttons(ui, &mut words);
    bottom_bar(ui, &mut words, ScreenView::Robinhood, true);
    words
}

fn activity_history_row(ui: &Ui, words: &mut Vec<u32>, order: &OrderProjection, y: i16) {
    rect(words, 24, y, 584, 112, 0xff0d_1013);
    push_text_bold(ui, words, &order.title, 40, y + 14, 27, 0xffff_ffff);
    push_text_bold(
        ui,
        words,
        &order.amount,
        472,
        y + 46,
        10,
        activity_color(order),
    );
    push_text(ui, words, &order.timestamp, 40, y + 48, 18, 0xff7d_8590);
    push_text(ui, words, &order.detail, 250, y + 48, 20, 0xff9c_a3af);
    push_text_bold(ui, words, &order.state, 40, y + 78, 18, UI_GAIN_GREEN);
}

fn activity_scroll_buttons(ui: &Ui, words: &mut Vec<u32>) {
    rect(words, 628, 156, 68, 132, 0xff18_1c20);
    push_text_bold(ui, words, "UP", 646, 210, 4, UI_GAIN_GREEN);
    rect(words, 628, 972, 68, 132, 0xff18_1c20);
    push_text_bold(ui, words, "DN", 646, 1026, 4, UI_GAIN_GREEN);
}

fn positions_draw_list(
    ui: &Ui,
    state: &DeviceState,
    portfolio: &PortfolioProjection,
    scroll: usize,
    telemetry: SystemTelemetry,
) -> Vec<u32> {
    let mut words = robinhood_base_words(ui, state, "POSITIONS", telemetry, true);
    if !portfolio.positions_available {
        empty_dark_row(ui, &mut words, "POSITIONS UNAVAILABLE", 160, 150);
    } else if portfolio.positions.is_empty() {
        empty_dark_row(ui, &mut words, "NO OPEN POSITIONS", 160, 150);
    } else {
        let max_scroll = portfolio
            .positions
            .len()
            .saturating_sub(POSITION_VISIBLE_ROWS);
        for (index, position) in portfolio
            .positions
            .iter()
            .skip(scroll.min(max_scroll))
            .take(POSITION_VISIBLE_ROWS)
            .enumerate()
        {
            let y = 126 + index as i16 * 110;
            rect(&mut words, 24, y, 584, 98, 0xff0d_1013);
            push_title(
                ui,
                &mut words,
                &position.symbol,
                42,
                y + 14,
                12,
                0xffff_ffff,
            );
            push_text_bold(
                ui,
                &mut words,
                &format!("{} SH", position.quantity),
                210,
                y + 20,
                18,
                0xffff_ffff,
            );
            push_text(
                ui,
                &mut words,
                &format!("AVERAGE COST  {}", position.average_price),
                42,
                y + 62,
                26,
                0xff9c_a3af,
            );
        }
    }
    position_scroll_buttons(ui, &mut words, portfolio.positions.len());
    bottom_bar(ui, &mut words, ScreenView::Robinhood, true);
    words
}

fn position_scroll_buttons(ui: &Ui, words: &mut Vec<u32>, position_count: usize) {
    let color = if position_count > POSITION_VISIBLE_ROWS {
        UI_GAIN_GREEN
    } else {
        0xff4b_5563
    };
    rect(words, 628, 156, 68, 132, 0xff18_1c20);
    push_text_bold(ui, words, "UP", 646, 210, 4, color);
    rect(words, 628, 972, 68, 132, 0xff18_1c20);
    push_text_bold(ui, words, "DN", 646, 1026, 4, color);
}

fn files_draw_list(
    ui: &Ui,
    state: &DeviceState,
    browser: &WorkspaceBrowser,
    portfolio_enabled: bool,
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
    bottom_bar(ui, &mut words, ScreenView::Files, portfolio_enabled);
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

fn bottom_bar(ui: &Ui, words: &mut Vec<u32>, active: ScreenView, portfolio_enabled: bool) {
    rect(words, 0, BOTTOM_BAR_Y, PANEL_WIDTH, 108, 0xff0f_172a);
    let tabs = if portfolio_enabled {
        vec![
            (10, 220, "CHAT", ScreenView::Chat),
            (250, 220, "FILES", ScreenView::Files),
            (490, 220, "ROBIN", ScreenView::Robinhood),
        ]
    } else {
        vec![
            (10, 340, "CHAT", ScreenView::Chat),
            (370, 340, "FILES", ScreenView::Files),
        ]
    };
    for (x, width, label, view) in tabs {
        rect(
            words,
            x,
            BOTTOM_BAR_Y + 16,
            width,
            76,
            if active == view {
                if view == ScreenView::Robinhood {
                    UI_GAIN_GREEN
                } else {
                    0xff25_63eb
                }
            } else {
                0xff1e_293b
            },
        );
        let label_x = if portfolio_enabled { x + 68 } else { x + 128 };
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

fn push_display(
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
        font::TextStyle::Display,
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
