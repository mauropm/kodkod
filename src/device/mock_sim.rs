use super::mock_data::Peer;
use super::types::{DeviceEvent, RadioStats, RxMeta};
use crate::protocol::Frame;
use std::time::SystemTime;
use tokio::sync::mpsc;

pub fn meta_of(peer: &Peer) -> RxMeta {
    RxMeta {
        rssi: Some(peer.rssi),
        snr: Some(peer.snr),
        received_at: SystemTime::now(),
    }
}

fn corrupted(mut bytes: Vec<u8>, rate: f64) -> Vec<u8> {
    if rand::random::<f64>() < rate && !bytes.is_empty() {
        let idx = rand::random::<usize>() % bytes.len();
        bytes[idx] ^= 0x55;
    }
    bytes
}

pub async fn deliver(
    event_tx: &mpsc::Sender<DeviceEvent>,
    cfg: &super::mock_data::MockConfig,
    stats: &mut RadioStats,
    peer: &Peer,
    mut frame: Frame,
) {
    frame.payload = corrupted(frame.payload, cfg.corrupt_rate);
    if rand::random::<f64>() < cfg.loss_rate {
        stats.dropped += 1;
        return;
    }
    let wire = frame.to_wire();
    stats.rx_count += 1;
    stats.last_rssi = Some(peer.rssi);
    stats.last_snr = Some(peer.snr);
    let _ = event_tx
        .send(DeviceEvent::Frame(meta_of(peer), wire.clone()))
        .await;
    if rand::random::<f64>() < cfg.dup_rate {
        let _ = event_tx.send(DeviceEvent::Frame(meta_of(peer), wire)).await;
        stats.rx_count += 1;
    }
}

pub async fn maybe_ack(
    event_tx: &mpsc::Sender<DeviceEvent>,
    cfg: &super::mock_data::MockConfig,
    stats: &mut RadioStats,
    msg_id: u32,
    want_ack: bool,
) {
    if !want_ack || rand::random::<f64>() < cfg.loss_rate * 0.5 {
        return;
    }
    stats.acks_received += 1;
    let _ = event_tx.send(DeviceEvent::Acked(msg_id)).await;
}
