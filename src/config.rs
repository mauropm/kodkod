use crate::error::{Error, Result};
use crate::privacy::PrivacySettings;
use crate::regions::{Bandwidth, CodingRate, Region};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceConfig {
    pub transport: String,
    pub device_path: String,
    pub firmware: String,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            transport: "usb".into(),
            device_path: "auto".into(),
            firmware: "auto".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IdentityConfig {
    pub name: String,
    pub short_name: String,
    pub node_id: u32,
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            name: whoami(),
            short_name: "NODE".into(),
            node_id: 0,
        }
    }
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "ANON".into())
        .to_uppercase()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RadioConfig {
    pub region: String,
    pub frequency_khz: f64,
    pub tx_power_dbm: i8,
    pub spreading_factor: u8,
    pub bandwidth_khz: u32,
    pub coding_rate: String,
    pub preamble_len: u16,
    pub sync_word: u16,
}

impl Default for RadioConfig {
    fn default() -> Self {
        Self {
            region: "US915".into(),
            frequency_khz: 915_000.0,
            tx_power_dbm: 14,
            spreading_factor: 7,
            bandwidth_khz: 125,
            coding_rate: "4/5".into(),
            preamble_len: 8,
            sync_word: 0x1234,
        }
    }
}

impl RadioConfig {
    pub fn to_settings(&self) -> Result<crate::regions::RadioSettings> {
        let region = Region::from_label(&self.region)
            .ok_or_else(|| Error::config(format!("unknown region '{}'", self.region)))?;
        let bandwidth = match self.bandwidth_khz {
            125 => Bandwidth::Bw125,
            250 => Bandwidth::Bw250,
            500 => Bandwidth::Bw500,
            other => return Err(Error::config(format!("unsupported bandwidth {other} kHz"))),
        };
        let coding_rate = match self.coding_rate.as_str() {
            "4/5" => CodingRate::Cr45,
            "4/6" => CodingRate::Cr46,
            "4/7" => CodingRate::Cr47,
            "4/8" => CodingRate::Cr48,
            other => return Err(Error::config(format!("unsupported coding rate '{other}'"))),
        };
        crate::regions::validate(&crate::regions::RadioSettings {
            region,
            frequency_khz: self.frequency_khz,
            tx_power_dbm: self.tx_power_dbm,
            spreading_factor: self.spreading_factor,
            bandwidth,
            coding_rate,
            preamble_len: self.preamble_len,
            sync_word: self.sync_word,
            crc_on: true,
            iq_inverted: false,
            explicit_header: true,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatConfig {
    pub max_frame_payload: usize,
    pub retry_count: u32,
    pub retry_base_secs: u64,
    pub ack_timeout_secs: u64,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            max_frame_payload: 200,
            retry_count: 3,
            retry_base_secs: 4,
            ack_timeout_secs: 25,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GpsConfig {
    pub enabled: bool,
    pub store_local: bool,
    pub transmit: bool,
    pub precision: String,
}

impl Default for GpsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            store_local: false,
            transmit: false,
            precision: "km1".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub interval_seconds: u64,
    pub include_battery: bool,
    pub include_location: bool,
    pub include_environment: bool,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_seconds: 300,
            include_battery: false,
            include_location: false,
            include_environment: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub timestamps: bool,
    pub theme: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            timestamps: true,
            theme: "dark".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub device: DeviceConfig,
    pub identity: IdentityConfig,
    pub radio: RadioConfig,
    pub chat: ChatConfig,
    pub gps: GpsConfig,
    pub privacy: PrivacySettings,
    pub telemetry: TelemetryConfig,
    pub ui: UiConfig,
    pub channels: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            device: DeviceConfig::default(),
            identity: IdentityConfig::default(),
            radio: RadioConfig::default(),
            chat: ChatConfig::default(),
            gps: GpsConfig::default(),
            privacy: PrivacySettings::default(),
            telemetry: TelemetryConfig::default(),
            ui: UiConfig::default(),
            channels: vec!["General".into(), "Emergency".into()],
        }
    }
}

pub fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lora-chat")
        .join("config.toml")
}

pub fn keystore_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lora-chat")
        .join("keys.toml")
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lora-chat")
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| Error::config(format!("{}: {e}", path.display())))?;
        let cfg: Config = toml::from_str(&raw).map_err(|e| Error::config(e.to_string()))?;
        cfg.radio.to_settings()?;
        Ok(cfg)
    }

    pub fn load_or_default(path: &Path) -> Self {
        Config::load(path).unwrap_or_default()
    }

    pub fn merge_profile(&mut self, profile: &ProfileOverride) {
        if let Some(g) = &profile.gps {
            if let Some(v) = g.enabled {
                self.gps.enabled = v;
            }
            if let Some(v) = g.transmit {
                self.gps.transmit = v;
            }
            if let Some(v) = g.store_local {
                self.gps.store_local = v;
            }
        }
        if let Some(t) = &profile.telemetry {
            if let Some(v) = t.enabled {
                self.telemetry.enabled = v;
            }
        }
        if let Some(p) = &profile.privacy {
            if let Some(v) = p.share_presence {
                self.privacy.share_presence = v;
            }
            if let Some(v) = p.share_battery {
                self.privacy.share_battery = v;
            }
            if let Some(v) = p.share_telemetry {
                self.privacy.share_telemetry = v;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ProfileOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub gps: Option<GpsOverride>,
    pub telemetry: Option<TelemetryOverride>,
    pub privacy: Option<PrivacyOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GpsOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_local: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transmit: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TelemetryOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PrivacyOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_presence: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_battery: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_telemetry: Option<bool>,
}

pub fn load_profile(dir: &Path, name: &str) -> Result<ProfileOverride> {
    let path = dir.join("profiles").join(format!("{name}.toml"));
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| Error::config(format!("profile {}: {e}", path.display())))?;
    toml::from_str(&raw).map_err(|e| Error::config(format!("profile {}: {e}", path.display())))
}
