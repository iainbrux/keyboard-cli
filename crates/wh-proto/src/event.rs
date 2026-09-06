//! Frames the board sends without being asked. Measured 2026-09-04: `cmd 0x00` sub-order `0xbe`
//! announces the board's own adjust mode, `be 00` entering and `be 01` leaving
//! (`docs/protocol.md`, "The board announces its own adjust mode").

use crate::frame::{parse, REPLY_BIT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardEvent {
    AdjustModeEntered,
    AdjustModeLeft,
    /// A device-initiated frame this build does not recognise. Kept, never dropped. Reaches a
    /// caller whenever a frame is certainly device-initiated: any `00 be` frame mid-roundtrip
    /// (`be_event`), or anything at all from a poll (`any_event`). A non-`be` mismatch
    /// mid-roundtrip may still be a late reply, so `roundtrip` keeps skipping that case instead.
    Unknown(Vec<u8>),
}

/// The two measured adjust-mode edges, and nothing else: an unmeasured third byte is not an
/// edge. The strict classifier, used by `any_event` below and by direct callers that want to
/// know only the two measured edges, never a wider `00 be` match.
pub fn adjust_event(report: &[u8; 64]) -> Option<BoardEvent> {
    let reply = parse(report).ok()?;
    if reply.cmd != REPLY_BIT {
        return None;
    }
    match reply.payload {
        [0x00, 0xbe, 0x00, ..] => Some(BoardEvent::AdjustModeEntered),
        [0x00, 0xbe, 0x01, ..] => Some(BoardEvent::AdjustModeLeft),
        _ => None,
    }
}

/// Any `cmd 0x80` frame whose payload begins `00 be`, wider than `adjust_event`: the two measured
/// edges parse as themselves, any other third byte is `Unknown` carrying its payload. Grounded in
/// the measured fact that sub-order `0xbe` appears in no request in the corpus (`docs/protocol.md`),
/// so every `00 be` frame is certainly unsolicited; `roundtrip` routes on this, not `adjust_event`.
pub fn be_event(report: &[u8; 64]) -> Option<BoardEvent> {
    let reply = parse(report).ok()?;
    if reply.cmd != REPLY_BIT {
        return None;
    }
    match reply.payload {
        [0x00, 0xbe, 0x00, ..] => Some(BoardEvent::AdjustModeEntered),
        [0x00, 0xbe, 0x01, ..] => Some(BoardEvent::AdjustModeLeft),
        [0x00, 0xbe, ..] => Some(BoardEvent::Unknown(reply.payload.to_vec())),
        _ => None,
    }
}

/// Any frame received with nothing awaited is device-initiated by definition: the known edges
/// parse as themselves, everything else is `Unknown` carrying its payload (or the whole report
/// when it does not even frame).
pub fn any_event(report: &[u8; 64]) -> BoardEvent {
    if let Some(e) = adjust_event(report) {
        return e;
    }
    match parse(report) {
        Ok(reply) => BoardEvent::Unknown(reply.payload.to_vec()),
        Err(_) => BoardEvent::Unknown(report.to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two 0xbe frames, byte for byte as the board sent them in
    /// `captures/board-side-ap-change.jsonl` (embedded because captures/ is gitignored).
    fn entering() -> [u8; 64] {
        let mut f = [0u8; 64];
        f[..7].copy_from_slice(&[0x5c, 0x03, 0x80, 0x14, 0x00, 0xbe, 0x00]);
        f
    }
    fn leaving() -> [u8; 64] {
        let mut f = [0u8; 64];
        f[..7].copy_from_slice(&[0x5c, 0x03, 0x80, 0x15, 0x00, 0xbe, 0x01]);
        f
    }

    #[test]
    fn adjust_event_parses_both_measured_edges() {
        assert_eq!(
            adjust_event(&entering()),
            Some(BoardEvent::AdjustModeEntered)
        );
        assert_eq!(adjust_event(&leaving()), Some(BoardEvent::AdjustModeLeft));
    }

    #[test]
    fn adjust_event_ignores_an_ordinary_reply() {
        // A bd poll reply, the commonest frame on the wire: cmd 0x80, payload 00 bd 01 ff.
        let mut f = [0u8; 64];
        f[..8].copy_from_slice(&[0x5c, 0x04, 0x80, 0x14, 0x00, 0xbd, 0x01, 0xff]);
        assert_eq!(adjust_event(&f), None);
    }

    #[test]
    fn adjust_event_keeps_an_unmeasured_third_byte_out_of_the_known_edges() {
        // be 02 has never been observed; it must not read as either measured edge.
        let mut f = [0u8; 64];
        f[..7].copy_from_slice(&[0x5c, 0x03, 0x80, 0x16, 0x00, 0xbe, 0x02]);
        assert_eq!(adjust_event(&f), None);
        assert_eq!(any_event(&f), BoardEvent::Unknown(vec![0x00, 0xbe, 0x02]));
    }

    #[test]
    fn be_event_parses_both_measured_edges_like_adjust_event() {
        assert_eq!(be_event(&entering()), Some(BoardEvent::AdjustModeEntered));
        assert_eq!(be_event(&leaving()), Some(BoardEvent::AdjustModeLeft));
    }

    /// `be_event` is wider than `adjust_event`: the same be-02 frame the strict classifier
    /// refuses is still certainly device-initiated (no request in the corpus ever carries
    /// sub-order `0xbe`), so `be_event` queues it as `Unknown` instead of returning `None`.
    #[test]
    fn be_event_wraps_an_unmeasured_third_byte_as_unknown_rather_than_none() {
        let mut f = [0u8; 64];
        f[..7].copy_from_slice(&[0x5c, 0x03, 0x80, 0x16, 0x00, 0xbe, 0x02]);
        assert_eq!(
            be_event(&f),
            Some(BoardEvent::Unknown(vec![0x00, 0xbe, 0x02]))
        );
    }

    #[test]
    fn be_event_ignores_a_non_be_reply() {
        // A bd poll reply, the commonest frame on the wire: cmd 0x80, payload 00 bd 01 ff.
        let mut f = [0u8; 64];
        f[..8].copy_from_slice(&[0x5c, 0x04, 0x80, 0x14, 0x00, 0xbd, 0x01, 0xff]);
        assert_eq!(be_event(&f), None);
    }

    /// The cmd guard still applies: a `00 be 00` payload under a different cmd byte is not
    /// certainly-unsolicited, so `be_event` refuses it exactly as `adjust_event` does.
    #[test]
    fn be_event_ignores_an_edge_shaped_payload_under_another_cmd() {
        let mut f = [0u8; 64];
        f[..7].copy_from_slice(&[0x5c, 0x03, 0xa3, 0x37, 0x00, 0xbe, 0x00]);
        assert_eq!(be_event(&f), None);
    }

    #[test]
    fn any_event_wraps_a_non_be_frame_as_unknown_with_its_payload() {
        let mut f = [0u8; 64];
        f[..8].copy_from_slice(&[0x5c, 0x04, 0x80, 0x14, 0x00, 0xbd, 0x01, 0xff]);
        assert_eq!(
            any_event(&f),
            BoardEvent::Unknown(vec![0x00, 0xbd, 0x01, 0xff])
        );
    }

    /// A payload shaped exactly like an edge under a different command must not read as one:
    /// the cmd guard, not the payload match, is what makes these frames certainly unsolicited.
    #[test]
    fn adjust_event_ignores_an_edge_shaped_payload_under_another_cmd() {
        // cmd 0xA3 (a KEY reply), payload 00 be 00, checksum per the formula:
        // (0x35 + 0x5C + 0x03 + 0xA3 + 0x00) & 0xFF = 0x37.
        let mut f = [0u8; 64];
        f[..7].copy_from_slice(&[0x5c, 0x03, 0xa3, 0x37, 0x00, 0xbe, 0x00]);
        assert_eq!(adjust_event(&f), None);
    }

    #[test]
    fn any_event_wraps_an_unparseable_report_whole() {
        let f = [0xffu8; 64];
        assert_eq!(any_event(&f), BoardEvent::Unknown(f.to_vec()));
    }
}
