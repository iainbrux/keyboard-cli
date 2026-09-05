//! SOCD device layer: discovery, the pair write, and the unpair read-modify-write. See
//! `docs/protocol.md` under "SOCD" for the measured evidence this module implements.
//!
//! Two things drive every decision here. The board normalises a `cmd 0x2c` reply per queried
//! key, so nothing compares raw priority bytes; `wh_proto::socd::Pairing` is the model that
//! absorbs that. And participation lives in MODE's advanced nibble as the enum value `8`, so
//! every test of it is `Mode::is_socd`, never a bit test: mode `9` (RS) shares the bit.

use crate::ops;
use crate::session::Session;
use crate::transport::{DeviceError, Transport};
use wh_proto::cmds::{self, layout, KeyRecord, Mode};
use wh_proto::keys::label;
use wh_proto::socd::{self, Pairing};

/// Queries one key's pairing and checks the reply is that key's own row. `Session::roundtrip`
/// matches on the command byte alone, so without this a stale `cmd 0x2c` reply could hand back
/// another key's pairing.
fn query_pairing<T: Transport>(s: &mut Session<T>, usage: u8) -> Result<Pairing, DeviceError> {
    let payload = s.roundtrip(&socd::read_pairing(usage))?;
    let p = socd::parse_pairing(&payload).map_err(|e| DeviceError::Decode(e.to_string()))?;
    if p.keys().0 != usage {
        return Err(DeviceError::SocdInconsistent(format!(
            "expected the SOCD pairing for {}, got a row starting at {}",
            label(usage),
            label(p.keys().0)
        )));
    }
    Ok(p)
}

/// What one SOCD read saw: the board's own key matrix, and the pairings on it.
///
/// The matrix is returned rather than discarded because every caller needs it: `wh socd`'s key
/// arguments are resolved against it, the same assertion every other key-taking surface makes,
/// and reading it twice in one command would be two chances to disagree as well as three wasted
/// roundtrips.
#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    matrix: Vec<u8>,
    pairings: Vec<Pairing>,
}

impl Board {
    /// Every key usage the board reports, in `ops::read_matrix` order. The universe a key
    /// selector resolves against.
    pub fn matrix(&self) -> &[u8] {
        &self.matrix
    }
    /// The pairings, in the order discovery meets each one's first member.
    pub fn pairings(&self) -> &[Pairing] {
        &self.pairings
    }
    /// The pairing `usage` belongs to, if any.
    pub fn pairing_of(&self, usage: u8) -> Option<Pairing> {
        self.pairings.iter().find(|p| p.contains(usage)).copied()
    }
}

/// Every SOCD pairing on the board, in the order discovery meets each pairing's first member,
/// alongside the key matrix the sweep ran over.
///
/// The vendor's own connect sequence is reproduced: sweep MODE over the board's key matrix,
/// take the keys whose advanced nibble reads `8`, and query `cmd 0x2c` for each. Both members
/// are queried, as the vendor does, and their two rows must agree; a flagged key whose partner
/// is not flagged, or whose partner disagrees, is an error naming the keys rather than a silent
/// skip, since either means the board holds a state `wh` cannot render honestly.
pub fn read_socd<T: Transport>(s: &mut Session<T>) -> Result<Board, DeviceError> {
    let matrix = ops::read_matrix(s)?;
    let mut flagged = Vec::new();
    for &u in &matrix {
        let mode = Mode::from_value(ops::read_layout_value(s, u, layout::MODE)?);
        if mode.is_socd() {
            flagged.push(u);
        }
    }
    let mut rows: Vec<(u8, Pairing)> = Vec::with_capacity(flagged.len());
    for &u in &flagged {
        rows.push((u, query_pairing(s, u)?));
    }
    let mut pairings: Vec<Pairing> = Vec::new();
    for &(u, p) in &rows {
        let partner = p
            .partner(u)
            .expect("query_pairing checked the row's own key");
        if !flagged.contains(&partner) {
            return Err(DeviceError::SocdInconsistent(format!(
                "{} is paired with {}, but {}'s mode does not have SOCD set",
                label(u),
                label(partner),
                label(partner)
            )));
        }
        let (_, partner_row) = rows
            .iter()
            .find(|&&(k, _)| k == partner)
            .expect("every flagged key was queried above");
        if *partner_row != p {
            return Err(DeviceError::SocdInconsistent(format!(
                "{} and {} answer with different pairings",
                label(u),
                label(partner)
            )));
        }
        if !pairings.contains(&p) {
            pairings.push(p);
        }
    }
    Ok(Board { matrix, pairings })
}

/// Writes one pairing and verifies it landed.
///
/// No MODE record is sent: the board sets the advanced nibble itself on a pair write, measured,
/// so writing it here would be `wh` claiming credit for something the firmware does. The
/// verification is what proves it: both keys are re-read and must now report `Mode::is_socd`,
/// and both are re-queried, which is where the per-key normalisation is exercised for real,
/// since the partner's row comes back reordered and re-based.
pub fn write_socd_pair<T: Transport>(s: &mut Session<T>, pair: Pairing) -> Result<(), DeviceError> {
    let echo = s.roundtrip(&socd::write_pair(pair))?;
    let echoed = socd::parse_pairing(&echo).map_err(|e| DeviceError::Decode(e.to_string()))?;
    if echoed != pair {
        return Err(DeviceError::SocdInconsistent(format!(
            "the board echoed a different SOCD pairing than the one written: {}",
            echoed.describe()
        )));
    }
    let (a, b) = pair.keys();
    for u in [a, b] {
        let mode = Mode::from_value(ops::read_layout_value(s, u, layout::MODE)?);
        if !mode.is_socd() {
            return Err(DeviceError::SocdInconsistent(format!(
                "{} reports mode {:#06x} after the pair write, whose advanced nibble is {} and \
                 not {} (SOCD)",
                label(u),
                mode.value(),
                mode.advanced,
                cmds::ADVANCED_SOCD
            )));
        }
        let got = query_pairing(s, u)?;
        if got != pair {
            return Err(DeviceError::SocdInconsistent(format!(
                "{} reports the SOCD pairing {} after writing {}",
                label(u),
                got.describe(),
                pair.describe()
            )));
        }
    }
    Ok(())
}

/// The MODE records an unpair writes, and what each key read before it. Fields are private and
/// the only way to build one is `plan_remove`, so a record can never reach the board without the
/// read it was derived from.
#[derive(Debug, Clone, PartialEq)]
pub struct RemovePlan {
    pair: Pairing,
    records: Vec<KeyRecord>,
    before: Vec<(u8, Mode)>,
}

impl RemovePlan {
    pub fn pair(&self) -> Pairing {
        self.pair
    }
    pub fn records(&self) -> &[KeyRecord] {
        &self.records
    }
    /// Each key's pre-write MODE, in the pair's own key order. What an announcement names when
    /// it says which touch mode each key keeps.
    pub fn before(&self) -> &[(u8, Mode)] {
        &self.before
    }
    /// One record per frame, matching the granularity `wh` uses for every other per-key
    /// membership write: a failure between the two lands on a key boundary.
    pub fn frames(&self) -> Vec<[u8; 64]> {
        cmds::write_key_records_singly(&self.records)
    }
}

/// Reads both keys' MODE and builds the records that clear the advanced nibble. Sends only
/// reads, so a caller can dry run.
///
/// The touch nibble is carried forward, not zeroed. Every captured vendor remove was on touch
/// nibble 0, so what the vendor does with a non-zero one is unmeasured; preserving it is `wh`'s
/// own read-modify-write rule applied past the measurement, on the grounds that clearing it
/// would silently detach the key from its own actuation point.
pub fn plan_remove<T: Transport>(
    s: &mut Session<T>,
    pair: Pairing,
) -> Result<RemovePlan, DeviceError> {
    let (a, b) = pair.keys();
    let mut records = Vec::with_capacity(2);
    let mut before = Vec::with_capacity(2);
    for u in [a, b] {
        let mode = Mode::from_value(ops::read_layout_value(s, u, layout::MODE)?);
        before.push((u, mode));
        records.push(KeyRecord {
            key: u,
            layout: layout::MODE,
            value: mode.with_advanced_cleared().value(),
        });
    }
    Ok(RemovePlan {
        pair,
        records,
        before,
    })
}

/// Sends `plan`'s MODE records and verifies both keys read back with the advanced nibble clear
/// and their touch mode untouched.
///
/// No `cmd 0x2c` frame is sent, matching the vendor: a remove clears the flag and nothing else.
/// Whether an orphaned pairing survives on the board is unmeasured, and `docs/protocol.md` says
/// so; the read path cannot see one, since discovery only queries flagged keys.
pub fn remove_socd_pair<T: Transport>(
    s: &mut Session<T>,
    plan: &RemovePlan,
) -> Result<(), DeviceError> {
    s.roundtrip_many(&plan.frames())?;
    for r in plan.records() {
        let got = Mode::from_value(ops::read_layout_value(s, r.key, layout::MODE)?);
        if got.value() != r.value {
            return Err(DeviceError::SocdInconsistent(format!(
                "{} reports mode {:#06x} after the unpair, wanted {:#06x} (SOCD nibble cleared, \
                 touch mode and high byte preserved)",
                label(r.key),
                got.value(),
                r.value
            )));
        }
    }
    Ok(())
}
