use crate::device::meshtastic::pb::PbWriter;
use crate::device::types::RxMeta;
use std::time::SystemTime;

pub const PORT_TEXT_MESSAGE: u64 = 1;
pub const PORT_NODEINFO: u64 = 4;
pub const PORT_ROUTING: u64 = 5;
pub const PORT_TELEMETRY: u64 = 67;
pub const PORT_PRIVATE_APP: u64 = 256;

pub const BROADCAST: u32 = 0xffff_ffff;
pub const MAX_FRAME: usize = 512;

#[derive(Debug, Clone, PartialEq)]
pub enum FromRadio {
    MyNodeNum(u32),
    Firmware(String),
    NodeInfo {
        num: u32,
        long: Option<String>,
        short: Option<String>,
    },
    LoRaConfig {
        region: Option<u64>,
        sf: Option<u32>,
        bw: Option<u32>,
    },
    Channel {
        index: u64,
        name: Option<String>,
    },
    ConfigComplete(u32),
    Packet(MeshPacketView),
    Rebooted,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeshPacketView {
    pub from: u32,
    pub to: u32,
    pub channel: u32,
    pub id: u32,
    pub portnum: Option<u64>,
    pub payload: Vec<u8>,
    pub request_id: Option<u32>,
    pub rssi: Option<i16>,
    pub snr: Option<f32>,
}

impl MeshPacketView {
    pub fn meta(&self) -> RxMeta {
        RxMeta {
            rssi: self.rssi,
            snr: self.snr,
            received_at: SystemTime::now(),
        }
    }
}

pub fn frame_bytes(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 4);
    out.push(0x94);
    out.push(0xc3);
    out.push((payload.len() >> 8) as u8);
    out.push((payload.len() & 0xff) as u8);
    out.extend_from_slice(payload);
    out
}

pub fn want_config(id: u32) -> Vec<u8> {
    frame_bytes(&want_config_proto(id))
}

pub fn want_config_proto(id: u32) -> Vec<u8> {
    let mut w = PbWriter::default();
    w.varint(100, id as u64);
    w.buf
}

pub fn build_text_packet(
    to: u32,
    channel: u32,
    id: u32,
    want_ack: bool,
    portnum: u64,
    payload: &[u8],
) -> Vec<u8> {
    frame_bytes(&text_packet_proto(
        to, channel, id, want_ack, portnum, payload,
    ))
}

pub fn text_packet_proto(
    to: u32,
    channel: u32,
    id: u32,
    want_ack: bool,
    portnum: u64,
    payload: &[u8],
) -> Vec<u8> {
    let mut pkt = PbWriter::default();
    pkt.fixed32(1, to);
    pkt.fixed32(2, BROADCAST);
    pkt.varint(3, channel as u64);
    pkt.nested(4, |d| {
        d.varint(1, portnum);
        d.bytes(2, payload);
    });
    pkt.fixed32(6, id);
    if want_ack {
        pkt.varint(10, 1);
    }
    let mut tr = PbWriter::default();
    tr.bytes(1, &pkt.buf);
    tr.buf
}
