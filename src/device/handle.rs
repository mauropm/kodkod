use crate::error::{Error, Result};
use crate::regions::RadioSettings;
use tokio::sync::{mpsc, oneshot};

use super::types::{Capabilities, DeviceEvent, DeviceInfo, RadioStats};

pub enum DeviceCommand {
    Info(oneshot::Sender<Result<DeviceInfo>>),
    Capabilities(oneshot::Sender<Result<Capabilities>>),
    ConfigureRadio(Box<RadioSettings>, oneshot::Sender<Result<RadioSettings>>),
    Send {
        data: Vec<u8>,
        want_ack: bool,
        reply: oneshot::Sender<Result<u32>>,
    },
    Stats(oneshot::Sender<Result<RadioStats>>),
    Reset(oneshot::Sender<Result<String>>),
}

#[derive(Debug, Clone)]
pub struct DeviceHandle {
    pub cmd_tx: mpsc::Sender<DeviceCommand>,
    pub info: DeviceInfo,
}

macro_rules! request {
    ($self:expr, $variant:ident, $build:expr) => {{
        let (tx, rx) = oneshot::channel();
        let cmd = $build(tx);
        $self
            .cmd_tx
            .send(cmd)
            .await
            .map_err(|_| Error::device("device task stopped"))?;
        rx.await
            .map_err(|_| Error::device("device dropped command"))?
    }};
}

impl DeviceHandle {
    pub async fn send_frame(&self, data: Vec<u8>, want_ack: bool) -> Result<u32> {
        request!(self, Send, |tx| DeviceCommand::Send {
            data,
            want_ack,
            reply: tx
        })
    }

    pub async fn configure_radio(&self, cfg: RadioSettings) -> Result<RadioSettings> {
        let boxed = Box::new(cfg);
        request!(self, ConfigureRadio, |tx| DeviceCommand::ConfigureRadio(
            boxed, tx
        ))
    }

    pub async fn stats(&self) -> Result<RadioStats> {
        request!(self, Stats, DeviceCommand::Stats)
    }

    pub async fn info(&self) -> Result<DeviceInfo> {
        request!(self, Info, DeviceCommand::Info)
    }

    pub async fn capabilities(&self) -> Result<Capabilities> {
        request!(self, Capabilities, DeviceCommand::Capabilities)
    }

    pub async fn reset(&self) -> Result<String> {
        request!(self, Reset, DeviceCommand::Reset)
    }
}

pub type EventReceiver = mpsc::Receiver<DeviceEvent>;
