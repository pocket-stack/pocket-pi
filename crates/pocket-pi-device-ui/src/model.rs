#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentState {
    Stopped,
    Starting,
    WaitingForAuth,
    Idle,
    Thinking,
    Acting,
    NetworkBlocked,
    Faulted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceState {
    pub agent: AgentState,
}

impl Default for DeviceState {
    fn default() -> Self {
        Self {
            agent: AgentState::Stopped,
        }
    }
}

pub use pocket_pi_protocols::model::{
    ModelBackendSettings, ModelSettings, UartProvider, WirelessProvider,
};

#[derive(Clone, Debug, Default)]
pub struct ScheduleProjection {
    pub name: Option<String>,
    pub prompt: String,
    pub next_in_seconds: Option<u64>,
    pub every_minutes: Option<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WifiNetworkProjection {
    pub ssid: String,
    pub rssi_dbm: i16,
    pub secured: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WifiSettingsProjection {
    pub connected_ssid: Option<String>,
    pub ip_address: Option<String>,
    pub rssi_dbm: Option<i16>,
    pub scanning: bool,
    pub networks: Vec<WifiNetworkProjection>,
    pub status: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SettingsProjection {
    pub wifi: WifiSettingsProjection,
    pub firmware_version: String,
    pub workspace_free_bytes: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingsCommand {
    ScanWifi,
    ConnectWifi { ssid: String, password: String },
    ForgetWifi,
    Restart,
}

#[derive(Clone, Debug, Default)]
pub struct PositionProjection {
    pub symbol: String,
    pub quantity: String,
    pub average_price: String,
}

#[derive(Clone, Debug, Default)]
pub struct OrderProjection {
    pub title: String,
    pub timestamp: String,
    pub detail: String,
    pub amount: String,
    pub symbol: String,
    pub side: String,
    pub state: String,
    pub quantity: String,
    pub price: String,
}

#[derive(Clone, Debug)]
pub struct ValueTrendProjection {
    pub change: String,
    pub percent: String,
    pub positive: bool,
    pub points: Vec<f32>,
    pub x_labels: Vec<String>,
}

impl Default for ValueTrendProjection {
    fn default() -> Self {
        Self {
            change: "$0.00".into(),
            percent: "0.00%".into(),
            positive: true,
            points: Vec::new(),
            x_labels: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PnlProjection {
    pub total: String,
    pub points: Vec<f32>,
}

#[derive(Clone, Debug)]
pub struct PortfolioProjection {
    pub status: String,
    pub account_label: String,
    pub account_suffix: String,
    pub agentic: bool,
    pub total_value: String,
    pub cash: String,
    pub buying_power: String,
    pub positions: Vec<PositionProjection>,
    pub positions_available: bool,
    pub orders: Vec<OrderProjection>,
    pub orders_available: bool,
    pub value_day: ValueTrendProjection,
    pub value_week: ValueTrendProjection,
    pub pnl_day: PnlProjection,
    pub pnl_day_available: bool,
    pub pnl_week: PnlProjection,
    pub pnl_week_available: bool,
}

impl Default for PortfolioProjection {
    fn default() -> Self {
        Self {
            status: "EXTERNAL PLUGIN NOT LOADED".into(),
            account_label: "ACCOUNT".into(),
            account_suffix: String::new(),
            agentic: false,
            total_value: "--".into(),
            cash: "--".into(),
            buying_power: "--".into(),
            positions: Vec::new(),
            positions_available: false,
            orders: Vec::new(),
            orders_available: false,
            value_day: ValueTrendProjection::default(),
            value_week: ValueTrendProjection::default(),
            pnl_day: PnlProjection::default(),
            pnl_day_available: false,
            pnl_week: PnlProjection::default(),
            pnl_week_available: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PortfolioCollection {
    pub accounts: Vec<PortfolioProjection>,
    placeholder: PortfolioProjection,
}

impl PortfolioCollection {
    pub fn account(&self, index: usize) -> &PortfolioProjection {
        self.accounts
            .get(index)
            .or_else(|| self.accounts.first())
            .unwrap_or(&self.placeholder)
    }
}
