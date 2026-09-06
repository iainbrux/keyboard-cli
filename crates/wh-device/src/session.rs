use crate::transport::{DeviceError, Transport};
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use wh_proto::event::BoardEvent;
use wh_proto::frame::{parse, FrameError, REPLY_BIT};

pub const READ_TIMEOUT: Duration = Duration::from_millis(250);
/// Wall-clock budget for one roundtrip. Time-based rather than attempt-based, since the
/// keyboard emits unsolicited input reports while the user types and a fixed attempt count
/// could be exhausted by ordinary typing and surface as a false Timeout.
pub const TOTAL_TIMEOUT: Duration = Duration::from_millis(1500);
/// Secondary, generous runaway guard on top of the wall-clock deadline.
pub const MAX_READS: usize = 256;

pub struct Session<T: Transport> {
    t: T,
    events: VecDeque<BoardEvent>,
}

impl<T: Transport> Session<T> {
    pub fn new(t: T) -> Self {
        Self {
            t,
            events: VecDeque::new(),
        }
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
            // Checked before the reply match: a 0xbe edge during a cmd 0x00 roundtrip carries
            // cmd 0x80 too, and would otherwise be mistaken for the awaited reply. Routed on
            // `be_event`, wider than `adjust_event`: every `00 be` frame is certainly
            // unsolicited (no request in the corpus carries sub-order `0xbe`), so an unmeasured
            // third byte is queued too, rather than falling through and matching as the reply.
            if let Some(e) = wh_proto::event::be_event(&report) {
                self.events.push_back(e);
                continue;
            }
            match parse(&report) {
                // The device sets the high bit on the reply's cmd byte (see `REPLY_BIT`),
                // never echoing the request's cmd byte unmodified.
                Ok(reply) if reply.cmd == req[2] | REPLY_BIT => return Ok(reply.payload.to_vec()),
                Err(FrameError::DeviceFail(code)) => {
                    return Err(DeviceError::Frame(FrameError::DeviceFail(code)))
                }
                _ => continue, // unrelated or malformed input report, skip
            }
        }
        Err(DeviceError::Timeout)
    }

    /// Send frames one at a time, one matched reply each. On failure, reports how many
    /// frames already reached the device so the caller can tell the user whether to
    /// restore from backup.
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

    /// One bounded listen for a device-initiated frame. Drains queued events first; a quiet
    /// wire is `Ok(None)`, the normal idle case.
    pub fn poll_event(&mut self, timeout: Duration) -> Result<Option<BoardEvent>, DeviceError> {
        if let Some(e) = self.events.pop_front() {
            return Ok(Some(e));
        }
        match self.t.recv(timeout) {
            Ok(report) => Ok(Some(wh_proto::event::any_event(&report))),
            Err(DeviceError::Timeout) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// What arrived uninvited while this session worked, drained: a caller reports each event
    /// once, and a second call answers nothing. Returns an owned `Vec`, not a lazy `Drain`: a
    /// short-circuiting read (`.any(...)`) over a `Drain` still silently discards whatever it
    /// never yielded, since dropping a `Drain` finishes draining the queue regardless.
    pub fn pending_events(&mut self) -> Vec<BoardEvent> {
        self.events.drain(..).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::{hex, ReplayTransport};

    /// Builds a reply frame with the high bit set on the command byte, matching how the
    /// real device sends it (`wh_proto::frame::REPLY_BIT`).
    fn reply_frame(cmd: u8, payload: &[u8]) -> [u8; 64] {
        wh_proto::frame::frame(cmd | wh_proto::frame::REPLY_BIT, payload).unwrap()
    }

    /// Builds a `ReplayTransport` directly from `Entry` values, via the same jsonl text
    /// path `from_jsonl` already takes, so no new parsing surface is introduced.
    fn replay_with(entries: Vec<crate::replay::Entry>) -> ReplayTransport {
        let script = entries
            .iter()
            .map(|e| match e {
                crate::replay::Entry::Out(b) => {
                    format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(b))
                }
                crate::replay::Entry::In(b) => {
                    format!("{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(b))
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        ReplayTransport::from_jsonl(&script).unwrap()
    }

    /// The `0xbe` entering edge, byte for byte as the board sent it, built via the real
    /// encoder rather than a hand-typed checksum.
    fn be_entering() -> [u8; 64] {
        wh_proto::frame::frame(0x80, &[0x00, 0xbe, 0x00]).unwrap()
    }
    fn be_leaving() -> [u8; 64] {
        wh_proto::frame::frame(0x80, &[0x00, 0xbe, 0x01]).unwrap()
    }
    /// An unmeasured third byte on a `0xbe` frame: never observed, but still certainly
    /// unsolicited, since sub-order `0xbe` appears in no request in the corpus.
    fn be_unmeasured() -> [u8; 64] {
        wh_proto::frame::frame(0x80, &[0x00, 0xbe, 0x02]).unwrap()
    }

    /// A `bd` poll reply, cmd 0x80, payload `00 bd 01 ff`.
    fn bd_reply() -> [u8; 64] {
        wh_proto::frame::frame(0x80, &[0x00, 0xbd, 0x01, 0xff]).unwrap()
    }

    /// A profile-read reply shaped `[status, sub-order, index, 0xff]` for profile index 0,
    /// matching `wh_proto::cmds::read_profile`'s expected reply shape.
    fn profile_reply_frame() -> [u8; 64] {
        wh_proto::frame::frame(0x80, &[0x00, 0x70, 0x00, 0xff]).unwrap()
    }

    #[test]
    fn poll_event_returns_a_scripted_edge_and_then_none_on_exhaustion() {
        let mut s = Session::new(replay_with(vec![crate::replay::Entry::In(be_entering())]));
        assert_eq!(
            s.poll_event(Duration::from_millis(1)).unwrap(),
            Some(BoardEvent::AdjustModeEntered)
        );
        assert_eq!(s.poll_event(Duration::from_millis(1)).unwrap(), None);
    }

    #[test]
    fn poll_event_wraps_a_non_be_frame_as_unknown() {
        let mut s = Session::new(replay_with(vec![crate::replay::Entry::In(bd_reply())]));
        match s.poll_event(Duration::from_millis(1)).unwrap() {
            Some(BoardEvent::Unknown(p)) => assert_eq!(p, vec![0x00, 0xbd, 0x01, 0xff]),
            other => panic!("wanted Unknown, got {other:?}"),
        }
    }

    #[test]
    fn poll_event_returns_a_queued_event_before_the_next_scripted_one_in_order() {
        // A roundtrip queues `Entered`; `Left` sits scripted right after the matched reply.
        // The queue must answer first, and answering it must not touch the transport, so
        // the second call finds `Left` still waiting rather than a now-empty script.
        let req = wh_proto::cmds::read_profile();
        let mut s = Session::new(replay_with(vec![
            crate::replay::Entry::Out(req),
            crate::replay::Entry::In(be_entering()),
            crate::replay::Entry::In(profile_reply_frame()),
            crate::replay::Entry::In(be_leaving()),
        ]));
        s.roundtrip(&req).unwrap();
        assert_eq!(
            s.poll_event(Duration::from_millis(1)).unwrap(),
            Some(BoardEvent::AdjustModeEntered)
        );
        assert_eq!(
            s.poll_event(Duration::from_millis(1)).unwrap(),
            Some(BoardEvent::AdjustModeLeft)
        );
    }

    #[test]
    fn roundtrip_queues_an_edge_and_still_matches_its_reply() {
        // Script: out request, in 0xbe edge, in real reply. The edge arrives mid-roundtrip.
        let req = wh_proto::cmds::read_profile();
        let mut s = Session::new(replay_with(vec![
            crate::replay::Entry::Out(req),
            crate::replay::Entry::In(be_entering()),
            crate::replay::Entry::In(profile_reply_frame()),
        ]));
        let payload = s.roundtrip(&req).unwrap();
        assert_eq!(payload[..3], [0x00, 0x70, 0x00]);
        assert_eq!(s.pending_events(), vec![BoardEvent::AdjustModeEntered]);
    }

    /// The closed failure this task fixed: before `roundtrip` routed on `be_event`, a `be 02`
    /// mid-`cmd 0x00` roundtrip fell through to the reply match, mismatched, and killed the
    /// whole command with an opaque decode error instead of being queued as unsolicited.
    #[test]
    fn roundtrip_queues_an_unmeasured_be_frame_as_unknown_and_still_succeeds() {
        let req = wh_proto::cmds::read_profile();
        let mut s = Session::new(replay_with(vec![
            crate::replay::Entry::Out(req),
            crate::replay::Entry::In(be_unmeasured()),
            crate::replay::Entry::In(profile_reply_frame()),
        ]));
        let payload = s.roundtrip(&req).unwrap();
        assert_eq!(payload[..3], [0x00, 0x70, 0x00]);
        assert_eq!(
            s.pending_events(),
            vec![BoardEvent::Unknown(vec![0x00, 0xbe, 0x02])]
        );
    }

    #[test]
    fn roundtrip_does_not_return_an_edge_as_a_cmd_zero_reply() {
        // The hazard: a bd poll's reply match is cmd 0x80, which a 0xbe frame also carries.
        // The edge must be queued and the real bd reply returned, not the edge as the reply.
        // No poll_bd-style encoder exists in wh_proto::cmds, so hand-built via the real
        // frame encoder rather than a checksum literal.
        let req = wh_proto::frame::frame(0x00, &[0xbd, 0x01, 0xff, 0xff]).unwrap();
        let mut s = Session::new(replay_with(vec![
            crate::replay::Entry::Out(req),
            crate::replay::Entry::In(be_entering()),
            crate::replay::Entry::In(bd_reply()),
        ]));
        let payload = s.roundtrip(&req).unwrap();
        assert_eq!(payload[..2], [0x00, 0xbd]);
        assert_eq!(s.pending_events(), vec![BoardEvent::AdjustModeEntered]);
    }

    #[test]
    fn roundtrip_queues_an_edge_during_a_non_cmd_zero_roundtrip() {
        // cmd 0x23 (KEY layout read), wh dump's shape and the commonest traffic in the repo.
        // The routing must fire regardless of which cmd the roundtrip is waiting on, not only
        // cmd 0x00: a version conditioned on `req[2] == 0x00` would silently drop this edge.
        let req = wh_proto::cmds::read_key_layout(0x04, 0x14);
        let reply = reply_frame(wh_proto::cmds::cmd::KEY, &[0x01, 0x04, 0x14, 0xF4, 0x01]);
        let mut s = Session::new(replay_with(vec![
            crate::replay::Entry::Out(req),
            crate::replay::Entry::In(be_entering()),
            crate::replay::Entry::In(reply),
        ]));
        let payload = s.roundtrip(&req).unwrap();
        assert_eq!(payload, vec![0x01, 0x04, 0x14, 0xF4, 0x01]);
        assert_eq!(s.pending_events(), vec![BoardEvent::AdjustModeEntered]);
    }

    #[test]
    fn pending_events_drains_once() {
        // Seed the queue through a real roundtrip rather than poll_event, so this pins the
        // drain itself: a `drain(..0)` mutation would leave the first collect empty.
        let req = wh_proto::cmds::read_profile();
        let mut s = Session::new(replay_with(vec![
            crate::replay::Entry::Out(req),
            crate::replay::Entry::In(be_leaving()),
            crate::replay::Entry::In(profile_reply_frame()),
        ]));
        s.roundtrip(&req).unwrap();
        assert_eq!(s.pending_events(), vec![BoardEvent::AdjustModeLeft]);
        // The first drain took everything; a second drain on the same queue is empty.
        assert_eq!(s.pending_events().len(), 0);
    }

    /// The hazard the reviewer measured: `.any(|e| e == AdjustModeLeft)` over a queue of `[Left,
    /// Entered]` short-circuits on the first match, and a `Drain`'s own `Drop` impl would then
    /// silently discard the trailing `Entered` it never yielded. Returning a `Vec` means the
    /// whole queue is already materialized before the caller runs any predicate over it, so a
    /// short-circuiting read cannot lose what it never looked at.
    #[test]
    fn pending_events_returns_a_vec_a_short_circuiting_read_cannot_lose_the_rest_of() {
        let req = wh_proto::cmds::read_profile();
        let mut s = Session::new(replay_with(vec![
            crate::replay::Entry::Out(req),
            crate::replay::Entry::In(be_leaving()),
            crate::replay::Entry::In(be_entering()),
            crate::replay::Entry::In(profile_reply_frame()),
        ]));
        s.roundtrip(&req).unwrap();
        let events = s.pending_events();
        // A short-circuiting read, the TUI's natural "is the board modal right now" idiom.
        assert!(events.contains(&BoardEvent::AdjustModeLeft));
        // The trailing edge must still be reachable in what was returned: proves the vec was
        // fully drained up front, not lazily during iteration.
        assert_eq!(events.len(), 2);
        assert_eq!(events[1], BoardEvent::AdjustModeEntered);
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
        assert!(matches!(
            s.roundtrip(&req),
            Err(crate::transport::DeviceError::Timeout)
        ));
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
        // Enough unrelated reports to exhaust MAX_READS. ReplayTransport's recv ignores
        // the timeout it's given and returns instantly, so this trips the attempt bound,
        // not the 1500ms wall-clock deadline.
        let req = wh_proto::cmds::read_global_travel();
        let noise = reply_frame(0x12, &[0x01]);
        let mut lines = vec![format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(&req))];
        for _ in 0..(MAX_READS + 10) {
            lines.push(format!("{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(&noise)));
        }
        let script = lines.join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&script).unwrap());
        assert!(matches!(
            s.roundtrip(&req),
            Err(crate::transport::DeviceError::Timeout)
        ));
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

    /// Returns `Timeout` a fixed number of times before a valid reply, to prove a read
    /// timeout doesn't abort the whole roundtrip.
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
        let mut t = FlakyTransport {
            timeouts_remaining: 2,
            reply: good,
            recv_calls: 0,
        };
        let mut s = Session::new(t);
        let payload = s.roundtrip(&req).unwrap();
        assert_eq!(payload[3], 0xF4);
        t = s.into_inner();
        assert!(
            t.recv_calls > 1,
            "expected more than one recv call, got {}",
            t.recv_calls
        );
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
            crate::transport::DeviceError::Batch {
                index,
                total,
                applied,
                ..
            } => {
                assert_eq!(index, 1);
                assert_eq!(total, 3);
                assert_eq!(applied, 1);
            }
            other => panic!("expected Batch error, got {other:?}"),
        }
    }
}
