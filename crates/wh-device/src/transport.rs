use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("no Wallhack keyboard found (is it plugged in?)")]
    NotFound,
    #[error("could not open the config interface, close the web configurator tab and retry: {0}")]
    Busy(String),
    #[error("timed out waiting for the keyboard to reply")]
    Timeout,
    #[error("protocol error: {0}")]
    Frame(#[from] wh_proto::frame::FrameError),
    #[error("replay script violation: {0}")]
    Replay(String),
    #[error("I/O error: {0}")]
    Io(String),
    #[error("could not decode reply: {0}")]
    Decode(String),
    /// A profile-read reply parsed fine but named an index the board's four profiles can
    /// never produce (`wh_proto::cmds::DecodeError::ProfileOutOfRange`, from `ops::profile`).
    /// Kept distinct from `Decode`, which covers a reply that isn't shaped like a profile
    /// reply at all.
    #[error(
        "board reported profile index {0}, but the board only has 4 profiles (wire index 0..=3)"
    )]
    ProfileOutOfRange(u8),
    // `source` isn't interpolated into the message: thiserror wires it into
    // `Error::source()`, and `main.rs`'s anyhow `{e:#}` already walks that chain, so the
    // cause text would otherwise print twice.
    #[error(
        "batch failed at frame {index} of {total}; {applied} frame(s) already reached the device"
    )]
    Batch {
        index: usize,
        total: usize,
        applied: usize,
        source: Box<DeviceError>,
    },
}

pub trait Transport {
    fn send(&mut self, report: &[u8; 64]) -> Result<(), DeviceError>;
    fn recv(&mut self, timeout: Duration) -> Result<[u8; 64], DeviceError>;
}

/// Lets `Session<Box<dyn Transport>>` pick between the real device and a replay script at
/// runtime instead of monomorphizing over both.
impl Transport for Box<dyn Transport> {
    fn send(&mut self, report: &[u8; 64]) -> Result<(), DeviceError> {
        (**self).send(report)
    }
    fn recv(&mut self, timeout: Duration) -> Result<[u8; 64], DeviceError> {
        (**self).recv(timeout)
    }
}
