use core::time::Duration;
use std::time::Instant;

use embedded_svc::wifi::{AuthMethod, ClientConfiguration, Configuration};
use esp_idf_svc::handle::RawHandle;
use esp_idf_svc::netif::{EspNetif, NetifStack};
use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition};
use esp_idf_svc::wifi::WifiDriver;

use crate::device_state::{SettingsFacts, WifiNetworkFacts};
use crate::{delay_current_task, esp_result, storage, DEVICE_NVS_NAMESPACE};

const WIFI_NVS_SSID_KEY: &str = "wifi_ssid";
const WIFI_NVS_PASSWORD_KEY: &str = "wifi_pass";

pub struct WifiConnection {
    driver: WifiDriver<'static>,
    sta_netif: EspNetif,
    nvs: EspDefaultNvsPartition,
    firmware_version: &'static str,
    pending: Option<PendingWifiConnect>,
}

struct PendingWifiConnect {
    ssid: String,
    password: String,
    started_at: Instant,
    persist_on_success: bool,
}

impl WifiConnection {
    pub fn attach(
        mut driver: WifiDriver<'static>,
        nvs: EspDefaultNvsPartition,
        provisioned_ssid: Option<&str>,
        provisioned_password: Option<&str>,
        firmware_version: &'static str,
    ) -> anyhow::Result<Self> {
        driver.set_configuration(&Configuration::Client(ClientConfiguration::default()))?;
        driver.start()?;
        driver.stop()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while driver.is_started()? {
            if Instant::now() >= deadline {
                anyhow::bail!("Wi-Fi driver did not stop before lwIP attach")
            }
            delay_current_task(Duration::from_millis(25));
        }
        let sta_netif = EspNetif::new(NetifStack::Sta)?;
        esp_result("esp_netif_attach_wifi_station", unsafe {
            esp_idf_svc::sys::esp_netif_attach_wifi_station(sta_netif.handle())
        })?;
        esp_result("esp_wifi_set_default_wifi_sta_handlers", unsafe {
            esp_idf_svc::sys::esp_wifi_set_default_wifi_sta_handlers()
        })?;
        let mut wifi = Self {
            driver,
            sta_netif,
            nvs,
            firmware_version,
            pending: None,
        };
        match load_wifi_credentials(wifi.nvs.clone(), provisioned_ssid, provisioned_password) {
            Ok((ssid, password)) => {
                if let Err(error) = wifi.begin_connect(&ssid, &password, false) {
                    log::warn!("saved Wi-Fi connection could not start: {error:#}");
                }
            }
            Err(error) => log::warn!("Wi-Fi is not configured: {error:#}"),
        }
        Ok(wifi)
    }

    pub fn scan(&mut self) -> anyhow::Result<Vec<WifiNetworkFacts>> {
        if !self.driver.is_started()? {
            self.driver.start()?;
        }
        let (access_points, _) = self.driver.scan_n::<16>()?;
        let mut networks = access_points
            .into_iter()
            .filter(|access_point| !access_point.ssid.is_empty())
            .map(|access_point| WifiNetworkFacts {
                ssid: access_point.ssid.as_str().to_owned(),
                rssi_dbm: access_point.signal_strength as i16,
                secured: access_point.auth_method != Some(AuthMethod::None),
            })
            .collect::<Vec<_>>();
        networks.sort_by_key(|network| core::cmp::Reverse(network.rssi_dbm));
        networks.dedup_by(|left, right| left.ssid == right.ssid);
        networks.truncate(5);
        Ok(networks)
    }

    pub fn begin_connect(
        &mut self,
        ssid: &str,
        password: &str,
        persist_on_success: bool,
    ) -> anyhow::Result<()> {
        validate_wifi_ssid(ssid)?;
        validate_wifi_password(password)?;
        if self.driver.is_started()? {
            let _ = self.driver.disconnect();
            delay_current_task(Duration::from_millis(50));
        }
        self.driver
            .set_configuration(&Configuration::Client(ClientConfiguration {
                ssid: ssid.try_into()?,
                bssid: None,
                auth_method: if password.is_empty() {
                    AuthMethod::None
                } else {
                    AuthMethod::WPA2Personal
                },
                password: password.try_into()?,
                channel: None,
                ..Default::default()
            }))?;
        if !self.driver.is_started()? {
            self.driver.start()?;
        }
        esp_result("disable Wi-Fi modem power save", unsafe {
            esp_idf_svc::sys::esp_wifi_set_ps(esp_idf_svc::sys::wifi_ps_type_t_WIFI_PS_NONE)
        })?;
        self.driver.connect()?;
        self.pending = Some(PendingWifiConnect {
            ssid: ssid.to_owned(),
            password: password.to_owned(),
            started_at: Instant::now(),
            persist_on_success,
        });
        Ok(())
    }

    pub fn poll_connect(&mut self) -> Option<anyhow::Result<()>> {
        let pending = self.pending.as_ref()?;
        match (self.driver.is_connected(), self.sta_netif.is_up()) {
            (Ok(true), Ok(true)) => {
                let pending = self.pending.take().expect("pending Wi-Fi connect");
                if pending.persist_on_success {
                    let result = (|| {
                        let storage =
                            EspDefaultNvs::new(self.nvs.clone(), DEVICE_NVS_NAMESPACE, true)?;
                        storage.set_str(WIFI_NVS_SSID_KEY, &pending.ssid)?;
                        storage.set_str(WIFI_NVS_PASSWORD_KEY, &pending.password)?;
                        Ok::<(), anyhow::Error>(())
                    })();
                    return Some(result);
                }
                Some(Ok(()))
            }
            (Err(error), _) => {
                self.pending = None;
                Some(Err(error.into()))
            }
            (_, Err(error)) => {
                self.pending = None;
                Some(Err(error.into()))
            }
            _ if pending.started_at.elapsed() >= Duration::from_secs(15) => {
                let _ = self.driver.disconnect();
                self.pending = None;
                Some(Err(anyhow::anyhow!("Wi-Fi association or DHCP timed out")))
            }
            _ => None,
        }
    }

    pub fn is_connecting(&self) -> bool {
        self.pending.is_some()
    }

    pub fn is_connected(&self) -> bool {
        self.driver.is_connected().unwrap_or(false) && self.sta_netif.is_up().unwrap_or(false)
    }

    pub fn forget(&mut self) -> anyhow::Result<()> {
        self.pending = None;
        if self.driver.is_connected()? {
            self.driver.disconnect()?;
        }
        let storage = EspDefaultNvs::new(self.nvs.clone(), DEVICE_NVS_NAMESPACE, true)?;
        storage.remove(WIFI_NVS_SSID_KEY)?;
        storage.remove(WIFI_NVS_PASSWORD_KEY)?;
        Ok(())
    }

    pub fn facts(&self, status: impl Into<String>) -> SettingsFacts {
        let mut facts = SettingsFacts {
            firmware_version: self.firmware_version.into(),
            workspace_free_bytes: storage::workspace_free_bytes().ok(),
            ..Default::default()
        };
        facts.wifi.status = status.into();
        let mut access_point = unsafe { core::mem::zeroed::<esp_idf_svc::sys::wifi_ap_record_t>() };
        if unsafe { esp_idf_svc::sys::esp_wifi_sta_get_ap_info(&mut access_point) }
            == esp_idf_svc::sys::ESP_OK
        {
            let end = access_point
                .ssid
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(access_point.ssid.len());
            facts.wifi.connected_ssid =
                Some(String::from_utf8_lossy(&access_point.ssid[..end]).into_owned());
            facts.wifi.rssi_dbm = Some(access_point.rssi as i16);
            facts.wifi.ip_address = self
                .sta_netif
                .get_ip_info()
                .ok()
                .map(|info| info.ip.to_string());
        }
        facts
    }
}

fn load_wifi_credentials(
    partition: EspDefaultNvsPartition,
    provisioned_ssid: Option<&str>,
    provisioned_password: Option<&str>,
) -> anyhow::Result<(String, String)> {
    let storage = EspDefaultNvs::new(partition, DEVICE_NVS_NAMESPACE, true)?;
    if let (Some(ssid), Some(password)) = (provisioned_ssid, provisioned_password) {
        validate_wifi_ssid(ssid)?;
        validate_wifi_password(password)?;
        storage.set_str(WIFI_NVS_SSID_KEY, ssid)?;
        storage.set_str(WIFI_NVS_PASSWORD_KEY, password)?;
        return Ok((ssid.to_owned(), password.to_owned()));
    }
    if provisioned_ssid.is_some() || provisioned_password.is_some() {
        anyhow::bail!("Wi-Fi SSID and password must be supplied together")
    }
    let mut ssid_buf = [0u8; 33];
    let mut password_buf = [0u8; 64];
    let ssid = storage.get_str(WIFI_NVS_SSID_KEY, &mut ssid_buf)?;
    let password = storage.get_str(WIFI_NVS_PASSWORD_KEY, &mut password_buf)?;
    if let (Some(ssid), Some(password)) = (ssid, password) {
        validate_wifi_ssid(ssid)?;
        validate_wifi_password(password)?;
        return Ok((ssid.to_owned(), password.to_owned()));
    }
    anyhow::bail!("Wi-Fi credentials are not configured")
}

fn validate_wifi_ssid(ssid: &str) -> anyhow::Result<()> {
    if ssid.is_empty() || ssid.len() > 32 {
        anyhow::bail!("Wi-Fi SSID must contain 1 to 32 bytes")
    }
    Ok(())
}

fn validate_wifi_password(password: &str) -> anyhow::Result<()> {
    if password.is_empty() {
        return Ok(());
    }
    if !(8..=63).contains(&password.len()) {
        anyhow::bail!("Wi-Fi password must contain 8 to 63 bytes")
    }
    if !password.is_ascii() {
        anyhow::bail!("Wi-Fi password must be ASCII")
    }
    Ok(())
}
