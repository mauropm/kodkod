use super::wire::*;
use crate::device::meshtastic::pb::{decode_fields, WireValue};

pub fn parse_from_radio(payload: &[u8]) -> FromRadio {
    let fields = decode_fields(payload);
    for (f, v) in &fields {
        match (f, v) {
            (3, WireValue::Varint(n)) => return FromRadio::MyNodeNum(*n as u32),
            (7, WireValue::Varint(n)) => return FromRadio::ConfigComplete(*n as u32),
            (8, _) => return FromRadio::Rebooted,
            (13, WireValue::Bytes(b)) => {
                let ver = decode_fields(b)
                    .into_iter()
                    .find(|(f, _)| *f == 1)
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or_default();
                return FromRadio::Firmware(ver);
            }
            (4, WireValue::Bytes(b)) => {
                let mut num = 0u32;
                let mut long = None;
                let mut short = None;
                for (nf, nv) in decode_fields(b) {
                    match nf {
                        1 => num = nv.as_varint().unwrap_or(0) as u32,
                        2 => {
                            for (uf, uv) in decode_fields(nv.as_bytes().unwrap_or(&[])) {
                                match uf {
                                    2 => long = uv.as_str(),
                                    3 => short = uv.as_str(),
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
                return FromRadio::NodeInfo { num, long, short };
            }
            (5, WireValue::Bytes(b)) => {
                for (cf, cv) in decode_fields(b) {
                    if cf == 6 {
                        if let Some(cb) = cv.as_bytes() {
                            let mut region = None;
                            let mut sf = None;
                            let mut bw = None;
                            for (lf, lv) in decode_fields(cb) {
                                match lf {
                                    3 => bw = lv.as_varint().map(|v| v as u32),
                                    4 => sf = lv.as_varint().map(|v| v as u32),
                                    7 => region = lv.as_varint(),
                                    _ => {}
                                }
                            }
                            return FromRadio::LoRaConfig { region, sf, bw };
                        }
                    }
                }
                return FromRadio::Other;
            }
            (10, WireValue::Bytes(b)) => {
                let mut index = 0u64;
                let mut name = None;
                for (cf, cv) in decode_fields(b) {
                    match cf {
                        1 => index = cv.as_varint().unwrap_or(0),
                        2 => {
                            for (sf_, sv) in decode_fields(cv.as_bytes().unwrap_or(&[])) {
                                if sf_ == 3 {
                                    name = sv.as_str();
                                }
                            }
                        }
                        _ => {}
                    }
                }
                return FromRadio::Channel { index, name };
            }
            (2, WireValue::Bytes(b)) => return FromRadio::Packet(parse_mesh_packet(b)),
            _ => {}
        }
    }
    FromRadio::Other
}

fn as_u32(v: &WireValue) -> Option<u32> {
    match v {
        WireValue::Varint(x) => Some(*x as u32),
        WireValue::Fixed32(bits) => Some(*bits),
        _ => None,
    }
}

fn parse_mesh_packet(b: &[u8]) -> MeshPacketView {
    let mut view = MeshPacketView {
        from: 0,
        to: BROADCAST,
        channel: 0,
        id: 0,
        portnum: None,
        payload: Vec::new(),
        request_id: None,
        rssi: None,
        snr: None,
    };
    for (f, v) in decode_fields(b) {
        match f {
            1 => view.from = as_u32(&v).unwrap_or(0),
            2 => view.to = as_u32(&v).unwrap_or(BROADCAST),
            3 => view.channel = v.as_varint().unwrap_or(0) as u32,
            6 => view.id = as_u32(&v).unwrap_or(0),
            8 => view.snr = v.as_f32(),
            12 => view.rssi = Some(v.as_i32().unwrap_or(0) as i16),
            4 | 5 => {
                if let Some(pb_bytes) = v.as_bytes() {
                    for (df, dv) in decode_fields(pb_bytes) {
                        match df {
                            1 => view.portnum = dv.as_varint(),
                            2 => {
                                if let Some(p) = dv.as_bytes() {
                                    view.payload = p.to_vec();
                                }
                            }
                            6 => view.request_id = as_u32(&dv),
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    view
}
