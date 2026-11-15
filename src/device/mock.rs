use super::handle::{DeviceCommand, DeviceHandle, EventReceiver};
use super::mock_data::{mock_info, MockConfig, PEERS, PEER_MESSAGES};
use super::mock_sim::{deliver, maybe_ack};
use super::types::{Capabilities, CapabilityState, DeviceEvent, RadioStats, SensorDesc};
use crate::protocol::{Frame, FrameType};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};

pub struct MockDevice;

impl MockDevice {
    pub fn spawn(cfg: MockConfig) -> (DeviceHandle, EventReceiver) {
        let (cmd_tx, cmd_rx) = mpsc::channel(32);
        let (event_tx, event_rx) = mpsc::channel(256);
        tokio::spawn(run(cmd_rx, event_tx, cfg));
        (
            DeviceHandle {
                cmd_tx,
                info: mock_info(),
            },
            event_rx,
        )
    }
}

fn capabilities() -> Capabilities {
    Capabilities {
        lora_p2p: CapabilityState::Supported,
        lorawan: CapabilityState::Unsupported,
        ble_peripheral: CapabilityState::NotDetected,
        gnss: CapabilityState::NotDetected,
        battery: CapabilityState::Supported,
        gpio: CapabilityState::Supported,
        adc: CapabilityState::Supported,
        sensors: vec![
            SensorDesc {
                id: "rak1906".into(),
                name: "RAK1906 environment".into(),
                kind: "temperature humidity pressure gas".into(),
                state: CapabilityState::Detected,
            },
            SensorDesc {
                id: "rak1904".into(),
                name: "RAK1904 accelerometer".into(),
                kind: "acceleration".into(),
                state: CapabilityState::NotDetected,
            },
            SensorDesc {
                id: "rak1910".into(),
                name: "RAK1910 GNSS".into(),
                kind: "gnss".into(),
                state: CapabilityState::RequiresOptionalModule,
            },
        ],
    }
}

async fn run(
    mut cmd_rx: mpsc::Receiver<DeviceCommand>,
    event_tx: mpsc::Sender<DeviceEvent>,
    cfg: MockConfig,
) {
    let mut stats = RadioStats::default();
    let mut hello_tick =
        tokio::time::interval(Duration::from_millis(cfg.hello_interval_ms.max(2000)));
    let mut chat_tick = tokio::time::interval(Duration::from_millis(23_000));
    let cursors = Arc::new(Mutex::new([0usize; PEERS.len()]));

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => match cmd {
                None => break,
                Some(DeviceCommand::Reset(tx)) => { let _ = tx.send(Ok("mock reset".into())); break; }
                Some(DeviceCommand::Info(tx)) => { let _ = tx.send(Ok(mock_info())); }
                Some(DeviceCommand::Capabilities(tx)) => { let _ = tx.send(Ok(capabilities())); }
                Some(DeviceCommand::Stats(tx)) => { let _ = tx.send(Ok(stats.clone())); }
                Some(DeviceCommand::ConfigureRadio(s, tx)) => {
                    let applied = s.as_ref().clone();
                    let _ = event_tx.send(DeviceEvent::Log(format!(
                        "radio configured: {} {:.3} MHz SF{} BW{} CR {} dBm",
                        applied.region.label(), applied.frequency_khz / 1000.0,
                        applied.spreading_factor, applied.bandwidth.khz(), applied.tx_power_dbm
                    ))).await;
                    let _ = tx.send(Ok(applied));
                }
                Some(DeviceCommand::Send { data, want_ack, reply }) => {
                    stats.tx_count += 1;
                    let parsed = Frame::from_wire(&data);
                    if let Ok(f) = &parsed {
                        maybe_ack(&event_tx, &cfg, &mut stats, f.header.msg_id, want_ack).await;
                    }
                    let _ = reply.send(Ok(rand::random()));
                }
            },
            _ = hello_tick.tick() => {
                for p in &PEERS {
                    let frame = Frame::new(
                        FrameType::Hello,
                        rand::random(),
                        p.id,
                        rand::random(),
                        format!("{}|{}", p.name, p.short).into_bytes(),
                    );
                    deliver(&event_tx, &cfg, &mut stats, p, frame).await;
                }
            }
            _ = chat_tick.tick(), if cfg.peer_chat_enabled => {
                let idx = rand::random::<usize>() % PEERS.len();
                let n_msgs = PEER_MESSAGES.len();
                let text = {
                    let mut c = cursors.lock().await;
                    let m = PEER_MESSAGES[c[idx] % n_msgs];
                    c[idx] += 1;
                    m
                };
                let frame = Frame::new(FrameType::Data, rand::random(), PEERS[idx].id, rand::random(), text.as_bytes().to_vec());
                deliver(&event_tx, &cfg, &mut stats, &PEERS[idx], frame).await;
            }
        }
    }
    let _ = event_tx
        .send(DeviceEvent::Disconnected("mock shutdown".into()))
        .await;
}
