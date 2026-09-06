//! Frames the board sends without being asked. Measured 2026-09-04: `cmd 0x00` sub-order `0xbe`
//! announces the board's own adjust mode, `be 00` entering and `be 01` leaving
//! (`docs/protocol.md`, "The board announces its own adjust mode").

use crate::frame::{parse, REPLY_BIT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardEvent {
    AdjustModeEntered,
    AdjustModeLeft,
    /// A device-initiated frame this build does not recognise. Kept, never dropped: the corpus
    /// proves the board volunteers frames, and a new one must surface rather than vanish.
    Unknown(Vec<u8>),
}

/// The two measured adjust-mode edges, and nothing else: an unmeasured third byte is not an
/// edge. Callers in a roundtrip use this, so only certainly-unsolicited frames leave the reply
/// path.
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
