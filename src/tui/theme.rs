use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

pub const DESKTOP: Color = Color::Rgb(0, 0, 170);
pub const PANEL: Color = Color::Rgb(0, 0, 130);
pub const BORDER_FG: Color = Color::White;
pub const TITLE_FG: Color = Color::Yellow;
pub const TEXT: Color = Color::White;
pub const HIGHLIGHT_BG: Color = Color::Cyan;
pub const HIGHLIGHT_FG: Color = Color::Black;
pub const KEYCHIP_BG: Color = Color::Cyan;
pub const KEYCHIP_FG: Color = Color::Black;
pub const MENU_BG: Color = Color::Black;
pub const OK: Color = Color::LightGreen;
pub const WARN: Color = Color::Yellow;
pub const BAD: Color = Color::Red;
pub const DIM: Color = Color::Gray;

pub fn desktop() -> Style {
    Style::default().bg(DESKTOP)
}

pub fn panel() -> Style {
    Style::default().bg(PANEL).fg(TEXT)
}

pub fn selected() -> Style {
    Style::default()
        .bg(HIGHLIGHT_BG)
        .fg(HIGHLIGHT_FG)
        .add_modifier(Modifier::BOLD)
}

pub fn title_span(t: &str) -> Span<'static> {
    Span::styled(
        format!(" {t} "),
        Style::default()
            .fg(TITLE_FG)
            .bg(PANEL)
            .add_modifier(Modifier::BOLD),
    )
}

pub fn chip_pair(key: &'static str, label: &'static str) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            format!(" {key} "),
            Style::default()
                .bg(KEYCHIP_BG)
                .fg(KEYCHIP_FG)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {label}"), Style::default().fg(TEXT).bg(DESKTOP)),
        Span::raw(" "),
    ]
}
