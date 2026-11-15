pub mod chat_view;
pub mod screens;
pub mod state;
pub mod theme;

use crate::app::{App, AppEvent, SendRequest};
use crate::device::types::DeviceEvent;
use crate::error::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use state::{Modal, Panel, Screen, UiState};
use std::time::Duration;
use tokio::sync::mpsc;

pub struct TuiHandles {
    pub device_rx: mpsc::Receiver<DeviceEvent>,
    pub app_tx: mpsc::Sender<AppEvent>,
    pub app_rx: mpsc::Receiver<AppEvent>,
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let _ = crossterm::terminal::disable_raw_mode();
        eprintln!("lora-chat crashed: {info}");
    }));
}

pub async fn run(app: &mut App, handles: TuiHandles) -> Result<()> {
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let terminal = Terminal::new(backend)?;
    let mut ui = UiState::default();
    let mut status_lines: Vec<String> = Vec::new();

    install_panic_hook();
    crossterm::terminal::enable_raw_mode()?;
    let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen);

    let res = event_loop(terminal, app, &mut ui, handles, &mut status_lines).await;

    let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
    let _ = crossterm::terminal::disable_raw_mode();
    res
}

async fn event_loop(
    mut terminal: Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    ui: &mut UiState,
    mut handles: TuiHandles,
    status_lines: &mut Vec<String>,
) -> Result<()> {
    let mut ticker = tokio::time::interval(Duration::from_millis(120));

    loop {
        if ui.force_repaint {
            ui.force_repaint = false;
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
            );
            terminal = Terminal::new(ratatui::backend::CrosstermBackend::new(std::io::stdout()))?;
        }
        let _ = terminal.draw(|f| chat_view::draw(f, app, ui, status_lines))?;

        if ui.quit_requested {
            return Ok(());
        }

        tokio::select! {
            dev = handles.device_rx.recv() => {
                if let Some(ev) = dev {
                    if let Some(ae) = app.on_device_event(ev).await {
                        absorb(app, ui, ae, status_lines).await;
                    }
                }
            }
            ae = handles.app_rx.recv() => {
                if let Some(ae) = ae {
                    absorb(app, ui, ae, status_lines).await;
                }
            }
            _ = ticker.tick() => {
                while crossterm::event::poll(Duration::from_millis(0))? {
                    if let Event::Key(key) = crossterm::event::read()? {
                        handle_key(app, ui, key, status_lines).await;
                    }
                }
                app.tick(&handles.app_tx).await;
            }
        }

        if let Modal::Notice(text) = ui.modal.clone() {
            status_lines.push(format!("NOTICE: {text}"));
            ui.modal = Modal::None;
        }
        if !status_lines.is_empty() && status_lines.len() > 200 {
            status_lines.drain(..100);
        }
    }
}

async fn absorb(app: &mut App, ui: &mut UiState, ae: AppEvent, status_lines: &mut Vec<String>) {
    match ae {
        AppEvent::Message(m) => {
            ui.chat_scroll = 0;
            status_lines.push(format!("[{}] {}: {}", m.sender_name, m.channel, m.text));
        }
        AppEvent::Delivery { id, status } => {
            status_lines.push(format!("msg {id:x} -> {status}"));
        }
        AppEvent::Connected(info) => status_lines.push(format!(
            "connected: {} {}",
            info.firmware_name, info.firmware_version
        )),
        AppEvent::Disconnected(r) => status_lines.push(format!("disconnected: {r}")),
        AppEvent::Battery { percent, .. } => {
            if let Some(p) = percent {
                status_lines.push(format!("battery {p}%"));
            }
        }
        AppEvent::StatusLine(l) => status_lines.push(l),
    }
    let _ = app;
}

async fn handle_key(app: &mut App, ui: &mut UiState, key: KeyEvent, status: &mut Vec<String>) {
    if let Some(screen) = Screen::from_fkey(key.code) {
        ui.screen = screen;
        return;
    }
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            ui.quit_requested = true;
            return;
        }
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            ui.force_repaint = true;
            return;
        }
        _ => {}
    }

    match ui.modal.clone() {
        Modal::ConfirmGpsGlobal { step } => match (step, key.code) {
            (_, KeyCode::Char('n') | KeyCode::Esc) => ui.modal = Modal::None,
            (0, KeyCode::Char('y') | KeyCode::Enter) => {
                ui.modal = Modal::ConfirmGpsGlobal { step: 1 }
            }
            (1, KeyCode::Char('y')) => {
                app.privacy.share_location = true;
                app.privacy.location_audience = crate::privacy::Audience::Everyone;
                app.privacy.confirm_everyone();
                app.cfg.gps.transmit = true;
                status.push("GPS transmission to EVERYONE is now ON".into());
                ui.modal = Modal::None;
            }
            _ => {}
        },
        Modal::ConfirmReset => match key.code {
            KeyCode::Char('n') | KeyCode::Esc => ui.modal = Modal::None,
            KeyCode::Char('y') => {
                let res = if let Some(h) = app.connected.as_ref() {
                    h.reset().await
                } else {
                    Ok("no device".into())
                };
                status.push(format!("reset: {res:?}"));
                ui.modal = Modal::None;
            }
            _ => {}
        },
        Modal::Notice(_) => ui.modal = Modal::None,
        Modal::None => {}
    }
    if !matches!(ui.modal, Modal::None) {
        return;
    }

    if ui.palette_mode {
        handle_palette_key(app, ui, key, status).await;
        return;
    }

    if key.code == KeyCode::Char(':') && ui.panel != Panel::Input {
        ui.palette_mode = true;
        ui.palette_buf.clear();
        return;
    }

    match ui.screen {
        Screen::Chat => handle_chat_key(app, ui, key, status).await,
        Screen::Nodes => match key.code {
            KeyCode::Up => ui.selected_node = ui.selected_node.saturating_sub(1),
            KeyCode::Down => {
                ui.selected_node = (ui.selected_node + 1).min(app.nodes.len().saturating_sub(1))
            }
            KeyCode::Char('t') => {
                if let Some(n) = sorted_nodes(app).get(ui.selected_node).map(|n| n.id) {
                    if let Some(n) = app.nodes.get_mut(n) {
                        n.trusted = !n.trusted;
                        status.push(format!("node {} trust toggled", n.name));
                    }
                }
            }
            KeyCode::Esc => ui.screen = Screen::Chat,
            _ => {}
        },
        Screen::Radio => match key.code {
            KeyCode::Esc => ui.screen = Screen::Chat,
            KeyCode::Up | KeyCode::Down => {}
            _ => {}
        },
        Screen::Privacy
        | Screen::Telemetry
        | Screen::Device
        | Screen::Diagnostics
        | Screen::Help => {
            if key.code == KeyCode::Esc {
                ui.screen = Screen::Chat;
            }
        }
    }
}

fn sorted_nodes(app: &App) -> Vec<crate::nodes::Node> {
    app.nodes.all_sorted().into_iter().cloned().collect()
}

async fn handle_chat_key(app: &mut App, ui: &mut UiState, key: KeyEvent, status: &mut Vec<String>) {
    match key.code {
        KeyCode::Tab => ui.panel = ui.panel.next(),
        KeyCode::BackTab => ui.panel = ui.panel.prev(),
        KeyCode::Up if ui.panel == Panel::ChatLog => {
            ui.chat_scroll = ui.chat_scroll.saturating_add(1)
        }
        KeyCode::Down if ui.panel == Panel::ChatLog => {
            ui.chat_scroll = ui.chat_scroll.saturating_sub(1)
        }
        KeyCode::Up if ui.panel == Panel::Sidebar => {
            ui.selected_channel = ui.selected_channel.saturating_sub(1)
        }
        KeyCode::Down if ui.panel == Panel::Sidebar => {
            ui.selected_channel =
                (ui.selected_channel + 1).min(app.cfg.channels.len().saturating_sub(1));
        }
        _ => {}
    }

    if ui.panel == Panel::Input {
        match key.code {
            KeyCode::Char(ch) => {
                if ch == ':' && ui.input.is_empty() {
                    ui.palette_mode = true;
                } else {
                    ui.input.push(ch);
                }
            }
            KeyCode::Backspace => {
                ui.input.pop();
            }
            KeyCode::Enter => {
                let text = std::mem::take(&mut ui.input);
                if text.trim().starts_with(':') {
                    run_command(app, ui, text.trim(), status).await;
                } else if !text.is_empty() {
                    send_current(app, ui, text, status).await;
                }
            }
            _ => {}
        }
    }
}

fn current_direct_target(ui: &UiState, app: &App) -> Option<u32> {
    let channel = app.cfg.channels.get(ui.selected_channel)?;
    channel.strip_prefix("@").and_then(|rest| {
        app.nodes
            .all_sorted()
            .iter()
            .find(|n| n.name.eq_ignore_ascii_case(rest))
            .map(|n| n.id)
    })
}

async fn send_current(app: &mut App, ui: &mut UiState, text: String, status: &mut Vec<String>) {
    let channel = app
        .cfg
        .channels
        .get(ui.selected_channel)
        .cloned()
        .unwrap_or_else(|| "General".into());
    let direct = current_direct_target(ui, app);
    let emergency = channel.eq_ignore_ascii_case("Emergency");
    let req = SendRequest {
        channel: &channel,
        text: &text,
        direct_to: direct,
        emergency,
        want_ack: direct.is_some() || emergency,
    };
    match app.send_text(req).await {
        Ok(id) => status.push(format!("queued {:x} on {}", id, channel)),
        Err(e) => status.push(format!("send failed: {e}")),
    }
}

async fn handle_palette_key(
    app: &mut App,
    ui: &mut UiState,
    key: KeyEvent,
    status: &mut Vec<String>,
) {
    match key.code {
        KeyCode::Esc => {
            ui.palette_mode = false;
            ui.palette_buf.clear();
        }
        KeyCode::Backspace => {
            ui.palette_buf.pop();
        }
        KeyCode::Char(ch) => ui.palette_buf.push(ch),
        KeyCode::Enter => {
            let cmd = std::mem::take(&mut ui.palette_buf);
            ui.palette_mode = false;
            run_command(app, ui, cmd.trim(), status).await;
        }
        _ => {}
    }
}

async fn run_command(app: &mut App, ui: &mut UiState, cmd: &str, status: &mut Vec<String>) {
    let (name, arg) = match cmd.split_once(' ') {
        Some((n, a)) => (n, a.trim()),
        None => (cmd, ""),
    };
    let screen = |ui: &mut UiState, s: Screen| {
        ui.screen = s;
    };
    match name.trim_start_matches(':') {
        "connect" => {
            status.push("use --device or restart to connect; auto-connect runs at startup".into())
        }
        "disconnect" => {
            if let Some(h) = app.connected.as_ref() {
                let _ = h.stats().await;
            }
            app.detach("user command".into());
            status.push("disconnected".into());
        }
        "nodes" => screen(ui, Screen::Nodes),
        "radio" => screen(ui, Screen::Radio),
        "privacy" => screen(ui, Screen::Privacy),
        "telemetry" => screen(ui, Screen::Telemetry),
        "device" => screen(ui, Screen::Device),
        "diagnostics" => screen(ui, Screen::Diagnostics),
        "help" => screen(ui, Screen::Help),
        "channels" => {
            ui.panel = Panel::Sidebar;
            screen(ui, Screen::Chat);
        }
        "gps" => {
            let on = !app.gps_enabled;
            app.gps_enabled = on;
            app.cfg.gps.enabled = on;
            if !on {
                app.cfg.gps.transmit = false;
                app.privacy.share_location = false;
                app.privacy.location_audience = crate::privacy::Audience::Nobody;
            }
            app.refresh_gps();
            status.push(format!(
                "GPS acquisition {}",
                if on { "enabled" } else { "disabled" }
            ));
        }
        "gps-share-everyone" => {
            if app.privacy.share_location
                && app.privacy.location_audience == crate::privacy::Audience::Everyone
            {
                app.privacy.revoke_everyone();
                app.cfg.gps.transmit = false;
                status.push("global GPS sharing revoked".into());
            } else {
                ui.modal = Modal::ConfirmGpsGlobal { step: 0 };
            }
        }
        "send" => {
            let channel = app
                .cfg
                .channels
                .get(ui.selected_channel)
                .cloned()
                .unwrap_or_else(|| "General".into());
            let req = SendRequest {
                channel: &channel,
                text: arg,
                direct_to: None,
                emergency: false,
                want_ack: false,
            };
            match app.send_text(req).await {
                Ok(id) => status.push(format!("queued {id:x}")),
                Err(e) => status.push(format!("failed: {e}")),
            }
        }
        "reset" => ui.modal = Modal::ConfirmReset,
        "quit" | "q" => ui.quit_requested = true,
        other => status.push(format!(
            "unknown command '{other}' (try :help :nodes :radio :privacy :gps :send :quit)"
        )),
    }
}
