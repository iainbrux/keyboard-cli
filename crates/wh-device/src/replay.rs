use crate::transport::{DeviceError, Transport};
use std::time::Duration;

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_digit(b: u8, pos: usize) -> Result<u8, DeviceError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(DeviceError::Replay(format!(
            "invalid hex digit 0x{b:02x} at byte offset {pos} (not 0-9/a-f/A-F)"
        ))),
    }
}

fn unhex(s: &str) -> Result<[u8; 64], DeviceError> {
    // Raw bytes, not `&str` slices: a `&str` index landing inside a multi-byte UTF-8
    // character panics, and this parses untrusted JSONL from fixtures and captures.
    let bytes = s.as_bytes();
    if bytes.len() != 128 {
        return Err(DeviceError::Replay(format!(
            "hex must be exactly 128 characters (64 bytes), got {}",
            bytes.len()
        )));
    }
    let mut out = [0u8; 64];
    for i in 0..64 {
        let hi = hex_digit(bytes[2 * i], 2 * i)?;
        let lo = hex_digit(bytes[2 * i + 1], 2 * i + 1)?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Out([u8; 64]),
    In([u8; 64]),
    Wait(u32),
}

pub fn parse_jsonl(text: &str) -> Result<Vec<Entry>, DeviceError> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(i, l)| {
            let v: serde_json::Value =
                serde_json::from_str(l).map_err(|e| DeviceError::Replay(e.to_string()))?;
            match v["dir"].as_str() {
                Some("out") => Ok(Entry::Out(unhex(v["hex"].as_str().unwrap_or_default())?)),
                Some("in") => Ok(Entry::In(unhex(v["hex"].as_str().unwrap_or_default())?)),
                Some("wait") => {
                    let count = v["count"].as_u64().unwrap_or(1) as u32;
                    if count == 0 {
                        return Err(DeviceError::Replay(format!(
                            "wait count 0 at line {}",
                            i + 1
                        )));
                    }
                    Ok(Entry::Wait(count))
                }
                other => Err(DeviceError::Replay(format!("bad dir {other:?}"))),
            }
        })
        .collect()
}

pub struct ReplayTransport {
    script: Vec<Entry>,
    pos: usize,
}

impl ReplayTransport {
    pub fn from_jsonl(text: &str) -> Result<Self, DeviceError> {
        Ok(Self {
            script: parse_jsonl(text)?,
            pos: 0,
        })
    }
    pub fn finished(&self) -> bool {
        self.pos == self.script.len()
    }
}

impl Transport for ReplayTransport {
    fn send(&mut self, report: &[u8; 64]) -> Result<(), DeviceError> {
        match self.script.get(self.pos) {
            Some(Entry::Out(expected)) if expected == report => {
                self.pos += 1;
                Ok(())
            }
            Some(Entry::Out(expected)) => Err(DeviceError::Replay(format!(
                "send mismatch at {}: got {}, script has {}",
                self.pos,
                hex(report),
                hex(expected)
            ))),
            Some(Entry::In(bytes)) => Err(DeviceError::Replay(format!(
                "unexpected send at {}: script expected a recv here (next reply is {})",
                self.pos,
                hex(bytes)
            ))),
            Some(Entry::Wait(n)) => Err(DeviceError::Replay(format!(
                "unexpected send at {}: script expects {n} more empty polls here",
                self.pos
            ))),
            None => Err(DeviceError::Replay(format!(
                "unexpected send at {}: script is exhausted",
                self.pos
            ))),
        }
    }
    fn recv(&mut self, _timeout: Duration) -> Result<[u8; 64], DeviceError> {
        match self.script.get_mut(self.pos) {
            Some(Entry::In(bytes)) => {
                let b = *bytes;
                self.pos += 1;
                Ok(b)
            }
            Some(Entry::Out(_)) => Err(DeviceError::Replay(format!(
                "unexpected recv at {}: script expected a send here",
                self.pos
            ))),
            Some(Entry::Wait(n)) => {
                if *n > 1 {
                    *n -= 1;
                } else {
                    self.pos += 1;
                }
                Err(DeviceError::Timeout)
            }
            None => Err(DeviceError::Timeout),
        }
    }
}

pub struct RecordingTransport<T: Transport> {
    inner: T,
    log: Vec<Entry>,
}

impl<T: Transport> RecordingTransport<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            log: Vec::new(),
        }
    }
    pub fn jsonl(&self) -> String {
        self.log
            .iter()
            .map(|e| match e {
                Entry::Out(b) => format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(b)),
                Entry::In(b) => format!("{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(b)),
                // RecordingTransport never pushes a Wait onto `log`, only real sends and
                // receives; this arm exists so the writer stays total over `Entry`.
                Entry::Wait(n) => format!("{{\"dir\":\"wait\",\"count\":{n}}}"),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl<T: Transport> Transport for RecordingTransport<T> {
    fn send(&mut self, report: &[u8; 64]) -> Result<(), DeviceError> {
        self.inner.send(report)?;
        self.log.push(Entry::Out(*report));
        Ok(())
    }
    fn recv(&mut self, timeout: Duration) -> Result<[u8; 64], DeviceError> {
        let r = self.inner.recv(timeout)?;
        self.log.push(Entry::In(r));
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::Transport;
    use std::time::Duration;

    fn line(dir: &str, bytes: &[u8; 64]) -> String {
        format!("{{\"dir\":\"{dir}\",\"hex\":\"{}\"}}", hex(bytes))
    }

    #[test]
    fn replay_walks_script_in_order() {
        let out = wh_proto::cmds::read_global_travel();
        let mut reply = [0u8; 64];
        reply[..4].copy_from_slice(&[0x5C, 0x09, 0x29, 0x00]);
        // fix checksum
        reply[3] = wh_proto::frame::checksum(0x09, 0x29, &reply[4..13]);
        let script = format!("{}\n{}\n", line("out", &out), line("in", &reply));

        let mut t = ReplayTransport::from_jsonl(&script).unwrap();
        t.send(&out).unwrap();
        assert_eq!(t.recv(Duration::from_millis(10)).unwrap(), reply);
    }

    #[test]
    fn replay_rejects_wrong_send() {
        let out = wh_proto::cmds::read_global_travel();
        let script = line("out", &out);
        let mut t = ReplayTransport::from_jsonl(&script).unwrap();
        let other = wh_proto::cmds::sync();
        assert!(t.send(&other).is_err());
    }

    #[test]
    fn recording_logs_both_directions() {
        let out = wh_proto::cmds::sync();
        let mut reply = [0u8; 64];
        reply[..4].copy_from_slice(&[0x5C, 0x00, 0x01, 0x92]); // 0x35+0x5C+0x00+0x01
        let script = format!("{}\n{}\n", line("out", &out), line("in", &reply));
        let inner = ReplayTransport::from_jsonl(&script).unwrap();
        let mut rec = RecordingTransport::new(inner);
        rec.send(&out).unwrap();
        rec.recv(Duration::from_millis(10)).unwrap();
        let log = rec.jsonl();
        assert_eq!(log.lines().count(), 2);
        assert!(log.lines().next().unwrap().contains("\"out\""));
    }

    #[test]
    fn unhex_rejects_odd_length_without_panicking() {
        assert!(unhex("abc").is_err());
    }

    #[test]
    fn unhex_rejects_non_hex_character_without_panicking() {
        let mut s = "a".repeat(128);
        s.replace_range(0..1, "z");
        assert!(unhex(&s).is_err());
    }

    #[test]
    fn unhex_rejects_non_ascii_character_without_panicking() {
        // "0é0" is 4 bytes; 32 repeats give exactly 128 bytes, so the case under test is
        // the multi-byte char itself, not a length mismatch.
        let s = "0é0".repeat(32);
        assert_eq!(s.len(), 128);
        assert!(unhex(&s).is_err());
    }

    #[test]
    fn unhex_rejects_too_short_hex() {
        let s = "a".repeat(126);
        assert!(unhex(&s).is_err());
    }

    #[test]
    fn unhex_rejects_too_long_hex() {
        let s = "a".repeat(130);
        assert!(unhex(&s).is_err());
    }

    #[test]
    fn parse_jsonl_rejects_bad_dir() {
        let out = wh_proto::cmds::sync();
        let bad = format!("{{\"dir\":\"sideways\",\"hex\":\"{}\"}}", hex(&out));
        assert!(parse_jsonl(&bad).is_err());
    }

    #[test]
    fn recv_when_script_expects_send_is_replay_error_not_timeout() {
        let out = wh_proto::cmds::sync();
        let script = line("out", &out);
        let mut t = ReplayTransport::from_jsonl(&script).unwrap();
        let err = t.recv(Duration::from_millis(10)).unwrap_err();
        assert!(
            matches!(err, DeviceError::Replay(_)),
            "expected Replay error, got {err:?}"
        );
    }

    #[test]
    fn recv_past_end_of_script_is_still_timeout() {
        let mut t = ReplayTransport::from_jsonl("").unwrap();
        let err = t.recv(Duration::from_millis(10)).unwrap_err();
        assert!(
            matches!(err, DeviceError::Timeout),
            "expected Timeout, got {err:?}"
        );
    }

    #[test]
    fn wait_entry_serves_one_timeout_then_advances() {
        let f = wh_proto::frame::frame(0x80, &[0x00, 0xbe, 0x00]).unwrap();
        let script = format!(
            "{{\"dir\":\"wait\"}}\n{{\"dir\":\"in\",\"hex\":\"{}\"}}",
            hex(&f)
        );
        let mut t = ReplayTransport::from_jsonl(&script).unwrap();
        assert!(matches!(
            t.recv(Duration::from_millis(1)),
            Err(DeviceError::Timeout)
        ));
        assert_eq!(t.recv(Duration::from_millis(1)).unwrap(), f);
        assert!(t.finished());
    }

    #[test]
    fn counted_wait_serves_that_many_timeouts() {
        // A trailing `In` entry after the wait, and a `finished()` check at every step, so an
        // implementation that consumes the whole `Wait` on the first `recv` cannot pass by
        // having its extra recv calls land on an exhausted script (which is Timeout too).
        let f = wh_proto::frame::frame(0x80, &[0x00, 0xbe, 0x00]).unwrap();
        let script = format!(
            "{{\"dir\":\"wait\",\"count\":3}}\n{{\"dir\":\"in\",\"hex\":\"{}\"}}",
            hex(&f)
        );
        let mut t = ReplayTransport::from_jsonl(&script).unwrap();
        assert!(!t.finished());
        assert!(matches!(
            t.recv(Duration::from_millis(1)),
            Err(DeviceError::Timeout)
        ));
        assert!(!t.finished());
        assert!(matches!(
            t.recv(Duration::from_millis(1)),
            Err(DeviceError::Timeout)
        ));
        assert!(!t.finished());
        assert!(matches!(
            t.recv(Duration::from_millis(1)),
            Err(DeviceError::Timeout)
        ));
        assert!(!t.finished());
        assert_eq!(t.recv(Duration::from_millis(1)).unwrap(), f);
        assert!(t.finished());
    }

    #[test]
    fn send_at_a_wait_entry_is_a_loud_replay_error() {
        let mut t = ReplayTransport::from_jsonl("{\"dir\":\"wait\"}").unwrap();
        let out = wh_proto::cmds::read_profile();
        match t.send(&out) {
            Err(DeviceError::Replay(msg)) => assert_eq!(
                msg,
                "unexpected send at 0: script expects 1 more empty polls here"
            ),
            other => panic!("expected Replay error, got {other:?}"),
        }
    }

    #[test]
    fn wait_count_zero_is_rejected_at_parse() {
        assert!(matches!(
            ReplayTransport::from_jsonl("{\"dir\":\"wait\",\"count\":0}"),
            Err(DeviceError::Replay(_))
        ));
    }

    #[test]
    fn poll_event_over_a_wait_script_returns_none_then_the_edge() {
        let f = wh_proto::frame::frame(0x80, &[0x00, 0xbe, 0x00]).unwrap();
        let script = format!(
            "{{\"dir\":\"wait\"}}\n{{\"dir\":\"in\",\"hex\":\"{}\"}}",
            hex(&f)
        );
        let t = ReplayTransport::from_jsonl(&script).unwrap();
        let mut s = crate::session::Session::new(t);
        assert!(s.poll_event(Duration::from_millis(1)).unwrap().is_none());
        assert!(matches!(
            s.poll_event(Duration::from_millis(1)).unwrap(),
            Some(wh_proto::event::BoardEvent::AdjustModeEntered)
        ));
    }
}
