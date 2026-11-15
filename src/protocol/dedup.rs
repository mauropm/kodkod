use super::Frame;
use std::collections::HashSet;

pub struct Deduper {
    seen: HashSet<(u32, u32, u8)>,
    capacity: usize,
    order: std::collections::VecDeque<(u32, u32, u8)>,
}

impl Default for Deduper {
    fn default() -> Self {
        Self::new(1024)
    }
}

impl Deduper {
    pub fn new(capacity: usize) -> Self {
        Self {
            seen: HashSet::with_capacity(capacity),
            capacity,
            order: std::collections::VecDeque::new(),
        }
    }

    pub fn is_duplicate(&mut self, frame: &Frame) -> bool {
        let key = (
            frame.header.sender,
            frame.header.msg_id,
            frame.header.frag_index,
        );
        if !self.seen.insert(key) {
            return true;
        }
        self.order.push_back(key);
        while self.order.len() > self.capacity {
            if let Some(old) = self.order.pop_front() {
                self.seen.remove(&old);
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(sender: u32, id: u32) -> Frame {
        Frame::new(super::super::FrameType::Data, id, sender, 1, vec![])
    }

    #[test]
    fn first_seen_not_duplicate() {
        let mut d = Deduper::default();
        assert!(!d.is_duplicate(&frame(1, 1)));
    }

    #[test]
    fn repeated_is_duplicate() {
        let mut d = Deduper::default();
        let f = frame(1, 1);
        assert!(!d.is_duplicate(&f));
        assert!(d.is_duplicate(&f));
    }

    #[test]
    fn same_id_different_sender_ok() {
        let mut d = Deduper::default();
        assert!(!d.is_duplicate(&frame(1, 1)));
        assert!(!d.is_duplicate(&frame(2, 1)));
    }

    #[test]
    fn capacity_evicts_oldest() {
        let mut d = Deduper::new(4);
        for i in 0..6 {
            assert!(!d.is_duplicate(&frame(1, i)), "id {i} should be fresh");
        }
        assert!(
            !d.is_duplicate(&frame(1, 0)),
            "evicted id should be accepted again"
        );
        assert!(d.is_duplicate(&frame(1, 5)));
    }
}
