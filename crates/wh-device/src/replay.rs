use crate::transport::{DeviceError, Transport};
use std::time::Duration;

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Result<[u8; 64], DeviceError> {
    let bytes: Result<Vec<u8>, _> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect();
    let v = bytes.map_err(|e| DeviceError::Replay(e.to_string()))?;
    v.try_into().map_err(|_| DeviceError::Replay("hex is not 64 bytes".into()))
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Out([u8; 64]),
    In([u8; 64]),
}

pub fn parse_jsonl(text: &str) -> Result<Vec<Entry>, DeviceError> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: serde_json::Value =
                serde_json::from_str(l).map_err(|e| DeviceError::Replay(e.to_string()))?;
            let bytes = unhex(v["hex"].as_str().unwrap_or_default())?;
            match v["dir"].as_str() {
                Some("out") => Ok(Entry::Out(bytes)),
                Some("in") => Ok(Entry::In(bytes)),
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
        Ok(Self { script: parse_jsonl(text)?, pos: 0 })
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
                self.pos, hex(report), hex(expected)
            ))),
            other => Err(DeviceError::Replay(format!("unexpected send at {}: {other:?}", self.pos))),
        }
    }
    fn recv(&mut self, _timeout: Duration) -> Result<[u8; 64], DeviceError> {
        match self.script.get(self.pos) {
            Some(Entry::In(bytes)) => {
                let b = *bytes;
                self.pos += 1;
                Ok(b)
            }
            _ => Err(DeviceError::Timeout),
        }
    }
}

pub struct RecordingTransport<T: Transport> {
    inner: T,
    log: Vec<Entry>,
}

impl<T: Transport> RecordingTransport<T> {
    pub fn new(inner: T) -> Self {
        Self { inner, log: Vec::new() }
    }
    pub fn jsonl(&self) -> String {
        self.log
            .iter()
            .map(|e| match e {
                Entry::Out(b) => format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(b)),
                Entry::In(b) => format!("{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(b)),
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
}
