use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("radio configuration rejected for region {region}: {reason}")]
    IllegalRadioConfig { region: String, reason: String },
    #[error("device error: {0}")]
    Device(String),
    #[error("device timeout")]
    Timeout,
    #[error("unsupported operation: {0}")]
    Unsupported(String),
    #[error("capability not detected: {0}")]
    NotDetected(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("privacy policy blocked transmission: {0}")]
    PrivacyBlocked(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }
    pub fn device(msg: impl Into<String>) -> Self {
        Self::Device(msg.into())
    }
    pub fn protocol(msg: impl Into<String>) -> Self {
        Self::Protocol(msg.into())
    }
    pub fn crypto(msg: impl Into<String>) -> Self {
        Self::Crypto(msg.into())
    }
}
