pub mod decode;
pub mod pb;
pub mod wire;

use super::handle::{DeviceCommand, DeviceHandle, EventReceiver};
use super::types::{Capabilities, CapabilityState, DeviceEvent, DeviceInfo, RadioStats};
use crate::error::{Error, Result};
use crate::protocol::Frame;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_serial::SerialStream;
use wire::*;

pub const BAUDS: [u32; 2] = [115200, 921600];
const WANT_CONFIG_ID: u32 = 0x1a2b_3c4d;
const RESYNC: [u8; 4] = [0x94; 4];

pub(crate) struct Snapshot {
    pub my_node_num: Option<u32>,
    pub firmware: String,
    pub region_code: Option<u64>,
    pub channels: Vec<String>,
}

pub(crate) fn capabilities() -> Capabilities {
    Capabilities {
        lora_p2p: CapabilityState::Unsupported,
        lorawan: CapabilityState::Unsupported,
        ble_peripheral: CapabilityState::RequiresFirmwareSupport,
        gnss: CapabilityState::RequiresOptionalModule,
        battery: CapabilityState::Supported,
        gpio: CapabilityState::NotDetected,
        adc: CapabilityState::NotDetected,
        sensors: Vec::new(),
    }
}

pub(crate) fn dev_info(path: &str, baud: u32, snap: &Snapshot) -> DeviceInfo {
    DeviceInfo {
        vendor: "Meshtastic project".into(),
        model: "RAK4631".into(),
        firmware_name: "Meshtastic".into(),
        firmware_version: if snap.firmware.is_empty() {
            "unknown".into()
        } else {
            snap.firmware.clone()
        },
        serial_number: String::new(),
        node_id: snap.my_node_num.map(|n| format!("!{n:08x}")),
        transport: format!("usb-serial:{path}@{baud}"),
    }
}

pub struct FrameReader {
    buf: Vec<u8>,
}

impl Default for FrameReader {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameReader {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(2048),
        }
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        loop {
            if self.buf.len() < 4 {
                break;
            }
            if self.buf[0] != 0x94 || self.buf[1] != 0xc3 {
                let skip = self
                    .buf
                    .windows(2)
                    .position(|w| w[0] == 0x94 && w[1] == 0xc3)
                    .unwrap_or(self.buf.len().saturating_sub(1));
                self.buf.drain(..skip.max(1));
                continue;
            }
            let len = ((self.buf[2] as usize) << 8) | self.buf[3] as usize;
            if len > MAX_FRAME {
                self.buf.drain(..2);
                continue;
            }
            if self.buf.len() < 4 + len {
                break;
            }
            let payload: Vec<u8> = self.buf.drain(..4 + len).skip(4).collect();
            out.push(payload);
        }
        if self.buf.len() > 64 * 1024 {
            self.buf.clear();
        }
        out
    }
}

async fn write_all(port: &mut SerialStream, data: &[u8]) -> Result<()> {
    port.write_all(data)
        .await
        .map_err(|e| Error::device(format!("serial write failed: {e}")))?;
    port.flush()
        .await
        .map_err(|e| Error::device(format!("serial flush failed: {e}")))
}

async fn read_available(port: &mut SerialStream, buf: &mut [u8]) -> std::io::Result<usize> {
    match port.read(buf).await {
        Ok(n) => Ok(n),
        Err(e)
            if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut =>
        {
            Ok(0)
        }
        Err(e) => Err(e),
    }
}

pub async fn quick_probe(path: &str) -> bool {
    for baud in BAUDS {
        let attempt = async {
            let Ok(mut port) = super::serial::open(path, baud) else {
                return ProbeOutcome::Error;
            };
            let mut reader = FrameReader::new();
            let deadline = tokio::time::Instant::now() + Duration::from_millis(3500);
            let mut chunk = [0u8; 512];
            let mut last_send = tokio::time::Instant::now() - Duration::from_secs(10);
            let mut banner_hit = false;
            while tokio::time::Instant::now() < deadline {
                if last_send.elapsed() > Duration::from_millis(1500) {
                    let _ = write_all(&mut port, &RESYNC).await;
                    let _ = write_all(&mut port, &want_config(WANT_CONFIG_ID)).await;
                    last_send = tokio::time::Instant::now();
                }
                match read_available(&mut port, &mut chunk).await {
                    Ok(0) => tokio::time::sleep(Duration::from_millis(25)).await,
                    Ok(n) => {
                        if !banner_hit && looks_like_meshtastic_banner(&chunk[..n]) {
                            banner_hit = true;
                        }
                        if !reader.feed(&chunk[..n]).is_empty() {
                            return ProbeOutcome::Framed;
                        }
                    }
                    Err(_) => return ProbeOutcome::Error,
                }
            }
            if banner_hit {
                return ProbeOutcome::Banner;
            }
            ProbeOutcome::Silent
        };
        match tokio::time::timeout(Duration::from_secs(12), attempt)
            .await
            .unwrap_or(ProbeOutcome::Silent)
        {
            ProbeOutcome::Framed | ProbeOutcome::Banner => return true,
            ProbeOutcome::Error => return false,
            ProbeOutcome::Silent => {}
        }
    }
    false
}

enum ProbeOutcome {
    Framed,
    Banner,
    Silent,
    Error,
}

fn looks_like_meshtastic_banner(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    text.contains("DEBUG") || text.contains(".proto") || text.contains("meshtastic")
}

async fn handshake(port: &mut SerialStream) -> Result<Snapshot> {
    write_all(port, &RESYNC).await?;
    write_all(port, &want_config(WANT_CONFIG_ID)).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut reader = FrameReader::new();
    let mut snap = Snapshot {
        my_node_num: None,
        firmware: String::new(),
        region_code: None,
        channels: Vec::new(),
    };
    let mut chunk = [0u8; 512];
    let mut last_send = tokio::time::Instant::now();
    while tokio::time::Instant::now() < deadline {
        if last_send.elapsed() > Duration::from_millis(2000) {
            let _ = write_all(port, &want_config(WANT_CONFIG_ID)).await;
            last_send = tokio::time::Instant::now();
        }
        let n = read_available(port, &mut chunk).await?;
        if n == 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            continue;
        }
        for frame in reader.feed(&chunk[..n]) {
            match decode::parse_from_radio(&frame) {
                FromRadio::ConfigComplete(id) if id == WANT_CONFIG_ID => return Ok(snap),
                FromRadio::MyNodeNum(n) => snap.my_node_num = Some(n),
                FromRadio::Firmware(v) => snap.firmware = v,
                FromRadio::LoRaConfig { region, .. } => snap.region_code = region,
                FromRadio::Channel { name: Some(n), .. } if !n.is_empty() => snap.channels.push(n),
                _ => {}
            }
        }
    }
    if snap.my_node_num.is_some() {
        Ok(snap)
    } else {
        Err(Error::device("no Meshtastic response to want_config_id"))
    }
}

pub async fn connect(path: &str) -> Result<(DeviceHandle, EventReceiver)> {
    let mut last_err = String::from("no baud attempted");
    for baud in BAUDS {
        let Ok(mut port) = super::serial::open(path, baud) else {
            continue;
        };
        match handshake(&mut port).await {
            Ok(snap) => {
                let info = dev_info(path, baud, &snap);
                let (cmd_tx, cmd_rx) = mpsc::channel(32);
                let (event_tx, event_rx) = mpsc::channel(256);
                tokio::spawn(session(port, cmd_rx, event_tx, snap, baud));
                return Ok((DeviceHandle { cmd_tx, info }, event_rx));
            }
            Err(e) => last_err = e.to_string(),
        }
    }
    Err(Error::device(format!(
        "Meshtastic connect failed on {path}: {last_err}"
    )))
}

fn channel_of(_snap: &Snapshot) -> u32 {
    0
}

async fn session(
    mut port: SerialStream,
    mut cmd_rx: mpsc::Receiver<DeviceCommand>,
    event_tx: mpsc::Sender<DeviceEvent>,
    snap: Snapshot,
    _baud: u32,
) {
    let mut stats = RadioStats::default();
    let mut reader = FrameReader::new();
    let mut chunk = [0u8; 512];

    loop {
        tokio::select! {
            read = read_available(&mut port, &mut chunk) => match read {
                Ok(n) if n > 0 => {
                    for frame in reader.feed(&chunk[..n]) {
                        handle_from_radio(&frame, &event_tx, &mut stats).await;
                    }
                }
                Ok(_) => tokio::time::sleep(Duration::from_millis(15)).await,
                Err(_) => {
                    let _ = event_tx.send(DeviceEvent::Disconnected("serial error".into())).await;
                    break;
                }
            },
            cmd = cmd_rx.recv() => match cmd {
                None | Some(DeviceCommand::Reset(_)) => break,
                Some(DeviceCommand::Info(tx)) => { let _ = tx.send(Ok(dev_info("", 0, &snap))); }
                Some(DeviceCommand::Capabilities(tx)) => { let _ = tx.send(Ok(capabilities())); }
                Some(DeviceCommand::Stats(tx)) => { let _ = tx.send(Ok(stats.clone())); }
                Some(DeviceCommand::ConfigureRadio(_, tx)) => {
                    let _ = tx.send(Err(Error::Unsupported(
                        "changing radio settings is not supported over the Meshtastic client API; use a Meshtastic admin client".into(),
                    )));
                }
                Some(DeviceCommand::Send { data, want_ack, reply }) => {
                    let id = rand::random();
                    let (to, portnum, payload, wa) = outbound_mapping(&data, want_ack);
                    let frame = build_text_packet(to, channel_of(&snap), id, wa, portnum, &payload);
                    match write_all(&mut port, &frame).await {
                        Ok(()) => { stats.tx_count += 1; let _ = reply.send(Ok(id)); }
                        Err(e) => { let _ = reply.send(Err(e)); }
                    }
                }
            },
        }
    }
}

pub(crate) fn outbound_mapping(data: &[u8], default_want_ack: bool) -> (u32, u64, Vec<u8>, bool) {
    use crate::protocol::{Frame, FrameType};
    match Frame::from_wire(data) {
        Ok(f)
            if f.header.frame_type == FrameType::Data
                && !f.header.is_encrypted()
                && f.header.frag_total <= 1
                && std::str::from_utf8(&f.payload).is_ok() =>
        {
            (
                BROADCAST,
                PORT_TEXT_MESSAGE,
                f.payload,
                default_want_ack || f.header.wants_ack(),
            )
        }
        other => {
            let raw = match other {
                Ok(f) if !f.payload.is_empty() && f.header.frag_total <= 1 => f.payload,
                _ => data.to_vec(),
            };
            (BROADCAST, PORT_PRIVATE_APP, raw, false)
        }
    }
}

pub(crate) async fn handle_from_radio(
    frame: &[u8],
    event_tx: &mpsc::Sender<DeviceEvent>,
    stats: &mut RadioStats,
) {
    match decode::parse_from_radio(frame) {
        FromRadio::Packet(p) => {
            stats.rx_count += 1;
            if let Some(rssi) = p.rssi {
                stats.last_rssi = Some(rssi);
            }
            if let Some(snr) = p.snr {
                stats.last_snr = Some(snr);
            }
            match p.portnum {
                Some(PORT_TEXT_MESSAGE) => {
                    let kkp = Frame::new(
                        crate::protocol::FrameType::Data,
                        p.id,
                        p.from,
                        (p.id & 0xffff) as u16,
                        p.payload.clone(),
                    );
                    let _ = event_tx
                        .send(DeviceEvent::Frame(p.meta(), kkp.to_wire()))
                        .await;
                }
                Some(PORT_PRIVATE_APP) => {
                    let _ = event_tx
                        .send(DeviceEvent::Frame(p.meta(), p.payload.clone()))
                        .await;
                }
                Some(PORT_NODEINFO) => {
                    let _ = event_tx
                        .send(DeviceEvent::NodeReport {
                            node_id: p.from,
                            name: None,
                            short: None,
                        })
                        .await;
                }
                Some(PORT_ROUTING) => {
                    if let Some(rid) = p.request_id {
                        let ok = p.payload.first().map(|b| b & 0x7f == 0).unwrap_or(true);
                        if ok {
                            stats.acks_received += 1;
                            let _ = event_tx.send(DeviceEvent::Acked(rid)).await;
                        } else {
                            stats.dropped += 1;
                            let _ = event_tx
                                .send(DeviceEvent::Log("routing NAK received".into()))
                                .await;
                        }
                    }
                }
                Some(PORT_TELEMETRY) => {
                    for (f, v) in pb::decode_fields(&p.payload) {
                        if f == 1 {
                            if let Some(b) = v.as_bytes() {
                                let mut percent = None;
                                let mut voltage = None;
                                for (df, dv) in pb::decode_fields(b) {
                                    match df {
                                        1 => percent = dv.as_varint().map(|x| x as u8),
                                        2 => voltage = dv.as_f32(),
                                        _ => {}
                                    }
                                }
                                let _ = event_tx
                                    .send(DeviceEvent::Battery { percent, voltage })
                                    .await;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        FromRadio::NodeInfo { num, long, short } => {
            let _ = event_tx
                .send(DeviceEvent::NodeReport {
                    node_id: num,
                    name: long,
                    short,
                })
                .await;
        }
        FromRadio::Rebooted => {
            let _ = event_tx
                .send(DeviceEvent::Disconnected("device rebooted".into()))
                .await;
        }
        FromRadio::Firmware(v) if !v.is_empty() => {
            let _ = event_tx
                .send(DeviceEvent::Log(format!("firmware {v}")))
                .await;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{FrameType, FLAG_WANT_ACK};

    #[test]
    fn frame_reader_resync_and_split() {
        let mut r = FrameReader::new();
        let a = frame_bytes(&[1, 2, 3]);
        let b = frame_bytes(&[9; 10]);
        let mut stream = vec![0x00, 0x12];
        stream.extend_from_slice(&a);
        stream.extend_from_slice(&[0xff, 0xee]);
        stream.extend_from_slice(&b);
        let frames = r.feed(&stream);
        assert_eq!(frames, vec![vec![1, 2, 3], vec![9; 10]]);
    }

    #[test]
    fn frame_reader_partial_then_rest() {
        let mut r = FrameReader::new();
        let full = frame_bytes(&(0..20).collect::<Vec<u8>>());
        assert!(r.feed(&full[..5]).is_empty());
        assert!(r.feed(&full[5..15]).is_empty());
        let out = r.feed(&full[15..]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], (0..20).collect::<Vec<u8>>());
    }

    #[test]
    fn oversized_len_is_discarded() {
        let mut r = FrameReader::new();
        let mut bad = vec![0x94, 0xc3, 0xff, 0xff];
        bad.extend_from_slice(&[7u8; 32]);
        assert!(r.feed(&bad).is_empty());
    }

    #[test]
    fn want_config_encodes_field_100() {
        let f = want_config(7);
        assert_eq!(&f[..4], &[0x94, 0xc3, 0x00, 0x03]);
        assert_eq!(&f[4..], &[0xa0, 0x06, 0x07]);
    }

    #[test]
    fn text_packet_roundtrip_through_decoder() {
        use crate::device::meshtastic::decode::parse_from_radio;
        use crate::device::meshtastic::pb::{decode_fields, PbWriter};

        let toradio = build_text_packet(0x11223344, 0, 99, true, PORT_TEXT_MESSAGE, b"hello");
        let inner = decode_fields(&toradio[4..])
            .into_iter()
            .find(|(f, _)| *f == 1)
            .and_then(|(_, v)| v.as_bytes().map(<[u8]>::to_vec))
            .expect("ToRadio.packet");

        let mut from_radio = PbWriter::default();
        from_radio.bytes(2, &inner);
        let mut tr = FrameReader::new();
        let frames = tr.feed(&frame_bytes(&from_radio.buf));
        assert_eq!(frames.len(), 1);
        match parse_from_radio(&frames[0]) {
            FromRadio::Packet(v) => {
                assert_eq!(v.to, BROADCAST);
                assert_eq!(v.id, 99);
                assert_eq!(v.portnum, Some(PORT_TEXT_MESSAGE));
                assert_eq!(v.payload, b"hello");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn outbound_plain_text_maps_to_text_port() {
        let f = crate::protocol::Frame::new(FrameType::Data, 5, 1, 1, b"hi there".to_vec());
        let (to, portnum, payload, wa) = outbound_mapping(&f.to_wire(), false);
        assert_eq!(to, BROADCAST);
        assert_eq!(portnum, PORT_TEXT_MESSAGE);
        assert_eq!(payload, b"hi there");
        assert!(!wa);
    }

    #[test]
    fn outbound_want_ack_propagates() {
        let mut f = crate::protocol::Frame::new(FrameType::Data, 5, 1, 1, b"ack me".to_vec());
        f.header.flags |= FLAG_WANT_ACK;
        let (_, _, _, wa) = outbound_mapping(&f.to_wire(), false);
        assert!(wa);
    }

    #[test]
    fn outbound_binary_maps_to_private_app() {
        let f = crate::protocol::Frame::new(FrameType::Data, 6, 1, 1, vec![0xff, 0xfe, 0xfd]);
        let (_, portnum, payload, _) = outbound_mapping(&f.to_wire(), false);
        assert_eq!(portnum, PORT_PRIVATE_APP);
        assert_eq!(payload, vec![0xff, 0xfe, 0xfd]);
    }
}
