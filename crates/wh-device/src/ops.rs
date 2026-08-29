//! High-level operations composed from wh-proto commands.

use crate::session::Session;
use crate::transport::{DeviceError, Transport};
use wh_proto::cmds::{self, layout, KeyRecord, Mode, TouchMode};
use wh_proto::value::Um;

#[derive(Debug, Clone, PartialEq)]
pub struct KeySettings {
    pub usage: u8,
    pub ap: Um,
    pub mode: Mode,
    pub rt_press: Um,
    pub rt_release: Um,
    /// Raw layout `0xFF` and `0xFE` values, uninterpreted. `0` is the value read for keys the
    /// vendor UI showed outside any keyset.
    pub ap_keyset: u16,
    pub rt_keyset: u16,
}

impl KeySettings {
    /// True for all three rapid trigger nibbles. `RtGlobal` (2) follows the board's global
    /// settings, `Rt` (3) carries the key's own, and `RtContinuous` (4) adds the device's
    /// continuous-mode toggle, which `wh` does not expose. All three mean rapid trigger is on.
    pub fn rt_enabled(&self) -> bool {
        matches!(
            self.mode.touch,
            TouchMode::RtGlobal | TouchMode::Rt | TouchMode::RtContinuous
        )
    }
}

/// All key usages on the board, from the 6x21 default-key matrix.
pub fn read_matrix<T: Transport>(s: &mut Session<T>) -> Result<Vec<u8>, DeviceError> {
    let mut usages = Vec::new();
    for (a, b) in [(0u8, 1u8), (2, 3), (4, 5)] {
        let payload = s.roundtrip(&cmds::read_defkey_rows(a, b))?;
        let rows = cmds::parse_defkey(&payload).map_err(|e| DeviceError::Decode(e.to_string()))?;
        for row in rows {
            for (_, usage) in row.keys {
                if !usages.contains(&usage) {
                    usages.push(usage);
                }
            }
        }
    }
    Ok(usages)
}

pub fn read_key_settings<T: Transport>(
    s: &mut Session<T>,
    usage: u8,
) -> Result<KeySettings, DeviceError> {
    let ap = Um(read_layout_value(s, usage, layout::AP)?);
    let mode = Mode::from_value(read_layout_value(s, usage, layout::MODE)?);
    let rt_press = Um(read_layout_value(s, usage, layout::RT_PRESS)?);
    let rt_release = Um(read_layout_value(s, usage, layout::RT_RELEASE)?);
    let ap_keyset = read_layout_value(s, usage, layout::KEYSET_AP)?;
    let rt_keyset = read_layout_value(s, usage, layout::KEYSET_RT)?;
    Ok(KeySettings {
        usage,
        ap,
        mode,
        rt_press,
        rt_release,
        ap_keyset,
        rt_keyset,
    })
}

/// Reads one key's layout value, rejecting a reply that doesn't echo back the same key and
/// layout id asked for. `Session::roundtrip` matches only on the command byte (0x23 for every
/// per-key read and write ack), so a stale reply could otherwise apply key A's value to key B.
pub(crate) fn read_layout_value<T: Transport>(
    s: &mut Session<T>,
    usage: u8,
    layout_id: u8,
) -> Result<u16, DeviceError> {
    let payload = s.roundtrip(&cmds::read_key_layout(usage, layout_id))?;
    let rec = cmds::parse_key_reply(&payload).map_err(|e| DeviceError::Decode(e.to_string()))?;
    if rec.key != usage || rec.layout != layout_id {
        return Err(DeviceError::Decode(format!(
            "expected reply for key {usage:#04x} layout {layout_id:#04x}, got key {:#04x} layout {:#04x}",
            rec.key, rec.layout
        )));
    }
    Ok(rec.value)
}

/// Write `records` in batches, or nothing at all if `records` is empty.
///
/// No SAVE order follows: the vendor never sends one across 1224 captured frames. Whether the
/// board persists by itself is unmeasured. `roundtrip_many` reports how many frames landed
/// before a mid-batch failure instead of a bare timeout.
fn write_records<T: Transport>(
    s: &mut Session<T>,
    records: &[KeyRecord],
) -> Result<(), DeviceError> {
    if records.is_empty() {
        return Ok(());
    }
    let frames = cmds::write_key_records(records);
    s.roundtrip_many(&frames)?;
    Ok(())
}

/// Builds the [mode, rt_press, rt_release] records to enable RT on `usages`, preserving each
/// key's advanced-mode nibble. Reads current MODE per key but sends nothing else, so a caller
/// can dry-run before writing.
///
/// Written touch nibble is `Rt`, unless the key already carries `RtContinuous` (the vendor UI's
/// own toggle, which `wh` has no flag for): reading MODE first lets a sensitivity-only change
/// (`wh set rt --keys w --set 0.5`) preserve that variant instead of silently collapsing it to
/// plain `Rt`.
pub fn rt_records<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
    press: Um,
    release: Um,
) -> Result<Vec<KeyRecord>, DeviceError> {
    let mut records = Vec::new();
    for &u in usages {
        let cur_value = read_layout_value(s, u, layout::MODE)?;
        let cur_mode = Mode::from_value(cur_value);
        // `RtGlobal` collapses to `Rt`: asking for a sensitivity is asking for the key's own,
        // which is nibble 3. That is what the vendor writes when it makes a rapid trigger keyset.
        let touch = match cur_mode.touch {
            TouchMode::RtContinuous => TouchMode::RtContinuous,
            _ => TouchMode::Rt,
        };
        let mode = Mode {
            touch,
            advanced: cur_mode.advanced,
            high: cur_mode.high,
        };
        records.push(KeyRecord {
            key: u,
            layout: layout::MODE,
            value: mode.value(),
        });
        records.push(KeyRecord {
            key: u,
            layout: layout::RT_PRESS,
            value: press.0,
        });
        records.push(KeyRecord {
            key: u,
            layout: layout::RT_RELEASE,
            value: release.0,
        });
    }
    Ok(records)
}

/// Builds the [mode] records to turn rapid trigger off on `usages`, preserving each key's
/// advanced-mode nibble. Reads current MODE per key but sends nothing else.
///
/// Only rewrites keys with rapid trigger on, to `Single` (nibble 1), never to `Global`
/// (nibble 0). Keys already `Global`, `Single`, or `Unknown` are left untouched: a key with
/// nothing to change gets no record (see the skip below), so `wh set rt --keys all --off` on a
/// board with few RT keys doesn't detach every other key from the global travel setting.
///
/// `RtGlobal` counts: a key following the board's global rapid trigger has it on, so `--off`
/// must turn it off. Nibble 1 is what the vendor itself writes when the global switch goes off
/// (`captures/ks-global-rt-off.jsonl`), so this is the same value from the same measurement.
///
/// Measured on the real device (`captures/rt-off-w.jsonl`): turning RT off wrote nibble 1, not
/// 0. What nibble 0 does to the key's own actuation-point value is unmeasured; this function
/// writes nibble 1 because that's what the vendor was observed writing here.
pub fn rt_off_records<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
) -> Result<Vec<KeyRecord>, DeviceError> {
    let mut records = Vec::new();
    for &u in usages {
        let cur_value = read_layout_value(s, u, layout::MODE)?;
        let cur_mode = Mode::from_value(cur_value);
        let touch = match cur_mode.touch {
            TouchMode::RtGlobal | TouchMode::Rt | TouchMode::RtContinuous => TouchMode::Single,
            other => other,
        };
        let mode = Mode {
            touch,
            advanced: cur_mode.advanced,
            high: cur_mode.high,
        };
        let new_value = mode.value();
        // Skip when nothing would change: a key that wasn't `Rt`/`RtContinuous` recomputes to
        // the value just read, and sending it anyway would write a MODE value (nibble 0
        // included) the vendor was never once observed sending, across 1224 captured frames.
        if new_value != cur_value {
            records.push(KeyRecord {
                key: u,
                layout: layout::MODE,
                value: new_value,
            });
        }
    }
    Ok(records)
}

/// Enables RT on `usages` with the given sensitivities, preserving each key's advanced nibble.
/// Returns the exact records written (one MODE/RT_PRESS/RT_RELEASE triple per key, in `usages`
/// order) so a caller can verify the write against what was actually sent.
pub fn set_rt<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
    press: Um,
    release: Um,
) -> Result<Vec<KeyRecord>, DeviceError> {
    let records = rt_records(s, usages, press, release)?;
    write_records(s, &records)?;
    Ok(records)
}

/// Disables RT (touch mode to `Single`, per-key actuation point), preserving the advanced
/// nibble. Returns the records actually written, in `usages` order; not necessarily one per
/// key, since a key with nothing to change (see `rt_off_records`) contributes none.
pub fn set_rt_off<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
) -> Result<Vec<KeyRecord>, DeviceError> {
    let records = rt_off_records(s, usages)?;
    write_records(s, &records)?;
    Ok(records)
}

/// What an actuation point write plans to send, and what it expects to find afterwards.
///
/// `prior_modes` carries the MODE value read from each key before the write, in `usages` order.
/// A key that got no MODE record must still read back that exact value, so a firmware that
/// changed MODE on its own (clearing rapid trigger, say) is a verifiable failure rather than an
/// unchecked read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ApPlan {
    pub records: Vec<KeyRecord>,
    pub prior_modes: Vec<(u8, u16)>,
}

impl ApPlan {
    /// The MODE value read from `usage` before the write, if that key is part of this plan.
    pub fn prior_mode(&self, usage: u8) -> Option<u16> {
        self.prior_modes
            .iter()
            .find(|&&(u, _)| u == usage)
            .map(|&(_, v)| v)
    }
}

/// Builds the [mode?, ap] records to set `usages`' actuation point (layout DB0), plus each key's
/// pre-write MODE. Reads current MODE per key but sends nothing else, so a caller can dry-run.
///
/// Always writes AP. Also writes MODE, promoted to `Single`, but only when the key currently
/// reads `Global`. The vendor writes MODE `0x18` on every actuation point change, including for
/// keys already at `0x18`, so our rule is a strict subset of the vendor's, not a match for it.
///
/// That MODE marker is the leading hypothesis for why a value written without it renders greyed
/// in the vendor configurator: nibble 1 has direct write evidence in every captured actuation
/// point change. It stays a hypothesis until the hardware session tests it.
///
/// MODE is ordered before AP per key, matching hardware. Across keys the vendor groups all MODE
/// writes before one AP write, measured three times over in `captures/ap-wasd-1.2.jsonl`; our
/// per-key interleaving is a deliberate divergence whose safety is untested.
///
/// `Single`, `RtGlobal`, `Rt`, `RtContinuous`, and `Unknown` are all left alone. The three
/// rapid trigger nibbles matter most: such a key still carries its own actuation point, so a
/// depth change must not silently turn rapid trigger off. Whether the vendor forces nibble 1
/// here is unmeasured, so this takes the non-destructive reading. `Unknown` nibbles have never
/// been observed on hardware, so overwriting one would discard state we cannot interpret.
///
/// A key's MODE and AP records can land in different write frames, and a failure between the two
/// leaves those keys `Single`, detached from global travel, still holding their old actuation
/// point. Over the 68 keys and modes of `captures/initial-load.jsonl` a whole-board write is 126
/// records in 9 frames, of which 4 straddle a boundary in DEFKEY order, 5 in usage order.
///
/// `DeviceError::Batch` reports how far the batch got, and `wh restore --last` rolls it back from
/// the auto-backup. The vendor's own MODE-then-AP grouping is strictly worse here: a failure
/// between its two phases would leave every key in the selection in that state, not four.
pub fn ap_records<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
    depth: Um,
) -> Result<ApPlan, DeviceError> {
    let mut plan = ApPlan::default();
    for &u in usages {
        let cur_value = read_layout_value(s, u, layout::MODE)?;
        let cur_mode = Mode::from_value(cur_value);
        plan.prior_modes.push((u, cur_value));
        if cur_mode.touch == TouchMode::Global {
            let mode = Mode {
                touch: TouchMode::Single,
                advanced: cur_mode.advanced,
                high: cur_mode.high,
            };
            plan.records.push(KeyRecord {
                key: u,
                layout: layout::MODE,
                value: mode.value(),
            });
        }
        plan.records.push(KeyRecord {
            key: u,
            layout: layout::AP,
            value: depth.0,
        });
    }
    Ok(plan)
}

/// Per-key actuation point (layout DB0), plus the MODE promotion `ap_records` builds. Returns the
/// plan actually written, so a caller can verify AP for every key, the promoted MODE for keys that
/// got a MODE record, and the unchanged pre-write MODE for keys that did not.
pub fn set_ap<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
    depth: Um,
) -> Result<ApPlan, DeviceError> {
    let plan = ap_records(s, usages, depth)?;
    write_records(s, &plan.records)?;
    Ok(plan)
}

pub fn device_info<T: Transport>(s: &mut Session<T>) -> Result<cmds::DeviceInfo, DeviceError> {
    let payload = s.roundtrip(&cmds::sync())?;
    cmds::parse_sync(&payload).map_err(|e| DeviceError::Decode(e.to_string()))
}

pub fn global_travel<T: Transport>(s: &mut Session<T>) -> Result<cmds::GlobalTravel, DeviceError> {
    let payload = s.roundtrip(&cmds::read_global_travel())?;
    cmds::parse_global_travel(&payload).map_err(|e| DeviceError::Decode(e.to_string()))
}

/// The board's currently active profile, already validated (`wh_proto::cmds::ProfileNumber`,
/// `parse_profile`). Read only: profile *select* isn't implemented, since nothing in Phase 1
/// needs to change the active profile.
///
/// A reply naming an index the board's four profiles can't produce surfaces as
/// `DeviceError::ProfileOutOfRange`, kept distinct from `Decode` (a reply that isn't shaped
/// like a profile reply at all), so callers can degrade gracefully on the former while still
/// hard-failing on the latter.
pub fn profile<T: Transport>(s: &mut Session<T>) -> Result<cmds::ProfileNumber, DeviceError> {
    let payload = s.roundtrip(&cmds::read_profile())?;
    cmds::parse_profile(&payload).map_err(|e| match e {
        cmds::DecodeError::ProfileOutOfRange(idx) => DeviceError::ProfileOutOfRange(idx),
        other => DeviceError::Decode(other.to_string()),
    })
}

/// Selects `p`, then reads the profile back to confirm. `select_profile`'s ack does not
/// reliably prove which profile landed, so a successful send alone is not treated as success:
/// this re-reads with `profile()` and errors if the board reports anything but `p`.
///
/// The ack's sub-order byte is checked first. `Session::roundtrip` matches only the command
/// byte, which `read_profile`, `select_profile` and every other cmd `0x00` order share, so a
/// reply to a different sub-order would otherwise pass for this one's ack.
pub fn set_profile<T: Transport>(
    s: &mut Session<T>,
    p: cmds::ProfileNumber,
) -> Result<cmds::ProfileNumber, DeviceError> {
    let ack = s.roundtrip(&cmds::select_profile(p))?;
    match ack.get(1) {
        Some(&sub) if sub == cmds::order::PROFILE => {}
        Some(&sub) => {
            return Err(DeviceError::Decode(format!(
                "select profile {p}: ack sub-order {sub:#04x}, expected {:#04x}",
                cmds::order::PROFILE
            )))
        }
        None => {
            return Err(DeviceError::Decode(format!(
                "select profile {p}: ack payload has no sub-order byte ({} bytes)",
                ack.len()
            )))
        }
    }
    let got = profile(s)?;
    if got != p {
        return Err(DeviceError::Decode(format!(
            "selected profile {p} but the board reports {got}"
        )));
    }
    Ok(got)
}

/// Writes a whole snapshot back to the board: global travel first, then every per-key record.
/// Global travel goes first so a restore that fails partway through the per-key batch still
/// leaves the board's overall travel consistent with what was intended.
pub fn restore_all<T: Transport>(
    s: &mut Session<T>,
    global: &cmds::GlobalTravel,
    records: &[KeyRecord],
) -> Result<(), DeviceError> {
    s.roundtrip(&cmds::write_global_travel(
        global.travel,
        global.press_dead,
        global.release_dead,
    ))?;
    write_records(s, records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::{hex, ReplayTransport};
    use crate::session::Session;
    use crate::transport::DeviceError;
    use wh_proto::cmds::{self, layout, KeyRecord, Mode, TouchMode};
    use wh_proto::value::Um;

    fn l(dir: &str, b: &[u8; 64]) -> String {
        format!("{{\"dir\":\"{dir}\",\"hex\":\"{}\"}}", hex(b))
    }
    /// Builds a reply frame with the high bit set on the command byte, matching how the real
    /// device sends it (`wh_proto::frame::REPLY_BIT`).
    fn rf(cmd: u8, payload: &[u8]) -> [u8; 64] {
        wh_proto::frame::frame(cmd | wh_proto::frame::REPLY_BIT, payload).unwrap()
    }

    /// Scripts a full read_matrix: 3 DEFKEY roundtrips, each row pair with distinct usages, so
    /// passing requires collecting from every row, not just the first.
    fn matrix_script() -> Vec<String> {
        let mut lines = Vec::new();
        for (a, b, ka, kb) in [
            (0u8, 1u8, 0x1Au8, 0x1Bu8),
            (2, 3, 0x1C, 0x1D),
            (4, 5, 0x1E, 0x1F),
        ] {
            let req = cmds::read_defkey_rows(a, b);
            let mut payload = vec![0u8; 45];
            payload[1] = a;
            payload[2] = ka; // row a, col 0
            payload[23] = b;
            payload[24] = kb; // row b, col 0
            lines.push(l("out", &req));
            lines.push(l("in", &rf(cmds::cmd::DEFKEY, &payload)));
        }
        lines
    }

    #[test]
    fn read_matrix_collects_usages_from_every_row_pair() {
        let script = matrix_script().join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&script).unwrap());
        let m = read_matrix(&mut s).unwrap();
        assert_eq!(m, vec![0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F]);
        assert!(s.into_inner().finished());
    }

    #[test]
    fn read_matrix_maps_decode_failure_to_decode_error_not_timeout() {
        let req = cmds::read_defkey_rows(0, 1);
        let bad_reply = rf(cmds::cmd::DEFKEY, &[0x00]); // too short to decode
        let lines = [l("out", &req), l("in", &bad_reply)].join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let err = read_matrix(&mut s).unwrap_err();
        assert!(
            matches!(err, DeviceError::Decode(_)),
            "expected Decode, got {err:?}"
        );
    }

    #[test]
    fn set_rt_writes_mode_and_both_sensitivities_and_sends_no_save() {
        // Expected: write [mode, rtp, rtr] per key, then nothing else. A SAVE order afterwards
        // would be rejected as an unexpected send against this exhausted script.
        let recs = vec![
            KeyRecord {
                key: 0x1A,
                layout: layout::MODE,
                value: 0x30,
            },
            KeyRecord {
                key: 0x1A,
                layout: layout::RT_PRESS,
                value: 500,
            },
            KeyRecord {
                key: 0x1A,
                layout: layout::RT_RELEASE,
                value: 500,
            },
        ];
        let batch = cmds::write_key_records(&recs);
        let mut lines = vec![
            // set_rt first reads current mode to preserve the advanced nibble
            l("out", &cmds::read_key_layout(0x1A, layout::MODE)),
            l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0x00, 0x00]),
            ),
        ];
        for f in &batch {
            lines.push(l("out", f));
            lines.push(l("in", &rf(cmds::cmd::KEY, &[0x01])));
        }
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());

        set_rt(&mut s, &[0x1A], Um(500), Um(500)).unwrap();
        assert!(s.into_inner().finished());
    }

    #[test]
    fn set_rt_over_five_keys_preserves_each_advanced_nibble_and_sends_no_save() {
        // Five keys with distinct MODE bytes (distinct advanced nibbles, including 0x1 and
        // 0xF), so a multi-key call must keep each key's own nibble, not reuse the first key's.
        // Script ends right after the write batch(es); a SAVE order after would be rejected.
        let keys = [0x04u8, 0x05, 0x06, 0x07, 0x08];
        let cur_modes = [0x01u8, 0x1F, 0x22, 0x53, 0x0F];
        let press = Um(400);
        let release = Um(450);

        let mut lines = Vec::new();
        let mut expected = Vec::new();
        for (&k, &m) in keys.iter().zip(cur_modes.iter()) {
            lines.push(l("out", &cmds::read_key_layout(k, layout::MODE)));
            lines.push(l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, k, layout::MODE, m, 0x00]),
            ));
            let cur_mode = Mode::from_value(m as u16);
            let new_mode = Mode {
                touch: TouchMode::Rt,
                advanced: cur_mode.advanced,
                high: cur_mode.high,
            }
            .value();
            expected.push(KeyRecord {
                key: k,
                layout: layout::MODE,
                value: new_mode,
            });
            expected.push(KeyRecord {
                key: k,
                layout: layout::RT_PRESS,
                value: press.0,
            });
            expected.push(KeyRecord {
                key: k,
                layout: layout::RT_RELEASE,
                value: release.0,
            });
        }
        assert_eq!(expected.len(), 15);

        let batches = cmds::write_key_records(&expected);
        assert_eq!(
            batches.len(),
            2,
            "15 records should split into a 14-record and a 1-record batch"
        );
        for f in &batches {
            lines.push(l("out", f));
            lines.push(l("in", &rf(cmds::cmd::KEY, &[0x01])));
        }

        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        set_rt(&mut s, &keys, press, release).unwrap();
        assert!(s.into_inner().finished());
    }

    #[test]
    fn rt_records_preserves_advanced_nibble_from_unknown_touch_mode() {
        // current mode byte 0x53: high nibble 5 (Unknown touch mode), low nibble 3 (advanced)
        let lines = [
            l("out", &cmds::read_key_layout(0x1A, layout::MODE)),
            l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0x53, 0x00]),
            ),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let recs = rt_records(&mut s, &[0x1A], Um(500), Um(600)).unwrap();
        assert_eq!(
            recs,
            vec![
                KeyRecord {
                    key: 0x1A,
                    layout: layout::MODE,
                    value: 0x33
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_PRESS,
                    value: 500
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_RELEASE,
                    value: 600
                },
            ]
        );
        assert!(s.into_inner().finished());
    }

    /// A key already carrying `RtContinuous` (nibble 4, the vendor UI's own toggle) must keep
    /// that variant when `wh set rt` only changes sensitivity: `wh` has no `--continuous` flag,
    /// so there's no way to ask for it back once lost. Current mode byte 0x48: touch
    /// RtContinuous(4), advanced nibble 8.
    #[test]
    fn rt_records_preserves_the_continuous_variant_when_the_key_already_has_it() {
        let lines = [
            l("out", &cmds::read_key_layout(0x1A, layout::MODE)),
            l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0x48, 0x00]),
            ),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let recs = rt_records(&mut s, &[0x1A], Um(700), Um(750)).unwrap();
        assert_eq!(
            recs,
            vec![
                KeyRecord {
                    key: 0x1A,
                    layout: layout::MODE,
                    value: 0x48, // still RtContinuous: only the sensitivity changed
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_PRESS,
                    value: 700
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_RELEASE,
                    value: 750
                },
            ]
        );
        assert!(s.into_inner().finished());
    }

    /// A key not already RT-enabled (touch mode `Global` here) gets plain `Rt` (nibble 3,
    /// continuous off), not `RtContinuous`: only an already-continuous key keeps that variant,
    /// so enabling RT fresh always starts from continuous off.
    #[test]
    fn rt_records_defaults_a_freshly_enabled_key_to_non_continuous_rt() {
        let lines = [
            l("out", &cmds::read_key_layout(0x1A, layout::MODE)),
            l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0x00, 0x00]),
            ),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let recs = rt_records(&mut s, &[0x1A], Um(500), Um(500)).unwrap();
        assert_eq!(recs[0].value, 0x30); // Rt, not RtContinuous
        assert!(s.into_inner().finished());
    }

    /// The reply's high byte (`payload[4]`, the wire byte `parse_key_reply` puts in
    /// `KeyRecord.value`'s upper bits) must survive a read-modify-write intact. Reply lo `0x21`
    /// (touch `Unknown(2)`, advanced `0x1`), hi `0x02`: `rt_records` forces touch to `Rt`
    /// (`0x3`) while preserving the advanced nibble and high byte, giving `0x0231`.
    ///
    /// The expected value is a hand-written literal, not built via `Mode { .. }.value()` again:
    /// reconstructing the expectation through the method under test would share any bug in it
    /// and assert nothing (see `wh-proto`'s
    /// `mode_round_trips_the_full_16_bit_value_including_a_non_zero_high_byte` test, which pins
    /// `Mode` the same way).
    #[test]
    fn rt_records_preserves_the_high_byte_of_mode_across_a_read_modify_write() {
        let lines = [
            l("out", &cmds::read_key_layout(0x1A, layout::MODE)),
            l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0x21, 0x02]),
            ),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let recs = rt_records(&mut s, &[0x1A], Um(500), Um(600)).unwrap();
        assert_eq!(
            recs,
            vec![
                KeyRecord {
                    key: 0x1A,
                    layout: layout::MODE,
                    value: 0x0231,
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_PRESS,
                    value: 500
                },
                KeyRecord {
                    key: 0x1A,
                    layout: layout::RT_RELEASE,
                    value: 600
                },
            ]
        );
        assert!(s.into_inner().finished());
    }

    #[test]
    fn rt_off_records_sets_touch_mode_single_preserving_advanced_nibble() {
        // Mode byte 0x37: touch Rt(3), advanced nibble 7. Must write Single(1), not Global(0):
        // `captures/rt-off-w.jsonl` shows the vendor writing nibble 1 in this exact transition.
        let lines = [
            l("out", &cmds::read_key_layout(0x1A, layout::MODE)),
            l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0x37, 0x00]),
            ),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let recs = rt_off_records(&mut s, &[0x1A]).unwrap();
        assert_eq!(
            recs,
            vec![KeyRecord {
                key: 0x1A,
                layout: layout::MODE,
                value: 0x17
            }]
        );
        assert!(s.into_inner().finished());
    }

    /// Sibling of the `rt_records` high-byte test above. Reply lo `0x37` (touch `Rt`, advanced
    /// `0x7`), hi `0x02`: `rt_off_records` forces touch to `Single` while preserving the
    /// advanced nibble and high byte, giving `0x0217`. Hand-written literal for the same reason.
    #[test]
    fn rt_off_records_preserves_the_high_byte_of_mode_across_a_read_modify_write() {
        let lines = [
            l("out", &cmds::read_key_layout(0x1A, layout::MODE)),
            l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0x37, 0x02]),
            ),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let recs = rt_off_records(&mut s, &[0x1A]).unwrap();
        assert_eq!(
            recs,
            vec![KeyRecord {
                key: 0x1A,
                layout: layout::MODE,
                value: 0x0217,
            }]
        );
        assert!(s.into_inner().finished());
    }

    /// A key already `Single` (not RT-enabled) must stay `Single` when turned "off" again, not
    /// get coerced to `Global`. Since `rt_off_records` skips unchanged keys, the correct output
    /// here is no record at all, not one echoing the value already on the board: a regression
    /// that coerced the key to `Global` would still produce a (wrong-value) record.
    #[test]
    fn rt_off_records_leaves_an_already_single_key_at_single_not_global() {
        let lines = [
            l("out", &cmds::read_key_layout(0x1A, layout::MODE)),
            l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0x18, 0x00]),
            ),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let recs = rt_off_records(&mut s, &[0x1A]).unwrap();
        assert_eq!(
            recs,
            vec![],
            "an already-Single key has nothing to change, so rt_off_records must not write it \
             back, and must especially not coerce it to Global (nibble 0)"
        );
        assert!(s.into_inner().finished());
    }

    /// Replays all 68 real per-key MODE values from `captures/initial-load.jsonl` (none of them
    /// RT-enabled, confirmed below) through `rt_off_records`, exactly what
    /// `wh set rt --keys all --off` sends. Guards against rewriting any of the 58 nibble-0 keys
    /// unconditionally, and against emitting a record that just echoes each key's unchanged
    /// value: the correct output is no records at all.
    #[test]
    fn rt_off_records_leaves_every_real_non_rt_key_from_initial_load_unchanged() {
        assert_eq!(REAL_MODES.len(), 68, "must be exactly the 68 keys captured");
        assert_eq!(
            REAL_MODES
                .iter()
                .filter(|&&(_, v)| matches!(
                    Mode::from_value(v).touch,
                    TouchMode::Rt | TouchMode::RtContinuous
                ))
                .count(),
            0,
            "none of the real captured keys have RT on; that is what makes this a regression test"
        );

        let mut lines = Vec::new();
        let usages: Vec<u8> = REAL_MODES.iter().map(|&(k, _)| k).collect();
        for &(k, v) in REAL_MODES {
            lines.push(l("out", &cmds::read_key_layout(k, layout::MODE)));
            lines.push(l(
                "in",
                &rf(
                    cmds::cmd::KEY,
                    &[0x00, k, layout::MODE, (v & 0xFF) as u8, (v >> 8) as u8],
                ),
            ));
        }
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let recs = rt_off_records(&mut s, &usages).unwrap();

        assert_eq!(
            recs,
            vec![],
            "none of these 68 real keys has RT on, so none has anything to change: \
             rt_off_records must write no records at all for this whole-board --off, not 68 \
             records that each echo the value already on the board (58 of them nibble 0)"
        );
        assert!(s.into_inner().finished());
    }

    #[test]
    fn set_rt_off_writes_mode_single_and_sends_no_save() {
        let rec = KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x10,
        };
        let batch = cmds::write_key_records(&[rec]);
        let mut lines = vec![
            l("out", &cmds::read_key_layout(0x1A, layout::MODE)),
            l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0x30, 0x00]),
            ),
        ];
        for f in &batch {
            lines.push(l("out", f));
            lines.push(l("in", &rf(cmds::cmd::KEY, &[0x01])));
        }
        // Script ends right after the write batch: a SAVE order afterwards would be rejected.
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        set_rt_off(&mut s, &[0x1A]).unwrap();
        assert!(s.into_inner().finished());
    }

    #[test]
    fn set_ap_writes_ap_records_and_sends_no_save() {
        // MODE reads back Single (0x18), so no MODE record joins the write batch.
        let recs = vec![KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 1500,
        }];
        let batch = cmds::write_key_records(&recs);
        let mut lines = vec![
            l("out", &cmds::read_key_layout(0x1A, layout::MODE)),
            l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0x18, 0x00]),
            ),
        ];
        for f in &batch {
            lines.push(l("out", f));
            lines.push(l("in", &rf(cmds::cmd::KEY, &[0x01])));
        }
        // Script ends right after the write batch: a SAVE order afterwards would be rejected.
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        set_ap(&mut s, &[0x1A], Um(1500)).unwrap();
        assert!(s.into_inner().finished());
    }

    #[test]
    fn ap_records_builds_ap_and_mode_records_across_multiple_keys() {
        // Both keys read back Single (0x18), so each contributes only its AP record.
        let mut lines = Vec::new();
        for &u in &[0x04u8, 0x05u8] {
            lines.push(l("out", &cmds::read_key_layout(u, layout::MODE)));
            lines.push(l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, u, layout::MODE, 0x18, 0x00]),
            ));
        }
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let plan = ap_records(&mut s, &[0x04, 0x05], Um(1200)).unwrap();
        assert_eq!(
            plan.records,
            vec![
                KeyRecord {
                    key: 0x04,
                    layout: layout::AP,
                    value: 1200
                },
                KeyRecord {
                    key: 0x05,
                    layout: layout::AP,
                    value: 1200
                },
            ]
        );
        assert!(s.into_inner().finished());
    }

    /// Every key on the board with its MODE value, verbatim from `captures/initial-load.jsonl`, in
    /// ascending usage order. The real board state two tests below are measured against.
    const REAL_MODES: &[(u8, u16)] = &[
        (0x01, 0x0010),
        (0x04, 0x0018),
        (0x05, 0x0000),
        (0x06, 0x0000),
        (0x07, 0x0018),
        (0x08, 0x0000),
        (0x09, 0x0000),
        (0x0A, 0x0000),
        (0x0B, 0x0000),
        (0x0C, 0x0000),
        (0x0D, 0x0000),
        (0x0E, 0x0000),
        (0x0F, 0x0000),
        (0x10, 0x0000),
        (0x11, 0x0000),
        (0x12, 0x0000),
        (0x13, 0x0000),
        (0x14, 0x0000),
        (0x15, 0x0000),
        (0x16, 0x0018),
        (0x17, 0x0000),
        (0x18, 0x0000),
        (0x19, 0x0000),
        (0x1A, 0x0018),
        (0x1B, 0x0000),
        (0x1C, 0x0000),
        (0x1D, 0x0000),
        (0x1E, 0x0000),
        (0x1F, 0x0000),
        (0x20, 0x0000),
        (0x21, 0x0000),
        (0x22, 0x0000),
        (0x23, 0x0000),
        (0x24, 0x0000),
        (0x25, 0x0000),
        (0x26, 0x0000),
        (0x27, 0x0000),
        (0x28, 0x0000),
        (0x29, 0x0010),
        (0x2A, 0x0000),
        (0x2B, 0x0000),
        (0x2C, 0x0000),
        (0x2D, 0x0000),
        (0x2E, 0x0000),
        (0x2F, 0x0000),
        (0x30, 0x0000),
        (0x31, 0x0000),
        (0x33, 0x0000),
        (0x34, 0x0000),
        (0x36, 0x0000),
        (0x37, 0x0000),
        (0x38, 0x0000),
        (0x39, 0x0000),
        (0x4F, 0x0000),
        (0x50, 0x0000),
        (0x51, 0x0000),
        (0x52, 0x0000),
        (0xD6, 0x0010),
        (0xE0, 0x0000),
        (0xE1, 0x0000),
        (0xE2, 0x0000),
        (0xE3, 0x0000),
        (0xE4, 0x0000),
        (0xE5, 0x0000),
        (0xE6, 0x0000),
        (0xFA, 0x0010),
        (0xFB, 0x0010),
        (0xFC, 0x0010),
    ];

    /// One key's MODE-only read roundtrip, mirroring `rt_records`' own script shape above.
    fn mode_read_script(usage: u8, mode_lo: u8, mode_hi: u8) -> Vec<String> {
        vec![
            l("out", &cmds::read_key_layout(usage, layout::MODE)),
            l(
                "in",
                &rf(
                    cmds::cmd::KEY,
                    &[0x00, usage, layout::MODE, mode_lo, mode_hi],
                ),
            ),
        ]
    }

    #[test]
    fn ap_records_promotes_a_global_key_to_single() {
        // touch nibble 0 (Global), advanced 8. MODE must come before AP, matching the one
        // ordering measured on hardware (captures/ap-wasd-1.2.jsonl).
        let lines = mode_read_script(0x09, 0x08, 0x00).join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let plan = ap_records(&mut s, &[0x09], Um(300)).unwrap();
        assert_eq!(
            plan.records,
            vec![
                KeyRecord {
                    key: 0x09,
                    layout: layout::MODE,
                    value: 0x18
                },
                KeyRecord {
                    key: 0x09,
                    layout: layout::AP,
                    value: 300
                },
            ]
        );
        assert!(s.into_inner().finished());
    }

    #[test]
    fn ap_records_writes_no_mode_when_the_key_is_already_single() {
        let lines = mode_read_script(0x09, 0x18, 0x00).join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let plan = ap_records(&mut s, &[0x09], Um(300)).unwrap();
        assert_eq!(
            plan.records,
            vec![KeyRecord {
                key: 0x09,
                layout: layout::AP,
                value: 300
            }]
        );
        assert_eq!(plan.prior_mode(0x09), Some(0x18));
        assert!(s.into_inner().finished());
    }

    /// The regression this rule exists for. A key with rapid trigger on (nibble 3) keeps nibble
    /// 3. Forcing nibble 1 would silently disable rapid trigger, and whether the vendor does
    /// that is unmeasured: every key in `captures/ap-wasd-1.2.jsonl` was already on nibble 1
    /// before the write, so the capture cannot distinguish forcing from preserving. An RT key
    /// still carries its own actuation point (`captures/rt-on-w-0.5.jsonl` writes nibble 3 and
    /// keeps 0x04=300), so a depth change is not a request to turn rapid trigger off.
    #[test]
    fn ap_records_never_clears_rapid_trigger() {
        for raw in [0x38u8, 0x48] {
            let lines = mode_read_script(0x1A, raw, 0x00).join("\n");
            let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
            let plan = ap_records(&mut s, &[0x1A], Um(800)).unwrap();
            assert_eq!(
                plan.records,
                vec![KeyRecord {
                    key: 0x1A,
                    layout: layout::AP,
                    value: 800
                }],
                "MODE {raw:#04x} must be left alone"
            );
            assert_eq!(
                plan.prior_mode(0x1A),
                Some(raw as u16),
                "the pre-write MODE must be handed back so a caller can verify it did not change"
            );
            assert!(s.into_inner().finished());
        }
    }

    /// An unobserved touch nibble is left exactly as found. Nibble 5 has never been seen on
    /// hardware, so overwriting it would discard state we cannot interpret.
    #[test]
    fn ap_records_leaves_an_unknown_touch_nibble_alone() {
        // MODE 0x0250: touch nibble 5, which maps to TouchMode::Unknown(5).
        let lines = mode_read_script(0x1A, 0x50, 0x02).join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let plan = ap_records(&mut s, &[0x1A], Um(1000)).unwrap();
        assert_eq!(
            plan.records,
            vec![KeyRecord {
                key: 0x1A,
                layout: layout::AP,
                value: 1000
            }],
            "an unknown touch nibble must yield no MODE record"
        );
        assert!(s.into_inner().finished());
    }

    /// A key following the board's global rapid trigger (nibble 2) has rapid trigger on. An
    /// actuation point change must not write MODE for it, or the depth change would silently
    /// turn rapid trigger off across every key the global switch covers.
    #[test]
    fn ap_records_leaves_the_global_rapid_trigger_nibble_alone() {
        let lines = mode_read_script(0x1A, 0x20, 0x00).join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let plan = ap_records(&mut s, &[0x1A], Um(1000)).unwrap();
        assert_eq!(
            plan.records,
            vec![KeyRecord {
                key: 0x1A,
                layout: layout::AP,
                value: 1000
            }],
            "TouchMode::RtGlobal must yield no MODE record"
        );
        assert!(s.into_inner().finished());
    }

    /// Nibble 2 is rapid trigger on, following the global settings. Reporting it as off is what
    /// `wh dump` did before the global rapid trigger switch was ever captured.
    #[test]
    fn rt_enabled_is_true_for_the_global_rapid_trigger_nibble() {
        let ks = KeySettings {
            usage: 0x1A,
            ap: Um(2000),
            mode: Mode::from_value(0x20),
            rt_press: Um(200),
            rt_release: Um(200),
            ap_keyset: 0,
            rt_keyset: 0,
        };
        assert_eq!(ks.mode.touch, TouchMode::RtGlobal);
        assert!(
            ks.rt_enabled(),
            "a key at touch nibble 2 has rapid trigger on"
        );
    }

    /// `--off` on a key following the global rapid trigger must actually turn it off. It writes
    /// nibble 1, the value the vendor itself writes when the global switch goes off
    /// (`captures/ks-global-rt-off.jsonl`).
    #[test]
    fn rt_off_records_turns_off_a_key_following_the_global_rapid_trigger() {
        // MODE 0x0028: touch nibble 2, advanced nibble 8, as measured on W.
        let lines = mode_read_script(0x1A, 0x28, 0x00).join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let recs = rt_off_records(&mut s, &[0x1A]).unwrap();
        assert_eq!(
            recs,
            vec![KeyRecord {
                key: 0x1A,
                layout: layout::MODE,
                value: 0x18,
            }],
            "nibble 2 must be written down to nibble 1, preserving the advanced nibble"
        );
        assert!(s.into_inner().finished());
    }

    /// The advanced nibble and the high byte are preserved when the touch nibble is promoted.
    #[test]
    fn ap_records_preserves_the_advanced_nibble_and_high_byte() {
        // high byte 0x27, touch 0, advanced 5
        let lines = mode_read_script(0x09, 0x05, 0x27).join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let plan = ap_records(&mut s, &[0x09], Um(300)).unwrap();
        assert_eq!(
            plan.records[0].value, 0x2715,
            "only the touch nibble may change"
        );
        assert!(s.into_inner().finished());
    }

    /// Pins the partial-failure figures `ap_records`' doc comment quotes, so the documented risk
    /// cannot drift from what the code does: a whole-board `wh set ap` over the real captured
    /// modes is 126 records in 9 frames, and some keys have their MODE record in a different frame
    /// from their AP record, which is what a mid-batch failure would strand.
    ///
    /// `REAL_MODES` is in ascending usage order, which splits 5 keys. `--keys all` sends in DEFKEY
    /// matrix order instead, which splits 4: the count depends on the order, the existence of the
    /// split does not.
    #[test]
    fn ap_records_over_the_whole_captured_board_splits_keys_across_write_frames() {
        let mut lines = Vec::new();
        let usages: Vec<u8> = REAL_MODES.iter().map(|&(k, _)| k).collect();
        for &(k, v) in REAL_MODES {
            lines.extend(mode_read_script(k, (v & 0xFF) as u8, (v >> 8) as u8));
        }
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let plan = ap_records(&mut s, &usages, Um(1200)).unwrap();
        assert!(s.into_inner().finished());

        assert_eq!(plan.records.len(), 126);
        let frames = cmds::write_key_records(&plan.records);
        assert_eq!(frames.len(), 9);

        // Which frame each record landed in, by its position in the flat record list.
        let frame_of = |i: usize| i / cmds::MAX_RECORDS_PER_REPORT;
        let split = usages
            .iter()
            .filter(|&&u| {
                let mode = plan
                    .records
                    .iter()
                    .position(|r| r.key == u && r.layout == layout::MODE);
                let ap = plan
                    .records
                    .iter()
                    .position(|r| r.key == u && r.layout == layout::AP)
                    .expect("every key gets an AP record");
                mode.is_some_and(|m| frame_of(m) != frame_of(ap))
            })
            .count();
        assert_eq!(
            split, 5,
            "in usage order 5 keys have MODE and AP in different frames; a mid-batch failure \
             strands exactly those"
        );
    }

    #[test]
    fn write_records_uses_roundtrip_many_so_mid_batch_failure_reports_progress() {
        // 16 AP records split into a 14-record and a 2-record frame; only the first gets a
        // reply, so this must surface as a Batch error with partial-progress detail, not a
        // bare Timeout. Every key reads back Single (0x18), so no MODE records join the batch.
        let usages: Vec<u8> = (0x04u8..0x14).collect();
        let mut lines = Vec::new();
        for &u in &usages {
            lines.push(l("out", &cmds::read_key_layout(u, layout::MODE)));
            lines.push(l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, u, layout::MODE, 0x18, 0x00]),
            ));
        }
        let records: Vec<KeyRecord> = usages
            .iter()
            .map(|&u| KeyRecord {
                key: u,
                layout: layout::AP,
                value: 1500,
            })
            .collect();
        let frames = cmds::write_key_records(&records);
        assert_eq!(frames.len(), 2);
        lines.push(l("out", &frames[0]));
        lines.push(l("in", &rf(cmds::cmd::KEY, &[0x01])));
        lines.push(l("out", &frames[1]));
        // no reply for frames[1]: script ends here
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let err = set_ap(&mut s, &usages, Um(1500)).unwrap_err();
        match err {
            DeviceError::Batch {
                index,
                total,
                applied,
                ..
            } => {
                assert_eq!(index, 1);
                assert_eq!(total, 2);
                assert_eq!(applied, 1);
            }
            other => panic!("expected Batch, got {other:?}"),
        }
    }

    #[test]
    fn write_records_sends_nothing_when_there_are_no_records() {
        // Empty script: any send at all would be rejected. Pins the resulting wire behaviour,
        // not the `is_empty` early return itself: `write_key_records(&[])` already yields no
        // frames on its own, but the guard documents the no-op explicitly.
        let mut s = Session::new(ReplayTransport::from_jsonl("").unwrap());
        set_ap(&mut s, &[], Um(1500)).unwrap();
        assert!(s.into_inner().finished());
    }

    #[test]
    fn read_layout_value_rejects_reply_echoing_wrong_key() {
        let lines = [
            l("out", &cmds::read_key_layout(0x1A, layout::AP)),
            l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, 0x1B, layout::AP, 0xB0, 0x04]),
            ), // echoes key 0x1B, not 0x1A
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let err = read_key_settings(&mut s, 0x1A).unwrap_err();
        assert!(
            matches!(err, DeviceError::Decode(_)),
            "expected Decode, got {err:?}"
        );
    }

    #[test]
    fn read_layout_value_rejects_reply_echoing_wrong_layout() {
        let lines = [
            l("out", &cmds::read_key_layout(0x1A, layout::AP)),
            l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0xB0, 0x04]),
            ), // echoes layout MODE, not AP
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let err = read_key_settings(&mut s, 0x1A).unwrap_err();
        assert!(
            matches!(err, DeviceError::Decode(_)),
            "expected Decode, got {err:?}"
        );
    }

    #[test]
    fn read_key_settings_reads_six_layouts() {
        // Press and release deliberately distinct (500 vs 650): equal values, or asserting
        // only one field, couldn't catch the pair being swapped.
        let mut lines = Vec::new();
        for (lid, val) in [
            (layout::AP, 1200u16),
            (layout::MODE, 0x30),
            (layout::RT_PRESS, 500),
            (layout::RT_RELEASE, 650),
            (layout::KEYSET_AP, 1),
            (layout::KEYSET_RT, 0),
        ] {
            lines.push(l("out", &cmds::read_key_layout(0x1A, lid)));
            lines.push(l(
                "in",
                &rf(
                    cmds::cmd::KEY,
                    &[0x00, 0x1A, lid, (val & 0xFF) as u8, (val >> 8) as u8],
                ),
            ));
        }
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let ks = read_key_settings(&mut s, 0x1A).unwrap();
        assert_eq!(ks.ap, Um(1200));
        assert!(ks.rt_enabled());
        assert_eq!(ks.rt_press, Um(500));
        assert_eq!(ks.rt_release, Um(650));
        // Proves all six scripted reads were actually sent, not just the first four: a
        // regression that stopped reading after RT_RELEASE would still pass every assertion
        // above, leaving the last two script lines unconsumed.
        assert!(s.into_inner().finished());
    }

    /// `read_key_settings` reads six layouts per key, in order AP, MODE, RT_PRESS, RT_RELEASE,
    /// KEYSET_AP, KEYSET_RT. The two keyset values are carried raw, with no interpretation,
    /// because whether 0xFE is a boolean or an index is unmeasured.
    #[test]
    fn read_key_settings_reads_both_keyset_layouts() {
        let mut lines = Vec::new();
        for (lid, val) in [
            (layout::AP, 300u16),
            (layout::MODE, 0x18),
            (layout::RT_PRESS, 100),
            (layout::RT_RELEASE, 100),
            (layout::KEYSET_AP, 1),
            (layout::KEYSET_RT, 0),
        ] {
            lines.push(l("out", &cmds::read_key_layout(0x1A, lid)));
            lines.push(l(
                "in",
                &rf(
                    cmds::cmd::KEY,
                    &[0x00, 0x1A, lid, (val & 0xFF) as u8, (val >> 8) as u8],
                ),
            ));
        }
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let ks = read_key_settings(&mut s, 0x1A).unwrap();
        assert_eq!(ks.ap, Um(300));
        assert_eq!(ks.ap_keyset, 1);
        assert_eq!(ks.rt_keyset, 0);
        assert!(s.into_inner().finished());
    }

    #[test]
    fn read_key_settings_maps_decode_failure_to_decode_error_not_timeout() {
        let bad_reply = rf(cmds::cmd::KEY, &[0x00]); // too short to decode
        let lines = [
            l("out", &cmds::read_key_layout(0x1A, layout::AP)),
            l("in", &bad_reply),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let err = read_key_settings(&mut s, 0x1A).unwrap_err();
        assert!(
            matches!(err, DeviceError::Decode(_)),
            "expected Decode, got {err:?}"
        );
    }

    #[test]
    fn device_info_reads_sync_reply() {
        let mut payload = vec![0u8; 60];
        payload[8] = 16; // serial length prefix
        payload[9..25].copy_from_slice(b"SN0123456789ABCD");
        payload[25] = 10; // firmware length prefix
        payload[26..36].copy_from_slice(b"V1.2.3.456");
        let lines = [
            l("out", &cmds::sync()),
            l("in", &rf(cmds::cmd::SYNC, &payload)),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let info = device_info(&mut s).unwrap();
        assert_eq!(info.serial, "SN0123456789ABCD");
        assert_eq!(info.firmware, "V1.2.3.456");
    }

    #[test]
    fn device_info_maps_decode_failure_to_decode_error_not_timeout() {
        let bad_reply = rf(cmds::cmd::SYNC, &[0x00]); // too short to decode
        let lines = [l("out", &cmds::sync()), l("in", &bad_reply)].join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let err = device_info(&mut s).unwrap_err();
        assert!(
            matches!(err, DeviceError::Decode(_)),
            "expected Decode, got {err:?}"
        );
    }

    #[test]
    fn global_travel_reads_reply() {
        let payload = [0x00, 0, 0, 0xF4, 0x01, 0xC8, 0x00, 0x64, 0x00];
        let lines = [
            l("out", &cmds::read_global_travel()),
            l("in", &rf(cmds::cmd::DB, &payload)),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let g = global_travel(&mut s).unwrap();
        assert_eq!(g.travel, Um(500));
        assert_eq!(g.press_dead, Um(200));
        assert_eq!(g.release_dead, Um(100));
    }

    #[test]
    fn global_travel_maps_decode_failure_to_decode_error_not_timeout() {
        let bad_reply = rf(cmds::cmd::DB, &[0x00]); // too short to decode
        let lines = [l("out", &cmds::read_global_travel()), l("in", &bad_reply)].join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let err = global_travel(&mut s).unwrap_err();
        assert!(
            matches!(err, DeviceError::Decode(_)),
            "expected Decode, got {err:?}"
        );
    }

    #[test]
    fn profile_reads_the_zero_based_index_from_the_reply() {
        let lines = [
            l("out", &cmds::read_profile()),
            l("in", &rf(cmds::cmd::CMD, &[0x00, 0x70, 0x01, 0xFF])),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        assert_eq!(profile(&mut s).unwrap().wire_index(), 1);
        assert!(s.into_inner().finished());
    }

    #[test]
    fn profile_maps_decode_failure_to_decode_error_not_timeout() {
        let bad_reply = rf(cmds::cmd::CMD, &[0x00]); // too short to decode
        let lines = [l("out", &cmds::read_profile()), l("in", &bad_reply)].join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let err = profile(&mut s).unwrap_err();
        assert!(
            matches!(err, DeviceError::Decode(_)),
            "expected Decode, got {err:?}"
        );
    }

    /// A reply that parses fine but names an index the board's four profiles could never
    /// produce must surface as `ProfileOutOfRange`, not `Decode`, so a caller can degrade only
    /// for the impossible-profile case, not a genuinely garbled reply.
    #[test]
    fn profile_maps_an_out_of_range_index_to_profile_out_of_range_not_decode() {
        let lines = [
            l("out", &cmds::read_profile()),
            l("in", &rf(cmds::cmd::CMD, &[0x00, 0x70, 0xFE, 0xFF])),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let err = profile(&mut s).unwrap_err();
        assert!(
            matches!(err, DeviceError::ProfileOutOfRange(0xFE)),
            "expected ProfileOutOfRange(0xFE), got {err:?}"
        );
    }

    /// A select is confirmed by re-reading, not by trusting the ack: `set_profile` must send
    /// exactly `cmds::select_profile`, then a fresh `cmds::read_profile`, in that order.
    #[test]
    fn set_profile_selects_then_reads_back() {
        let target = cmds::ProfileNumber::from_one_based(2).unwrap();
        let lines = [
            l("out", &cmds::select_profile(target)),
            l("in", &rf(cmds::cmd::CMD, &[0x00, 0x70])),
            l("out", &cmds::read_profile()),
            l("in", &rf(cmds::cmd::CMD, &[0x00, 0x70, 0x01, 0xFF])),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let got = set_profile(&mut s, target).unwrap();
        assert_eq!(got.one_based(), 2);
        assert!(s.into_inner().finished());
    }

    /// `Session::roundtrip` matches only the command byte, and `captures/profile-switch.jsonl`
    /// shows cmd `0x00` replies for sub-orders `0x50`, `0x22`, `0xbb` and others arriving on that
    /// same byte. An ack whose sub-order is not `order::PROFILE` is therefore someone else's
    /// reply and must be rejected, before the re-read below can paper over it.
    #[test]
    fn set_profile_rejects_an_ack_for_a_different_sub_order() {
        let target = cmds::ProfileNumber::from_one_based(2).unwrap();
        let lines = [
            l("out", &cmds::select_profile(target)),
            l("in", &rf(cmds::cmd::CMD, &[0x00, 0x50, 0x00])), // order::POLLING, not PROFILE
            // A re-read would find the right profile anyway; the run must still fail on the ack.
            l("out", &cmds::read_profile()),
            l("in", &rf(cmds::cmd::CMD, &[0x00, 0x70, 0x01, 0xFF])),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let err = set_profile(&mut s, target).unwrap_err();
        let msg = err.to_string();
        assert!(
            matches!(err, DeviceError::Decode(_)) && msg.contains("0x50") && msg.contains("0x70"),
            "expected a Decode naming both sub-orders, got {err:?}"
        );
    }

    /// An ack too short to carry a sub-order byte at all is rejected the same way, rather than
    /// indexing past the end of the payload.
    #[test]
    fn set_profile_rejects_an_ack_with_no_sub_order_byte() {
        let target = cmds::ProfileNumber::from_one_based(2).unwrap();
        let lines = [
            l("out", &cmds::select_profile(target)),
            l("in", &rf(cmds::cmd::CMD, &[0x00])),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let err = set_profile(&mut s, target).unwrap_err();
        assert!(
            matches!(err, DeviceError::Decode(_)),
            "expected Decode, got {err:?}"
        );
    }

    /// If the board reports a different profile after the select, that is a failure, not a
    /// success to report cheerfully: the ack alone never proves the switch landed.
    #[test]
    fn set_profile_errors_when_the_board_lands_on_a_different_profile() {
        let target = cmds::ProfileNumber::from_one_based(2).unwrap();
        let lines = [
            l("out", &cmds::select_profile(target)),
            l("in", &rf(cmds::cmd::CMD, &[0x00, 0x70])),
            l("out", &cmds::read_profile()),
            l("in", &rf(cmds::cmd::CMD, &[0x00, 0x70, 0x00, 0xFF])),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        assert!(set_profile(&mut s, target).is_err());
    }

    /// Writes the global travel DB record first, then the per-key batch(es), no SAVE
    /// afterwards, mirroring `set_rt`/`set_ap`'s own script shape above.
    #[test]
    fn restore_all_writes_global_travel_then_key_batches_and_sends_no_save() {
        let global = cmds::GlobalTravel {
            travel: Um(2000),
            press_dead: Um(100),
            release_dead: Um(150),
        };
        // Two keys, each with distinct ap/mode/press/release values, so a swapped field or a
        // key mix-up would show up as a wrong assertion rather than passing by coincidence.
        let recs = vec![
            KeyRecord {
                key: 0x1A,
                layout: layout::AP,
                value: 1200,
            },
            KeyRecord {
                key: 0x1A,
                layout: layout::MODE,
                value: 0x20,
            },
            KeyRecord {
                key: 0x1A,
                layout: layout::RT_PRESS,
                value: 500,
            },
            KeyRecord {
                key: 0x1A,
                layout: layout::RT_RELEASE,
                value: 650,
            },
            KeyRecord {
                key: 0x04,
                layout: layout::AP,
                value: 1500,
            },
            KeyRecord {
                key: 0x04,
                layout: layout::MODE,
                value: 0x00,
            },
            KeyRecord {
                key: 0x04,
                layout: layout::RT_PRESS,
                value: 0,
            },
            KeyRecord {
                key: 0x04,
                layout: layout::RT_RELEASE,
                value: 0,
            },
        ];

        let db_write =
            cmds::write_global_travel(global.travel, global.press_dead, global.release_dead);
        let batches = cmds::write_key_records(&recs);

        let mut lines = vec![
            l("out", &db_write),
            l("in", &rf(cmds::cmd::DB, &[0x01, 0, 0])),
        ];
        for f in &batches {
            lines.push(l("out", f));
            lines.push(l("in", &rf(cmds::cmd::KEY, &[0x01])));
        }
        // Script ends right after the key batches: a SAVE order afterwards would be rejected.

        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        restore_all(&mut s, &global, &recs).unwrap();
        assert!(s.into_inner().finished());
    }

    #[test]
    fn restore_all_skips_key_batch_when_there_are_no_records() {
        // Only the global travel write should reach the wire; an empty `records` produces no
        // key batch frames. Pins the resulting wire behaviour, not a specific early-return
        // branch in `write_records`.
        let global = cmds::GlobalTravel {
            travel: Um(2000),
            press_dead: Um(100),
            release_dead: Um(150),
        };
        let db_write =
            cmds::write_global_travel(global.travel, global.press_dead, global.release_dead);
        let lines = [
            l("out", &db_write),
            l("in", &rf(cmds::cmd::DB, &[0x01, 0, 0])),
        ]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        restore_all(&mut s, &global, &[]).unwrap();
        assert!(s.into_inner().finished());
    }
}
