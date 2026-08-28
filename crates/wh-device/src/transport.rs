use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("no Wallhack keyboard found (is it plugged in?)")]
    NotFound,
    #[error("could not open the config interface — close the web configurator tab and retry")]
    Busy(String),
    #[error("timed out waiting for the keyboard to reply")]
    Timeout,
    #[error("protocol error: {0}")]
    Frame(#[from] wh_proto::frame::FrameError),
    #[error("replay script violation: {0}")]
    Replay(String),
    #[error("I/O error: {0}")]
    Io(String),
    #[error("batch failed at frame {index} of {total}; {applied} frame(s) already reached the device: {source}")]
    Batch { index: usize, total: usize, applied: usize, source: Box<DeviceError> },
}

pub trait Transport {
    fn send(&mut self, report: &[u8; 64]) -> Result<(), DeviceError>;
    fn recv(&mut self, timeout: Duration) -> Result<[u8; 64], DeviceError>;
}
