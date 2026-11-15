use crate::error::{Error, Result};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::SerialStream;

pub fn open(path: &str, baud: u32) -> Result<SerialStream> {
    use tokio_serial::SerialPortBuilderExt as _;
    tokio_serial::new(path, baud)
        .timeout(Duration::from_millis(200))
        .open_native_async()
        .map_err(|e| super::open_error(path, e))
}

pub struct LineReader {
    buf: Vec<u8>,
}

pub enum ReadOutcome {
    Line(String),
    Idle,
    Eof,
}

impl Default for LineReader {
    fn default() -> Self {
        Self::new()
    }
}

impl LineReader {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(512),
        }
    }

    pub async fn next_line(&mut self, port: &mut SerialStream) -> std::io::Result<ReadOutcome> {
        loop {
            if let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
                let mut line: Vec<u8> = self.buf.drain(..=pos).collect();
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return Ok(ReadOutcome::Line(
                    String::from_utf8_lossy(&line).into_owned(),
                ));
            }
            if self.buf.len() > 4096 {
                self.buf.clear();
            }
            let mut chunk = [0u8; 256];
            match port.read(&mut chunk).await {
                Ok(0) => return Ok(ReadOutcome::Eof),
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    return Ok(ReadOutcome::Idle)
                }
                Err(e) => return Err(e),
            }
        }
    }

    pub async fn drain_idle(&mut self, port: &mut SerialStream, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        while tokio::time::Instant::now() < deadline {
            match self.next_line(port).await {
                Ok(ReadOutcome::Idle) | Ok(ReadOutcome::Eof) => break,
                Ok(ReadOutcome::Line(_)) => {}
                Err(_) => break,
            }
        }
    }
}

pub async fn write_line(port: &mut SerialStream, line: &str) -> Result<()> {
    port.write_all(line.as_bytes())
        .await
        .map_err(|e| Error::device(format!("serial write failed: {e}")))?;
    port.write_all(b"\r\n")
        .await
        .map_err(|e| Error::device(format!("serial write failed: {e}")))?;
    port.flush()
        .await
        .map_err(|e| Error::device(format!("serial flush failed: {e}")))
}
