use super::state::{Modal, UiState};
use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub fn draw_screen(f: &mut Frame, app: &App, ui: &UiState, status_lines: &[String]) {
    let _ = ui_selected();
    let area = f.area();
    let body = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(3),
    };

    match ui.screen {
        super::state::Screen::Nodes => draw_nodes(f, app, ui, body),
        super::state::Screen::Device => draw_device(f, app, body),
        super::state::Screen::Radio => draw_radio(f, app, body),
        super::state::Screen::Privacy => draw_privacy(f, app, body),
        super::state::Screen::Telemetry => draw_telemetry(f, app, body),
        super::state::Screen::Diagnostics => draw_diagnostics(f, app, body),
        _ => draw_help(f, app, body),
    }

    let footer = format!(
        " {} | Esc back | {}",
        app.privacy.status_line(),
        status_lines.last().cloned().unwrap_or_default()
    );
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            footer,
            Style::default().fg(Color::DarkGray),
        ))),
        Rect {
            y: area.height - 2,
            height: 1,
            ..area
        },
    );
    draw_modal(f, ui);
}

fn block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} (Esc=back) "))
}

fn draw_nodes(f: &mut Frame, app: &App, ui: &UiState, area: Rect) {
    let mut lines = Vec::new();
    for (i, n) in app.nodes.all_sorted().iter().enumerate() {
        let sel = i == ui.selected_node;
        let marker = n.presence_dot();
        let style = if sel {
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Yellow)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(
            format!("{marker} {}", n.name),
            style,
        )));
        lines.push(Line::from(Span::raw(format!(
            "   ID: {:06X}  Last seen: {}  RSSI: {}  SNR: {}  GPS: {}{}{}",
            n.id,
            n.last_seen_ago(),
            n.rssi.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
            n.snr
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            n.gps_state,
            if n.trusted { "  [trusted]" } else { "" },
            if n.blocked { "  [blocked]" } else { "" },
        ))));
    }
    let _ = ui;
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No nodes seen yet. Nodes appear when they beacon or message.",
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines.push(Line::from(Span::styled(
        "t=trust/untrust selected",
        Style::default().fg(Color::DarkGray),
    )));
    f.render_widget(Paragraph::new(lines).block(block("NODES")), area);
}

fn ui_selected() -> bool {
    false
}

fn draw_device(f: &mut Frame, app: &App, area: Rect) {
    let mut lines = vec![Line::from("Device"), Line::from(Span::raw("-------"))];
    match &app.device_info {
        Some(info) => {
            for (k, v) in [
                ("Hardware", info.model.as_str()),
                ("Vendor", info.vendor.as_str()),
                ("Firmware", info.firmware_name.as_str()),
                ("Firmware version", info.firmware_version.as_str()),
                (
                    "Serial number",
                    if info.serial_number.is_empty() {
                        "not exposed"
                    } else {
                        info.serial_number.as_str()
                    },
                ),
                (
                    "Node ID",
                    app.device_info
                        .as_ref()
                        .and_then(|d| d.node_id.clone())
                        .unwrap_or_else(|| "-".into())
                        .leak(),
                ),
                ("Transport", info.transport.as_str()),
            ] {
                lines.push(Line::from(Span::raw(format!("{k}: {v}"))));
            }
        }
        None => lines.push(Line::from(Span::styled(
            "No device connected. Start with --device /dev/ttyACM0 or --mock.",
            Style::default().fg(Color::Yellow),
        ))),
    }
    let (pct, volt) = app.battery;
    lines.push(Line::from(Span::raw(format!(
        "Battery: {}",
        pct.map(|p| format!("{p}% ({:.2}V)", volt.unwrap_or(0.0)))
            .unwrap_or_else(|| "not reported".into())
    ))));
    lines.push(Line::from(Span::raw(format!(
        "GPS: {} | Sensors: scan via diagnostics",
        app.gps_status.label()
    ))));
    lines.push(Line::from(Span::styled(
        ":reset asks the module to reboot (requires confirmation)",
        Style::default().fg(Color::DarkGray),
    )));
    f.render_widget(Paragraph::new(lines).block(block("DEVICE")), area);
}

fn draw_radio(f: &mut Frame, app: &App, area: Rect) {
    let r = &app.radio;
    let policy = r.region.policy();
    let air = crate::regions::estimate_airtime_ms(r, app.cfg.chat.max_frame_payload);
    let duty = policy
        .duty_cycle_percent
        .map(|d| format!("{d:.1}% duty cycle applies"))
        .unwrap_or_else(|| "no duty cycle rule captured".into());
    let lines = vec![
        Line::from(Span::styled("Region-aware radio configuration", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(Span::raw(format!(
            "Region: {}  ({:.3}-{:.3} MHz legal)",
            r.region.label(),
            policy.min_hz as f64 / 1e6,
            policy.max_hz as f64 / 1e6
        ))),
        Line::from(Span::raw(format!(
            "Frequency: {:.3} MHz   TX power: {} dBm (regional cap {} dBm)",
            r.frequency_khz / 1000.0,
            r.tx_power_dbm,
            policy.max_power_dbm.min(crate::regions::SX1262_MAX_POWER_DBM)
        ))),
        Line::from(Span::raw(format!(
            "SF{}  BW{} kHz  CR 4/{}  Preamble {}",
            r.spreading_factor,
            r.bandwidth.khz(),
            r.coding_rate.denom(),
            r.preamble_len
        ))),
        Line::from(Span::raw(format!("Estimated airtime for {} B payload: ~{air:.0} ms", app.cfg.chat.max_frame_payload))),
        Line::from(Span::raw(duty)),
        Line::from(Span::styled(policy.note, Style::default().fg(Color::DarkGray))),
        Line::from(Span::styled(
            "Changing settings requires firmware support; Meshtastic manages radio config via its admin API.",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    f.render_widget(Paragraph::new(lines).block(block("RADIO")), area);
}

fn flag(on: bool) -> Span<'static> {
    if on {
        Span::styled("[x]", Style::default().fg(Color::Green))
    } else {
        Span::styled("[ ]", Style::default().fg(Color::DarkGray))
    }
}

fn draw_privacy(f: &mut Frame, app: &App, area: Rect) {
    let p = &app.privacy;
    let aud = match &p.location_audience {
        crate::privacy::Audience::Nobody => "Nobody",
        crate::privacy::Audience::Contacts(_) => "Selected contacts",
        crate::privacy::Audience::Channels(_) => "Selected channels",
        crate::privacy::Audience::TrustedNodes => "All trusted nodes",
        crate::privacy::Audience::Everyone => "EVERYONE",
    };
    let lines = vec![
        Line::from(Span::styled("Location", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(vec![flag(app.gps_enabled), Span::raw(" GPS hardware acquisition (':gps' toggles)")]),
        Line::from(vec![flag(p.share_location), Span::raw(" share location")]),
        Line::from(vec![flag(p.share_altitude), Span::raw(" share altitude")]),
        Line::from(vec![flag(p.share_speed), Span::raw(" share speed")]),
        Line::from(vec![flag(p.share_heading), Span::raw(" share heading")]),
        Line::from(vec![flag(p.include_timestamp), Span::raw(" include timestamp")]),
        Line::from(Span::raw(format!("  precision: {}   audience: {}", p.location_precision.label(), aud))),
        Line::from(Span::styled(
            if app.cfg.gps.transmit { "  TRANSMIT: ON (global)" } else { "  transmit: gated per message" },
            Style::default().fg(if app.cfg.gps.transmit { Color::Red } else { Color::Green }),
        )),
        Line::from(Span::raw("")),
        Line::from(Span::styled("Other telemetry", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(vec![flag(p.share_battery), Span::raw(" battery reporting")]),
        Line::from(vec![flag(p.share_telemetry), Span::raw(" telemetry")]),
        Line::from(vec![flag(p.share_presence), Span::raw(" presence beacons")]),
        Line::from(vec![flag(p.share_signal_stats), Span::raw(" signal statistics display")]),
        Line::from(vec![flag(p.share_hardware_info), Span::raw(" hardware information")]),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "Profiles never override explicit privacy settings without confirmation. ':gps-share-everyone' requires y+y confirmation.",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    f.render_widget(Paragraph::new(lines).block(block("PRIVACY / SAFETY")), area);
}

fn draw_telemetry(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.cfg.telemetry;
    let lines = vec![
        Line::from(vec![
            flag(t.enabled),
            Span::raw(format!(
                " telemetry enabled (interval {}s)",
                t.interval_seconds
            )),
        ]),
        Line::from(vec![flag(t.include_battery), Span::raw(" include battery")]),
        Line::from(vec![
            flag(t.include_location),
            Span::raw(" include location (also needs privacy + GPS transmit)"),
        ]),
        Line::from(vec![
            flag(t.include_environment),
            Span::raw(" include environment sensors"),
        ]),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "Telemetry is opt-in only. Nothing is sent while disabled.",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    f.render_widget(Paragraph::new(lines).block(block("TELEMETRY")), area);
}

fn draw_diagnostics(f: &mut Frame, app: &App, area: Rect) {
    let s = &app.stats;
    let conn = match &app.connected {
        Some(h) => h.info.transport.clone(),
        None => "not connected".into(),
    };
    let lines = vec![
        Line::from(Span::raw(format!("Connection: {conn}"))),
        Line::from(Span::raw(format!(
            "TX: {}  RX: {}  CRC errors: {}  Dropped: {}  Retries: {}  ACKs: {}",
            s.tx_count, s.rx_count, s.crc_errors, s.dropped, s.retries, s.acks_received
        ))),
        Line::from(Span::raw(format!(
            "Last RSSI: {}  Last SNR: {}",
            s.last_rssi.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
            s.last_snr.map(|v| format!("{v:.1}")).unwrap_or_else(|| "-".into())
        ))),
        Line::from(Span::raw("Radio test / loopback / sensor scan: run `lora-chat --diagnostics` for the non-interactive suite.")),
        Line::from(Span::styled(":reset reboots the module (confirmation required).", Style::default().fg(Color::DarkGray))),
    ];
    f.render_widget(Paragraph::new(lines).block(block("DIAGNOSTICS")), area);
}

fn draw_help(f: &mut Frame, _app: &App, area: Rect) {
    let lines = vec![
        Line::from(Span::styled("Keys", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("F1 Help  F2 Nodes  F3 Channels/Chat  F4 Device  F5 Radio  F6 Privacy  F7 Telemetry  F8 Diagnostics"),
        Line::from("Tab / Shift+Tab move between panels; Up/Down scroll or select."),
        Line::from("Enter sends the message typed in the input panel. Esc returns to Chat."),
        Line::from("Ctrl+C quits. Ctrl+L repaints."),
        Line::from(""),
        Line::from(Span::styled("Command palette (press ':' anywhere)", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(":nodes :radio :privacy :telemetry :device :diagnostics :help :channels"),
        Line::from(":gps toggles acquisition  :gps-share-everyone (double confirmation)"),
        Line::from(":send <text>  :reset  :disconnect  :quit"),
        Line::from(""),
        Line::from(Span::styled("Security labels", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("Encrypted = ChaCha20-Poly1305 with a shared key. Unauthenticated = plaintext broadcast; treat as public radio traffic."),
    ];
    f.render_widget(Paragraph::new(lines).block(block("HELP")), area);
}

pub fn draw_modal(f: &mut Frame, ui: &UiState) {
    let text = match &ui.modal {
        Modal::None => return,
        Modal::ConfirmGpsGlobal { step } => match step {
            0 => "WARNING\n\nYou are about to enable transmitting your location to EVERYONE.\nThis information goes out over LoRa radio to anyone listening.\n\nContinue? [y/N]".to_string(),
            _ => "Final confirmation:\n\nShare exact location with EVERYONE unless precision is reduced?\nPress 'y' to enable, any other key to cancel.".to_string(),
        },
        Modal::ConfirmReset => "Reset the RAK4631 module?\nThe radio will reboot and the connection will drop.\n\n[y/N]".to_string(),
        Modal::Notice(t) => format!("{t}\n\n[Esc]"),
    };
    let area = centered(f.area(), 60, text.lines().count() as u16 + 2);
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).title(" CONFIRM ")),
        area,
    );
}

fn centered(full: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(full.width.saturating_sub(4));
    let h = h.min(full.height.saturating_sub(2));
    Rect {
        x: full.x + (full.width - w) / 2,
        y: full.y + (full.height - h) / 2,
        width: w,
        height: h,
    }
}
