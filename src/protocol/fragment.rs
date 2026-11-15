use super::{now_secs, Frame};
use crate::error::Result;
use std::collections::HashMap;

pub const DEFAULT_MAX_FRAME_PAYLOAD: usize = 200;
pub const MAX_MESSAGE_LEN: usize = 4096;
const REASSEMBLY_TIMEOUT_SECS: u64 = 30;

pub struct Fragmenter {
    pub max_frame_payload: usize,
}

impl Default for Fragmenter {
    fn default() -> Self {
        Self {
            max_frame_payload: DEFAULT_MAX_FRAME_PAYLOAD,
        }
    }
}

impl Fragmenter {
    pub fn new(max_frame_payload: usize) -> Self {
        Self { max_frame_payload }
    }

    pub fn fragments_needed(&self, msg_len: usize) -> usize {
        if msg_len == 0 {
            1
        } else {
            msg_len.div_ceil(self.max_frame_payload)
        }
    }

    pub fn fragment(&self, base: &Frame, message: &[u8]) -> Result<Vec<Frame>> {
        if message.len() > MAX_MESSAGE_LEN {
            return Err(crate::error::Error::protocol(format!(
                "message {} bytes exceeds maximum {}",
                message.len(),
                MAX_MESSAGE_LEN
            )));
        }
        let total = self.fragments_needed(message.len()) as u8;
        let mut out = Vec::new();
        for (idx, chunk) in message.chunks(self.max_frame_payload.max(1)).enumerate() {
            let mut f = Frame::new(
                base.header.frame_type,
                base.header.msg_id,
                base.header.sender,
                base.header.seq.wrapping_add(idx as u16),
                chunk.to_vec(),
            );
            f.header.timestamp = base.header.timestamp;
            f.header.flags = base.header.flags;
            f.header.frag_total = total;
            f.header.frag_index = idx as u8;
            out.push(f);
        }
        Ok(out)
    }
}

struct PendingMessage {
    parts: HashMap<u8, Vec<u8>>,
    total: u8,
    started: u32,
}

#[derive(Default)]
pub struct Reassembler {
    pending: HashMap<(u32, u32), PendingMessage>,
}

#[derive(Debug, PartialEq)]
pub enum ReassembleOutcome {
    Complete(Vec<u8>),
    Incomplete,
}

impl Reassembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, frame: &Frame) -> Result<ReassembleOutcome> {
        let key = (frame.header.sender, frame.header.msg_id);
        let now = now_secs();
        self.expire(now);

        if !frame.header.is_fragmented() {
            return Ok(ReassembleOutcome::Complete(frame.payload.clone()));
        }

        let total = frame.header.frag_total;
        if total == 0 || frame.header.frag_index >= total {
            return Err(crate::error::Error::protocol("invalid fragment index"));
        }

        let entry = self.pending.entry(key).or_insert_with(|| PendingMessage {
            parts: HashMap::new(),
            total,
            started: now,
        });

        if entry.total != total {
            self.pending.remove(&key);
            return Err(crate::error::Error::protocol(
                "conflicting fragment totals for message",
            ));
        }

        entry
            .parts
            .insert(frame.header.frag_index, frame.payload.clone());
        if entry.parts.len() == total as usize {
            let entry = self.pending.remove(&key).expect("just inserted");
            let mut data = Vec::with_capacity(entry.parts.values().map(|v| v.len()).sum());
            for i in 0..total {
                data.extend_from_slice(&entry.parts[&i]);
            }
            Ok(ReassembleOutcome::Complete(data))
        } else {
            Ok(ReassembleOutcome::Incomplete)
        }
    }

    fn expire(&mut self, now: u32) {
        self.pending
            .retain(|_, p| u64::from(now.saturating_sub(p.started)) < REASSEMBLY_TIMEOUT_SECS);
    }
}

#[cfg(test)]
mod tests {
    use super::super::FrameType;
    use super::*;
    use ReassembleOutcome::*;

    fn frame(msg_id: u32, payload: Vec<u8>) -> Frame {
        Frame::new(FrameType::Data, msg_id, 100, 1, payload)
    }

    #[test]
    fn unfragmented_passes_through() {
        let mut re = Reassembler::new();
        let out = re.push(&frame(1, b"hi".to_vec())).unwrap();
        assert_eq!(out, ReassembleOutcome::Complete(b"hi".to_vec()));
    }

    #[test]
    fn roundtrip_multi_fragment() {
        let frag = Fragmenter::new(16);
        let msg: Vec<u8> = (0..50).collect();
        let frames = frag.fragment(&frame(9, vec![]), &msg).unwrap();
        assert_eq!(frames.len(), 4);

        let mut re = Reassembler::new();
        let outcomes: Vec<ReassembleOutcome> = frames.iter().map(|f| re.push(f).unwrap()).collect();
        assert_eq!(
            outcomes[..3],
            [Incomplete, Incomplete, Incomplete],
            "first three incomplete"
        );
        assert_eq!(outcomes[3], ReassembleOutcome::Complete(msg));
    }

    #[test]
    fn out_of_order_fragments() {
        let frag = Fragmenter::new(10);
        let frames = frag
            .fragment(&frame(5, vec![]), &(0..25).collect::<Vec<u8>>())
            .unwrap();
        let mut re = Reassembler::new();
        assert_eq!(re.push(&frames[2]).unwrap(), Incomplete);
        assert_eq!(re.push(&frames[0]).unwrap(), Incomplete);
        match re.push(&frames[1]).unwrap() {
            ReassembleOutcome::Complete(d) => assert_eq!(d, (0..25).collect::<Vec<u8>>()),
            o => panic!("expected complete, got {o:?}"),
        }
    }

    #[test]
    fn conflicting_totals_rejected() {
        let mut re = Reassembler::new();
        let mut a = frame(3, vec![0; 5]);
        a.header.frag_total = 2;
        a.header.frag_index = 0;
        let mut b = frame(3, vec![0; 5]);
        b.header.frag_total = 3;
        b.header.frag_index = 0;
        assert!(re.push(&a).unwrap() == Incomplete);
        assert!(re.push(&b).is_err());
    }

    #[test]
    fn invalid_fragment_index_rejected() {
        let mut re = Reassembler::new();
        let mut a = frame(4, vec![0; 5]);
        a.header.frag_total = 2;
        a.header.frag_index = 5;
        assert!(re.push(&a).is_err());
    }

    #[test]
    fn oversize_message_rejected() {
        let frag = Fragmenter::default();
        assert!(frag
            .fragment(&frame(1, vec![]), &vec![0u8; MAX_MESSAGE_LEN + 1])
            .is_err());
    }

    #[test]
    fn empty_message_single_fragment() {
        let frag = Fragmenter::new(16);
        assert_eq!(frag.fragments_needed(0), 1);
    }
}
