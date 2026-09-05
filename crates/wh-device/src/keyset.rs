//! Keyset device layer: membership grouping, allocation, and the read-modify-write plan for
//! creating, valuing, and deleting actuation point and rapid trigger keysets. See
//! `docs/keysets.md` for the measured evidence this module implements.

use crate::ops::{self, KeySettings};
use crate::session::Session;
use crate::transport::{DeviceError, Transport};
use wh_proto::cmds::{self, layout, KeyRecord, Mode, TouchMode};
use wh_proto::value::Um;

/// Which of the two independent groupings a keyset belongs to. Lives in `wh_proto::cmds` as
/// `KeysetKind`, since it does no I/O; re-exported here under the short name callers use.
pub use wh_proto::cmds::KeysetKind as Kind;

/// One keyset: the index its members hold, and the keys holding it, in the order given.
#[derive(Debug, Clone, PartialEq)]
pub struct Keyset {
    pub index: u16,
    pub members: Vec<u8>,
}

/// Every key's membership for one layout, and which layout that is. The two layouts have
/// separate counters (`docs/keysets.md`); fields are private and the only way to build one is
/// `read_membership`, so a caller can never pair one layout's entries with the other's kind.
#[derive(Debug, Clone, PartialEq)]
pub struct Membership {
    kind: Kind,
    entries: Vec<(u8, u16)>,
}

impl Membership {
    /// Which layout `entries` was read from.
    pub fn kind(&self) -> Kind {
        self.kind
    }
    /// Each key's raw membership value for `kind`'s layout, in the order `read_membership` read
    /// them.
    pub fn entries(&self) -> &[(u8, u16)] {
        &self.entries
    }
}

/// Reads the board's own key matrix, then one layout, `0xFF` or `0xFE` depending on `kind`, for
/// every key it reports. Whole-board by construction: a caller cannot pass a partial view, which
/// would let `next_index` allocate an index a key outside that view already holds.
pub fn read_membership<T: Transport>(
    s: &mut Session<T>,
    kind: Kind,
) -> Result<Membership, DeviceError> {
    let usages = ops::read_matrix(s)?;
    let mut entries = Vec::with_capacity(usages.len());
    for u in usages {
        let v = ops::read_layout_value(s, u, kind.layout())?;
        entries.push((u, v));
    }
    Ok(Membership { kind, entries })
}

/// The keysets present, ascending by index. Membership `0` means "in no keyset" and is excluded.
pub fn group(m: &Membership) -> Vec<Keyset> {
    let mut keysets: Vec<Keyset> = Vec::new();
    for &(usage, index) in &m.entries {
        if index == 0 {
            continue;
        }
        match keysets.iter_mut().find(|k| k.index == index) {
            Some(ks) => ks.members.push(usage),
            None => keysets.push(Keyset {
                index,
                members: vec![usage],
            }),
        }
    }
    keysets.sort_by_key(|k| k.index);
    keysets
}

/// A keyset index and the layout it was allocated from. Fields are private: the only ways to
/// get one are `next_index`, `KeysetIndex::clear`, and `KeysetIndex::restoring`, so an index can
/// never be relabelled to the other layout after the fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeysetIndex {
    kind: Kind,
    value: u16,
}

impl KeysetIndex {
    /// The value that clears membership (`0`) for `kind`, named so a caller doesn't have to
    /// spell out the zero itself.
    pub fn clear(kind: Kind) -> Self {
        KeysetIndex { kind, value: 0 }
    }
    /// An index taken from a recorded snapshot rather than allocated. `wh restore` needs this:
    /// a snapshot's indices can include gaps allocation never reuses (`docs/keysets.md`), so
    /// `next_index` can never reproduce one. Every other caller wants `next_index`.
    pub fn restoring(kind: Kind, value: u16) -> Self {
        KeysetIndex { kind, value }
    }
    /// The layout this index was allocated from, or cleared for.
    pub fn kind(&self) -> Kind {
        self.kind
    }
    /// The raw index value.
    pub fn value(&self) -> u16 {
        self.value
    }
}

/// Membership records for keys that each carry their own index, which `plan` cannot express: it
/// writes one index to every key it is given. `wh restore` is the only caller, since a snapshot's
/// indices can include gaps allocation never reuses. Errors if the entries mix kinds, so one
/// layout's index can never be written to the other's layout.
///
/// Named for its one caller, not `membership_records` like `WritePlan`'s own method: the two
/// return the same shape from different contracts (this one builds fresh records from raw
/// indices; `WritePlan::membership_records` returns ones `plan` already built), and a shared name
/// on two reachable things with different contracts is a trap for whichever gets called by
/// mistake.
pub fn membership_records_for_restore(
    entries: &[(u8, KeysetIndex)],
) -> Result<Vec<KeyRecord>, DeviceError> {
    let Some((_, first)) = entries.first() else {
        return Ok(Vec::new());
    };
    let kind = first.kind;
    for (_, idx) in entries {
        if idx.kind != kind {
            return Err(DeviceError::KeysetKindMismatch {
                expected: kind,
                found: idx.kind,
            });
        }
    }
    Ok(entries
        .iter()
        .map(|&(key, idx)| KeyRecord {
            key,
            layout: kind.layout(),
            value: idx.value,
        })
        .collect())
}

/// The next index to allocate: the highest live membership value plus one, or `1` when no key
/// holds any, carrying `m`'s own kind forward. `u16::MAX` is a valid *output* (reached when the
/// highest live value is `u16::MAX - 1`); only allocating *past* it, when `u16::MAX` is already
/// live, errors instead of wrapping to `0`.
pub fn next_index(m: &Membership) -> Result<KeysetIndex, DeviceError> {
    let max = m.entries.iter().map(|&(_, v)| v).filter(|&v| v != 0).max();
    let value = match max {
        None => 1,
        Some(u16::MAX) => return Err(DeviceError::KeysetIndexExhausted),
        Some(v) => v + 1,
    };
    Ok(KeysetIndex {
        kind: m.kind,
        value,
    })
}

/// What an operation does to a key's touch nibble. Internal representation only: a `Change` is
/// built through its own constructors, which pick the right variant for the kind of operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TouchChange {
    /// Leave it exactly as read.
    Keep,
    /// `Global` (0) becomes `Single` (1); every other nibble is left alone.
    PromoteGlobalToSingle,
    /// `Rt` (3), except a key already `RtContinuous` (4) stays `RtContinuous`.
    RapidTrigger,
    /// Turns rapid trigger off: `RtGlobal` (2), `Rt` (3) and `RtContinuous` (4) become `Single`.
    /// Every other nibble, including `Global` and any `Unknown`, is left exactly as read. This
    /// agrees with `ops::rt_off_records` on the nibble mapping and, since `plan` never emits an
    /// unchanged nibble-0 MODE record, on never sending one either.
    Off,
}

fn apply_touch(current: TouchMode, change: TouchChange) -> TouchMode {
    match change {
        TouchChange::Keep => current,
        TouchChange::PromoteGlobalToSingle => match current {
            TouchMode::Global => TouchMode::Single,
            other => other,
        },
        TouchChange::RapidTrigger => match current {
            TouchMode::RtContinuous => TouchMode::RtContinuous,
            _ => TouchMode::Rt,
        },
        TouchChange::Off => match current {
            TouchMode::RtGlobal | TouchMode::Rt | TouchMode::RtContinuous => TouchMode::Single,
            other => other,
        },
    }
}

/// One operation's targets, and which kind of keyset it belongs to. Fields are private: the only
/// way to build one is through the constructors below, so a rapid trigger change can never be
/// paired with `Kind::Ap` (or the reverse), which would otherwise compile and silently write the
/// wrong layout for membership.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    kind: Kind,
    touch: TouchChange,
    ap: Option<Um>,
    rt: Option<(Um, Um)>,
}

impl Change {
    /// An actuation point operation: set every member's `0x04` to `value`, promoting a `Global`
    /// key to `Single` first. The promotion itself is measured (`ks-value-ap`, key `x`: MODE
    /// `0x0000` to `0x0010`); whether it depends on keyset membership specifically is not, so
    /// it is applied unconditionally here, the non-destructive default. `ap_keeping_touch` has no
    /// production caller today and is the measured road not taken, kept for an operation that must
    /// not move a key off global travel rather than offered as an alternative to this one.
    pub fn ap(value: Um) -> Self {
        Change {
            kind: Kind::Ap,
            touch: TouchChange::PromoteGlobalToSingle,
            ap: Some(value),
            rt: None,
        }
    }

    /// An actuation point operation that does not promote: a `Global` key stays `Global`. No
    /// MODE record is sent for it either way: `plan` never writes touch nibble 0, and this
    /// constructor's whole point is not to move a key off it.
    pub fn ap_keeping_touch(value: Um) -> Self {
        Change {
            kind: Kind::Ap,
            touch: TouchChange::Keep,
            ap: Some(value),
            rt: None,
        }
    }

    /// A rapid trigger operation: turn it on at `press`/`release`. Touch becomes `Rt`, unless
    /// the key is already `RtContinuous`, which is preserved. `wh set rt --set` still reaches
    /// the board through `ops::rt_records` instead of this constructor, the one intent/route
    /// split among three (see `ap`/`rt_off`) not yet closed in `plan`'s favour.
    pub fn rt_on(press: Um, release: Um) -> Self {
        Change {
            kind: Kind::Rt,
            touch: TouchChange::RapidTrigger,
            ap: None,
            rt: Some((press, release)),
        }
    }

    /// A rapid trigger operation: turn it off, resetting the sensitivities to `press`/`release`
    /// (the board's global, per the measured delete template).
    pub fn rt_off(press: Um, release: Um) -> Self {
        Change {
            kind: Kind::Rt,
            touch: TouchChange::Off,
            ap: None,
            rt: Some((press, release)),
        }
    }

    /// Membership only, changing no value. Used by a create over keys already at the target.
    pub fn membership_only(kind: Kind) -> Self {
        Change {
            kind,
            touch: TouchChange::Keep,
            ap: None,
            rt: None,
        }
    }

    /// Which kind of keyset this operation belongs to, so a caller can check it against a
    /// `KeysetIndex` before pairing them, the way `plan` itself does.
    pub fn kind(&self) -> Kind {
        self.kind
    }
}

/// The records one operation writes, plus what was on the board before it. Fields are private,
/// constructed only by `plan`: a hand-built `WritePlan` would let `apply` send a forged record
/// none of `plan`'s own checks ever saw, which is exactly the read-modify-write invariant this
/// module exists to protect.
#[derive(Debug, Clone, PartialEq)]
pub struct WritePlan {
    value_records: Vec<KeyRecord>,
    membership_records: Vec<KeyRecord>,
    before: Vec<KeySettings>,
}

impl WritePlan {
    /// Value records, flat and unpacked: packing into per-key groups happens in `frames()`,
    /// where a batch of exactly 14 is reachable.
    pub fn value_records(&self) -> &[KeyRecord] {
        &self.value_records
    }
    /// Membership records, written one per frame.
    pub fn membership_records(&self) -> &[KeyRecord] {
        &self.membership_records
    }
    /// Each key's settings as read before the write, in `usages` order, so a caller can verify
    /// every key including ones that got no records.
    pub fn before(&self) -> &[KeySettings] {
        &self.before
    }

    /// Frames in send order: the value batches, then one frame per membership record.
    ///
    /// Value records are packed key by key, taking whole groups until the next would not fit.
    /// This never splits a group in practice because `plan` is only ever called with
    /// deduplicated `usages` (`Selector::resolve`, `read_matrix`), keeping each key's own group
    /// at 4 records (3 at the nibble-0 omission); `plan` itself has no dedup, so a caller passing
    /// a repeated usage gets a larger group, and this function does split it.
    ///
    /// Packs whole per-key groups up to 14 records, rather than the vendor's own layout-major
    /// batching: a further measured divergence, since a MODE-only write frame there caps at two
    /// records (300 in the corpus, 275 carry two and 25 carry one, none more). For a deduplicated
    /// selection, this strictly improves partial-failure behaviour: a failure between frames
    /// lands on a key boundary rather than inside one key's own records.
    pub fn frames(&self) -> Vec<[u8; 64]> {
        let mut frames = Vec::new();
        let mut batch: Vec<KeyRecord> = Vec::new();
        let mut i = 0;
        while i < self.value_records.len() {
            let key = self.value_records[i].key;
            let mut j = i + 1;
            while j < self.value_records.len() && self.value_records[j].key == key {
                j += 1;
            }
            let group = &self.value_records[i..j];
            if !batch.is_empty() && batch.len() + group.len() > cmds::MAX_RECORDS_PER_REPORT {
                frames.extend(cmds::write_key_records(&batch));
                batch.clear();
            }
            batch.extend_from_slice(group);
            i = j;
        }
        if !batch.is_empty() {
            frames.extend(cmds::write_key_records(&batch));
        }
        frames.extend(cmds::write_key_records_singly(&self.membership_records));
        frames
    }
    /// True when nothing at all would be sent.
    pub fn is_empty(&self) -> bool {
        self.value_records.is_empty() && self.membership_records.is_empty()
    }
}

/// Reads every key in `usages`, applies `change`, and builds the plan. Sends only reads, so a
/// caller can dry run. `membership` is `Some(index)` to write that index to every key in
/// `usages` (`KeysetIndex::clear` clears); `None` leaves membership untouched. Errors, before
/// sending anything, if `membership`'s kind doesn't match `change`'s: an index allocated from
/// one counter must never be written to the other layout.
///
/// Reads all six of `read_key_settings`'s layouts per key, though only four feed
/// `value_records`: the other two land in `before`'s `ap_keyset`/`rt_keyset`, so a caller can
/// verify membership as well as values afterwards. `plan` runs over the selected keys rather
/// than the whole board, so this is two extra reads per selected key, not per key on the board.
///
/// Per key, MODE, AP, RT_PRESS and RT_RELEASE are read, the target computed, and either all four
/// are written or none, matching the vendor's own all-or-nothing template, except MODE itself is
/// dropped from that four when it would only echo an unchanged touch nibble 0 back: nibble 0
/// means "follow global travel", so writing it unchanged would be a semantic change, not an echo.
///
/// Deliberate divergences from the vendor: layouts `0x16`/`0x17` are never written, on the
/// grounds that `wh` never reads them and a constant would be an invented value; records are
/// emitted key-major rather than the vendor's layout-major order, the same divergence
/// `ops::ap_records` documents, so a mid-batch failure stops at a few keys rather than every key
/// selected; `frames()` packs whole per-key groups rather than the vendor's own layout-major
/// batching, so for a deduplicated selection a failure lands on a key boundary rather than
/// inside one key's own records; and the vendor writes MODE twice per key per operation (write
/// template steps 1 and 3), where this writes it once.
pub fn plan<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
    change: &Change,
    membership: Option<KeysetIndex>,
) -> Result<WritePlan, DeviceError> {
    if let Some(idx) = membership {
        if idx.kind != change.kind {
            return Err(DeviceError::KeysetKindMismatch {
                expected: change.kind,
                found: idx.kind,
            });
        }
    }

    let mut value_records = Vec::new();
    let mut before = Vec::with_capacity(usages.len());

    for &u in usages {
        let settings = ops::read_key_settings(s, u)?;

        let cur_mode = settings.mode;
        let cur_mode_value = cur_mode.value();
        let new_touch = apply_touch(cur_mode.touch, change.touch);
        let new_mode = Mode {
            touch: new_touch,
            advanced: cur_mode.advanced,
            high: cur_mode.high,
        };
        let new_mode_value = new_mode.value();

        let target_ap = change.ap.unwrap_or(settings.ap);
        let (target_press, target_release) = change
            .rt
            .unwrap_or((settings.rt_press, settings.rt_release));

        if new_mode_value != cur_mode_value
            || target_ap != settings.ap
            || target_press != settings.rt_press
            || target_release != settings.rt_release
        {
            // Nibble 0 means "follow global travel": writing it unchanged would be a semantic
            // change, not an echo, which is why `ops::rt_off_records` refuses it too. Measured:
            // 1150 MODE write records across the corpus, none at nibble 0.
            let unchanged_at_global =
                new_mode_value == cur_mode_value && new_touch == TouchMode::Global;
            if !unchanged_at_global {
                value_records.push(KeyRecord {
                    key: u,
                    layout: layout::MODE,
                    value: new_mode_value,
                });
            }
            value_records.push(KeyRecord {
                key: u,
                layout: layout::AP,
                value: target_ap.0,
            });
            value_records.push(KeyRecord {
                key: u,
                layout: layout::RT_PRESS,
                value: target_press.0,
            });
            value_records.push(KeyRecord {
                key: u,
                layout: layout::RT_RELEASE,
                value: target_release.0,
            });
        }

        before.push(settings);
    }

    // Every key in `usages` gets a membership record, even one already at `index`: whether the
    // vendor skips such a key is unmeasured, and an unconditional rewrite is non-destructive.
    let membership_records = match membership {
        Some(idx) => usages
            .iter()
            .map(|&u| KeyRecord {
                key: u,
                layout: idx.kind.layout(),
                value: idx.value,
            })
            .collect(),
        None => Vec::new(),
    };

    Ok(WritePlan {
        value_records,
        membership_records,
        before,
    })
}

/// Sends `plan.frames()`. No-op when the plan is empty.
pub fn apply<T: Transport>(s: &mut Session<T>, plan: &WritePlan) -> Result<(), DeviceError> {
    if plan.is_empty() {
        return Ok(());
    }
    s.roundtrip_many(&plan.frames())?;
    Ok(())
}

/// A value read from the keys that are in no keyset, and how well they agree.
#[derive(Debug, Clone, PartialEq)]
pub enum Global<T> {
    /// Every key outside a keyset holds the same value.
    Agreed(T),
    /// They do not. Each distinct value with the number of keys holding it, descending by count,
    /// so a caller can name the odd ones out; `wh-device` does not pick a winner itself.
    Split(Vec<(T, usize)>),
    /// No key is outside a keyset, so nothing can report it. For `global_ap_excluding` and
    /// `global_rt_excluding` this variant carries a second, distinct cause: a free key can exist
    /// while every one of them is in `exclude`, which reads identically to none existing at all.
    /// A caller building an error from this variant must check `entries()` for a key holding
    /// membership `0` before naming a cause. Assume the first cause and you report "no key is
    /// outside a keyset" on a board where every key is outside one.
    NoneOutsideAKeyset,
}

/// Counts occurrences of each distinct value, preserving first-seen order among ties, and
/// classifies the result: empty, one distinct value, or several.
fn summarize<T: PartialEq>(values: Vec<T>) -> Global<T> {
    if values.is_empty() {
        return Global::NoneOutsideAKeyset;
    }
    let mut counts: Vec<(T, usize)> = Vec::new();
    for v in values {
        match counts.iter_mut().find(|(existing, _)| *existing == v) {
            Some((_, c)) => *c += 1,
            None => counts.push((v, 1)),
        }
    }
    if counts.len() == 1 {
        let (v, _) = counts.into_iter().next().expect("just checked len == 1");
        return Global::Agreed(v);
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    Global::Split(counts)
}

/// The board's actuation point outside any keyset: layout `0x04` read from every key `m` holds
/// no membership for. Errors if `m` isn't `Kind::Ap` membership.
///
/// The vendor's own method for finding this is unmeasured: in `ks-value-ap` it reads `0x04` from
/// every key across five 14-record frames, not five keys singled out (`0x29`, `0xfa`, `0x31`,
/// `0x28`, `0x52` are each frame's first key); one read key was in a keyset and disagreed, outcome
/// undetermined. 5 of the 39 captures read no `0x04` at all. Reading only unkeyset keys is
/// narrower than that whole-board read, not an alternative to a heuristic that does not exist.
pub fn global_ap<T: Transport>(
    s: &mut Session<T>,
    m: &Membership,
) -> Result<Global<Um>, DeviceError> {
    global_ap_excluding(s, m, &[])
}

/// The board's base actuation point, read from `0x04` of every key holding no actuation point
/// membership, ignoring any key in `exclude`. Callers resetting keys to the base pass those keys
/// here: they are frequently the reason the remaining keys disagree.
///
/// `NoneOutsideAKeyset` is ambiguous here in a way it is not for `global_ap`: it can mean no key on
/// the board is free, or it can mean free keys exist but every one of them is in `exclude`. A
/// caller reporting this case must decide which happened, from `m.entries()` if it matters, rather
/// than reusing wording that assumes the first.
pub fn global_ap_excluding<T: Transport>(
    s: &mut Session<T>,
    m: &Membership,
    exclude: &[u8],
) -> Result<Global<Um>, DeviceError> {
    if m.kind != Kind::Ap {
        return Err(DeviceError::KeysetKindMismatch {
            expected: Kind::Ap,
            found: m.kind,
        });
    }
    let mut values = Vec::new();
    for &(usage, membership) in &m.entries {
        if membership != 0 || exclude.contains(&usage) {
            continue;
        }
        values.push(Um(ops::read_layout_value(s, usage, layout::AP)?));
    }
    Ok(summarize(values))
}

/// The global rapid trigger sensitivity, read the same way from `0x14`/`0x15` of every key `m`
/// holds no rapid trigger membership for. Errors if `m` isn't `Kind::Rt` membership. See
/// `global_ap` for the same honest limit: the vendor's own method is unmeasured.
pub fn global_rt<T: Transport>(
    s: &mut Session<T>,
    m: &Membership,
) -> Result<Global<(Um, Um)>, DeviceError> {
    global_rt_excluding(s, m, &[])
}

/// The board's base rapid trigger sensitivity, read from `0x14`/`0x15` of every key holding no
/// rapid trigger membership, ignoring any key in `exclude`. See `global_ap_excluding` for why a
/// caller resetting keys to the base needs this, and for the same ambiguity in
/// `NoneOutsideAKeyset`: no free key at all, or every free key excluded, read identically here too.
pub fn global_rt_excluding<T: Transport>(
    s: &mut Session<T>,
    m: &Membership,
    exclude: &[u8],
) -> Result<Global<(Um, Um)>, DeviceError> {
    if m.kind != Kind::Rt {
        return Err(DeviceError::KeysetKindMismatch {
            expected: Kind::Rt,
            found: m.kind,
        });
    }
    let mut values = Vec::new();
    for &(usage, membership) in &m.entries {
        if membership != 0 || exclude.contains(&usage) {
            continue;
        }
        let press = ops::read_layout_value(s, usage, layout::RT_PRESS)?;
        let release = ops::read_layout_value(s, usage, layout::RT_RELEASE)?;
        values.push((Um(press), Um(release)));
    }
    Ok(summarize(values))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::{hex, ReplayTransport};
    use wh_proto::cmds;

    fn l(dir: &str, b: &[u8; 64]) -> String {
        format!("{{\"dir\":\"{dir}\",\"hex\":\"{}\"}}", hex(b))
    }
    /// Builds a reply frame with the high bit set on the command byte, matching how the real
    /// device sends it (`wh_proto::frame::REPLY_BIT`).
    fn rf(cmd: u8, payload: &[u8]) -> [u8; 64] {
        wh_proto::frame::frame(cmd | wh_proto::frame::REPLY_BIT, payload).unwrap()
    }

    fn read_reply(usage: u8, lid: u8, val: u16) -> Vec<String> {
        vec![
            l("out", &cmds::read_key_layout(usage, lid)),
            l(
                "in",
                &rf(
                    cmds::cmd::KEY,
                    &[0x00, usage, lid, (val & 0xFF) as u8, (val >> 8) as u8],
                ),
            ),
        ]
    }

    /// One key's full `read_key_settings` script, in the order it issues reads: AP, MODE,
    /// RT_PRESS, RT_RELEASE, KEYSET_AP, KEYSET_RT.
    #[allow(clippy::too_many_arguments)]
    fn settings_script(
        usage: u8,
        ap: u16,
        mode: u16,
        rt_press: u16,
        rt_release: u16,
        ap_keyset: u16,
        rt_keyset: u16,
    ) -> Vec<String> {
        let mut lines = Vec::new();
        for (lid, val) in [
            (layout::AP, ap),
            (layout::MODE, mode),
            (layout::RT_PRESS, rt_press),
            (layout::RT_RELEASE, rt_release),
            (layout::KEYSET_AP, ap_keyset),
            (layout::KEYSET_RT, rt_keyset),
        ] {
            lines.extend(read_reply(usage, lid, val));
        }
        lines
    }

    /// The `ops::read_matrix` script for up to six usages, one per row-pair column, in the
    /// order `read_matrix` reports them. Fewer than six leaves the remaining columns empty.
    fn matrix_lines(usages: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        for (i, &(a, b)) in [(0u8, 1u8), (2u8, 3u8), (4u8, 5u8)].iter().enumerate() {
            let req = cmds::read_defkey_rows(a, b);
            let mut payload = vec![0u8; 45];
            payload[1] = a;
            if let Some(&u) = usages.get(i * 2) {
                payload[2] = u;
            }
            payload[23] = b;
            if let Some(&u) = usages.get(i * 2 + 1) {
                payload[24] = u;
            }
            lines.push(l("out", &req));
            lines.push(l("in", &rf(cmds::cmd::DEFKEY, &payload)));
        }
        lines
    }

    fn membership(kind: Kind, entries: &[(u8, u16)]) -> Membership {
        Membership {
            kind,
            entries: entries.to_vec(),
        }
    }

    // -- next_index --

    #[test]
    fn next_index_with_no_members_is_one() {
        let m = membership(Kind::Ap, &[(0x04, 0), (0x05, 0)]);
        assert_eq!(
            next_index(&m).unwrap(),
            KeysetIndex {
                kind: Kind::Ap,
                value: 1
            }
        );
    }

    #[test]
    fn next_index_is_max_plus_one() {
        let m = membership(Kind::Ap, &[(0x04, 1), (0x05, 2)]);
        assert_eq!(
            next_index(&m).unwrap(),
            KeysetIndex {
                kind: Kind::Ap,
                value: 3
            }
        );
    }

    #[test]
    fn next_index_skips_a_freed_gap_rather_than_filling_it() {
        let m = membership(Kind::Ap, &[(0x04, 1), (0x05, 2), (0x06, 4)]);
        assert_eq!(
            next_index(&m).unwrap(),
            KeysetIndex {
                kind: Kind::Ap,
                value: 5
            }
        );
    }

    #[test]
    fn next_index_reaches_u16_max_as_a_valid_output() {
        let m = membership(Kind::Ap, &[(0x04, u16::MAX - 1)]);
        assert_eq!(
            next_index(&m).unwrap(),
            KeysetIndex {
                kind: Kind::Ap,
                value: u16::MAX
            }
        );
    }

    /// `next_index` must carry the `Membership`'s own kind forward, not default to `Kind::Ap`:
    /// every other test in this block uses a `Kind::Ap` membership, so none of them could catch
    /// a `KeysetIndex` that always reported `Ap` regardless of what was asked.
    #[test]
    fn next_index_carries_the_memberships_own_kind() {
        let m = membership(Kind::Rt, &[(0x04, 1)]);
        assert_eq!(
            next_index(&m).unwrap(),
            KeysetIndex {
                kind: Kind::Rt,
                value: 2
            }
        );
    }

    #[test]
    fn next_index_errors_when_u16_max_is_already_live() {
        let m = membership(Kind::Ap, &[(0x04, u16::MAX)]);
        let err = next_index(&m).unwrap_err();
        assert!(
            matches!(err, DeviceError::KeysetIndexExhausted),
            "got {err:?}"
        );
    }

    // -- group --

    #[test]
    fn group_is_ascending_by_index_and_excludes_zero() {
        let m = membership(
            Kind::Ap,
            &[(0x04, 2), (0x05, 0), (0x06, 1), (0x07, 2), (0x08, 1)],
        );
        let got = group(&m);
        assert_eq!(
            got,
            vec![
                Keyset {
                    index: 1,
                    members: vec![0x06, 0x08]
                },
                Keyset {
                    index: 2,
                    members: vec![0x04, 0x07]
                },
            ]
        );
    }

    #[test]
    fn group_preserves_member_order_within_a_keyset() {
        let m = membership(Kind::Ap, &[(0x10, 3), (0x05, 3), (0x08, 3)]);
        let got = group(&m);
        assert_eq!(got[0].members, vec![0x10, 0x05, 0x08]);
    }

    // -- read_membership --

    #[test]
    fn read_membership_reads_the_rt_layout_for_kind_rt() {
        let mut lines = matrix_lines(&[0x04, 0x05]);
        lines.extend(read_reply(0x04, layout::KEYSET_RT, 1));
        lines.extend(read_reply(0x05, layout::KEYSET_RT, 0));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let got = read_membership(&mut s, Kind::Rt).unwrap();
        assert_eq!(got.kind, Kind::Rt);
        assert_eq!(got.entries, vec![(0x04, 1), (0x05, 0)]);
        assert!(s.into_inner().finished());
    }

    /// `ReplayTransport` matches the outgoing frame byte for byte, so this fails if
    /// `read_membership` asked for any layout byte but `0xFF`, the actuation point one.
    #[test]
    fn read_membership_reads_the_ap_layout_for_kind_ap() {
        let mut lines = matrix_lines(&[0x04, 0x05]);
        lines.extend(read_reply(0x04, layout::KEYSET_AP, 3));
        lines.extend(read_reply(0x05, layout::KEYSET_AP, 0));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let got = read_membership(&mut s, Kind::Ap).unwrap();
        assert_eq!(got.kind, Kind::Ap);
        assert_eq!(got.entries, vec![(0x04, 3), (0x05, 0)]);
        assert!(s.into_inner().finished());
    }

    /// Proves the matrix is actually read, not assumed: these usages appear in no other test in
    /// this file, so a `read_membership` that assumed a hard-coded key list would send the wrong
    /// requests and `ReplayTransport` would reject them.
    #[test]
    fn read_membership_covers_whatever_the_live_matrix_reports() {
        let usages = [0x50u8, 0x51, 0x52, 0x53, 0x54, 0x55];
        let mut lines = matrix_lines(&usages);
        for &u in &usages {
            lines.extend(read_reply(u, layout::KEYSET_AP, 0));
        }
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let got = read_membership(&mut s, Kind::Ap).unwrap();
        assert_eq!(
            got.entries.iter().map(|&(u, _)| u).collect::<Vec<_>>(),
            usages.to_vec()
        );
        assert!(s.into_inner().finished());
    }

    // -- global_ap / global_rt --

    #[test]
    fn global_ap_reports_none_outside_a_keyset_when_every_key_is_in_one() {
        let m = membership(Kind::Ap, &[(0x04, 1), (0x05, 2)]);
        let mut s = Session::new(ReplayTransport::from_jsonl("").unwrap());
        assert_eq!(global_ap(&mut s, &m).unwrap(), Global::NoneOutsideAKeyset);
        assert!(s.into_inner().finished());
    }

    #[test]
    fn global_rt_reports_none_outside_a_keyset_when_every_key_is_in_one() {
        let m = membership(Kind::Rt, &[(0x04, 1), (0x05, 1)]);
        let mut s = Session::new(ReplayTransport::from_jsonl("").unwrap());
        assert_eq!(global_rt(&mut s, &m).unwrap(), Global::NoneOutsideAKeyset);
        assert!(s.into_inner().finished());
    }

    #[test]
    fn global_ap_agrees_when_every_unkeyset_key_reads_the_same_value() {
        let m = membership(Kind::Ap, &[(0x04, 1), (0x05, 0), (0x06, 0)]);
        let mut lines = read_reply(0x05, layout::AP, 2000);
        lines.extend(read_reply(0x06, layout::AP, 2000));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        assert_eq!(global_ap(&mut s, &m).unwrap(), Global::Agreed(Um(2000)));
        assert!(s.into_inner().finished());
    }

    /// `global_ap` reading only the first unkeyset key would report `W`'s own private travel as
    /// though it were the board's global, after an ordinary `wh set ap --keys w --set 1.0` gave
    /// `W` a value no other key shares.
    #[test]
    fn global_ap_splits_when_unkeyset_keys_disagree() {
        let m = membership(Kind::Ap, &[(0x04, 0), (0x05, 0), (0x06, 0)]);
        let mut lines = read_reply(0x04, layout::AP, 1000); // W, changed alone
        lines.extend(read_reply(0x05, layout::AP, 2000));
        lines.extend(read_reply(0x06, layout::AP, 2000));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        assert_eq!(
            global_ap(&mut s, &m).unwrap(),
            Global::Split(vec![(Um(2000), 2), (Um(1000), 1)])
        );
        assert!(s.into_inner().finished());
    }

    /// Reads `0x04` from every key in a rapid trigger `Membership` would be a bug wearing a
    /// working test: rejected before any frame is sent, not just before the count is trusted.
    #[test]
    fn global_ap_rejects_a_rapid_trigger_membership() {
        let m = membership(Kind::Rt, &[(0x04, 1), (0x05, 0)]);
        let mut s = Session::new(ReplayTransport::from_jsonl("").unwrap());
        let err = global_ap(&mut s, &m).unwrap_err();
        assert!(
            matches!(
                err,
                DeviceError::KeysetKindMismatch {
                    expected: Kind::Ap,
                    found: Kind::Rt
                }
            ),
            "got {err:?}"
        );
        assert!(s.into_inner().finished());
    }

    #[test]
    fn global_rt_agrees_when_every_unkeyset_key_reads_the_same_value() {
        // Two keys outside a keyset, not one: with only one, reading just the first is
        // indistinguishable from reading all of them, which is the coverage gap this replaces.
        let m = membership(Kind::Rt, &[(0x04, 1), (0x05, 0), (0x06, 0)]);
        let mut lines = read_reply(0x05, layout::RT_PRESS, 100);
        lines.extend(read_reply(0x05, layout::RT_RELEASE, 150));
        lines.extend(read_reply(0x06, layout::RT_PRESS, 100));
        lines.extend(read_reply(0x06, layout::RT_RELEASE, 150));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        assert_eq!(
            global_rt(&mut s, &m).unwrap(),
            Global::Agreed((Um(100), Um(150)))
        );
        assert!(s.into_inner().finished());
    }

    /// The `global_ap` split regression, mirrored for rapid trigger sensitivity: reading only
    /// the first unkeyset key would report one key's own sensitivity as though every unkeyset
    /// key agreed.
    #[test]
    fn global_rt_splits_when_unkeyset_keys_disagree() {
        let m = membership(Kind::Rt, &[(0x04, 0), (0x05, 0), (0x06, 0)]);
        let mut lines = read_reply(0x04, layout::RT_PRESS, 50);
        lines.extend(read_reply(0x04, layout::RT_RELEASE, 60));
        lines.extend(read_reply(0x05, layout::RT_PRESS, 100));
        lines.extend(read_reply(0x05, layout::RT_RELEASE, 150));
        lines.extend(read_reply(0x06, layout::RT_PRESS, 100));
        lines.extend(read_reply(0x06, layout::RT_RELEASE, 150));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        assert_eq!(
            global_rt(&mut s, &m).unwrap(),
            Global::Split(vec![((Um(100), Um(150)), 2), ((Um(50), Um(60)), 1)])
        );
        assert!(s.into_inner().finished());
    }

    /// The `global_ap` rejection, mirrored: an actuation point `Membership` into `global_rt`.
    #[test]
    fn global_rt_rejects_an_actuation_point_membership() {
        let m = membership(Kind::Ap, &[(0x04, 1), (0x05, 0)]);
        let mut s = Session::new(ReplayTransport::from_jsonl("").unwrap());
        let err = global_rt(&mut s, &m).unwrap_err();
        assert!(
            matches!(
                err,
                DeviceError::KeysetKindMismatch {
                    expected: Kind::Rt,
                    found: Kind::Ap
                }
            ),
            "got {err:?}"
        );
        assert!(s.into_inner().finished());
    }

    // -- global_ap_excluding / global_rt_excluding --

    /// `w` is the odd one out at 1100, three other free keys read 2000. Excluding `w` must
    /// agree; the same board with an empty exclusion (asserted in the same test) must split,
    /// which is what proves the parameter is doing the work rather than the fixture agreeing
    /// on its own.
    #[test]
    fn global_ap_excluding_ignores_the_named_keys() {
        let m = membership(Kind::Ap, &[(0x04, 0), (0x05, 0), (0x06, 0), (0x07, 0)]);

        let mut excluded_lines = read_reply(0x05, layout::AP, 2000);
        excluded_lines.extend(read_reply(0x06, layout::AP, 2000));
        excluded_lines.extend(read_reply(0x07, layout::AP, 2000));
        let mut s = Session::new(ReplayTransport::from_jsonl(&excluded_lines.join("\n")).unwrap());
        assert_eq!(
            global_ap_excluding(&mut s, &m, &[0x04]).unwrap(),
            Global::Agreed(Um(2000))
        );
        assert!(s.into_inner().finished());

        let mut all_lines = read_reply(0x04, layout::AP, 1100);
        all_lines.extend(read_reply(0x05, layout::AP, 2000));
        all_lines.extend(read_reply(0x06, layout::AP, 2000));
        all_lines.extend(read_reply(0x07, layout::AP, 2000));
        let mut s2 = Session::new(ReplayTransport::from_jsonl(&all_lines.join("\n")).unwrap());
        assert_eq!(
            global_ap_excluding(&mut s2, &m, &[]).unwrap(),
            Global::Split(vec![(Um(2000), 3), (Um(1100), 1)])
        );
        assert!(s2.into_inner().finished());
    }

    #[test]
    fn global_ap_excluding_still_splits_when_the_rest_disagree() {
        let m = membership(Kind::Ap, &[(0x04, 0), (0x05, 0), (0x06, 0), (0x07, 0)]);
        let mut lines = read_reply(0x04, layout::AP, 2000);
        lines.extend(read_reply(0x05, layout::AP, 2000));
        lines.extend(read_reply(0x06, layout::AP, 1500));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        assert_eq!(
            global_ap_excluding(&mut s, &m, &[0x07]).unwrap(),
            Global::Split(vec![(Um(2000), 2), (Um(1500), 1)])
        );
        assert!(s.into_inner().finished());
    }

    #[test]
    fn global_ap_excluding_reports_none_when_every_free_key_is_excluded() {
        let m = membership(Kind::Ap, &[(0x04, 0), (0x05, 0)]);
        let mut s = Session::new(ReplayTransport::from_jsonl("").unwrap());
        assert_eq!(
            global_ap_excluding(&mut s, &m, &[0x04, 0x05]).unwrap(),
            Global::NoneOutsideAKeyset
        );
        assert!(s.into_inner().finished());
    }

    /// The `global_ap_excluding` agreement/split pairing, mirrored for rapid trigger.
    #[test]
    fn global_rt_excluding_ignores_the_named_keys() {
        let m = membership(Kind::Rt, &[(0x04, 0), (0x05, 0), (0x06, 0), (0x07, 0)]);

        let mut excluded_lines = read_reply(0x05, layout::RT_PRESS, 100);
        excluded_lines.extend(read_reply(0x05, layout::RT_RELEASE, 150));
        excluded_lines.extend(read_reply(0x06, layout::RT_PRESS, 100));
        excluded_lines.extend(read_reply(0x06, layout::RT_RELEASE, 150));
        excluded_lines.extend(read_reply(0x07, layout::RT_PRESS, 100));
        excluded_lines.extend(read_reply(0x07, layout::RT_RELEASE, 150));
        let mut s = Session::new(ReplayTransport::from_jsonl(&excluded_lines.join("\n")).unwrap());
        assert_eq!(
            global_rt_excluding(&mut s, &m, &[0x04]).unwrap(),
            Global::Agreed((Um(100), Um(150)))
        );
        assert!(s.into_inner().finished());

        let mut all_lines = read_reply(0x04, layout::RT_PRESS, 50);
        all_lines.extend(read_reply(0x04, layout::RT_RELEASE, 60));
        all_lines.extend(read_reply(0x05, layout::RT_PRESS, 100));
        all_lines.extend(read_reply(0x05, layout::RT_RELEASE, 150));
        all_lines.extend(read_reply(0x06, layout::RT_PRESS, 100));
        all_lines.extend(read_reply(0x06, layout::RT_RELEASE, 150));
        all_lines.extend(read_reply(0x07, layout::RT_PRESS, 100));
        all_lines.extend(read_reply(0x07, layout::RT_RELEASE, 150));
        let mut s2 = Session::new(ReplayTransport::from_jsonl(&all_lines.join("\n")).unwrap());
        assert_eq!(
            global_rt_excluding(&mut s2, &m, &[]).unwrap(),
            Global::Split(vec![((Um(100), Um(150)), 3), ((Um(50), Um(60)), 1)])
        );
        assert!(s2.into_inner().finished());
    }

    // -- plan: the skip rule, one test per OR-term --

    #[test]
    fn plan_skip_rule_skips_a_key_already_at_every_target_but_still_writes_membership() {
        // MODE 0x18 (Single, advanced 8), already at the target AP: nothing to write.
        let lines = settings_script(0x1A, 2000, 0x18, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change::ap(Um(2000));
        // Obtained the way a real caller would, through `next_index`, not a bare literal: a
        // membership with max 2 allocates 3.
        let idx = next_index(&membership(Kind::Ap, &[(0x99, 2)])).unwrap();
        let plan = plan(&mut s, &[0x1A], &change, Some(idx)).unwrap();
        assert_eq!(plan.value_records, vec![]);
        assert_eq!(
            plan.membership_records,
            vec![KeyRecord {
                key: 0x1A,
                layout: layout::KEYSET_AP,
                value: 3
            }]
        );
        assert!(s.into_inner().finished());
    }

    #[test]
    fn plan_skip_rule_pins_the_ap_term() {
        // Only AP differs (2000 read, target 2500); MODE/RT stay as read but still ride along.
        let lines = settings_script(0x1A, 2000, 0x18, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change::ap(Um(2500));
        let plan = plan(&mut s, &[0x1A], &change, None).unwrap();
        assert_eq!(
            plan.value_records,
            vec![
                KeyRecord {
                    key: 0x1A,
                    layout: layout::MODE,
                    value: 0x18
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::AP,
                    value: 2500
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_PRESS,
                    value: 100
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_RELEASE,
                    value: 150
                },
            ]
        );
        assert!(plan.membership_records.is_empty());
        assert!(s.into_inner().finished());
    }

    /// Real scenario from `captures/ks-steal-rt.jsonl`: key `,` (`0x36`) needed only a MODE
    /// change (already at the target sensitivity) and still got the full template.
    #[test]
    fn plan_skip_rule_pins_the_mode_term() {
        // touch Global(0), so RapidTrigger's own rule changes it to Rt(3); sensitivities already
        // match the target, so only the MODE term can be driving the write.
        let lines = settings_script(0x36, 2000, 0x00, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change::rt_on(Um(100), Um(150));
        let plan = plan(&mut s, &[0x36], &change, None).unwrap();
        assert_eq!(
            plan.value_records,
            vec![
                KeyRecord {
                    key: 0x36,
                    layout: layout::MODE,
                    value: 0x30
                },
                KeyRecord {
                    key: 0x36,
                    layout: layout::AP,
                    value: 2000
                },
                KeyRecord {
                    key: 0x36,
                    layout: layout::RT_PRESS,
                    value: 100
                },
                KeyRecord {
                    key: 0x36,
                    layout: layout::RT_RELEASE,
                    value: 150
                },
            ]
        );
        assert!(s.into_inner().finished());
    }

    /// Real scenario from `captures/ks-steal-rt.jsonl`: key `N` (`0x11`) needed only a
    /// sensitivity change (already at the target MODE) and still got the full template.
    #[test]
    fn plan_skip_rule_pins_the_rt_press_term() {
        // touch already Rt(3), so RapidTrigger's rule leaves MODE unchanged; release already
        // matches the target, so only the press term can be driving the write.
        let lines = settings_script(0x11, 2000, 0x30, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change::rt_on(Um(200), Um(150));
        let plan = plan(&mut s, &[0x11], &change, None).unwrap();
        assert_eq!(
            plan.value_records,
            vec![
                KeyRecord {
                    key: 0x11,
                    layout: layout::MODE,
                    value: 0x30
                },
                KeyRecord {
                    key: 0x11,
                    layout: layout::AP,
                    value: 2000
                },
                KeyRecord {
                    key: 0x11,
                    layout: layout::RT_PRESS,
                    value: 200
                },
                KeyRecord {
                    key: 0x11,
                    layout: layout::RT_RELEASE,
                    value: 150
                },
            ]
        );
        assert!(s.into_inner().finished());
    }

    #[test]
    fn plan_skip_rule_pins_the_rt_release_term() {
        // Mirror of the press-term test above: press already matches the target, so only the
        // release term can be driving the write.
        let lines = settings_script(0x11, 2000, 0x30, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change::rt_on(Um(100), Um(200));
        let plan = plan(&mut s, &[0x11], &change, None).unwrap();
        assert_eq!(
            plan.value_records,
            vec![
                KeyRecord {
                    key: 0x11,
                    layout: layout::MODE,
                    value: 0x30
                },
                KeyRecord {
                    key: 0x11,
                    layout: layout::AP,
                    value: 2000
                },
                KeyRecord {
                    key: 0x11,
                    layout: layout::RT_PRESS,
                    value: 100
                },
                KeyRecord {
                    key: 0x11,
                    layout: layout::RT_RELEASE,
                    value: 200
                },
            ]
        );
        assert!(s.into_inner().finished());
    }

    /// Nothing in `plan`'s tests above supplies a rapid trigger membership index: every one that
    /// does uses `Change::ap`. Pins that `Change::rt_on`/`rt_off`'s own kind sends membership to
    /// `0xFE`, not `0xFF`.
    #[test]
    fn plan_writes_rapid_trigger_membership_to_the_rt_layout() {
        // Already at every target: no value records, membership only.
        let lines = settings_script(0x1A, 2000, 0x30, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change::rt_on(Um(100), Um(150));
        // Obtained through `next_index`: a membership with max 1 allocates 2.
        let idx = next_index(&membership(Kind::Rt, &[(0x99, 1)])).unwrap();
        let plan = plan(&mut s, &[0x1A], &change, Some(idx)).unwrap();
        assert_eq!(plan.value_records, vec![]);
        assert_eq!(
            plan.membership_records,
            vec![KeyRecord {
                key: 0x1A,
                layout: layout::KEYSET_RT,
                value: 2
            }]
        );
        assert!(s.into_inner().finished());
    }

    /// `wh restore` writes an index a live allocation could never produce: a snapshot recorded
    /// index 4 while the board now has 5 as its highest live value, so `next_index` would return
    /// 6, never 4. `KeysetIndex::restoring` is the only way to send the gap value anyway.
    #[test]
    fn plan_writes_a_gap_index_next_index_would_never_allocate() {
        let m = membership(Kind::Ap, &[(0x99, 5)]);
        assert_eq!(
            next_index(&m).unwrap().value(),
            6,
            "confirms 4 really is a gap, not what allocation would give here"
        );

        let lines = settings_script(0x1A, 2000, 0x18, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change::ap(Um(2000));
        let idx = KeysetIndex::restoring(Kind::Ap, 4);
        let plan = plan(&mut s, &[0x1A], &change, Some(idx)).unwrap();
        assert_eq!(
            plan.membership_records,
            vec![KeyRecord {
                key: 0x1A,
                layout: layout::KEYSET_AP,
                value: 4
            }]
        );
        assert!(s.into_inner().finished());
    }

    /// A rapid trigger `Change` paired with an actuation point `KeysetIndex` must be rejected,
    /// and rejected before any frame is sent: the empty script would reject any send at all.
    #[test]
    fn plan_rejects_membership_from_the_wrong_kind() {
        let mut s = Session::new(ReplayTransport::from_jsonl("").unwrap());
        let change = Change::rt_on(Um(100), Um(150));
        let idx = next_index(&membership(Kind::Ap, &[(0x99, 2)])).unwrap();
        let err = plan(&mut s, &[0x1A], &change, Some(idx)).unwrap_err();
        assert!(
            matches!(
                err,
                DeviceError::KeysetKindMismatch {
                    expected: Kind::Rt,
                    found: Kind::Ap
                }
            ),
            "got {err:?}"
        );
        assert!(s.into_inner().finished());
    }

    // -- membership_records_for_restore --

    /// Mixing an actuation point index with a rapid trigger one in one call must error rather
    /// than writing one layout's index into the other layout, which would silently move keys
    /// between groupings.
    #[test]
    fn membership_records_for_restore_refuses_mixed_kinds() {
        let entries = [
            (0x1Au8, KeysetIndex::restoring(Kind::Ap, 1)),
            (0x04, KeysetIndex::restoring(Kind::Rt, 1)),
        ];
        assert!(matches!(
            membership_records_for_restore(&entries),
            Err(DeviceError::KeysetKindMismatch { .. })
        ));
    }

    /// Every record carries its own key's index and the layout its kind names.
    #[test]
    fn membership_records_for_restore_carries_one_index_per_key() {
        let entries = [
            (0x1Au8, KeysetIndex::restoring(Kind::Ap, 3)),
            (0x04, KeysetIndex::restoring(Kind::Ap, 0)),
        ];
        let r = membership_records_for_restore(&entries).unwrap();
        assert_eq!(
            r[0],
            KeyRecord {
                key: 0x1A,
                layout: layout::KEYSET_AP,
                value: 3
            }
        );
        assert_eq!(
            r[1],
            KeyRecord {
                key: 0x04,
                layout: layout::KEYSET_AP,
                value: 0
            }
        );
    }

    // -- apply_touch, the internal touch-nibble rule --

    #[test]
    fn touch_change_keep_leaves_the_nibble_exactly_as_read() {
        assert_eq!(
            apply_touch(TouchMode::RtContinuous, TouchChange::Keep),
            TouchMode::RtContinuous
        );
        assert_eq!(
            apply_touch(TouchMode::Global, TouchChange::Keep),
            TouchMode::Global
        );
    }

    #[test]
    fn touch_change_promote_global_to_single_only_touches_global() {
        assert_eq!(
            apply_touch(TouchMode::Global, TouchChange::PromoteGlobalToSingle),
            TouchMode::Single
        );
        // RtGlobal (nibble 2) must be left alone by the promotion, not swept up with Global.
        assert_eq!(
            apply_touch(TouchMode::RtGlobal, TouchChange::PromoteGlobalToSingle),
            TouchMode::RtGlobal
        );
        assert_eq!(
            apply_touch(TouchMode::Rt, TouchChange::PromoteGlobalToSingle),
            TouchMode::Rt
        );
    }

    #[test]
    fn touch_change_rapid_trigger_leaves_rt_continuous_alone() {
        assert_eq!(
            apply_touch(TouchMode::RtContinuous, TouchChange::RapidTrigger),
            TouchMode::RtContinuous
        );
        assert_eq!(
            apply_touch(TouchMode::Global, TouchChange::RapidTrigger),
            TouchMode::Rt
        );
        assert_eq!(
            apply_touch(TouchMode::Single, TouchChange::RapidTrigger),
            TouchMode::Rt
        );
    }

    /// `Off` now means "turn rapid trigger off", not "force Single unconditionally": it must
    /// leave `Global` and any `Unknown` nibble exactly as read, matching `ops::rt_off_records`.
    #[test]
    fn touch_change_off_turns_off_rapid_trigger_but_leaves_other_nibbles_alone() {
        assert_eq!(
            apply_touch(TouchMode::RtGlobal, TouchChange::Off),
            TouchMode::Single
        );
        assert_eq!(
            apply_touch(TouchMode::Rt, TouchChange::Off),
            TouchMode::Single
        );
        assert_eq!(
            apply_touch(TouchMode::RtContinuous, TouchChange::Off),
            TouchMode::Single
        );
        assert_eq!(
            apply_touch(TouchMode::Global, TouchChange::Off),
            TouchMode::Global
        );
        assert_eq!(
            apply_touch(TouchMode::Unknown(5), TouchChange::Off),
            TouchMode::Unknown(5)
        );
    }

    // -- plan: advanced nibble and high byte survive every touch change --
    //
    // Each case forces a value record to be emitted (the touch nibble changes, or AP changes
    // alongside an unchanged MODE) and asserts the full record list: four when MODE moves or
    // stays at a non-zero nibble, three (MODE absent) when it would only echo nibble 0 back. A
    // variant that wrongly emits nothing, or corrupts the high byte only where touch is
    // unchanged, fails rather than passing silently.

    #[test]
    fn plan_keep_preserves_advanced_nibble_and_high_byte_when_ap_changes() {
        // touch Rt(3), advanced 7, high 0x02: 0x0237. Keep never touches MODE, so this also
        // proves MODE rides along unchanged when only AP differs.
        let lines = settings_script(0x1A, 2000, 0x0237, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change::ap(Um(2500));
        let p = plan(&mut s, &[0x1A], &change, None).unwrap();
        assert_eq!(
            p.value_records,
            vec![
                KeyRecord {
                    key: 0x1A,
                    layout: layout::MODE,
                    value: 0x0237
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::AP,
                    value: 2500
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_PRESS,
                    value: 100
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_RELEASE,
                    value: 150
                },
            ]
        );
        assert!(s.into_inner().finished());
    }

    #[test]
    fn plan_ap_promotes_global_to_single_preserving_advanced_nibble_and_high_byte() {
        // touch Global(0), advanced 7, high 0x02: 0x0207. Promotion changes only the nibble.
        let lines = settings_script(0x1A, 2000, 0x0207, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change::ap(Um(2000));
        let p = plan(&mut s, &[0x1A], &change, None).unwrap();
        assert_eq!(
            p.value_records,
            vec![
                KeyRecord {
                    key: 0x1A,
                    layout: layout::MODE,
                    value: 0x0217
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::AP,
                    value: 2000
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_PRESS,
                    value: 100
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_RELEASE,
                    value: 150
                },
            ]
        );
        assert!(s.into_inner().finished());
    }

    /// The swapped-back regression: `ap_keeping_touch` must leave a `Global` key `Global`, not
    /// promote it, or the two constructors' touch rules have traded places again.
    #[test]
    fn plan_ap_keeping_touch_does_not_promote_a_global_key() {
        // touch Global(0), advanced 7, high 0x02: 0x0207. AP differs, MODE would only echo the
        // unchanged nibble-0 value back, so it must be absent, not sent as a fourth record.
        let lines = settings_script(0x1A, 2000, 0x0207, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change::ap_keeping_touch(Um(2500));
        let p = plan(&mut s, &[0x1A], &change, None).unwrap();
        assert_eq!(
            p.value_records,
            vec![
                KeyRecord {
                    key: 0x1A,
                    layout: layout::AP,
                    value: 2500
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_PRESS,
                    value: 100
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_RELEASE,
                    value: 150
                },
            ],
            "no MODE record: the key stays Global, and the vendor never writes nibble 0"
        );
        assert!(s.into_inner().finished());
    }

    /// The nibble-0 omission is a rule of `plan` itself, not something specific to
    /// `Change::ap_keeping_touch`: a key at touch Global that `Change::rt_off` leaves at Global
    /// (its own rule never touches that nibble) still gets no MODE record when only its
    /// sensitivity differs.
    #[test]
    fn plan_omits_a_nibble_0_mode_record_when_only_rt_differs_and_touch_stays_global() {
        // touch Global(0), advanced 5, high 0: 0x05.
        let lines = settings_script(0x1A, 2000, 0x05, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change::rt_off(Um(200), Um(150));
        let p = plan(&mut s, &[0x1A], &change, None).unwrap();
        assert_eq!(
            p.value_records,
            vec![
                KeyRecord {
                    key: 0x1A,
                    layout: layout::AP,
                    value: 2000
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_PRESS,
                    value: 200
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_RELEASE,
                    value: 150
                },
            ],
            "no MODE record: touch stays Global, only the press sensitivity differs"
        );
        assert!(s.into_inner().finished());
    }

    #[test]
    fn plan_rt_on_preserves_advanced_nibble_and_high_byte() {
        // touch Global(0), advanced 7, high 0x02: 0x0207. RapidTrigger changes only the nibble.
        let lines = settings_script(0x1A, 2000, 0x0207, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change::rt_on(Um(100), Um(150));
        let p = plan(&mut s, &[0x1A], &change, None).unwrap();
        assert_eq!(
            p.value_records,
            vec![
                KeyRecord {
                    key: 0x1A,
                    layout: layout::MODE,
                    value: 0x0237
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::AP,
                    value: 2000
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_PRESS,
                    value: 100
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_RELEASE,
                    value: 150
                },
            ]
        );
        assert!(s.into_inner().finished());
    }

    #[test]
    fn plan_rt_off_preserves_advanced_nibble_and_high_byte() {
        // touch Rt(3), advanced 7, high 0x02: 0x0237. Off's own rule changes only the nibble.
        let lines = settings_script(0x1A, 2000, 0x0237, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change::rt_off(Um(100), Um(150));
        let p = plan(&mut s, &[0x1A], &change, None).unwrap();
        assert_eq!(
            p.value_records,
            vec![
                KeyRecord {
                    key: 0x1A,
                    layout: layout::MODE,
                    value: 0x0217
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::AP,
                    value: 2000
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_PRESS,
                    value: 100
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_RELEASE,
                    value: 150
                },
            ]
        );
        assert!(s.into_inner().finished());
    }

    // -- plan: before --

    /// `before` is what lets a caller verify every selected key, including ones the skip rule
    /// gave no value records to. Two keys: `0x04` changes, `0x05` is already at the target and
    /// gets none, but both must still appear in `before`, verbatim and in `usages` order.
    #[test]
    fn plan_before_covers_every_key_including_ones_the_skip_rule_gave_no_records() {
        let mut lines = settings_script(0x04, 2000, 0x18, 100, 150, 0, 0);
        lines.extend(settings_script(0x05, 2500, 0x18, 100, 150, 2, 1));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change::ap(Um(2500));
        let p = plan(&mut s, &[0x04, 0x05], &change, None).unwrap();
        assert!(s.into_inner().finished());

        assert_eq!(
            p.before,
            vec![
                KeySettings {
                    usage: 0x04,
                    ap: Um(2000),
                    mode: Mode::from_value(0x18),
                    rt_press: Um(100),
                    rt_release: Um(150),
                    ap_keyset: 0,
                    rt_keyset: 0,
                },
                KeySettings {
                    usage: 0x05,
                    ap: Um(2500),
                    mode: Mode::from_value(0x18),
                    rt_press: Um(100),
                    rt_release: Um(150),
                    ap_keyset: 2,
                    rt_keyset: 1,
                },
            ]
        );
        // 0x05 was already at the target: confirms `before` covers it despite no value record.
        assert!(p.value_records.iter().all(|r| r.key == 0x04));
        assert!(!p.value_records.is_empty());
    }

    // -- WritePlan::frames: no key's records split across a report boundary --

    /// Before the nibble-0 omission, every key contributed exactly four records, so a key's
    /// MODE always sat at an even index and could never land on offset 13 of a 14-record report:
    /// the pairing held by parity, not by construction. A uniform fixture cannot exercise the
    /// mixed 3-and-4-record case that broke it, which is why this replaces that fixture rather
    /// than keeping it alongside a weaker one.
    ///
    /// Eight keys: the first at touch `Global` (its MODE omitted, 3 records) and seven at touch
    /// `Rt`, which `Change::rt_off` moves to `Single` (4 records each), all with sensitivities
    /// differing from the target. 31 value records, packed by hand here into the three batches
    /// `frames()` must produce: 11 (3+4+4), 12 (4+4+4), 8 (4+4), never splitting a key's group.
    #[test]
    fn frames_never_splits_a_keys_records_across_a_report_boundary_with_mixed_group_sizes() {
        let usages: Vec<u8> = (0x04u8..0x0C).collect();
        assert_eq!(usages.len(), 8);
        let mut lines = settings_script(usages[0], 2000, 0x05, 100, 150, 0, 0);
        for &u in &usages[1..] {
            lines.extend(settings_script(u, 2000, 0x35, 100, 150, 0, 0));
        }
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change::rt_off(Um(200), Um(250));
        let p = plan(&mut s, &usages, &change, None).unwrap();
        assert!(s.into_inner().finished());

        let vr = p.value_records();
        assert_eq!(
            vr.len(),
            31,
            "3 (Global, MODE omitted) + 7 * 4 (Rt, MODE included)"
        );

        let expected = vec![
            cmds::write_key_records(&vr[0..11])[0],
            cmds::write_key_records(&vr[11..23])[0],
            cmds::write_key_records(&vr[23..31])[0],
        ];
        assert_eq!(
            p.frames(),
            expected,
            "each batch must be exactly one key group run, never a group split across two"
        );
    }

    // -- WritePlan::frames --

    #[test]
    fn frames_batches_values_but_not_membership_and_membership_comes_last() {
        let value_records = vec![
            KeyRecord {
                key: 0x04,
                layout: layout::AP,
                value: 1000,
            },
            KeyRecord {
                key: 0x05,
                layout: layout::AP,
                value: 1000,
            },
        ];
        let membership_records = vec![
            KeyRecord {
                key: 0x04,
                layout: layout::KEYSET_AP,
                value: 3,
            },
            KeyRecord {
                key: 0x05,
                layout: layout::KEYSET_AP,
                value: 3,
            },
        ];
        let plan = WritePlan {
            value_records: value_records.clone(),
            membership_records: membership_records.clone(),
            before: vec![],
        };
        let frames = plan.frames();
        let expected_values = cmds::write_key_records(&value_records);
        let expected_membership = cmds::write_key_records_singly(&membership_records);
        assert_eq!(expected_values.len(), 1, "both AP records batch together");
        assert_eq!(
            expected_membership.len(),
            2,
            "membership never batches, one frame per record"
        );
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0], expected_values[0]);
        assert_eq!(frames[1], expected_membership[0]);
        assert_eq!(frames[2], expected_membership[1]);
    }

    #[test]
    fn is_empty_is_true_only_when_both_record_lists_are_empty() {
        let empty = WritePlan {
            value_records: vec![],
            membership_records: vec![],
            before: vec![],
        };
        assert!(empty.is_empty());

        let with_membership = WritePlan {
            value_records: vec![],
            membership_records: vec![KeyRecord {
                key: 0x04,
                layout: layout::KEYSET_AP,
                value: 1,
            }],
            before: vec![],
        };
        assert!(!with_membership.is_empty());
    }

    // -- apply --

    #[test]
    fn apply_sends_nothing_for_an_empty_plan() {
        let plan = WritePlan {
            value_records: vec![],
            membership_records: vec![],
            before: vec![],
        };
        let mut s = Session::new(ReplayTransport::from_jsonl("").unwrap());
        apply(&mut s, &plan).unwrap();
        assert!(s.into_inner().finished());
    }

    #[test]
    fn apply_sends_value_frames_then_membership_frames() {
        let value_records = vec![KeyRecord {
            key: 0x04,
            layout: layout::AP,
            value: 1000,
        }];
        let membership_records = vec![KeyRecord {
            key: 0x04,
            layout: layout::KEYSET_AP,
            value: 3,
        }];
        let plan = WritePlan {
            value_records,
            membership_records,
            before: vec![],
        };
        let mut lines = Vec::new();
        for f in plan.frames() {
            lines.push(l("out", &f));
            lines.push(l("in", &rf(cmds::cmd::KEY, &[0x01])));
        }
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        apply(&mut s, &plan).unwrap();
        assert!(s.into_inner().finished());
    }
}
