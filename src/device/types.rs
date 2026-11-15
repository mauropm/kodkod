use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    Supported,
    Detected,
    Unsupported,
    NotDetected,
    RequiresFirmwareSupport,
    RequiresOptionalModule,
    NotTested,
}

impl CapabilityState {
    pub fn label(self) -> &'static str {
        match self {
            CapabilityState::Supported => "supported",
            CapabilityState::Detected => "detected",
            CapabilityState::Unsupported => "unsupported",
            CapabilityState::NotDetected => "not detected",
            CapabilityState::RequiresFirmwareSupport => "requires firmware support",
            CapabilityState::RequiresOptionalModule => "requires optional module",
            CapabilityState::NotTested => "not tested",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SensorDesc {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub state: CapabilityState,
}

#[derive(Debug, Clone)]
pub struct Capabilities {
    pub lora_p2p: CapabilityState,
    pub lorawan: CapabilityState,
    pub ble_peripheral: CapabilityState,
    pub gnss: CapabilityState,
    pub battery: CapabilityState,
    pub gpio: CapabilityState,
    pub adc: CapabilityState,
    pub sensors: Vec<SensorDesc>,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            lora_p2p: CapabilityState::NotTested,
            lorawan: CapabilityState::NotTested,
            ble_peripheral: CapabilityState::NotTested,
            gnss: CapabilityState::RequiresOptionalModule,
            battery: CapabilityState::NotTested,
            gpio: CapabilityState::NotTested,
            adc: CapabilityState::NotTested,
            sensors: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DeviceInfo {
    pub vendor: String,
    pub model: String,
    pub firmware_name: String,
    pub firmware_version: String,
    pub serial_number: String,
    pub node_id: Option<String>,
    pub transport: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RadioStats {
    pub tx_count: u64,
    pub rx_count: u64,
    pub crc_errors: u64,
    pub dropped: u64,
    pub retries: u64,
    pub acks_received: u64,
    pub last_rssi: Option<i16>,
    pub last_snr: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct RxMeta {
    pub rssi: Option<i16>,
    pub snr: Option<f32>,
    pub received_at: std::time::SystemTime,
}

#[derive(Debug)]
pub enum DeviceEvent {
    Connected(Box<DeviceInfo>),
    Disconnected(String),
    Frame(RxMeta, Vec<u8>),
    Acked(u32),
    NodeReport {
        node_id: u32,
        name: Option<String>,
        short: Option<String>,
    },
    Battery {
        percent: Option<u8>,
        voltage: Option<f32>,
    },
    Log(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FirmwareKind {
    Rui3,
    Meshtastic,
    Mock,
    Unknown,
}

impl FirmwareKind {
    pub fn label(&self) -> &'static str {
        match self {
            FirmwareKind::Rui3 => "RAKwireless RUI3 (AT)",
            FirmwareKind::Meshtastic => "Meshtastic",
            FirmwareKind::Mock => "Mock device",
            FirmwareKind::Unknown => "Unknown",
        }
    }
}
