//! `cmd 0x2c`: SOCD pairings. Measured 2026-09-05, see `docs/protocol.md` under "SOCD".
//!
//! A pairing is modelled as an unordered pair of keys plus a winner, never as the wire's own
//! priority byte. The board normalises replies per queried key, reordering the two records and
//! re-basing the priority to put the queried key first, so one setting has two spellings on the
//! wire and only a model without a raw byte in it can compare them.

use crate::cmds::{cmd, DecodeError, RW_READ, RW_WRITE};
use crate::frame::{frame, REPORT_LEN};

/// Wire priority `0`: whichever key was pressed last wins. The UI calls this LAST-INPUT.
pub const PRIO_LAST_INPUT: u8 = 0;
/// Wire priority `1`: the first key in the frame's own record order wins.
pub const PRIO_FIRST: u8 = 1;
/// Wire priority `2`: the second key in the frame's own record order wins.
pub const PRIO_SECOND: u8 = 2;

/// The payload length every measured `cmd 0x2c` frame carries, request and reply alike.
const PAYLOAD_LEN: usize = 10;

/// How `Priority::LastInput` is spelled everywhere it is written or read: the `--priority` value
/// an operator types, and the word every announcement and error prints. One constant, so the flag
/// and the output can never drift apart.
pub const LAST_INPUT: &str = "last-input";

/// How a pairing resolves when both keys are held, in the UI's own vocabulary rather than the
/// wire's. `Wins` names the key that wins, so it survives the board's per-key re-basing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    LastInput,
    Wins(u8),
}

impl Priority {
    /// `last-input`, or the winning key's name. Never a wire byte: the byte only means anything
    /// beside the record order it came with.
    pub fn label(self) -> String {
        match self {
            Priority::LastInput => LAST_INPUT.to_string(),
            Priority::Wins(w) => crate::keys::label(w),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PairError {
    #[error("a SOCD pair needs two different keys, got {0:#04x} twice")]
    SameKey(u8),
    #[error("priority key {0:#04x} is not one of the pair's two keys")]
    WinnerNotInPair(u8),
}

/// One SOCD pairing: two keys and a winner. The keys are held in whatever order they arrived,
/// which is what renders to the wire and what `list` prints, but equality ignores that order:
/// see the `PartialEq` below.
#[derive(Debug, Clone, Copy, Eq)]
pub struct Pairing {
    keys: (u8, u8),
    priority: Priority,
}

/// Two pairings are equal when they hold the same two keys and the same winner, whichever order
/// each carries them in. This is the whole point of the model: the board answers a query on one
/// member with the records reordered and the priority byte re-based, so the two spellings of one
/// pairing must compare equal or every readback would look like a mismatch.
impl PartialEq for Pairing {
    fn eq(&self, other: &Self) -> bool {
        let mine = ordered(self.keys);
        let theirs = ordered(other.keys);
        mine == theirs && self.priority == other.priority
    }
}

fn ordered(k: (u8, u8)) -> (u8, u8) {
    if k.0 <= k.1 {
        k
    } else {
        (k.1, k.0)
    }
}

impl Pairing {
    /// Refuses a pair of one key with itself, and a winner that is neither of the two: both are
    /// unrepresentable on the wire, so catching them here means no caller can build a `Pairing`
    /// that `write_pair` would have to guess about.
    pub fn new(a: u8, b: u8, priority: Priority) -> Result<Self, PairError> {
        if a == b {
            return Err(PairError::SameKey(a));
        }
        if let Priority::Wins(w) = priority {
            if w != a && w != b {
                return Err(PairError::WinnerNotInPair(w));
            }
        }
        Ok(Pairing {
            keys: (a, b),
            priority,
        })
    }

    /// The two keys in the order this pairing holds them, which is the order `write_pair` puts
    /// on the wire and the order `list` prints.
    pub fn keys(self) -> (u8, u8) {
        self.keys
    }

    pub fn priority(self) -> Priority {
        self.priority
    }

    pub fn contains(self, usage: u8) -> bool {
        self.keys.0 == usage || self.keys.1 == usage
    }

    /// This pairing in the vendor UI's vocabulary: `w + s, priority: s`. The one rendering of a
    /// pairing anywhere in `wh`, shared by the CLI's output and `wh-device`'s errors so the two
    /// can never describe the same board state differently.
    pub fn describe(self) -> String {
        format!(
            "{} + {}, priority: {}",
            crate::keys::label(self.keys.0),
            crate::keys::label(self.keys.1),
            self.priority.label()
        )
    }

    /// The other key of the pair, or `None` if `usage` is not in it at all.
    pub fn partner(self, usage: u8) -> Option<u8> {
        match self.keys {
            (a, b) if a == usage => Some(b),
            (a, b) if b == usage => Some(a),
            _ => None,
        }
    }
}

/// The query request: `00 <key> ff` then zeros, byte-exact against `socd-reload-read`'s four
/// reads.
pub fn read_pairing(key: u8) -> [u8; REPORT_LEN] {
    let mut p = [0u8; PAYLOAD_LEN];
    p[0] = RW_READ;
    p[1] = key;
    p[2] = 0xFF;
    frame(cmd::SOCD, &p).expect("fixed size")
}

/// The pair write: `01 <a> <b> 00 <b> <a> 00 00 <prio> 00`, one frame carrying one pair.
///
/// Byte 7 is the vendored spec's `type` field ("send by position" versus "send by key"), `0` in
/// every captured frame and written `0` here.
pub fn write_pair(p: Pairing) -> [u8; REPORT_LEN] {
    let (a, b) = p.keys();
    let prio = match p.priority() {
        Priority::LastInput => PRIO_LAST_INPUT,
        Priority::Wins(w) if w == a => PRIO_FIRST,
        Priority::Wins(_) => PRIO_SECOND,
    };
    let payload = [RW_WRITE, a, b, 0, b, a, 0, 0, prio, 0];
    frame(cmd::SOCD, &payload).expect("fixed size")
}

/// Parses a `cmd 0x2c` reply, a query answer or a write echo alike, into the normalised model.
/// The keys come back in the reply's own order, so the caller can print what the board said;
/// `Pairing`'s equality is what makes the two spellings compare equal.
///
/// Every structural check is enforced: the two records must mirror each other, the pair must be
/// two different non-zero keys, and the priority must be one of the three measured values.
pub fn parse_pairing(payload: &[u8]) -> Result<Pairing, DecodeError> {
    if payload.len() < PAYLOAD_LEN {
        return Err(DecodeError::Short(payload.len()));
    }
    let (a, b) = (payload[1], payload[2]);
    if a == 0 || b == 0 {
        return Err(DecodeError::Socd("pairing names key 0x00"));
    }
    if payload[4] != b || payload[5] != a {
        return Err(DecodeError::Socd(
            "the two records do not mirror each other",
        ));
    }
    let priority = match payload[8] {
        PRIO_LAST_INPUT => Priority::LastInput,
        PRIO_FIRST => Priority::Wins(a),
        PRIO_SECOND => Priority::Wins(b),
        other => return Err(DecodeError::SocdPriority(other)),
    };
    Pairing::new(a, b, priority).map_err(|_| DecodeError::Socd("pairing names one key twice"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `captures/socd-toggle-on.jsonl` frame 2: enabling W+S with LAST-INPUT priority. Frames 0
    /// and 1 are a `bd` poll pair. The capture files are gitignored, so the bytes are literals.
    const TOGGLE_ON_WRITE: [u8; PAYLOAD_LEN] =
        [0x01, 0x1a, 0x16, 0x00, 0x16, 0x1a, 0x00, 0x00, 0x00, 0x00];
    /// `captures/socd-add-qe.jsonl` frame 0: adding Q+E with W+S already paired. A write carries
    /// one pair, not the table.
    const ADD_QE_WRITE: [u8; PAYLOAD_LEN] =
        [0x01, 0x14, 0x08, 0x00, 0x08, 0x14, 0x00, 0x00, 0x00, 0x00];
    /// `captures/socd-mode-change-s.jsonl` frame 23: W+S again, priority `2`, S wins.
    const MODE_CHANGE_S_WRITE: [u8; PAYLOAD_LEN] =
        [0x01, 0x1a, 0x16, 0x00, 0x16, 0x1a, 0x00, 0x00, 0x02, 0x00];

    /// `captures/socd-reload-read.jsonl` frames 150 to 157: the connect sequence's four queries
    /// and their replies, in capture order (Q, W, E, S).
    const RELOAD_READS: [([u8; PAYLOAD_LEN], [u8; PAYLOAD_LEN]); 4] = [
        (
            [0x00, 0x14, 0xff, 0, 0, 0, 0, 0, 0, 0],
            [0x00, 0x14, 0x08, 0x00, 0x08, 0x14, 0x00, 0x00, 0x00, 0x00],
        ),
        (
            [0x00, 0x1a, 0xff, 0, 0, 0, 0, 0, 0, 0],
            [0x00, 0x1a, 0x16, 0x00, 0x16, 0x1a, 0x00, 0x00, 0x02, 0x00],
        ),
        (
            [0x00, 0x08, 0xff, 0, 0, 0, 0, 0, 0, 0],
            [0x00, 0x08, 0x14, 0x00, 0x14, 0x08, 0x00, 0x00, 0x00, 0x00],
        ),
        (
            [0x00, 0x16, 0xff, 0, 0, 0, 0, 0, 0, 0],
            [0x00, 0x16, 0x1a, 0x00, 0x1a, 0x16, 0x00, 0x00, 0x01, 0x00],
        ),
    ];

    const W: u8 = 0x1a;
    const S: u8 = 0x16;
    const Q: u8 = 0x14;
    const E: u8 = 0x08;

    fn payload_of(f: &[u8; REPORT_LEN]) -> &[u8] {
        let len = f[1] as usize;
        &f[4..4 + len]
    }

    #[test]
    fn write_pair_reproduces_the_captured_write_frames_byte_for_byte() {
        let toggle_on = write_pair(Pairing::new(W, S, Priority::LastInput).unwrap());
        assert_eq!(
            payload_of(&toggle_on),
            TOGGLE_ON_WRITE,
            "socd-toggle-on's W+S write"
        );
        assert_eq!(toggle_on, frame(cmd::SOCD, &TOGGLE_ON_WRITE).unwrap());

        let add_qe = write_pair(Pairing::new(Q, E, Priority::LastInput).unwrap());
        assert_eq!(payload_of(&add_qe), ADD_QE_WRITE, "socd-add-qe's Q+E write");
        assert_eq!(add_qe, frame(cmd::SOCD, &ADD_QE_WRITE).unwrap());
    }

    /// The winner-to-byte mapping in both directions: S wins is `2` when W is written first and
    /// `1` when S is. The first of those is the captured `socd-mode-change-s` frame.
    #[test]
    fn write_pair_encodes_the_winner_from_the_order_it_holds() {
        let w_first = write_pair(Pairing::new(W, S, Priority::Wins(S)).unwrap());
        assert_eq!(payload_of(&w_first), MODE_CHANGE_S_WRITE);

        let s_first = write_pair(Pairing::new(S, W, Priority::Wins(S)).unwrap());
        assert_eq!(
            payload_of(&s_first),
            [0x01, 0x16, 0x1a, 0x00, 0x1a, 0x16, 0x00, 0x00, 0x01, 0x00]
        );

        let w_wins = write_pair(Pairing::new(W, S, Priority::Wins(W)).unwrap());
        assert_eq!(
            payload_of(&w_wins),
            [0x01, 0x1a, 0x16, 0x00, 0x16, 0x1a, 0x00, 0x00, 0x01, 0x00]
        );
    }

    #[test]
    fn read_pairing_reproduces_the_captured_query_requests() {
        for (request, _) in RELOAD_READS {
            let key = request[1];
            assert_eq!(
                payload_of(&read_pairing(key)),
                request,
                "query for {key:#04x}"
            );
        }
    }

    /// The reload's four replies parse into two pairings, with the right winners, from either
    /// member's row. W+S reads `02` from W and `01` from S; both mean S wins.
    #[test]
    fn parse_pairing_reads_both_spellings_of_the_same_pairing() {
        let from_q = parse_pairing(&RELOAD_READS[0].1).unwrap();
        let from_w = parse_pairing(&RELOAD_READS[1].1).unwrap();
        let from_e = parse_pairing(&RELOAD_READS[2].1).unwrap();
        let from_s = parse_pairing(&RELOAD_READS[3].1).unwrap();

        assert_eq!(from_q.priority(), Priority::LastInput);
        assert_eq!(from_e.priority(), Priority::LastInput);
        assert_eq!(from_w.priority(), Priority::Wins(S));
        assert_eq!(from_s.priority(), Priority::Wins(S));

        // Key order differs between the two rows of each pairing, and they still compare equal.
        assert_eq!(from_q.keys(), (Q, E));
        assert_eq!(from_e.keys(), (E, Q));
        assert_eq!(from_q, from_e);
        assert_eq!(from_w.keys(), (W, S));
        assert_eq!(from_s.keys(), (S, W));
        assert_eq!(from_w, from_s);

        // Two distinct pairings, not one.
        assert_ne!(from_q, from_w);
    }

    /// A round trip through the wire in the other direction: whatever the board said, writing it
    /// back reproduces the board's own frame for that spelling.
    #[test]
    fn parse_then_write_reproduces_each_captured_spelling() {
        for (_, replied) in RELOAD_READS {
            let p = parse_pairing(&replied).unwrap();
            let mut expected = replied;
            expected[0] = RW_WRITE;
            assert_eq!(payload_of(&write_pair(p)), expected);
        }
    }

    #[test]
    fn pairing_refuses_a_key_paired_with_itself() {
        assert_eq!(
            Pairing::new(W, W, Priority::LastInput),
            Err(PairError::SameKey(W))
        );
    }

    #[test]
    fn pairing_refuses_a_winner_outside_the_pair() {
        assert_eq!(
            Pairing::new(W, S, Priority::Wins(Q)),
            Err(PairError::WinnerNotInPair(Q))
        );
    }

    /// The one rendering of a pairing, in both its forms. Pinned here because `wh-device`'s
    /// errors and `wh-cli`'s output both print it and neither can now spell it differently.
    #[test]
    fn describe_names_the_winner_not_the_wire_byte() {
        assert_eq!(
            Pairing::new(W, S, Priority::Wins(S)).unwrap().describe(),
            "w + s, priority: s"
        );
        assert_eq!(
            Pairing::new(S, W, Priority::Wins(S)).unwrap().describe(),
            "s + w, priority: s"
        );
        assert_eq!(
            Pairing::new(Q, E, Priority::LastInput).unwrap().describe(),
            "q + e, priority: last-input"
        );
    }

    #[test]
    fn partner_and_contains_ignore_order() {
        let p = Pairing::new(W, S, Priority::Wins(S)).unwrap();
        assert_eq!(p.partner(W), Some(S));
        assert_eq!(p.partner(S), Some(W));
        assert_eq!(p.partner(Q), None);
        assert!(p.contains(W) && p.contains(S) && !p.contains(Q));
    }

    /// The two priority modes the vendored docs name and the corpus never reached are refused,
    /// loudly and by their own error, rather than silently read as LAST-INPUT.
    #[test]
    fn parse_pairing_refuses_the_unmeasured_priority_modes() {
        for prio in [3u8, 4] {
            let mut payload = RELOAD_READS[1].1;
            payload[8] = prio;
            assert_eq!(
                parse_pairing(&payload),
                Err(DecodeError::SocdPriority(prio)),
                "priority {prio}"
            );
        }
    }

    #[test]
    fn parse_pairing_refuses_a_reply_whose_records_do_not_mirror() {
        let mut payload = RELOAD_READS[1].1;
        payload[4] = 0x07; // the reverse record now names a third key
        assert!(matches!(parse_pairing(&payload), Err(DecodeError::Socd(_))));
    }

    #[test]
    fn parse_pairing_refuses_a_short_reply() {
        assert_eq!(parse_pairing(&[0x00, 0x1a]), Err(DecodeError::Short(2)));
    }

    #[test]
    fn parse_pairing_refuses_a_zero_key() {
        let payload = [0x00, 0x00, 0x00, 0, 0x00, 0x00, 0, 0, 0, 0];
        assert!(matches!(parse_pairing(&payload), Err(DecodeError::Socd(_))));
    }
}
