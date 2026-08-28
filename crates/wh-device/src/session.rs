use crate::transport::{DeviceError, Transport};
use std::time::{Duration, Instant};
use wh_proto::frame::{parse, FrameError};

pub const READ_TIMEOUT: Duration = Duration::from_millis(250);
/// Wall-clock budget for one roundtrip. The keyboard emits unsolicited input
/// reports continuously while the user types, so the read budget must be
/// time-based rather than attempt-based: a fixed attempt count could be
/// exhausted by ordinary typing and surface as a false Timeout.
pub const TOTAL_TIMEOUT: Duration = Duration::from_millis(1500);
/// Secondary, generous runaway guard on top of the wall-clock deadline.
pub const MAX_READS: usize = 256;

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
        let deadline = Instant::now() + TOTAL_TIMEOUT;
        let mut attempts = 0usize;
        while attempts < MAX_READS && Instant::now() < deadline {
            attempts += 1;
            let remaining = deadline.saturating_duration_since(Instant::now());
            let timeout = READ_TIMEOUT.min(remaining);
            let report = match self.t.recv(timeout) {
                Ok(r) => r,
                Err(DeviceError::Timeout) => continue,
                Err(e) => return Err(e),
            };
            match parse(&report) {
                Ok(reply) if reply.cmd == req[2] => return Ok(reply.payload.to_vec()),
                Err(FrameError::DeviceFail(code)) => {
                    return Err(DeviceError::Frame(FrameError::DeviceFail(code)))
                }
                _ => continue, // unrelated or malformed input report, skip
            }
        }
        Err(DeviceError::Timeout)
    }

    /// Send frames one at a time, one matched reply each. If a frame fails,
    /// the frames before it already reached the device: report that partial
    /// progress instead of discarding it, since the caller (the CLI's write
    /// path) needs to know whether to tell the user to restore from backup.
    pub fn roundtrip_many(&mut self, reqs: &[[u8; 64]]) -> Result<Vec<Vec<u8>>, DeviceError> {
        let total = reqs.len();
        let mut out = Vec::with_capacity(total);
        for (index, req) in reqs.iter().enumerate() {
            match self.roundtrip(req) {
                Ok(payload) => out.push(payload),
                Err(source) => {
                    return Err(DeviceError::Batch {
                        index,
                        total,
                        applied: index,
                        source: Box::new(source),
                    })
                }
            }
        }
        Ok(out)
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

    #[test]
    fn reads_exhausted_returns_timeout() {
        // Enough parseable-but-unrelated reports to exhaust the MAX_READS
        // attempt cap without ever waiting out the wall-clock deadline
        // (ReplayTransport's recv ignores the timeout it's given and
        // returns instantly, so this must trip the attempt bound, not
        // the real 1500ms deadline).
        let req = wh_proto::cmds::read_global_travel();
        let noise = reply_frame(0x12, &[0x01]);
        let mut lines = vec![format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(&req))];
        for _ in 0..(MAX_READS + 10) {
            lines.push(format!("{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(&noise)));
        }
        let script = lines.join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&script).unwrap());
        assert!(matches!(s.roundtrip(&req), Err(crate::transport::DeviceError::Timeout)));
    }

    #[test]
    fn roundtrip_many_collects_all_replies_in_order() {
        let req1 = wh_proto::cmds::read_global_travel();
        let req2 = wh_proto::cmds::sync();
        let reply1 = reply_frame(0x29, &[0x00, 0, 0, 0xF4, 0x01, 0xC8, 0, 0xC8, 0]);
        let reply2 = reply_frame(req2[2], &[0x07]);
        let script = [
            format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(&req1)),
            format!("{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(&reply1)),
            format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(&req2)),
            format!("{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(&reply2)),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&script).unwrap());
        let replies = s.roundtrip_many(&[req1, req2]).unwrap();
        assert_eq!(replies.len(), 2);
        assert_eq!(replies[0][3], 0xF4);
        assert_eq!(replies[1], vec![0x07]);
    }

    /// Transport double that returns `Timeout` a fixed number of times before
    /// handing back a valid reply, to prove a read timeout no longer aborts
    /// the whole roundtrip.
    struct FlakyTransport {
        timeouts_remaining: usize,
        reply: [u8; 64],
        recv_calls: usize,
    }

    impl Transport for FlakyTransport {
        fn send(&mut self, _req: &[u8; 64]) -> Result<(), DeviceError> {
            Ok(())
        }
        fn recv(&mut self, _timeout: Duration) -> Result<[u8; 64], DeviceError> {
            self.recv_calls += 1;
            if self.timeouts_remaining > 0 {
                self.timeouts_remaining -= 1;
                return Err(DeviceError::Timeout);
            }
            Ok(self.reply)
        }
    }

    #[test]
    fn roundtrip_retries_past_a_read_timeout() {
        let req = wh_proto::cmds::read_global_travel();
        let good = reply_frame(req[2], &[0x00, 0, 0, 0xF4, 0x01, 0xC8, 0, 0xC8, 0]);
        let mut t = FlakyTransport { timeouts_remaining: 2, reply: good, recv_calls: 0 };
        let mut s = Session::new(t);
        let payload = s.roundtrip(&req).unwrap();
        assert_eq!(payload[3], 0xF4);
        t = s.into_inner();
        assert!(t.recv_calls > 1, "expected more than one recv call, got {}", t.recv_calls);
    }

    #[test]
    fn roundtrip_many_reports_partial_batch_failure() {
        let req1 = wh_proto::cmds::read_global_travel();
        let req2 = wh_proto::cmds::sync();
        let req3 = wh_proto::cmds::read_global_travel();
        let reply1 = reply_frame(0x29, &[0x00, 0, 0, 0xF4, 0x01, 0xC8, 0, 0xC8, 0]);
        // Frame 2 (req2) never gets a reply: the script ends right after it.
        let script = [
            format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(&req1)),
            format!("{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(&reply1)),
            format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(&req2)),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&script).unwrap());
        let err = s.roundtrip_many(&[req1, req2, req3]).unwrap_err();
        match err {
            crate::transport::DeviceError::Batch { index, total, applied, .. } => {
                assert_eq!(index, 1);
                assert_eq!(total, 3);
                assert_eq!(applied, 1);
            }
            other => panic!("expected Batch error, got {other:?}"),
        }
    }
}
