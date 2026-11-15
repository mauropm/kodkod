use crate::protocol::now_secs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: u32,
    pub name: String,
    pub short_name: String,
    pub first_seen: u32,
    pub last_seen: u32,
    pub rssi: Option<i16>,
    pub snr: Option<f32>,
    pub trusted: bool,
    pub blocked: bool,
    pub muted: bool,
    pub gps_state: String,
}

pub fn is_placeholder_name(id: u32, name: &str) -> bool {
    name.eq_ignore_ascii_case(&format!("{id:06X}"))
}

impl Node {
    pub fn new(id: u32, name: Option<String>, short: Option<String>) -> Self {
        let now = now_secs();
        Self {
            id,
            name: name.clone().unwrap_or_else(|| format!("{id:06X}")),
            short_name: short.unwrap_or_else(|| {
                name.map(|n| n.chars().take(4).collect())
                    .unwrap_or_else(|| format!("{id:04X}"))
            }),
            first_seen: now,
            last_seen: now,
            rssi: None,
            snr: None,
            trusted: false,
            blocked: false,
            muted: false,
            gps_state: "Not shared".into(),
        }
    }

    pub fn seen(&mut self, rssi: Option<i16>, snr: Option<f32>) {
        self.last_seen = now_secs();
        if rssi.is_some() {
            self.rssi = rssi;
        }
        if snr.is_some() {
            self.snr = snr;
        }
    }

    pub fn last_seen_ago(&self) -> String {
        let now = now_secs() as i64;
        let d = (now - self.last_seen as i64).max(0);
        match d {
            0..=59 => format!("{d} sec ago"),
            60..=3599 => format!("{} min ago", d / 60),
            3600..=86399 => format!("{} h ago", d / 3600),
            _ => format!("{} d ago", d / 86400),
        }
    }

    pub fn presence_dot(&self) -> &'static str {
        if now_secs().saturating_sub(self.last_seen) < 300 {
            "●"
        } else {
            "○"
        }
    }
}

const MAX_NODES: usize = 512;

#[derive(Default)]
pub struct NodeRegistry {
    nodes: HashMap<u32, Node>,
}

impl NodeRegistry {
    pub fn get(&self, id: u32) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    pub fn observe(&mut self, id: u32, name: Option<String>, short: Option<String>) -> &mut Node {
        if !self.nodes.contains_key(&id) {
            if self.nodes.len() >= MAX_NODES {
                if let Some(oldest) = self
                    .nodes
                    .iter()
                    .min_by_key(|(_, n)| n.last_seen)
                    .map(|(k, _)| *k)
                {
                    self.nodes.remove(&oldest);
                }
            }
            self.nodes.insert(id, Node::new(id, name, short));
        } else if let Some(n) = self.nodes.get_mut(&id) {
            if let Some(name) = &name {
                if is_placeholder_name(id, &n.name) {
                    n.name = name.clone();
                }
            }
        }
        self.nodes.get_mut(&id).expect("just inserted")
    }

    pub fn all_sorted(&self) -> Vec<&Node> {
        let mut v: Vec<&Node> = self.nodes.values().collect();
        v.sort_by_key(|n| std::cmp::Reverse(n.last_seen));
        v
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn observe_creates_then_updates() {
        let mut reg = NodeRegistry::default();
        assert_eq!(reg.len(), 0);
        reg.observe(1, Some("BETA".into()), None);
        assert_eq!(reg.get(1).expect("n").name, "BETA");
        reg.observe(1, Some("BETA2".into()), None);
        assert_eq!(
            reg.get(1).expect("n").name,
            "BETA",
            "hex-default names get replaced once"
        );
    }

    #[test]
    fn hex_placeholder_replaced_by_real_name() {
        let mut reg = NodeRegistry::default();
        let n = reg.observe(0x1234, None, None);
        assert!(n.name.contains("1234"));
        reg.observe(0x1234, Some("CHARLIE".into()), None);
        assert_eq!(reg.get(0x1234).expect("n").name, "CHARLIE");
    }

    #[test]
    fn registry_evicts_oldest_beyond_capacity() {
        let mut reg = NodeRegistry::default();
        for i in 0..600u32 {
            reg.observe(i, None, None);
        }
        assert!(reg.len() <= 512);
        assert!(reg.get(599).is_some());
    }

    #[test]
    fn presence_dot_depends_on_recency() {
        let mut reg = NodeRegistry::default();
        let n = reg.observe(7, Some("X".into()), None);
        assert_eq!(n.presence_dot(), "●");
    }
}
