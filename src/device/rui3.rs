use super::handle::{DeviceCommand, DeviceHandle, EventReceiver};
use super::serial::{write_line, LineReader, ReadOutcome};
use super::types::{Capabilities, CapabilityState, DeviceEvent, DeviceInfo, RadioStats};
use crate::error::{Error, Result};
use crate::regions::{self, Bandwidth, CodingRate, RadioSettings};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_serial::SerialStream;

const DEFAULT_BAUD: u32 = 115200;

fn capabilities() -> Capabilities {
    Capabilities {
        lora_p2p: CapabilityState::Supported,
        lorawan: CapabilityState::Supported,
        ble_peripheral: CapabilityState::RequiresFirmwareSupport,
        gnss: CapabilityState::RequiresOptionalModule,
        battery: CapabilityState::RequiresFirmwareSupport,
        gpio: CapabilityState::Supported,
        adc: CapabilityState::Supported,
        sensors: Vec::new(),
    }
}

fn info(path: &str, version: &str) -> DeviceInfo {
    DeviceInfo {
        vendor: "RAKwireless".into(),
        model: "RAK4631 (WisBlock Core)".into(),
        firmware_name: "RUI3".into(),
        firmware_version: version.into(),
        serial_number: String::new(),
        node_id: None,
        transport: format!("usb-serial:{path}@{DEFAULT_BAUD}"),
    }
}

fn bw_index(bw: Bandwidth) -> u32 {
    match bw {
        Bandwidth::Bw125 => 0,
        Bandwidth::Bw250 => 1,
        Bandwidth::Bw500 => 2,
    }
}

fn cr_index(cr: CodingRate) -> u32 {
    u32::from(cr.denom() - 5)
}

fn p2p_combined(s: &RadioSettings) -> String {
    let hz = (s.frequency_khz * 1000.0).round() as u64;
    format!(
        "AT+P2P={hz}:{}:{}:{}:{}:{}",
        s.spreading_factor,
        s.bandwidth.khz(),
        cr_index(s.coding_rate),
        s.preamble_len,
        s.tx_power_dbm
    )
}

pub(crate) fn parse_evt_rxp2p(line: &str) -> Option<(i16, f32, Vec<u8>)> {
    let rest = line.strip_prefix("+EVT:RXP2P:")?;
    let mut parts = rest.splitn(3, ':');
    let rssi = parts.next()?.trim().parse::<i16>().ok()?;
    let snr = parts.next()?.trim().parse::<f32>().ok()?;
    let hex = parts.next()?.trim();
    let bytes = crate::crypto::hex_decode(hex)?;
    Some((rssi, snr, bytes))
}

pub async fn quick_probe(path: &str) -> bool {
    tokio::time::timeout(Duration::from_secs(5), async {
        quick_probe_inner(path).await
    })
    .await
    .unwrap_or(false)
}

async fn quick_probe_inner(path: &str) -> bool {
    let Ok(mut port) = super::serial::open(path, DEFAULT_BAUD) else {
        return false;
    };
    let mut lr = LineReader::new();
    lr.drain_idle(&mut port, 300).await;
    if write_line(&mut port, "AT").await.is_err() {
        return false;
    }
    let deadline = tokio::time::Instant::now() + Duration::from_millis(700);
    while tokio::time::Instant::now() < deadline {
        match lr.next_line(&mut port).await {
            Ok(ReadOutcome::Line(l)) => {
                let t = l.trim();
                if t == "OK" {
                    return true;
                }
                if is_error_response(t) {
                    return false;
                }
            }
            Ok(_) => break,
            Err(_) => break,
        }
    }
    false
}

fn is_error_response(t: &str) -> bool {
    t == "ERROR"
        || t.starts_with("AT_ERROR")
        || t.starts_with("AT_BUSY_ERROR")
        || t == "PARAM_ERROR"
        || t == "AT_TIMEOUT"
}

enum CmdOutcome {
    Ok(Vec<String>),
    Err(String),
    Timeout,
}

async fn command(
    port: &mut SerialStream,
    lr: &mut LineReader,
    cmd: &str,
    timeout_ms: u64,
) -> CmdOutcome {
    if write_line(port, cmd).await.is_err() {
        return CmdOutcome::Err("write failed".into());
    }
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    let mut extra = Vec::new();
    while tokio::time::Instant::now() < deadline {
        match lr.next_line(port).await {
            Ok(ReadOutcome::Line(l)) => {
                let t = l.trim().to_string();
                if t.is_empty() || t == cmd {
                    continue;
                }
                if t == "OK" {
                    return CmdOutcome::Ok(extra);
                }
                if is_error_response(&t) {
                    return CmdOutcome::Err(t);
                }
                extra.push(t);
            }
            Ok(ReadOutcome::Idle) => {}
            Ok(ReadOutcome::Eof) => return CmdOutcome::Err("port closed".into()),
            Err(e) => return CmdOutcome::Err(e.to_string()),
        }
    }
    CmdOutcome::Timeout
}

async fn expect_ok(
    port: &mut SerialStream,
    lr: &mut LineReader,
    cmd: &str,
    timeout_ms: u64,
) -> Result<()> {
    match command(port, lr, cmd, timeout_ms).await {
        CmdOutcome::Ok(_) => Ok(()),
        CmdOutcome::Err(e) => Err(Error::device(format!("{cmd}: {e}"))),
        CmdOutcome::Timeout => Err(Error::device(format!("{cmd}: no response (timeout)"))),
    }
}

async fn wait_alive(port: &mut SerialStream, lr: &mut LineReader, attempts: usize) -> bool {
    for _ in 0..attempts {
        if matches!(command(port, lr, "AT", 800).await, CmdOutcome::Ok(_)) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(600)).await;
    }
    false
}

async fn init_radio(
    port: &mut SerialStream,
    lr: &mut LineReader,
    s: &RadioSettings,
) -> Result<String> {
    lr.drain_idle(port, 500).await;
    if !wait_alive(port, lr, 3).await {
        return Err(Error::device("RAK4631 not responding to AT commands"));
    }

    let version = match command(port, lr, "AT+VER=?", 900).await {
        CmdOutcome::Ok(lines) => lines
            .iter()
            .find(|l| l.contains("RUI_"))
            .cloned()
            .unwrap_or_else(|| "unknown".into()),
        _ => "unknown".into(),
    };

    let _ = expect_ok(port, lr, "AT+NWM=0", 1200).await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    lr.drain_idle(port, 300).await;
    if !wait_alive(port, lr, 5).await {
        return Err(Error::device("module unresponsive after P2P mode switch"));
    }

    let combined = p2p_combined(s);
    if expect_ok(port, lr, &combined, 1500).await.is_err() {
        let hz = (s.frequency_khz * 1000.0).round() as u64;
        let cmds = [
            format!("AT+PFREQ={hz}"),
            format!("AT+PSF={}", s.spreading_factor),
            format!("AT+PBW={}", bw_index(s.bandwidth)),
            format!("AT+PCR={}", cr_index(s.coding_rate)),
            format!("AT+PPL={}", s.preamble_len),
            format!("AT+PTP={}", s.tx_power_dbm),
        ];
        for c in &cmds {
            if let Err(e) = expect_ok(port, lr, c, 900).await {
                return Err(Error::device(format!(
                    "radio rejected {combined} and also failed on {}: {}",
                    c.split('=').next().unwrap_or(c),
                    e
                )));
            }
        }
    }

    expect_ok(port, lr, "AT+PRECV=65533", 1200).await?;
    Ok(version)
}

pub async fn connect(path: &str, radio: &RadioSettings) -> Result<(DeviceHandle, EventReceiver)> {
    let connect_fut = async {
        let mut port = super::serial::open(path, DEFAULT_BAUD)?;
        let mut lr = LineReader::new();
        let version = init_radio(&mut port, &mut lr, radio).await?;
        Ok::<_, Error>((port, lr, version))
    };
    let (port, lr, version) = tokio::time::timeout(Duration::from_secs(30), connect_fut)
        .await
        .map_err(|_| {
            Error::device("RUI3 connection timed out; is another program holding the port?")
        })??;

    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    let (event_tx, event_rx) = mpsc::channel(256);
    let dev_info = info(path, &version);
    let handle_info = dev_info.clone();

    let dev_info = info(path, &version);
    tokio::spawn(session(port, lr, cmd_rx, event_tx, radio.clone(), dev_info));

    Ok((
        DeviceHandle {
            cmd_tx,
            info: handle_info,
        },
        event_rx,
    ))
}

async fn session(
    mut port: SerialStream,
    mut lr: LineReader,
    mut cmd_rx: mpsc::Receiver<DeviceCommand>,
    event_tx: mpsc::Sender<DeviceEvent>,
    radio: RadioSettings,
    dev_info: DeviceInfo,
) {
    let mut stats = RadioStats::default();

    loop {
        tokio::select! {
            read = lr.next_line(&mut port) => match read {
                Ok(ReadOutcome::Line(line)) => handle_line(&line, &event_tx, &mut stats).await,
                Ok(ReadOutcome::Idle) => {}
                Ok(_) | Err(_) => {
                    let _ = event_tx.send(DeviceEvent::Disconnected("serial port closed".into())).await;
                    break;
                }
            },
            cmd = cmd_rx.recv() => match cmd {
                None => break,
                Some(DeviceCommand::Reset(tx)) => {
                    let _ = expect_ok(&mut port, &mut lr, "ATZ", 1500).await;
                    let _ = tx.send(Ok("module rebooted".into()));
                    let _ = event_tx.send(DeviceEvent::Disconnected("device reset".into())).await;
                    break;
                }
                Some(DeviceCommand::Info(tx)) => { let _ = tx.send(Ok(dev_info.clone())); }
                Some(DeviceCommand::Capabilities(tx)) => { let _ = tx.send(Ok(capabilities())); }
                Some(DeviceCommand::Stats(tx)) => { let _ = tx.send(Ok(stats.clone())); }
                Some(DeviceCommand::ConfigureRadio(s, tx)) => {
                    let outcome = reconfigure(&mut port, &mut lr, &s).await.map(|_| s.as_ref().clone());
                    let _ = radio;
                    let _ = tx.send(outcome);
                }
                Some(DeviceCommand::Send { data, want_ack: _, reply }) => {
                    let _ = reply.send(send_frame(&mut port, &mut lr, &data).await.map(|_| rand::random()));
                    stats.tx_count += 1;
                }
            },
        }
    }
}

async fn handle_line(line: &str, event_tx: &mpsc::Sender<DeviceEvent>, stats: &mut RadioStats) {
    let t = line.trim();
    if t.is_empty() {
        return;
    }
    if t.starts_with("+EVT:RXP2P:") {
        if let Some((rssi, snr, bytes)) = parse_evt_rxp2p(t) {
            stats.rx_count += 1;
            stats.last_rssi = Some(rssi);
            stats.last_snr = Some(snr);
            let _ = event_tx
                .send(DeviceEvent::Frame(
                    super::types::RxMeta {
                        rssi: Some(rssi),
                        snr: Some(snr),
                        received_at: std::time::SystemTime::now(),
                    },
                    bytes,
                ))
                .await;
        }
        return;
    }
    if t.contains("RXP2P RECEIVE TIMEOUT") || t.contains("RXP2P: RECEIVE ERROR") {
        stats.dropped += 1;
        return;
    }
    if t.contains("TXP2P DONE") {
        let _ = event_tx.send(DeviceEvent::Log("TX completed".into())).await;
        return;
    }
    if !t.starts_with('+') && !t.starts_with("AT_") && t != "OK" && t != "ERROR" {
        let _ = event_tx.send(DeviceEvent::Log(t.to_string())).await;
    }
}

fn to_hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02X}")).collect()
}

async fn send_frame(port: &mut SerialStream, lr: &mut LineReader, data: &[u8]) -> Result<()> {
    let cmd = format!("AT+PSEND={}", to_hex(data));
    match expect_ok(port, lr, &cmd, 2500).await {
        Ok(()) => Ok(()),
        Err(Error::Device(msg)) if msg.contains("BUSY") => {
            let _ = expect_ok(port, lr, "AT+PRECV=0", 800).await;
            let r = expect_ok(port, lr, &cmd, 2500).await;
            let _ = expect_ok(port, lr, "AT+PRECV=65533", 1200).await;
            r.map_err(|_| Error::device("AT+PSEND busy even after RX restart"))
        }
        Err(e) => Err(e),
    }
}

async fn reconfigure(
    port: &mut SerialStream,
    lr: &mut LineReader,
    s: &RadioSettings,
) -> Result<()> {
    regions::validate(s)?;
    let _ = expect_ok(port, lr, "AT+PRECV=0", 800).await;
    if expect_ok(port, lr, &p2p_combined(s), 1500).await.is_err() {
        for c in individual_param_cmds(s) {
            expect_ok(port, lr, &c, 900).await?;
        }
    }
    expect_ok(port, lr, "AT+PRECV=65533", 1200).await
}

fn individual_param_cmds(s: &RadioSettings) -> Vec<String> {
    let hz = (s.frequency_khz * 1000.0).round() as u64;
    vec![
        format!("AT+PFREQ={hz}"),
        format!("AT+PSF={}", s.spreading_factor),
        format!("AT+PBW={}", bw_index(s.bandwidth)),
        format!("AT+PCR={}", cr_index(s.coding_rate)),
        format!("AT+PPL={}", s.preamble_len),
        format!("AT+PTP={}", s.tx_power_dbm),
    ]
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn parses_evt_line() {
        let (rssi, snr, bytes) = parse_evt_rxp2p("+EVT:RXP2P:-60:11:AABBCC").expect("parse");
        assert_eq!(rssi, -60);
        assert_eq!(snr, 11.0);
        assert_eq!(bytes, vec![0xaa, 0xbb, 0xcc]);
    }

    #[test]
    fn parses_evt_float_snr() {
        let (_, snr, _) = parse_evt_rxp2p("+EVT:RXP2P:-29:7.5:00").expect("parse");
        assert_eq!(snr, 7.5);
    }

    #[test]
    fn rejects_malformed_events() {
        assert!(parse_evt_rxp2p("+EVT:RXP2P RECEIVE TIMEOUT").is_none());
        assert!(parse_evt_rxp2p("+EVT:RXP2P: RECEIVE ERROR").is_none());
        assert!(parse_evt_rxp2p("+EVT:RXP2P:-60:11:ZZ").is_none());
        assert!(parse_evt_rxp2p("+EVT:TXP2P DONE").is_none());
    }

    #[test]
    fn combined_p2p_string_format() {
        let mut s = RadioSettings::default();
        s.bandwidth = Bandwidth::Bw125;
        s.coding_rate = CodingRate::Cr45;
        assert_eq!(p2p_combined(&s), "AT+P2P=915000000:7:125:0:8:14");
    }

    #[test]
    fn hex_encoding_uppercase() {
        assert_eq!(to_hex(&[0xde, 0xad, 0x01]), "DEAD01");
    }

    #[test]
    fn error_classifier() {
        assert!(is_error_response("ERROR"));
        assert!(is_error_response("AT_BUSY_ERROR"));
        assert!(!is_error_response("+EVT:RXP2P:-60:11:AABB"));
    }

    use crate::protocol::FLAG_WANT_ACK;
    use crate::protocol::{Frame, FrameType};

    #[test]
    fn wire_roundtrip_through_adapter_path() {
        let mut f = Frame::new(FrameType::Data, 9, 1, 2, b"x".to_vec());
        f.header.flags |= FLAG_WANT_ACK;
        let hex = to_hex(&f.to_wire());
        let bytes = crate::crypto::hex_decode(&hex).expect("hex");
        assert_eq!(Frame::from_wire(&bytes).unwrap().header.msg_id, 9);
    }
}
