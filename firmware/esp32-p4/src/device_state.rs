#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WifiNetworkFacts {
    pub ssid: String,
    pub rssi_dbm: i16,
    pub secured: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WifiSettingsFacts {
    pub connected_ssid: Option<String>,
    pub ip_address: Option<String>,
    pub rssi_dbm: Option<i16>,
    pub scanning: bool,
    pub networks: Vec<WifiNetworkFacts>,
    pub status: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SettingsFacts {
    pub wifi: WifiSettingsFacts,
    pub firmware_version: String,
    pub workspace_free_bytes: Option<u64>,
}
