pub mod handle;
pub mod mock;
pub mod types;

use crate::error::Error;
use types::FirmwareKind;

#[cfg(feature = "ble")]
pub mod ble;
pub mod meshtastic;
pub mod mock_data;
mod mock_sim;
pub mod rui3;
pub mod serial;

pub struct DetectedDevice {
    pub path: String,
    pub kind: FirmwareKind,
    pub usb_vendor: Option<String>,
    pub usb_product: Option<String>,
    pub serial: Option<String>,
}

impl DetectedDevice {
    pub fn compatibility(&self) -> &'static str {
        match self.kind {
            FirmwareKind::Rui3 | FirmwareKind::Meshtastic => "compatible",
            FirmwareKind::Mock => "mock",
            FirmwareKind::Unknown => "unsupported firmware (no documented host protocol)",
        }
    }
}

pub fn list_candidate_ports() -> Vec<DetectedDevice> {
    let mut out = Vec::new();
    let ports = match serialport::available_ports() {
        Ok(p) => p,
        Err(_) => return out,
    };
    for p in ports {
        let name = p.port_name.clone();
        match p.port_type {
            serialport::SerialPortType::UsbPort(ref info) => {
                let vendor = usb_id_name(info.vid);
                let product = info.product.clone();
                let is_rak4631 = (info.vid == 0x239a && matches!(info.pid, 0x0029 | 0x8029))
                    || info.vid == 0x23a1
                    || product.as_deref().unwrap_or("").contains("RAK");
                if !is_rak4631 && !looks_like_serial_tty(&name) {
                    continue;
                }
                out.push(DetectedDevice {
                    path: name.clone(),
                    kind: FirmwareKind::Unknown,
                    usb_vendor: Some(vendor),
                    usb_product: product,
                    serial: info.serial_number.clone(),
                });
                continue;
            }
            _ => {
                if looks_like_serial_tty(&name) {
                    out.push(DetectedDevice {
                        path: name,
                        kind: FirmwareKind::Unknown,
                        usb_vendor: None,
                        usb_product: None,
                        serial: None,
                    });
                }
            }
        }
    }
    for entry in std::fs::read_dir("/dev").into_iter().flatten() {
        let name = match entry {
            Ok(e) => e.file_name().to_string_lossy().to_string(),
            Err(_) => continue,
        };
        let is_tty = (name.starts_with("ttyACM") || name.starts_with("ttyUSB"))
            && !out.iter().any(|d| d.path.ends_with(&name));
        if is_tty {
            out.push(DetectedDevice {
                path: format!("/dev/{name}"),
                kind: FirmwareKind::Unknown,
                usb_vendor: None,
                usb_product: None,
                serial: None,
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out.dedup_by(|a, b| a.path == b.path);
    out
}

fn looks_like_serial_tty(name: &str) -> bool {
    name.contains("ttyACM") || name.contains("ttyUSB")
}

fn usb_id_name(vid: u16) -> String {
    match vid {
        0x239a => "Adafruit".into(),
        0x23a1 => "RAKwireless".into(),
        0x10c4 => "Silicon Labs".into(),
        0x1a86 => "WCH".into(),
        _ => format!("VID {vid:04x}"),
    }
}

pub async fn probe_firmware(path: &str) -> FirmwareKind {
    if std::fs::metadata(path).is_err() {
        return FirmwareKind::Unknown;
    }
    for attempt in 0..2 {
        if attempt > 0 {
            tracing::debug!("probe attempt {} for {path}", attempt + 1);
            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        }
        if rui3::quick_probe(path).await {
            return FirmwareKind::Rui3;
        }
        if meshtastic::quick_probe(path).await {
            return FirmwareKind::Meshtastic;
        }
    }
    FirmwareKind::Unknown
}

pub async fn scan() -> Vec<DetectedDevice> {
    let mut devices = list_candidate_ports();
    for d in &mut devices {
        d.kind = probe_firmware(&d.path).await;
    }
    devices
}

pub(crate) fn open_error(path: &str, e: impl std::fmt::Display) -> Error {
    Error::device(format!(
        "cannot open {path}: {e}. Check permissions (dialout group) and that no other program holds the port"
    ))
}
