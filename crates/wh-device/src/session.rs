use crate::transport::{DeviceError, Transport};
use std::time::Duration;
use wh_proto::frame::{parse, FrameError};

pub const READ_TIMEOUT: Duration = Duration::from_millis(250);
pub const MAX_READS: usize = 8;

pub struct Session<T: Transport> {
    t: T,
}

impl<T: Transport> Session<T> {
    pub fn new(t: T) -> Self {
        Self { t }
    }
    pub fn into_inner(self) -> T {
        self.t
    }

    /// Send one frame; return the payload of the first valid reply with the same cmd.
    pub fn roundtrip(&mut self, req: &[u8; 64]) -> Result<Vec<u8>, DeviceError> {
        self.t.send(req)?;
        for _ in 0..MAX_READS {
            let report = self.t.recv(READ_TIMEOUT)?;
            match parse(&report) {
                Ok(reply) if reply.cmd == req[2] => return Ok(reply.payload.to_vec()),
                Err(FrameError::DeviceFail(code)) => {
                    return Err(DeviceError::Frame(FrameError::DeviceFail(code)))
                }
                _ => continue, // unrelated or malformed input report — skip
            }
        }
        Err(DeviceError::Timeout)
    }

    /// Send frames one at a time, one matched reply each.
    pub fn roundtrip_many(&mut self, reqs: &[[u8; 64]]) -> Result<Vec<Vec<u8>>, DeviceError> {
        reqs.iter().map(|r| self.roundtrip(r)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::{hex, ReplayTransport};

    fn reply_frame(cmd: u8, payload: &[u8]) -> [u8; 64] {
        wh_proto::frame::frame(cmd, payload).unwrap()
    }

    #[test]
    fn roundtrip_skips_unrelated_reports() {
        let req = wh_proto::cmds::read_global_travel();
        let noise = reply_frame(0x12, &[0x01]); // unrelated realtime-matrix report
        let good = reply_frame(0x29, &[0x00, 0, 0, 0xF4, 0x01, 0xC8, 0, 0xC8, 0]);
        let script = [
            format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(&req)),
            format!("{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(&noise)),
            format!("{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(&good)),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&script).unwrap());
        let payload = s.roundtrip(&req).unwrap();
        assert_eq!(payload[3], 0xF4);
    }

    #[test]
    fn roundtrip_times_out() {
        let req = wh_proto::cmds::sync();
        let script = format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(&req));
        let mut s = Session::new(ReplayTransport::from_jsonl(&script).unwrap());
        assert!(matches!(s.roundtrip(&req), Err(crate::transport::DeviceError::Timeout)));
    }

    #[test]
    fn device_fail_reply_is_error() {
        let req = wh_proto::cmds::read_global_travel();
        let fail = reply_frame(0xFF, &[0x02]);
        let script = [
            format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(&req)),
            format!("{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(&fail)),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&script).unwrap());
        assert!(s.roundtrip(&req).is_err());
    }
}
