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
