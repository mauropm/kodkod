pub mod dedup;
pub mod fragment;
pub mod wire;

use crate::error::Result;
use serde::{Deserialize, Serialize};
pub use wire::{put_varint, Reader};

pub const PROTOCOL_VERSION: u8 = 1;

pub const FLAG_ENCRYPTED: u8 = 0x01;
pub const FLAG_AUTHENTICATED: u8 = 0x02;
pub const FLAG_EMERGENCY: u8 = 0x04;
pub const FLAG_WANT_ACK: u8 = 0x08;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum FrameType {
    Data = 1,
    Ack = 2,
    Hello = 3,
    Telemetry = 4,
    NodeInfo = 5,
}

impl FrameType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(FrameType::Data),
            2 => Some(FrameType::Ack),
            3 => Some(FrameType::Hello),
            4 => Some(FrameType::Telemetry),
            5 => Some(FrameType::NodeInfo),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameHeader {
    pub version: u8,
    pub frame_type: FrameType,
    pub flags: u8,
    pub msg_id: u32,
    pub sender: u32,
    pub seq: u16,
    pub timestamp: u32,
    pub frag_total: u8,
    pub frag_index: u8,
}

impl FrameHeader {
    pub fn wants_ack(&self) -> bool {
        self.flags & FLAG_WANT_ACK != 0
    }
    pub fn is_encrypted(&self) -> bool {
        self.flags & FLAG_ENCRYPTED != 0
    }
    pub fn is_authenticated(&self) -> bool {
        self.flags & FLAG_AUTHENTICATED != 0
    }
    pub fn is_fragmented(&self) -> bool {
        self.frag_total > 1
    }

    pub fn encode_aad(&self, payload_len: usize) -> Vec<u8> {
        let mut aad = Vec::with_capacity(32);
        aad.push(self.version);
        aad.push(((self.frame_type as u8) << 4) | (self.flags & 0x0f));
        put_varint(&mut aad, self.msg_id as u64);
        put_varint(&mut aad, self.sender as u64);
        put_varint(&mut aad, self.seq as u64);
        put_varint(&mut aad, self.timestamp as u64);
        aad.push(self.frag_total);
        aad.push(self.frag_index);
        put_varint(&mut aad, payload_len as u64);
        aad
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub header: FrameHeader,
    pub payload: Vec<u8>,
    pub nonce: Option<[u8; 24]>,
}

pub fn now_secs() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as u32)
        .unwrap_or(0)
}

impl Frame {
    pub fn new(
        frame_type: FrameType,
        msg_id: u32,
        sender: u32,
        seq: u16,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            header: FrameHeader {
                version: PROTOCOL_VERSION,
                frame_type,
                flags: 0,
                msg_id,
                sender,
                seq,
                timestamp: now_secs(),
                frag_total: 1,
                frag_index: 0,
            },
            payload,
            nonce: None,
        }
    }

    pub fn to_wire(&self) -> Vec<u8> {
        let h = &self.header;
        let mut out = Vec::with_capacity(48 + self.payload.len());
        out.push(h.version);
        out.push(((h.frame_type as u8) << 4) | (h.flags & 0x0f));
        put_varint(&mut out, h.msg_id as u64);
        put_varint(&mut out, h.sender as u64);
        put_varint(&mut out, h.seq as u64);
        put_varint(&mut out, h.timestamp as u64);
        out.push(h.frag_total);
        out.push(h.frag_index);
        if let Some(n) = &self.nonce {
            out.extend_from_slice(n);
        }
        put_varint(&mut out, self.payload.len() as u64);
        out.extend_from_slice(&self.payload);
        out
    }

    pub fn from_wire(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let version = r.u8()?;
        if version != PROTOCOL_VERSION {
            return Err(crate::error::Error::protocol(format!(
                "unsupported protocol version {version}"
            )));
        }
        let tf = r.u8()?;
        let frame_type = FrameType::from_u8(tf >> 4).ok_or_else(|| {
            crate::error::Error::protocol(format!("unknown frame type {}", tf >> 4))
        })?;
        let flags = tf & 0x0f;
        let msg_id = r.varint()? as u32;
        let sender = r.varint()? as u32;
        let seq = r.varint()? as u16;
        let timestamp = r.varint()? as u32;
        let frag_total = r.u8()?;
        let frag_index = r.u8()?;
        let nonce = if flags & FLAG_ENCRYPTED != 0 {
            let b: &[u8] = r.take(24)?;
            let mut n = [0u8; 24];
            n.copy_from_slice(b);
            Some(n)
        } else {
            None
        };
        let plen = r.varint()? as usize;
        if plen > 4096 {
            return Err(crate::error::Error::protocol("payload too large"));
        }
        let payload = r.take(plen)?.to_vec();
        Ok(Self {
            header: FrameHeader {
                version,
                frame_type,
                flags,
                msg_id,
                sender,
                seq,
                timestamp,
                frag_total,
                frag_index,
            },
            payload,
            nonce,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        let mut f = Frame::new(FrameType::Data, 42, 0x7a39f2, 7, b"hello".to_vec());
        f.header.flags = FLAG_ENCRYPTED | FLAG_AUTHENTICATED | FLAG_WANT_ACK;
        f.nonce = Some([9u8; 24]);
        let wire = f.to_wire();
        assert!(wire.len() < 60);
        let back = Frame::from_wire(&wire).unwrap();
        assert_eq!(back.header.msg_id, 42);
        assert_eq!(back.payload, b"hello");
        assert_eq!(back.nonce.as_ref().unwrap()[3], 9);
        assert!(back.header.wants_ack());
    }

    #[test]
    fn rejects_unknown_version_and_type() {
        let f = Frame::new(FrameType::Data, 1, 1, 1, vec![1]);
        let mut w = f.to_wire();
        w[0] = 9;
        assert!(Frame::from_wire(&w).is_err());
        w[0] = 1;
        w[1] = (0x7 << 4) | 1;
        assert!(Frame::from_wire(&w).is_err());
    }

    #[test]
    fn truncated_frame_errors() {
        let f = Frame::new(FrameType::Data, 1, 2, 3, vec![4, 5, 6]);
        let w = f.to_wire();
        for cut in [1, 5, w.len() - 1] {
            assert!(Frame::from_wire(&w[..cut]).is_err(), "cut at {cut}");
        }
    }

    #[test]
    fn oversized_payload_declared_rejected() {
        let mut w = Frame::new(FrameType::Data, 1, 2, 3, vec![]).to_wire();
        let last = w.len() - 1;
        w[last] = 0xff;
        w.extend_from_slice(&[0xff, 0xff, 0x7f]);
        assert!(Frame::from_wire(&w).is_err());
    }
}
