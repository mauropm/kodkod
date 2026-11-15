pub mod keystore;

use crate::error::{Error, Result};
use crate::protocol::FrameHeader;
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand::RngCore;

pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 24;
const REPLAY_WINDOW: u16 = 64;

pub type Key = [u8; KEY_LEN];
pub type Nonce = [u8; NONCE_LEN];

pub fn seal(key: &Key, header: &FrameHeader, plaintext: &[u8]) -> Result<(Vec<u8>, Nonce)> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);
    let aad = header.encode_aad(plaintext.len());
    let ct = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| Error::crypto("encryption failed"))?;
    Ok((ct, nonce_bytes))
}

pub const TAG_LEN: usize = 16;

pub fn open(
    key: &Key,
    header: &FrameHeader,
    nonce: &[u8; 24],
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    if ciphertext.len() < TAG_LEN {
        return Err(Error::crypto("ciphertext shorter than authentication tag"));
    }
    let cipher = XChaCha20Poly1305::new(key.into());
    let aad = header.encode_aad(ciphertext.len() - TAG_LEN);
    cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| Error::crypto("authentication failed"))
}

#[derive(Debug)]
struct SenderWindow {
    highest: u16,
    seen: u64,
}

#[derive(Debug, Default)]
pub struct ReplayGuard {
    windows: std::collections::HashMap<u32, Option<SenderWindow>>,
    max_nodes: usize,
}

impl ReplayGuard {
    pub fn new(max_nodes: usize) -> Self {
        Self {
            windows: std::collections::HashMap::new(),
            max_nodes,
        }
    }

    pub fn accept(&mut self, sender: u32, seq: u16) -> bool {
        if !self.windows.contains_key(&sender) && self.windows.len() >= self.max_nodes {
            if let Some(oldest) = self.windows.keys().next().copied() {
                self.windows.remove(&oldest);
            }
        }
        let entry = self.windows.entry(sender).or_insert(None);
        match entry {
            None => {
                *entry = Some(SenderWindow {
                    highest: seq,
                    seen: 1,
                });
                true
            }
            Some(w) => {
                let d = seq.wrapping_sub(w.highest) as i16;
                if d == 0 {
                    return false;
                }
                if d > 0 {
                    let ahead = d as u32;
                    w.seen = if ahead >= REPLAY_WINDOW as u32 {
                        1
                    } else {
                        w.seen.checked_shl(ahead).unwrap_or(0) | 1
                    };
                    w.highest = seq;
                    true
                } else {
                    let behind = (-d) as u32;
                    if behind >= REPLAY_WINDOW as u32 {
                        return false;
                    }
                    if w.seen & (1u64 << behind) != 0 {
                        return false;
                    }
                    w.seen |= 1u64 << behind;
                    true
                }
            }
        }
    }
}

pub fn generate_key() -> Key {
    let mut k = [0u8; KEY_LEN];
    rand::thread_rng().fill_bytes(&mut k);
    k
}

pub fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Frame, FrameType, FLAG_AUTHENTICATED, FLAG_ENCRYPTED};

    fn header() -> FrameHeader {
        let mut f = Frame::new(FrameType::Data, 7, 0xabc, 3, vec![]);
        f.header.flags = FLAG_ENCRYPTED | FLAG_AUTHENTICATED;
        f.header
    }

    #[test]
    fn seal_open_roundtrip() {
        let key = generate_key();
        let h = header();
        let (ct, nonce) = seal(&key, &h, b"secret message").unwrap();
        assert_ne!(ct, b"secret message");
        assert_eq!(open(&key, &h, &nonce, &ct).unwrap(), b"secret message");
    }

    #[test]
    fn wrong_key_fails() {
        let h = header();
        let (ct, nonce) = seal(&generate_key(), &h, b"data").unwrap();
        assert!(open(&generate_key(), &h, &nonce, &ct).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = generate_key();
        let h = header();
        let (mut ct, nonce) = seal(&key, &h, b"payload bytes here").unwrap();
        ct[0] ^= 1;
        assert!(open(&key, &h, &nonce, &ct).is_err());
    }

    #[test]
    fn tampered_header_fails() {
        let key = generate_key();
        let mut h = header();
        let (ct, nonce) = seal(&key, &h, b"x").unwrap();
        h.msg_id += 1;
        assert!(open(&key, &h, &nonce, &ct).is_err());
    }

    #[test]
    fn unique_nonces() {
        let key = generate_key();
        let h = header();
        let (_, n1) = seal(&key, &h, b"a").unwrap();
        let (_, n2) = seal(&key, &h, b"a").unwrap();
        assert_ne!(n1, n2);
    }

    fn guard_seq(g: &mut ReplayGuard, sender: u32, seq: u16) -> bool {
        g.accept(sender, seq)
    }

    #[test]
    fn replay_same_seq_rejected() {
        let mut g = ReplayGuard::new(16);
        assert!(guard_seq(&mut g, 1, 100));
        assert!(!guard_seq(&mut g, 1, 100));
    }

    #[test]
    fn in_window_out_of_order_accepted_once() {
        let mut g = ReplayGuard::new(16);
        assert!(guard_seq(&mut g, 1, 100));
        assert!(guard_seq(&mut g, 1, 95));
        assert!(!guard_seq(&mut g, 1, 95));
        assert!(
            guard_seq(&mut g, 1, 37),
            "lowest in-window sequence accepted"
        );
        assert!(!guard_seq(&mut g, 1, 36), "one below window rejected");
    }

    #[test]
    fn window_slides_forward() {
        let mut g = ReplayGuard::new(16);
        assert!(guard_seq(&mut g, 1, 1000));
        for s in 1001..1100u16 {
            if s == 1050 {
                continue;
            }
            assert!(guard_seq(&mut g, 1, s));
        }
        assert!(
            !guard_seq(&mut g, 1, 1035),
            "packet fallen off the window rejected"
        );
        assert!(
            guard_seq(&mut g, 1, 1050),
            "gap inside window accepted once"
        );
    }

    #[test]
    fn sequence_wraps_correctly() {
        let mut g = ReplayGuard::new(16);
        assert!(guard_seq(&mut g, 1, u16::MAX - 2));
        assert!(guard_seq(&mut g, 1, 0));
        assert!(guard_seq(&mut g, 1, 1));
        assert!(
            guard_seq(&mut g, 1, u16::MAX - 1),
            "recent pre-wrap sequence still in window"
        );
        assert!(
            !guard_seq(&mut g, 1, u16::MAX - 1),
            "and its replay is rejected"
        );
    }

    #[test]
    fn far_behind_across_wrap_rejected() {
        let mut g = ReplayGuard::new(16);
        assert!(guard_seq(&mut g, 1, 5));
        assert!(!guard_seq(&mut g, 1, u16::MAX - 70));
    }

    #[test]
    fn independent_senders() {
        let mut g = ReplayGuard::new(16);
        assert!(guard_seq(&mut g, 1, 5));
        assert!(guard_seq(&mut g, 2, 5));
    }
}
