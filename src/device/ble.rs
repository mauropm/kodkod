use super::handle::{DeviceCommand, DeviceHandle, EventReceiver};
use super::meshtastic as mt;
use super::types::{DeviceEvent, DeviceInfo, RadioStats};
use crate::error::{Error, Result};
use btleplug::api::{Central as _, Manager as _};
use btleplug::api::{Characteristic, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use std::time::Duration;
use tokio::sync::mpsc;
use uuid::Uuid;

pub const MESHTASTIC_SERVICE: &str = "6ba1b218-15a8-461f-9fa8-5dcae273eafd";
pub const CHAR_FROM_RADIO: &str = "2c55e69e-4993-11ed-b878-0242ac120002";
pub const CHAR_TO_RADIO: &str = "f75c76d2-129e-4dad-a1dd-7866124401e7";
pub const CHAR_FROM_NUM: &str = "5a3d6e49-06e6-4423-9944-e9de8cdf9547";

const WANT_CONFIG_ID: u32 = 0x1a2b_3c4d;

#[derive(Debug, Clone)]
pub struct BlePeer {
    pub id: String,
    pub name: Option<String>,
    pub rssi: Option<i16>,
}

fn uuid(s: &str) -> Uuid {
    Uuid::parse_str(s).expect("valid uuid constant")
}

pub async fn scan_peers(timeout_ms: u64, name_filter: Option<&str>) -> Result<Vec<BlePeer>> {
    let manager = Manager::new()
        .await
        .map_err(|e| Error::device(format!("bluetooth manager: {e}")))?;
    let adapters = manager
        .adapters()
        .await
        .map_err(|e| Error::device(format!("bt adapters: {e}")))?;
    let mut out = Vec::new();
    for adapter in adapters.into_iter().take(1) {
        adapter
            .start_scan(ScanFilter::default())
            .await
            .map_err(|e| Error::device(format!("ble scan: {e}")))?;
        tokio::time::sleep(Duration::from_millis(timeout_ms.max(800))).await;
        for p in adapter
            .peripherals()
            .await
            .map_err(|e| Error::device(format!("bt peripherals: {e}")))?
        {
            let Ok(Some(props)) = p.properties().await else {
                continue;
            };
            let name = props.local_name.clone();
            let looks_relevant = match &name {
                Some(n) => n.contains("Meshtastic") || n.contains("RAK") || n.contains("WisBlock"),
                None => false,
            };
            let matches_filter = match name_filter {
                Some(f) => {
                    name.as_deref().map(|n| n.contains(f)).unwrap_or(false)
                        || p.id().to_string().contains(f)
                }
                None => looks_relevant,
            };
            if matches_filter {
                out.push(BlePeer {
                    id: p.id().to_string(),
                    name,
                    rssi: props.rssi,
                });
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.id == b.id);
    Ok(out)
}

async fn find_peripheral(name_filter: Option<&str>) -> Result<(Adapter, Peripheral)> {
    let manager = Manager::new()
        .await
        .map_err(|e| Error::device(format!("bluetooth manager: {e}")))?;
    let adapters = manager
        .adapters()
        .await
        .map_err(|e| Error::device(format!("bt adapters: {e}")))?;
    let adapter = adapters
        .into_iter()
        .next()
        .ok_or_else(|| Error::device("no bluetooth adapter found"))?;
    adapter
        .start_scan(ScanFilter::default())
        .await
        .map_err(|e| Error::device(format!("ble scan: {e}")))?;
    tokio::time::sleep(Duration::from_secs(4)).await;
    for p in adapter
        .peripherals()
        .await
        .map_err(|e| Error::device(format!("bt peripherals: {e}")))?
    {
        let Ok(Some(props)) = p.properties().await else {
            continue;
        };
        let name = props.local_name.clone();
        let hit = match name_filter {
            Some(f) => {
                name.as_deref().map(|n| n.contains(f)).unwrap_or(false)
                    || p.id().to_string().contains(f)
            }
            None => name
                .as_deref()
                .map(|n| n.contains("Meshtastic") || n.contains("RAK") || n.contains("WisBlock"))
                .unwrap_or(false),
        };
        if hit {
            return Ok((adapter, p));
        }
    }
    Err(Error::NotDetected(format!(
        "no matching BLE device found (filter {:?}). Is the node powered and advertising? Run lora-chat --scan --transport ble",
        name_filter.unwrap_or("(Meshtastic/RAK)")
    )))
}

#[allow(dead_code)]
struct BleChars {
    from_radio: Characteristic,
    to_radio: Characteristic,
    from_num: Characteristic,
}

fn find_chars(peripheral: &Peripheral) -> Result<BleChars> {
    let chars = peripheral.characteristics();
    let svc = uuid(MESHTASTIC_SERVICE);
    let in_scope: Vec<Characteristic> = chars
        .iter()
        .filter(|c| c.service_uuid == svc)
        .cloned()
        .collect();
    let pick = |target: &str| -> Option<Characteristic> {
        let want = uuid(target);
        in_scope
            .iter()
            .find(|c| c.uuid == want)
            .cloned()
            .or_else(|| {
                in_scope
                    .iter()
                    .find(|c| c.uuid.to_string().ends_with(&target[..8]))
                    .cloned()
            })
    };
    for c in &in_scope {
        tracing::debug!("gatt char {} props {:?}", c.uuid, c.properties);
    }
    match (
        pick(CHAR_FROM_RADIO),
        pick(CHAR_TO_RADIO),
        pick(CHAR_FROM_NUM),
    ) {
        (Some(from_radio), Some(to_radio), Some(from_num)) => Ok(BleChars {
            from_radio,
            to_radio,
            from_num,
        }),
        _ => Err(Error::Unsupported(format!(
            "connected device does not expose the Meshtastic GATT service {MESHTASTIC_SERVICE}"
        ))),
    }
}

pub async fn connect(name_filter: Option<&str>) -> Result<(DeviceHandle, EventReceiver)> {
    match connect_once(name_filter).await {
        Ok(v) => Ok(v),
        Err(e) => {
            tracing::warn!("BLE connect attempt failed ({e}); retrying once");
            tokio::time::sleep(Duration::from_secs(2)).await;
            connect_once(name_filter).await
        }
    }
}

async fn connect_once(name_filter: Option<&str>) -> Result<(DeviceHandle, EventReceiver)> {
    let connect_fut = async {
        let (_adapter, peripheral) = find_peripheral(name_filter).await?;
        let name = match peripheral.properties().await {
            Ok(Some(p)) => p.local_name.unwrap_or_default(),
            _ => String::new(),
        };
        println!("Connecting to BLE device '{name}' ...");
        peripheral
            .connect()
            .await
            .map_err(|e| Error::device(format!("ble connect: {e}")))?;
        for _ in 0..40 {
            if peripheral
                .is_connected()
                .await
                .map_err(|e| Error::device(e.to_string()))?
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        peripheral
            .discover_services()
            .await
            .map_err(|e| Error::device(format!("gatt discovery: {e}")))?;
        let chars = find_chars(&peripheral)?;

        let snap = handshake(&peripheral, &chars).await?;
        Ok::<_, Error>((peripheral, chars, name, snap))
    };
    let (peripheral, chars, name, snap) =
        tokio::time::timeout(Duration::from_secs(60), connect_fut)
            .await
            .map_err(|_| Error::device("BLE connection timed out"))??;

    let info = DeviceInfo {
        transport: format!("ble:{name}"),
        ..mt::dev_info("", 0, &snap)
    };
    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    let (event_tx, event_rx) = mpsc::channel(256);
    tokio::spawn(session(
        peripheral,
        chars,
        cmd_rx,
        event_tx,
        snap,
        info.clone(),
    ));
    Ok((DeviceHandle { cmd_tx, info }, event_rx))
}

async fn read_from_radio(peripheral: &Peripheral, ch: &Characteristic) -> Result<Vec<u8>> {
    peripheral
        .read(ch)
        .await
        .map_err(|e| Error::device(format!("fromradio read: {e}")))
}

async fn write_to_radio(peripheral: &Peripheral, ch: &Characteristic, data: &[u8]) -> Result<()> {
    tokio::time::sleep(Duration::from_millis(800)).await;
    let mut last = String::new();
    for attempt in 0..10 {
        let wt = if attempt % 2 == 0 {
            WriteType::WithoutResponse
        } else {
            WriteType::WithResponse
        };
        match peripheral.write(ch, data, wt).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = e.to_string();
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }
    Err(Error::device(format!("toradio write: {last}")))
}

async fn drain_available(peripheral: &Peripheral, chars: &BleChars) -> Result<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    for _ in 0..32 {
        let payload = read_from_radio(peripheral, &chars.from_radio).await?;
        if payload.is_empty() {
            return Ok(out);
        }
        out.push(payload);
    }
    Ok(out)
}

async fn handshake(peripheral: &Peripheral, chars: &BleChars) -> Result<mt::Snapshot> {
    use super::meshtastic::wire::want_config_proto;

    write_to_radio(
        peripheral,
        &chars.to_radio,
        &want_config_proto(WANT_CONFIG_ID),
    )
    .await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    let mut snap = mt::Snapshot {
        my_node_num: None,
        firmware: String::new(),
        region_code: None,
        channels: Vec::new(),
    };
    while tokio::time::Instant::now() < deadline {
        for payload in drain_available(peripheral, chars).await? {
            match mt::decode::parse_from_radio(&payload) {
                mt::wire::FromRadio::ConfigComplete(_) => return Ok(snap),
                mt::wire::FromRadio::MyNodeNum(n) => snap.my_node_num = Some(n),
                mt::wire::FromRadio::Firmware(v) if !v.is_empty() => snap.firmware = v,
                mt::wire::FromRadio::LoRaConfig { region, .. } => snap.region_code = region,
                mt::wire::FromRadio::Channel { name: Some(n), .. } if !n.is_empty() => {
                    snap.channels.push(n)
                }
                _ => {}
            }
        }
        if snap.my_node_num.is_some() {
            return Ok(snap);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Err(Error::device("no Meshtastic config received over BLE"))
}

async fn session(
    peripheral: Peripheral,
    chars: BleChars,
    mut cmd_rx: mpsc::Receiver<DeviceCommand>,
    event_tx: mpsc::Sender<DeviceEvent>,
    _snap: mt::Snapshot,
    dev_info: DeviceInfo,
) {
    let mut stats = RadioStats::default();
    let mut poll = tokio::time::interval(Duration::from_millis(300));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = poll.tick() => {
                match drain_available(&peripheral, &chars).await {
                    Ok(frames) => {
                        for frame in frames {
                            mt::handle_from_radio(&frame, &event_tx, &mut stats).await;
                        }
                    }
                    Err(_) => {
                        let _ = event_tx.send(DeviceEvent::Disconnected("BLE link lost".into())).await;
                        break;
                    }
                }
            }
            cmd = cmd_rx.recv() => match cmd {
                None | Some(DeviceCommand::Reset(_)) => break,
                Some(DeviceCommand::Info(tx)) => { let _ = tx.send(Ok(dev_info.clone())); }
                Some(DeviceCommand::Capabilities(tx)) => { let _ = tx.send(Ok(mt::capabilities())); }
                Some(DeviceCommand::Stats(tx)) => { let _ = tx.send(Ok(stats.clone())); }
                Some(DeviceCommand::ConfigureRadio(_, tx)) => {
                    let _ = tx.send(Err(Error::Unsupported(
                        "changing radio settings is not supported over the Meshtastic client API; use a Meshtastic admin client".into(),
                    )));
                }
                Some(DeviceCommand::Send { data, want_ack, reply }) => {
                    let id = rand::random();
                    let (to, portnum, payload, wa) = mt::outbound_mapping(&data, want_ack);
                    let proto = mt::wire::text_packet_proto(to, 0, id, wa, portnum, &payload);
                    match write_to_radio(&peripheral, &chars.to_radio, &proto).await {
                        Ok(()) => { stats.tx_count += 1; let _ = reply.send(Ok(id)); }
                        Err(e) => { let _ = reply.send(Err(e)); }
                    }
                }
            },
        }
    }
}
