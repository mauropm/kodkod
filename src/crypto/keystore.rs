use super::{hex_decode, Key, KEY_LEN};
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeyStore {
    #[serde(default)]
    pub channels: BTreeMap<String, String>,
    #[serde(default)]
    pub contacts: BTreeMap<String, String>,
    #[serde(default)]
    pub identity: BTreeMap<String, String>,
}

fn parse_key(s: &str) -> Option<Key> {
    let bytes = hex_decode(s)?;
    if bytes.len() == KEY_LEN {
        let mut k = [0u8; KEY_LEN];
        k.copy_from_slice(&bytes);
        Some(k)
    } else {
        None
    }
}

impl KeyStore {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => toml::from_str(&s).map_err(|e| Error::crypto(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Error::crypto(format!("reading keystore: {e}"))),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let s = toml::to_string_pretty(self).map_err(|e| Error::crypto(e.to_string()))?;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        f.write_all(s.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    pub fn channel_key(&self, channel: &str) -> Option<Key> {
        parse_key(self.channels.get(channel)?)
    }

    pub fn contact_key(&self, node_id: &str) -> Option<Key> {
        parse_key(self.contacts.get(node_id)?)
    }

    pub fn set_channel_key(&mut self, channel: &str, key: Key) {
        self.channels.insert(channel.to_string(), hex_encode(&key));
    }

    pub fn set_contact_key(&mut self, node_id: &str, key: Key) {
        self.contacts.insert(node_id.to_string(), hex_encode(&key));
    }
}

pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
