use lora_chat::{app, config, crypto, device, gps, storage};

use anyhow::{Context, Result};
use clap::Parser;
use config::{Config, ProfileOverride};

#[derive(Parser, Debug)]
#[command(
    name = "lora-chat",
    version,
    about = "Privacy-first LoRa chat over a RAK4631 / WisBlock device"
)]
struct Cli {
    #[arg(long, help = "path to config TOML")]
    config: Option<std::path::PathBuf>,
    #[arg(long, help = "serial device path (e.g. /dev/ttyACM0)")]
    device: Option<String>,
    #[arg(long, help = "use the built-in mock device (no hardware needed)")]
    mock: bool,
    #[arg(long, help = "transport: usb | ble | meshtastic | mock")]
    transport: Option<String>,
    #[arg(long, help = "scan for devices and exit")]
    scan: bool,
    #[arg(long, help = "run diagnostics against the selected device and exit")]
    diagnostics: bool,
    #[arg(long, help = "send one message and exit")]
    send: Option<String>,
    #[arg(long, help = "recipient node name for --send (direct message)")]
    node: Option<String>,
    #[arg(long, help = "print machine-readable status JSON and exit")]
    json_status: bool,
    #[arg(long, help = "export message history to this path and exit")]
    export: Option<std::path::PathBuf>,
    #[arg(long, help = "apply a profile from profiles/<name>.toml")]
    profile: Option<String>,
    #[arg(long, default_value = "info", help = "trace|debug|info|warn|error")]
    log_level: String,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    init_logging(&cli.log_level);

    match run(cli).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            if let Some(src) = e.source() {
                eprintln!("  caused by: {src}");
            }
            std::process::ExitCode::FAILURE
        }
    }
}

fn init_logging(level: &str) {
    let filter = tracing_subscriber::EnvFilter::try_new(format!("lora_chat={level}"))
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

async fn run(cli: Cli) -> Result<std::process::ExitCode> {
    let cfg_path = cli
        .config
        .clone()
        .unwrap_or_else(config::default_config_path);
    let mut cfg = Config::load_or_default(&cfg_path);
    if let Some(profile) = &cli.profile {
        let dir = cfg_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        let over: ProfileOverride = config::load_profile(&dir, profile)?;
        cfg.merge_profile(&over);
        tracing::info!("profile '{profile}' merged");
    }

    if cli.scan {
        return cmd_scan().await;
    }
    if let Some(path) = &cli.export {
        return cmd_export(path);
    }

    let store = storage::MessageStore::open(config::data_dir().join("history.jsonl"))
        .context("opening message history")?;
    let keys = crypto::keystore::KeyStore::load(&config::keystore_path())?;
    let gps_source: Box<dyn gps::LocationSource> = if cli.mock || cfg.device.transport == "mock" {
        Box::new(gps::MockProvider::default())
    } else {
        Box::new(gps::DisabledProvider)
    };
    let mut application = app::App::new(cfg.clone(), store, keys, gps_source);

    let (handle, device_rx) = connect_device(&cli, &cfg).await?;
    let info = handle.info.clone();
    application.attach(handle, info);

    if cli.diagnostics {
        return cmd_diagnostics(&mut application).await;
    }
    if let Some(text) = &cli.send {
        return cmd_send(&mut application, text, cli.node.as_deref()).await;
    }
    if cli.json_status {
        return cmd_json_status(&application);
    }

    let (app_tx, app_rx) = tokio::sync::mpsc::channel(256);
    lora_chat::tui::run(
        &mut application,
        lora_chat::tui::TuiHandles {
            device_rx,
            app_tx,
            app_rx,
        },
    )
    .await?;
    Ok(std::process::ExitCode::SUCCESS)
}

async fn connect_device(
    cli: &Cli,
    cfg: &Config,
) -> Result<(
    device::handle::DeviceHandle,
    tokio::sync::mpsc::Receiver<device::types::DeviceEvent>,
)> {
    let transport = cli
        .transport
        .clone()
        .unwrap_or_else(|| cfg.device.transport.clone());
    if cli.mock || transport == "mock" {
        tracing::info!("using mock device");
        return Ok(device::mock::MockDevice::spawn(Default::default()));
    }
    if transport == "ble" {
        return device::ble::connect(cli.device.as_deref())
            .await
            .map_err(anyhow::Error::msg);
    }

    let path = resolve_device_path(cli, cfg)
        .await
        .context("resolving serial device")?;
    let kind = match cfg.device.firmware.as_str() {
        "rui3" => Some(device::types::FirmwareKind::Rui3),
        "meshtastic" => Some(device::types::FirmwareKind::Meshtastic),
        _ => None,
    };

    match kind.unwrap_or(device::probe_firmware(&path).await) {
        device::types::FirmwareKind::Meshtastic => {
            println!("Connecting to Meshtastic firmware on {path} ...");
            device::meshtastic::connect(&path)
                .await
                .context("connecting to Meshtastic firmware")
        }
        _ => {
            let radio = cfg.radio.to_settings()?;
            println!(
                "Connecting RUI3 on {path} ({} {} MHz) ...",
                radio.region.label(),
                radio.frequency_khz / 1000.0
            );
            device::rui3::connect(&path, &radio)
                .await
                .context("connecting to RUI3 firmware")
        }
    }
}

async fn resolve_device_path(cli: &Cli, cfg: &Config) -> Option<String> {
    if let Some(p) = &cli.device {
        return Some(p.clone());
    }
    if cfg.device.device_path != "auto" {
        return Some(cfg.device.device_path.clone());
    }
    let candidates = device::list_candidate_ports();
    candidates.into_iter().map(|d| d.path).next()
}

async fn cmd_scan() -> Result<std::process::ExitCode> {
    println!("Scanning for serial devices ...");
    let devices = device::scan().await;
    if devices.is_empty() {
        println!("No candidate serial devices found.");
        return Ok(std::process::ExitCode::SUCCESS);
    }
    println!("\nDetected devices\n");
    for (i, d) in devices.iter().enumerate() {
        let usb = match (&d.usb_vendor, &d.usb_product) {
            (Some(v), Some(p)) => format!("{v} {p}"),
            (Some(v), None) => v.clone(),
            _ => "unknown USB identity".into(),
        };
        println!("{}. {}", i + 1, d.path);
        println!("   USB:      {usb}");
        if let Some(s) = &d.serial {
            println!("   Serial:   {s}");
        }
        println!("   Firmware: {}", d.kind.label());
        println!("   Status:   {}\n", d.compatibility());
    }
    println!(
        "Note: probing only reads identification responses; nothing is transmitted over LoRa."
    );

    #[cfg(feature = "ble")]
    match lora_chat::device::ble::scan_peers(4000, None).await {
        Ok(peers) if !peers.is_empty() => {
            println!("\nBluetooth peers (Meshtastic / RAK):\n");
            for p in peers {
                println!(
                    "  {}  {}  {}",
                    p.id,
                    p.name.clone().unwrap_or_else(|| "(unnamed)".into()),
                    p.rssi.map(|r| format!("{r} dBm")).unwrap_or_default()
                );
            }
        }
        Ok(_) => println!("\nNo Bluetooth peers found advertising Meshtastic/RAK names."),
        Err(e) => println!("\nBluetooth scan unavailable: {e}"),
    }

    Ok(std::process::ExitCode::SUCCESS)
}

async fn cmd_diagnostics(app: &mut app::App) -> Result<std::process::ExitCode> {
    println!("lora-chat diagnostics");
    println!("=====================");
    let handle = app.connected.clone().context("no device connected")?;
    let info = handle.info().await?;
    println!(
        "Firmware: {} {} ({})",
        info.firmware_name, info.firmware_version, info.transport
    );
    let caps = handle.capabilities().await?;
    println!("LoRa P2P:  {}", caps.lora_p2p.label());
    println!("LoRaWAN:   {}", caps.lorawan.label());
    println!("BLE:       {}", caps.ble_peripheral.label());
    println!("GNSS:      {}", caps.gnss.label());
    println!("Battery:   {}", caps.battery.label());
    for s in &caps.sensors {
        println!("Sensor:    {} [{}] - {}", s.name, s.id, s.state.label());
    }

    println!("\nListening window (5 s) to collect traffic stats ...");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let stats = handle.stats().await?;
    println!(
        "\nTX {} | RX {} | CRC errors {} | dropped {} | retries {}",
        stats.tx_count, stats.rx_count, stats.crc_errors, stats.dropped, stats.retries
    );
    match (stats.last_rssi, stats.last_snr) {
        (Some(r), Some(s)) => println!("Last packet RSSI {r} dBm, SNR {s:.1} dB"),
        _ => println!("No packets observed in window."),
    }
    Ok(std::process::ExitCode::SUCCESS)
}

async fn cmd_send(
    application: &mut app::App,
    text: &str,
    node: Option<&str>,
) -> Result<std::process::ExitCode> {
    let direct_to = node.and_then(|name| {
        application
            .nodes
            .all_sorted()
            .iter()
            .find(|n| n.name.eq_ignore_ascii_case(name))
            .map(|n| n.id)
    });
    let channel = application
        .cfg
        .channels
        .first()
        .cloned()
        .unwrap_or_else(|| "General".into());
    let req = app::SendRequest {
        channel: &channel,
        text,
        direct_to,
        emergency: false,
        want_ack: true,
    };
    match application.send_text(req).await {
        Ok(id) => {
            println!("queued {id:x}");
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            Ok(std::process::ExitCode::SUCCESS)
        }
        Err(e) => Err(anyhow::anyhow!("{e}")),
    }
}

fn cmd_json_status(application: &app::App) -> Result<std::process::ExitCode> {
    let status = serde_json::json!({
        "connected": application.connected.is_some(),
        "device": application.device_info,
        "radio": {
            "region": application.radio.region.label(),
            "frequency_khz": application.radio.frequency_khz,
            "tx_power_dbm": application.radio.tx_power_dbm,
            "sf": application.radio.spreading_factor,
            "bandwidth_khz": application.radio.bandwidth.khz(),
        },
        "privacy": application.privacy.status_line(),
        "gps": application.gps_status.label(),
        "battery_percent": application.battery.0,
        "stats": {
            "tx": application.stats.tx_count,
            "rx": application.stats.rx_count,
            "crc_errors": application.stats.crc_errors,
            "dropped": application.stats.dropped,
            "retries": application.stats.retries,
        },
        "nodes": application.nodes.len(),
    });
    println!("{status}");
    Ok(std::process::ExitCode::SUCCESS)
}

fn cmd_export(path: &std::path::Path) -> Result<std::process::ExitCode> {
    let store = storage::MessageStore::open(config::data_dir().join("history.jsonl"))
        .context("opening message history")?;
    storage::MessageStore::export(path, store.all())?;
    println!(
        "exported {} messages to {}",
        store.all().len(),
        path.display()
    );
    Ok(std::process::ExitCode::SUCCESS)
}
