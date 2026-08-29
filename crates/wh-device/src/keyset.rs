//! Keyset device layer: membership grouping, allocation, and the read-modify-write plan for
//! creating, valuing, and deleting actuation point and rapid trigger keysets. See
//! `docs/keysets.md` for the measured evidence this module implements.

use crate::ops::{self, KeySettings};
use crate::session::Session;
use crate::transport::{DeviceError, Transport};
use wh_proto::cmds::{self, layout, KeyRecord, Mode, TouchMode};
use wh_proto::value::Um;

/// Which of the two independent groupings a keyset belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ap,
    Rt,
}

impl Kind {
    /// `layout::KEYSET_AP` (0xFF) or `layout::KEYSET_RT` (0xFE).
    pub fn layout(self) -> u8 {
        match self {
            Kind::Ap => layout::KEYSET_AP,
            Kind::Rt => layout::KEYSET_RT,
        }
    }
}

/// One keyset: the index its members hold, and the keys holding it, in the order given.
#[derive(Debug, Clone, PartialEq)]
pub struct Keyset {
    pub index: u16,
    pub members: Vec<u8>,
}

/// Each key's raw membership value for one layout, in `usages` order.
pub fn read_membership<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
    kind: Kind,
) -> Result<Vec<(u8, u16)>, DeviceError> {
    let mut out = Vec::with_capacity(usages.len());
    for &u in usages {
        let v = ops::read_layout_value(s, u, kind.layout())?;
        out.push((u, v));
    }
    Ok(out)
}

/// The keysets present, ascending by index. Membership `0` means "in no keyset" and is excluded.
pub fn group(membership: &[(u8, u16)]) -> Vec<Keyset> {
    let mut keysets: Vec<Keyset> = Vec::new();
    for &(usage, index) in membership {
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

/// The next index to allocate: the highest live membership value plus one, or `1` when no key
/// holds any. Errors rather than wrapping if the highest is already `u16::MAX`.
pub fn next_index(membership: &[(u8, u16)]) -> Result<u16, DeviceError> {
    let max = membership.iter().map(|&(_, v)| v).filter(|&v| v != 0).max();
    match max {
        None => Ok(1),
        Some(u16::MAX) => Err(DeviceError::Decode(
            "keyset index already at u16::MAX, cannot allocate another".to_string(),
        )),
        Some(m) => Ok(m + 1),
    }
}

/// What an operation does to a key's touch nibble.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchChange {
    /// Leave it exactly as read.
    Keep,
    /// `Global` (0) becomes `Single` (1); every other nibble is left alone.
    PromoteGlobalToSingle,
    /// `Rt` (3), except a key already `RtContinuous` (4) stays `RtContinuous`.
    RapidTrigger,
    /// `Single` (1), whatever the key held.
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
        TouchChange::Off => TouchMode::Single,
    }
}

/// One operation's targets. `None` means "keep the key's current value".
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub touch: TouchChange,
    pub ap: Option<Um>,
    /// Rapid trigger press and release, always set together, as every capture does.
    pub rt: Option<(Um, Um)>,
}

/// The records one operation writes, plus what was on the board before it.
#[derive(Debug, Clone, PartialEq)]
pub struct WritePlan {
    /// Value records, batched normally by `cmds::write_key_records`.
    pub value_records: Vec<KeyRecord>,
    /// Membership records, written one per frame.
    pub membership_records: Vec<KeyRecord>,
    /// Each key's settings as read before the write, in `usages` order, so a caller can verify
    /// every key including ones that got no records.
    pub before: Vec<KeySettings>,
}

impl WritePlan {
    /// Frames in send order: the value batches, then one frame per membership record.
    pub fn frames(&self) -> Vec<[u8; 64]> {
        let mut frames = cmds::write_key_records(&self.value_records);
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
/// `usages` (`0` clears), or `None` for an operation that does not touch membership.
///
/// `kind` says which layout, `0xFF` or `0xFE`, membership targets: the brief's signature for
/// `plan` omitted it, but `Change` carries no such field and the two layouts are otherwise
/// indistinguishable here, so this deviates by adding it. Flagged for review.
///
/// Per key, MODE, AP, RT_PRESS and RT_RELEASE are read, the target computed, and either all four
/// are written or none, matching the vendor's own all-or-nothing template.
///
/// Two deliberate divergences from the vendor: layouts `0x16`/`0x17` are never written, since we
/// have never read them and a constant would be an invented value; and records are emitted
/// key-major rather than the vendor's layout-major order, the same divergence `ops::ap_records`
/// documents, so a mid-batch failure stops at a few keys rather than every key selected.
pub fn plan<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
    change: &Change,
    kind: Kind,
    membership: Option<u16>,
) -> Result<WritePlan, DeviceError> {
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
            value_records.push(KeyRecord {
                key: u,
                layout: layout::MODE,
                value: new_mode_value,
            });
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
        Some(index) => usages
            .iter()
            .map(|&u| KeyRecord {
                key: u,
                layout: kind.layout(),
                value: index,
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

/// The board's global actuation point: what a key in no actuation point keyset holds in layout
/// `0x04`. `None` when every key is in a keyset, so no key can report it.
pub fn global_ap<T: Transport>(
    s: &mut Session<T>,
    membership: &[(u8, u16)],
) -> Result<Option<Um>, DeviceError> {
    let Some(&(usage, _)) = membership.iter().find(|&&(_, v)| v == 0) else {
        return Ok(None);
    };
    let value = ops::read_layout_value(s, usage, layout::AP)?;
    Ok(Some(Um(value)))
}

/// The global rapid trigger sensitivity, read the same way from `0x14`/`0x15` of a key in no
/// rapid trigger keyset. `None` when every key is in one.
pub fn global_rt<T: Transport>(
    s: &mut Session<T>,
    membership: &[(u8, u16)],
) -> Result<Option<(Um, Um)>, DeviceError> {
    let Some(&(usage, _)) = membership.iter().find(|&&(_, v)| v == 0) else {
        return Ok(None);
    };
    let press = ops::read_layout_value(s, usage, layout::RT_PRESS)?;
    let release = ops::read_layout_value(s, usage, layout::RT_RELEASE)?;
    Ok(Some((Um(press), Um(release))))
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

    // -- next_index --

    #[test]
    fn next_index_with_no_members_is_one() {
        let membership = vec![(0x04u8, 0u16), (0x05, 0)];
        assert_eq!(next_index(&membership).unwrap(), 1);
    }

    #[test]
    fn next_index_is_max_plus_one() {
        let membership = vec![(0x04u8, 1u16), (0x05, 2)];
        assert_eq!(next_index(&membership).unwrap(), 3);
    }

    #[test]
    fn next_index_skips_a_freed_gap_rather_than_filling_it() {
        let membership = vec![(0x04u8, 1u16), (0x05, 2), (0x06, 4)];
        assert_eq!(next_index(&membership).unwrap(), 5);
    }

    #[test]
    fn next_index_errors_rather_than_wrapping_at_u16_max() {
        let membership = vec![(0x04u8, u16::MAX)];
        let err = next_index(&membership).unwrap_err();
        assert!(matches!(err, DeviceError::Decode(_)), "got {err:?}");
    }

    // -- group --

    #[test]
    fn group_is_ascending_by_index_and_excludes_zero() {
        let membership = vec![(0x04u8, 2u16), (0x05, 0), (0x06, 1), (0x07, 2), (0x08, 1)];
        let got = group(&membership);
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
        let membership = vec![(0x10u8, 3u16), (0x05, 3), (0x08, 3)];
        let got = group(&membership);
        assert_eq!(got[0].members, vec![0x10, 0x05, 0x08]);
    }

    // -- read_membership --

    #[test]
    fn read_membership_reads_each_usage_for_the_given_kind_layout() {
        let mut lines = Vec::new();
        lines.extend(read_reply(0x04, layout::KEYSET_RT, 1));
        lines.extend(read_reply(0x05, layout::KEYSET_RT, 0));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let got = read_membership(&mut s, &[0x04, 0x05], Kind::Rt).unwrap();
        assert_eq!(got, vec![(0x04, 1), (0x05, 0)]);
        assert!(s.into_inner().finished());
    }

    // -- global_ap / global_rt --

    #[test]
    fn global_ap_returns_none_when_every_key_is_in_a_keyset() {
        let membership = vec![(0x04u8, 1u16), (0x05, 2)];
        let mut s = Session::new(ReplayTransport::from_jsonl("").unwrap());
        assert_eq!(global_ap(&mut s, &membership).unwrap(), None);
        assert!(s.into_inner().finished());
    }

    #[test]
    fn global_rt_returns_none_when_every_key_is_in_a_keyset() {
        let membership = vec![(0x04u8, 1u16), (0x05, 1)];
        let mut s = Session::new(ReplayTransport::from_jsonl("").unwrap());
        assert_eq!(global_rt(&mut s, &membership).unwrap(), None);
        assert!(s.into_inner().finished());
    }

    #[test]
    fn global_ap_reads_the_first_unkeyset_key() {
        let membership = vec![(0x04u8, 1u16), (0x05, 0)];
        let lines = read_reply(0x05, layout::AP, 2000);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        assert_eq!(global_ap(&mut s, &membership).unwrap(), Some(Um(2000)));
        assert!(s.into_inner().finished());
    }

    #[test]
    fn global_rt_reads_the_first_unkeyset_key() {
        let membership = vec![(0x04u8, 1u16), (0x05, 0)];
        let mut lines = read_reply(0x05, layout::RT_PRESS, 100);
        lines.extend(read_reply(0x05, layout::RT_RELEASE, 150));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        assert_eq!(
            global_rt(&mut s, &membership).unwrap(),
            Some((Um(100), Um(150)))
        );
        assert!(s.into_inner().finished());
    }

    // -- plan: the skip rule --

    #[test]
    fn plan_skips_value_records_for_a_key_already_at_target_but_still_writes_membership() {
        // MODE 0x18 (Single, advanced 8), already at the target AP/RT: nothing to write.
        let lines = settings_script(0x1A, 2000, 0x18, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change {
            touch: TouchChange::Keep,
            ap: Some(Um(2000)),
            rt: None,
        };
        let plan = plan(&mut s, &[0x1A], &change, Kind::Ap, Some(3)).unwrap();
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
    fn plan_writes_all_four_value_records_when_exactly_one_differs() {
        // Only AP differs (2000 read, target 2500); MODE/RT stay as read but still ride along.
        let lines = settings_script(0x1A, 2000, 0x18, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = Change {
            touch: TouchChange::Keep,
            ap: Some(Um(2500)),
            rt: None,
        };
        let plan = plan(&mut s, &[0x1A], &change, Kind::Ap, None).unwrap();
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

    // -- plan: TouchChange variants --

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

    #[test]
    fn touch_change_off_sets_single_unconditionally_including_from_rt_continuous() {
        assert_eq!(
            apply_touch(TouchMode::RtContinuous, TouchChange::Off),
            TouchMode::Single
        );
        assert_eq!(
            apply_touch(TouchMode::Rt, TouchChange::Off),
            TouchMode::Single
        );
        assert_eq!(
            apply_touch(TouchMode::Global, TouchChange::Off),
            TouchMode::Single
        );
    }

    #[test]
    fn plan_preserves_the_advanced_nibble_and_high_byte_across_every_touch_change() {
        // MODE 0x0237: touch Rt(3), advanced 7, high byte 0x02.
        for change in [
            TouchChange::Keep,
            TouchChange::PromoteGlobalToSingle,
            TouchChange::RapidTrigger,
            TouchChange::Off,
        ] {
            let lines = settings_script(0x1A, 2000, 0x0237, 100, 150, 0, 0);
            let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
            let c = Change {
                touch: change,
                ap: None,
                rt: None,
            };
            let p = plan(&mut s, &[0x1A], &c, Kind::Ap, None).unwrap();
            assert!(s.into_inner().finished());
            if let Some(rec) = p.value_records.iter().find(|r| r.layout == layout::MODE) {
                assert_eq!(
                    rec.value & 0xFF0F,
                    0x0207,
                    "{change:?}: advanced nibble and high byte must survive"
                );
            }
        }
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
