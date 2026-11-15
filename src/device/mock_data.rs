pub struct Peer {
    pub id: u32,
    pub name: &'static str,
    pub short: &'static str,
    pub rssi: i16,
    pub snr: f32,
}

#[derive(Clone)]
pub struct MockConfig {
    pub loss_rate: f64,
    pub dup_rate: f64,
    pub corrupt_rate: f64,
    pub peer_chat_enabled: bool,
    pub hello_interval_ms: u64,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            loss_rate: 0.05,
            dup_rate: 0.05,
            corrupt_rate: 0.02,
            peer_chat_enabled: true,
            hello_interval_ms: 15_000,
        }
    }
}

pub const PEERS: [Peer; 3] = [
    Peer {
        id: 0x91A2_0001,
        name: "BETA",
        short: "BETA",
        rssi: -82,
        snr: 7.5,
    },
    Peer {
        id: 0x21FF_0002,
        name: "CHARLIE",
        short: "CHRL",
        rssi: -104,
        snr: 1.5,
    },
    Peer {
        id: 0x33AA_0003,
        name: "DELTA",
        short: "DLTA",
        rssi: -118,
        snr: -9.0,
    },
];

pub const PEER_MESSAGES: &[&str] = &[
    "Are you there?",
    "Signal is good from my side.",
    "Battery at 63%, still fine.",
    "GPS sharing stays off for me.",
    "Meet at the usual spot.",
    "Copy that.",
];

pub(super) use super::types::DeviceInfo;

pub fn mock_info() -> DeviceInfo {
    DeviceInfo {
        vendor: "mock".into(),
        model: "virtual RAK4631".into(),
        firmware_name: "lora-chat-mock".into(),
        firmware_version: env!("CARGO_PKG_VERSION").into(),
        serial_number: "MOCK-0001".into(),
        node_id: Some("ALPHA".into()),
        transport: "in-memory".into(),
    }
}
