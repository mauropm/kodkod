use crate::config::Config;
use crate::crypto::keystore::KeyStore;
use crate::crypto::{open as crypto_open, seal as crypto_seal, ReplayGuard};
use crate::device::handle::DeviceHandle;
use crate::device::types::{DeviceEvent, DeviceInfo, RadioStats, RxMeta};
use crate::error::{Error, Result};
use crate::gps::{GpsStatus, LocationSource};
use crate::nodes::NodeRegistry;
use crate::privacy::PrivacySettings;
use crate::protocol::{
    dedup::Deduper,
    fragment::{Fragmenter, ReassembleOutcome, Reassembler},
    now_secs, Frame, FrameType, FLAG_AUTHENTICATED, FLAG_EMERGENCY, FLAG_ENCRYPTED, FLAG_WANT_ACK,
};
use crate::regions::RadioSettings;
use crate::storage::{MessageStore, StoredMessage, UnreadCounts};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum AppEvent {
    Connected(Box<DeviceInfo>),
    Disconnected(String),
    Message(StoredMessage),
    Delivery {
        id: u32,
        status: String,
    },
    StatusLine(String),
    Battery {
        percent: Option<u8>,
        voltage: Option<f32>,
    },
}

struct PendingAck {
    wire_frames: Vec<Vec<u8>>,
    retries_left: u32,
    next_attempt_secs: u32,
    base_secs: u64,
}

pub struct App {
    pub cfg: Config,
    pub privacy: PrivacySettings,
    pub radio: RadioSettings,
    pub keys: KeyStore,
    pub nodes: NodeRegistry,
    pub store: MessageStore,
    pub unread: UnreadCounts,
    pub stats: RadioStats,
    pub connected: Option<DeviceHandle>,
    pub device_info: Option<DeviceInfo>,
    pub battery: (Option<u8>, Option<f32>),
    pub gps_enabled: bool,
    pub gps_status: GpsStatus,
    pub seq: u16,
    my_msg_counter: u32,
    dedup: Deduper,
    reasm: Reassembler,
    frag: Fragmenter,
    replay: ReplayGuard,
    pending: HashMap<u32, PendingAck>,
    gps_source: Box<dyn LocationSource>,
    last_hello: u32,
}

pub fn sender_name(nodes: &NodeRegistry, id: u32) -> String {
    nodes
        .get(id)
        .map(|n| n.name.clone())
        .unwrap_or_else(|| format!("{id:06X}"))
}

impl App {
    pub fn new(
        cfg: Config,
        store: MessageStore,
        keys: KeyStore,
        gps_source: Box<dyn LocationSource>,
    ) -> Self {
        let mut cfg = cfg;
        if cfg.identity.node_id == 0 {
            use sha2::{Digest, Sha256};
            let hash = Sha256::digest(format!("lora-chat:{}", cfg.identity.name));
            let derived = u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]) & 0x7fff_ffff;
            cfg.identity.node_id = if derived == 0 { 1 } else { derived };
        }
        let privacy = cfg.privacy.clone();
        let radio = cfg.radio.to_settings().unwrap_or_default();
        let gps_enabled = cfg.gps.enabled;
        let frag = Fragmenter::new(cfg.chat.max_frame_payload);
        Self {
            privacy,
            radio,
            gps_enabled,
            frag,
            cfg,
            keys,
            store,
            gps_source,
            nodes: NodeRegistry::default(),
            unread: UnreadCounts::default(),
            stats: RadioStats::default(),
            connected: None,
            device_info: None,
            battery: (None, None),
            gps_status: GpsStatus::Off,
            seq: rand::random(),
            my_msg_counter: rand::random::<u32>() & 0xffff,
            dedup: Deduper::new(1024),
            reasm: Reassembler::new(),
            replay: ReplayGuard::new(256),
            pending: HashMap::new(),
            last_hello: 0,
        }
    }

    pub fn attach(&mut self, handle: DeviceHandle, info: DeviceInfo) {
        self.connected = Some(handle);
        self.device_info = Some(info);
    }

    pub fn detach(&mut self, reason: String) {
        self.connected = None;
        let _ = reason;
    }

    fn next_seq(&mut self) -> u16 {
        self.seq = self.seq.wrapping_add(1);
        self.seq
    }

    fn next_msg_id(&mut self) -> u32 {
        self.my_msg_counter = self.my_msg_counter.wrapping_add(1);
        (now_secs() << 12) ^ self.my_msg_counter ^ (rand::random::<u32>() & 0xfff)
    }
}

pub struct SendRequest<'a> {
    pub channel: &'a str,
    pub text: &'a str,
    pub direct_to: Option<u32>,
    pub emergency: bool,
    pub want_ack: bool,
}

impl App {
    pub async fn send_text(&mut self, req: SendRequest<'_>) -> Result<u32> {
        let handle = self
            .connected
            .as_ref()
            .ok_or_else(|| Error::device("not connected"))?
            .clone();

        let target_name = match req.direct_to {
            Some(id) => sender_name(&self.nodes, id),
            None => req.channel.to_string(),
        };
        let key = match req.direct_to {
            Some(id) => self
                .keys
                .contact_key(&target_name)
                .or_else(|| self.keys.contact_key(&format!("{id:06X}")))
                .or_else(|| self.keys.channel_key(req.channel)),
            None => self.keys.channel_key(req.channel),
        };

        if req.direct_to.is_some() && key.is_none() {
            return Err(Error::PrivacyBlocked(format!(
                "no encryption key for contact '{target_name}'. Refusing to send an unencrypted direct message. Add a shared key in keys.toml."
            )));
        }

        let mut base = Frame::new(
            FrameType::Data,
            self.next_msg_id(),
            self.cfg.identity.node_id,
            self.next_seq(),
            Vec::new(),
        );
        base.header.timestamp = now_secs();
        base.header.flags = 0;
        if req.want_ack || req.emergency {
            base.header.flags |= FLAG_WANT_ACK;
        }
        if req.emergency {
            base.header.flags |= FLAG_EMERGENCY;
        }

        let plain_frames = self.frag.fragment(&base, req.text.as_bytes())?;
        let mut wires = Vec::with_capacity(plain_frames.len());
        for mut f in plain_frames {
            if let Some(k) = &key {
                f.header.flags |= FLAG_ENCRYPTED | FLAG_AUTHENTICATED;
                let (ct, nonce) = crypto_seal(k, &f.header, &f.payload)?;
                f.payload = ct;
                f.nonce = Some(nonce);
            } else {
                f.header.timestamp = base.header.timestamp;
            }
            wires.push(f.to_wire());
        }
        for w in &wires {
            handle
                .send_frame(w.clone(), base.header.wants_ack())
                .await?;
        }

        let label = if key.is_some() {
            "Encrypted"
        } else {
            "Unauthenticated broadcast"
        };
        let msg_id = base.header.msg_id;
        self.store.append(StoredMessage {
            id: msg_id,
            sender_id: self.cfg.identity.node_id,
            sender_name: self.cfg.identity.name.clone(),
            channel: req.channel.to_string(),
            direct_to: req.direct_to,
            text: req.text.to_string(),
            timestamp: now_secs() as u64,
            outgoing: true,
            security_label: label.into(),
            delivery: "Sending".into(),
        })?;
        if base.header.wants_ack() {
            self.pending.insert(
                msg_id,
                PendingAck {
                    wire_frames: wires,
                    retries_left: self.cfg.chat.retry_count,
                    next_attempt_secs: now_secs() + self.cfg.chat.ack_timeout_secs as u32,
                    base_secs: self.cfg.chat.ack_timeout_secs,
                },
            );
        }
        Ok(msg_id)
    }
}

impl App {
    pub async fn on_device_event(&mut self, ev: DeviceEvent) -> Option<AppEvent> {
        match ev {
            DeviceEvent::Connected(info) => {
                self.device_info = Some(*info.clone());
                Some(AppEvent::Connected(Box::new(*info)))
            }
            DeviceEvent::Disconnected(reason) => {
                self.detach(reason.clone());
                Some(AppEvent::Disconnected(reason))
            }
            DeviceEvent::Frame(meta, bytes) => self.on_frame(meta, bytes).await,
            DeviceEvent::Acked(id) => {
                self.pending.remove(&id);
                self.store.update_delivery(id, "Delivered");
                Some(AppEvent::Delivery {
                    id,
                    status: "Delivered".into(),
                })
            }
            DeviceEvent::NodeReport {
                node_id,
                name,
                short,
            } => {
                self.nodes.observe(node_id, name, short);
                Some(AppEvent::StatusLine(format!("node {node_id:06X} reported")))
            }
            DeviceEvent::Battery { percent, voltage } => {
                self.battery = (percent, voltage);
                Some(AppEvent::Battery { percent, voltage })
            }
            DeviceEvent::Log(line) => Some(AppEvent::StatusLine(line)),
        }
    }

    pub async fn on_frame(&mut self, meta: RxMeta, bytes: Vec<u8>) -> Option<AppEvent> {
        let frame = match Frame::from_wire(&bytes) {
            Ok(f) => f,
            Err(_) => {
                self.stats.crc_errors += 1;
                return Some(AppEvent::StatusLine(
                    "dropped corrupt frame (decode failed)".into(),
                ));
            }
        };

        if frame.header.sender == self.cfg.identity.node_id
            && !matches!(
                self.device_info.as_ref().map(|d| d.firmware_name.as_str()),
                Some("Meshtastic")
            )
        {
            return None;
        }
        if self.dedup.is_duplicate(&frame) {
            self.stats.dropped += 1;
            return None;
        }

        let node = self.nodes.observe(frame.header.sender, None, None);
        node.seen(meta.rssi, meta.snr);
        let sender_name = node.name.clone();

        if frame.header.frame_type == FrameType::Hello {
            if let Ok(text) = String::from_utf8(frame.payload.clone()) {
                if let Some((long, short)) = text.split_once('|') {
                    let n = self.nodes.get_mut(frame.header.sender).expect("observed");
                    if crate::nodes::is_placeholder_name(frame.header.sender, &n.name) {
                        n.name = long.to_string();
                    }
                    n.short_name = short.to_string();
                }
            }
            return Some(AppEvent::StatusLine(format!("{sender_name} is present")));
        }

        let key = self.find_key_for(frame.header.sender);
        if frame.header.is_encrypted() {
            let Some(k) = key.as_ref() else {
                return Some(AppEvent::StatusLine(format!(
                    "encrypted frame from {sender_name} but no shared key; dropped"
                )));
            };
            let nonce = frame.nonce?;
            let Ok(plain) = crypto_open(k, &frame.header, &nonce, &frame.payload) else {
                self.stats.crc_errors += 1;
                return Some(AppEvent::StatusLine(format!(
                    "unauthenticated encrypted frame from {sender_name}; dropped"
                )));
            };
            return self.complete_frame(&meta, frame, plain, sender_name).await;
        }
        if frame.header.is_authenticated() && key.is_none() {
            return Some(AppEvent::StatusLine(format!(
                "authenticated frame from {sender_name} but no shared key; dropped"
            )));
        }
        let payload = frame.payload.clone();
        self.complete_frame(&meta, frame, payload, sender_name)
            .await
    }

    fn find_key_for(&self, sender: u32) -> Option<crate::crypto::Key> {
        if let Some(name) = self.nodes.get(sender).map(|n| n.name.clone()) {
            if let Some(k) = self.keys.contact_key(&name) {
                return Some(k);
            }
        }
        for ch in &self.cfg.channels {
            if let Some(k) = self.keys.channel_key(ch) {
                return Some(k);
            }
        }
        None
    }

    async fn complete_frame(
        &mut self,
        meta: &RxMeta,
        frame: Frame,
        payload: Vec<u8>,
        sender_name: String,
    ) -> Option<AppEvent> {
        if !self.replay.accept(frame.header.sender, frame.header.seq) {
            self.stats.dropped += 1;
            return Some(AppEvent::StatusLine(format!(
                "replayed sequence from {sender_name} dropped"
            )));
        }

        if frame.header.frame_type == FrameType::Ack {
            self.pending.remove(&frame.header.msg_id);
            self.store.update_delivery(frame.header.msg_id, "Delivered");
            return Some(AppEvent::Delivery {
                id: frame.header.msg_id,
                status: "Delivered".into(),
            });
        }

        let header = frame.header.clone();
        match self.reasm.push(&Frame { payload, ..frame }) {
            Ok(ReassembleOutcome::Incomplete) => None,
            Ok(ReassembleOutcome::Complete(full)) => {
                self.stats.rx_count += 1;
                self.stats.last_rssi = meta.rssi.or(self.stats.last_rssi);
                self.stats.last_snr = meta.snr.or(self.stats.last_snr);
                let label = if header.is_encrypted() {
                    "Encrypted"
                } else {
                    "Unauthenticated"
                };
                let text = match String::from_utf8(full) {
                    Ok(t) => t,
                    Err(_) => {
                        return Some(AppEvent::StatusLine(
                            "binary telemetry payload ignored".into(),
                        ))
                    }
                };
                let stored = StoredMessage {
                    id: header.msg_id,
                    sender_id: header.sender,
                    sender_name,
                    channel: "General".into(),
                    direct_to: None,
                    text,
                    timestamp: meta
                        .received_at
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                    outgoing: false,
                    security_label: label.into(),
                    delivery: "Received".into(),
                };
                self.unread.bump(&stored.channel);
                let ev = Some(AppEvent::Message(stored.clone()));
                if let Err(e) = self.store.append(stored) {
                    tracing::warn!("history append failed: {e}");
                }
                ev
            }
            Err(e) => Some(AppEvent::StatusLine(format!("reassembly error: {e}"))),
        }
    }
}

impl App {
    pub async fn tick(&mut self, events: &tokio::sync::mpsc::Sender<AppEvent>) {
        let now = now_secs();
        let due: Vec<u32> = self
            .pending
            .iter()
            .filter(|(_, p)| now >= p.next_attempt_secs)
            .map(|(k, _)| *k)
            .collect();

        for id in due {
            let entry = self.pending.get_mut(&id).expect("checked");
            if entry.retries_left == 0 {
                self.pending.remove(&id);
                self.store.update_delivery(id, "Failed");
                let _ = events
                    .send(AppEvent::Delivery {
                        id,
                        status: "Failed".into(),
                    })
                    .await;
                continue;
            }
            entry.retries_left -= 1;
            entry.next_attempt_secs = now
                + (entry.base_secs * 3u64.pow(self.cfg.chat.retry_count - entry.retries_left - 1))
                    as u32;
            self.stats.retries += 1;
            if let Some(handle) = self.connected.as_ref() {
                for w in &entry.wire_frames.clone() {
                    let _ = handle.send_frame(w.clone(), true).await;
                }
                let _ = events
                    .send(AppEvent::Delivery {
                        id,
                        status: format!("Retrying ({} left)", entry.retries_left),
                    })
                    .await;
            } else {
                self.store.update_delivery(id, "Failed");
                let _ = events
                    .send(AppEvent::Delivery {
                        id,
                        status: "Failed".into(),
                    })
                    .await;
            }
        }

        if self.gps_enabled {
            self.gps_status = self.gps_source.status();
        } else {
            self.gps_status = GpsStatus::Off;
        }

        if now.saturating_sub(self.last_hello) > 30 && self.connected.is_some() {
            self.last_hello = now;
            let _ = self.send_beacon().await;
        }
    }

    pub async fn send_beacon(&mut self) -> Result<()> {
        let handle = self
            .connected
            .as_ref()
            .ok_or_else(|| Error::device("not connected"))?
            .clone();
        let payload = format!(
            "{}|{}",
            self.cfg.identity.name, self.cfg.identity.short_name
        );
        let mut frame = Frame::new(
            FrameType::Hello,
            self.next_msg_id(),
            self.cfg.identity.node_id,
            self.next_seq(),
            payload.into_bytes(),
        );
        frame.header.timestamp = now_secs();
        let wire = frame.to_wire();
        handle.send_frame(wire, false).await.map(|_| ())
    }

    #[doc(hidden)]
    pub fn attach_connected_for_test(&mut self) {
        use crate::device::handle::DeviceCommand;
        use crate::device::types::{Capabilities, RadioStats};
        let (cmd_tx, mut rx) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    DeviceCommand::Send { reply, .. } => {
                        let _ = reply.send(Ok(rand::random()));
                    }
                    DeviceCommand::Stats(tx) => {
                        let _ = tx.send(Ok(RadioStats::default()));
                    }
                    DeviceCommand::Info(tx) => {
                        let _ = tx.send(Ok(Default::default()));
                    }
                    DeviceCommand::Capabilities(tx) => {
                        let _ = tx.send(Ok(Capabilities::default()));
                    }
                    DeviceCommand::ConfigureRadio(_, tx) => {
                        let _ = tx.send(Err(crate::error::Error::Unsupported("test".into())));
                    }
                    DeviceCommand::Reset(tx) => {
                        let _ = tx.send(Ok("ok".into()));
                    }
                }
            }
        });
        let info = crate::device::types::DeviceInfo {
            vendor: "test".into(),
            model: "test".into(),
            firmware_name: "mock".into(),
            firmware_version: "0".into(),
            serial_number: String::new(),
            node_id: None,
            transport: "test".into(),
        };
        self.connected = Some(crate::device::handle::DeviceHandle {
            cmd_tx,
            info: info.clone(),
        });
        self.device_info = Some(info);
    }

    #[doc(hidden)]
    pub fn all_sent_wires_for_test(&self, msg_id: u32) -> Vec<Vec<u8>> {
        self.pending
            .get(&msg_id)
            .map(|p| p.wire_frames.clone())
            .unwrap_or_default()
    }

    pub fn has_pending_acks(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn refresh_gps(&mut self) {
        self.gps_status = if self.gps_enabled {
            self.gps_source.status()
        } else {
            GpsStatus::Off
        };
    }
}

pub struct DeviceLoops;

impl DeviceLoops {
    pub fn spawn_forwarder(
        mut event_rx: mpsc::Receiver<DeviceEvent>,
        app_tx: tokio::sync::mpsc::Sender<DeviceEvent>,
    ) {
        tokio::spawn(async move {
            while let Some(ev) = event_rx.recv().await {
                if app_tx.send(ev).await.is_err() {
                    break;
                }
            }
        });
    }
}

pub fn sleep_dur(ms: u64) -> Duration {
    Duration::from_millis(ms)
}
