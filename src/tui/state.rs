#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Chat,
    Nodes,
    Device,
    Radio,
    Privacy,
    Telemetry,
    Diagnostics,
    Help,
}

impl Screen {
    pub fn title(self) -> &'static str {
        match self {
            Screen::Chat => "Chat",
            Screen::Nodes => "Nodes",
            Screen::Device => "Device",
            Screen::Radio => "Radio",
            Screen::Privacy => "Privacy",
            Screen::Telemetry => "Telemetry",
            Screen::Diagnostics => "Diagnostics",
            Screen::Help => "Help",
        }
    }

    pub fn from_fkey(code: crossterm::event::KeyCode) -> Option<Screen> {
        use crossterm::event::KeyCode::*;
        match code {
            F(1) => Some(Screen::Help),
            F(2) => Some(Screen::Nodes),
            F(3) => Some(Screen::Chat),
            F(4) => Some(Screen::Device),
            F(5) => Some(Screen::Radio),
            F(6) => Some(Screen::Privacy),
            F(7) => Some(Screen::Telemetry),
            F(8) => Some(Screen::Diagnostics),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Sidebar,
    ChatLog,
    Input,
}

impl Panel {
    pub fn next(self) -> Self {
        match self {
            Panel::Sidebar => Panel::ChatLog,
            Panel::ChatLog => Panel::Input,
            Panel::Input => Panel::Sidebar,
        }
    }
    pub fn prev(self) -> Self {
        match self {
            Panel::Sidebar => Panel::Input,
            Panel::ChatLog => Panel::Sidebar,
            Panel::Input => Panel::ChatLog,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Modal {
    None,
    ConfirmGpsGlobal { step: u8 },
    ConfirmReset,
    Notice(String),
}

pub struct UiState {
    pub force_repaint: bool,
    pub screen: Screen,
    pub panel: Panel,
    pub palette_mode: bool,
    pub palette_buf: String,
    pub input: String,
    pub selected_channel: usize,
    pub selected_node: usize,
    pub chat_scroll: u16,
    pub modal: Modal,
    pub quit_requested: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            force_repaint: false,
            screen: Screen::Chat,
            panel: Panel::Input,
            palette_mode: false,
            palette_buf: String::new(),
            input: String::new(),
            selected_channel: 0,
            selected_node: 0,
            chat_scroll: 0,
            modal: Modal::None,
            quit_requested: false,
        }
    }
}
