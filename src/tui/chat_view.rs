use super::state::{Panel, Screen, UiState};
use super::theme;
use crate::app::App;
use crate::regions::estimate_airtime_ms;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

pub fn draw(f: &mut Frame, app: &App, ui: &UiState, status_lines: &[String]) {
    f.render_widget(Block::default().style(theme::desktop()), f.area());

    if ui.screen != Screen::Chat {
        super::screens::draw_screen(f, app, ui, status_lines);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    draw_menu_bar(f, app, rows[0]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(rows[1]);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
        .split(columns[0]);

    draw_channels_panel(f, app, ui, left[0]);
    draw_nodes_panel(f, app, left[1]);
    draw_log_panel(f, app, ui, status_lines, columns[1]);
    draw_input_bar(f, app, ui, rows[2]);
    draw_fkey_bar(f, app, rows[3]);
}

fn panel_block(title: &str, focused: bool) -> Block<'_> {
    let mut b = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .style(theme::panel())
        .title(theme::title_span(title));
    if focused {
        b = b.border_style(Style::default().fg(Color::Yellow));
    } else {
        b = b.border_style(Style::default().fg(theme::BORDER_FG));
    }
    b
}

fn draw_menu_bar(f: &mut Frame, app: &App, area: Rect) {
    let conn = if app.connected.is_some() {
        Span::styled(
            " ● ONLINE ",
            Style::default()
                .bg(Color::Rgb(0, 100, 0))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " ○ OFFLINE ",
            Style::default()
                .bg(Color::Rgb(120, 0, 0))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
    };
    let line = Line::from(vec![
        Span::styled(
            format!(
                " ── lora-chat │ {} │ SF{:<2} {:.3} MHz │ {:>4} dBm ",
                app.cfg.identity.name,
                app.radio.spreading_factor,
                app.radio.frequency_khz / 1000.0,
                app.radio.tx_power_dbm
            ),
            Style::default()
                .fg(Color::Black)
                .bg(theme::KEYCHIP_BG)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default().bg(theme::MENU_BG)),
        conn,
        Span::styled("  ", Style::default().bg(theme::MENU_BG)),
        privacy_chip(app),
        Span::styled(
            " ".repeat(area.width.saturating_sub(10) as usize),
            Style::default().bg(theme::MENU_BG),
        ),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn privacy_chip(app: &App) -> Span<'static> {
    let p = &app.privacy;
    let label = format!(
        "GPS:{} TLM:{} PRS:{} BAT:{}",
        onoff(p.share_location),
        onoff(p.share_telemetry),
        onoff(p.share_presence),
        onoff(p.share_battery),
    );
    let (bg, fg) = if p.share_location {
        (theme::BAD, Color::White)
    } else {
        (Color::Rgb(0, 100, 0), Color::White)
    };
    Span::styled(label, Style::default().bg(bg).fg(fg))
}

fn onoff(b: bool) -> &'static str {
    if b {
        "ON"
    } else {
        "OFF"
    }
}

fn draw_channels_panel(f: &mut Frame, app: &App, ui: &UiState, area: Rect) {
    let items: Vec<ListItem> = app
        .cfg
        .channels
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            let unread = app.unread.get(ch);
            let sel = i == ui.selected_channel && ui.panel == Panel::Sidebar;
            let label = match unread {
                0 => format!("   {ch}"),
                n => format!("   {ch}  [{n} new]"),
            };
            ListItem::new(Line::from(Span::styled(
                label,
                if sel {
                    theme::selected()
                } else {
                    theme::panel()
                },
            )))
        })
        .collect();
    let focused = ui.screen == Screen::Chat && ui.panel == Panel::Sidebar;
    render_list(f, items, panel_block("CHANNELS", focused), area);
}

fn draw_nodes_panel(f: &mut Frame, app: &App, area: Rect) {
    let nodes = app.nodes.all_sorted();
    let items: Vec<ListItem> = nodes
        .iter()
        .take(area.height.saturating_sub(3) as usize)
        .map(|n| {
            let dot_style = Style::default()
                .fg(if n.presence_dot() == "●" {
                    theme::OK
                } else {
                    theme::DIM
                })
                .bg(theme::PANEL);
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {} ", n.presence_dot()), dot_style),
                Span::styled(short_name(n), theme::panel()),
                Span::styled(
                    format!(
                        " {:>4}",
                        n.rssi.map(|v| v.to_string()).unwrap_or_else(|| "—".into())
                    ),
                    Style::default()
                        .fg(rssi_color(n.rssi.unwrap_or(-255)))
                        .bg(theme::PANEL),
                ),
                Span::styled("dBm", Style::default().fg(theme::DIM).bg(theme::PANEL)),
            ]))
        })
        .collect();
    let title = if nodes.is_empty() {
        "NODES (none yet)"
    } else {
        "NODES"
    };
    render_list(f, items, panel_block(title, false), area);
}

fn short_name(n: &crate::nodes::Node) -> String {
    format!("{:<9}", n.short_name.chars().take(8).collect::<String>())
}

fn rssi_color(rssi: i16) -> Color {
    match rssi {
        r if r >= -85 => theme::OK,
        r if r >= -105 => theme::WARN,
        _ => theme::BAD,
    }
}

fn render_list(f: &mut Frame, items: Vec<ListItem>, block: Block, area: Rect) {
    f.render_stateful_widget(
        List::new(items).block(block),
        area,
        &mut ListState::default(),
    );
}

fn draw_log_panel(f: &mut Frame, app: &App, ui: &UiState, status_lines: &[String], area: Rect) {
    let channel = app
        .cfg
        .channels
        .get(ui.selected_channel)
        .cloned()
        .unwrap_or_else(|| "General".into());
    let msgs = app.store.channel_messages(&channel);
    let visible = area.height.saturating_sub(2) as usize;
    let end = msgs.len().saturating_sub(ui.chat_scroll as usize);
    let start = end.saturating_sub(visible.max(1));

    let lines: Vec<Line> = msgs[start..end]
        .iter()
        .flat_map(|m| message_lines(m))
        .chain(status_lines.iter().rev().take(3).rev().map(|s| {
            Line::from(Span::styled(
                format!(" · {s}"),
                Style::default().fg(theme::DIM).bg(theme::PANEL),
            ))
        }))
        .collect();

    let title = format!("CHAT ▸ {channel}");
    f.render_widget(
        Paragraph::new(lines).block(panel_block(&title, ui.panel == Panel::ChatLog)),
        area,
    );
}

fn message_lines(m: &crate::storage::StoredMessage) -> Vec<Line<'static>> {
    let time = chrono::DateTime::from_timestamp(m.timestamp as i64, 0)
        .map(|d| d.format("%H:%M").to_string())
        .unwrap_or_else(|| "--:--".into());
    let who = if m.outgoing {
        Span::styled(
            format!(" {time} YOU "),
            Style::default().bg(Color::Rgb(0, 0, 100)).fg(Color::Cyan),
        )
    } else {
        Span::styled(
            format!(" {time} {} ", m.sender_name),
            Style::default()
                .bg(Color::Rgb(0, 0, 100))
                .fg(Color::Magenta),
        )
    };
    let sec_glyph = match m.security_label.as_str() {
        "Encrypted" => Span::styled(" ⚿", Style::default().fg(theme::OK).bg(theme::PANEL)),
        "Unauthenticated broadcast" | "Unauthenticated" => {
            Span::styled(" !", Style::default().fg(theme::WARN).bg(theme::PANEL))
        }
        _ => Span::raw(""),
    };
    let delivery = if m.outgoing {
        match m.delivery.as_str() {
            d if d.starts_with("Delivered") => {
                Span::styled(" ✓✓", Style::default().fg(theme::OK).bg(theme::PANEL))
            }
            d if d.starts_with("Retrying") => {
                Span::styled(" ⟳", Style::default().fg(theme::WARN).bg(theme::PANEL))
            }
            d if d.starts_with("Failed") => {
                Span::styled(" ✗", Style::default().fg(theme::BAD).bg(theme::PANEL))
            }
            d if d.starts_with("Sending") || d.starts_with("Queued") => {
                Span::styled(" …", Style::default().fg(theme::DIM).bg(theme::PANEL))
            }
            _ => Span::raw(""),
        }
    } else {
        Span::raw("")
    };
    vec![Line::from(vec![
        who,
        sec_glyph,
        Span::styled(format!(" {}", m.text), theme::panel()),
        delivery,
    ])]
}

fn draw_input_bar(f: &mut Frame, _app: &App, ui: &UiState, area: Rect) {
    let (title, content, style) = if ui.palette_mode {
        (
            " COMMAND ",
            format!(":{}_", ui.palette_buf),
            Style::default()
                .fg(Color::Black)
                .bg(theme::KEYCHIP_BG)
                .add_modifier(Modifier::BOLD),
        )
    } else if ui.panel == Panel::Input && ui.screen == Screen::Chat {
        (
            " MESSAGE — Enter send · Tab panels · : commands ",
            format!(" {}_", ui.input),
            Style::default()
                .fg(theme::HIGHLIGHT_FG)
                .bg(theme::HIGHLIGHT_BG)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        (" MESSAGE ", format!(" {}", ui.input), theme::panel())
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .style(Style::default().bg(if ui.palette_mode {
            theme::DESKTOP
        } else {
            theme::PANEL
        }))
        .border_style(Style::default().fg(if ui.panel == Panel::Input {
            Color::Yellow
        } else {
            theme::BORDER_FG
        }))
        .title(Span::styled(
            title.to_string(),
            Style::default()
                .fg(theme::TITLE_FG)
                .add_modifier(Modifier::BOLD),
        ));
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(content, style))).block(block),
        area,
    );
}

fn draw_fkey_bar(f: &mut Frame, app: &App, area: Rect) {
    use ratatui::text::{Line as L, Span as S};
    let air = estimate_airtime_ms(&app.radio, app.cfg.chat.max_frame_payload);
    let mut spans = vec![];
    for (k, label) in [
        ("F1", "Help"),
        ("F2", "Nodes"),
        ("F3", "Chat"),
        ("F4", "Device"),
        ("F5", "Radio"),
        ("F6", "Privacy"),
        ("F7", "Telem"),
        ("F8", "Diag"),
    ] {
        spans.extend(theme::chip_pair(k, label));
    }
    spans.push(S::styled(
        format!("airtime≈{air:.0}ms"),
        Style::default().fg(theme::DIM).bg(theme::DESKTOP),
    ));
    f.render_widget(Paragraph::new(L::from(spans)).style(theme::desktop()), area);
}
