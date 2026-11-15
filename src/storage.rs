use crate::protocol::Frame;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: u32,
    pub sender_id: u32,
    pub sender_name: String,
    pub channel: String,
    pub direct_to: Option<u32>,
    pub text: String,
    pub timestamp: u64,
    pub outgoing: bool,
    pub security_label: String,
    pub delivery: String,
}

pub struct MessageStore {
    path: PathBuf,
    messages: Vec<StoredMessage>,
    max_in_memory: usize,
}

impl MessageStore {
    pub fn open(path: PathBuf) -> std::io::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut messages = Vec::new();
        if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            for line in raw.lines() {
                if let Ok(m) = serde_json::from_str::<StoredMessage>(line) {
                    messages.push(m);
                }
            }
            if messages.len() > 4000 {
                messages.drain(..messages.len() - 2000);
            }
        }
        Ok(Self {
            path,
            max_in_memory: 2000,
            messages,
        })
    }

    pub fn append(&mut self, m: StoredMessage) -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(f, "{}", serde_json::to_string(&m)?)?;
        self.messages.push(m);
        if self.messages.len() > self.max_in_memory * 2 {
            let excess = self.messages.len() - self.max_in_memory;
            self.messages.drain(..excess);
        }
        Ok(())
    }

    pub fn update_delivery(&mut self, id: u32, status: &str) {
        for m in self.messages.iter_mut().rev() {
            if m.id == id && m.outgoing && m.delivery != status {
                m.delivery = status.to_string();
                break;
            }
        }
    }

    pub fn all(&self) -> &[StoredMessage] {
        &self.messages
    }

    pub fn channel_messages(&self, channel: &str) -> Vec<&StoredMessage> {
        self.messages
            .iter()
            .filter(|m| m.channel == channel)
            .collect()
    }

    pub fn rotate_if_large(&self, limit_bytes: u64) -> std::io::Result<bool> {
        if self.path.exists() {
            let size = std::fs::metadata(&self.path)?.len();
            if size > limit_bytes {
                std::fs::rename(&self.path, self.path.with_extension("jsonl.1"))?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn export(path: &Path, messages: &[StoredMessage]) -> std::io::Result<()> {
        let mut out = std::fs::File::create(path)?;
        for m in messages {
            writeln!(out, "{}", serde_json::to_string(m)?)?;
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct UnreadCounts(HashMap<String, usize>);

impl UnreadCounts {
    pub fn bump(&mut self, channel: &str) {
        *self.0.entry(channel.to_string()).or_insert(0) += 1;
    }
    pub fn clear(&mut self, channel: &str) {
        self.0.insert(channel.to_string(), 0);
    }
    pub fn get(&self, channel: &str) -> usize {
        self.0.get(channel).copied().unwrap_or(0)
    }
}

pub enum SecurityLabel {
    Encrypted,
    AuthenticatedOnly,
    Unauthenticated,
}

pub fn frame_security_label(f: &Frame, key_present: bool) -> SecurityLabel {
    match (
        f.header.is_encrypted(),
        f.header.is_authenticated(),
        key_present,
    ) {
        (true, true, true) => SecurityLabel::Encrypted,
        (true, false, true) => SecurityLabel::Encrypted,
        (_, true, true) => SecurityLabel::AuthenticatedOnly,
        _ => SecurityLabel::Unauthenticated,
    }
}
