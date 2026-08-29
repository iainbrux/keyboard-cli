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
}

impl KeySettings {
    /// True for either RT variant, continuous or not: `TouchMode::RtContinuous` is still rapid
    /// trigger from the CLI's point of view, just with the device's own continuous-mode toggle
    /// (not something `wh` exposes) left on. See `rt_records`' own comment for why the CLI
    /// preserves that variant on a read-modify-write instead of collapsing it to plain `Rt`.
    pub fn rt_enabled(&self) -> bool {
        matches!(self.mode.touch, TouchMode::Rt | TouchMode::RtContinuous)
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
    Ok(KeySettings {
        usage,
        ap,
        mode,
        rt_press,
        rt_release,
    })
}

/// Read one key's layout value, rejecting a reply that doesn't echo back the
/// same key and layout id it was asked for. `Session::roundtrip` only matches
/// on the command byte (0x23 for every per-key read and write ack alike), so
/// a late or duplicated report could otherwise satisfy the wrong request and
/// silently apply key A's value to key B.
fn read_layout_value<T: Transport>(
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

/// Write `records` in batches. A no-op selection (empty `records`) returns immediately without
/// writing anything.
///
/// No SAVE order follows the batch: across 1224 captured frames covering ten scenarios and
/// five complete write sequences, the vendor web configurator never sends one (`cmds::order::SAVE`'s
/// own comment in `wh-proto`). Either the board persists automatically, or order `0x02` means
/// something else on this firmware, or it is unimplemented; that is unmeasured, so this function
/// does not send it and does not claim the board persists automatically.
///
/// Uses `roundtrip_many` rather than a hand-rolled send loop so a mid-batch failure reports how
/// many frames already reached the device (`DeviceError::Batch`) instead of a bare timeout.
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

/// Build the [mode, rt_press, rt_release] records to enable RT on `usages`,
/// preserving each key's advanced-mode nibble. Reads current MODE per key but
/// sends nothing else, so a caller can inspect the records for a dry run
/// before deciding to write them.
///
/// The touch nibble written is `Rt` unless the key already carries `RtContinuous`, in which
/// case that variant is preserved rather than collapsed to plain `Rt`. The CLI has no
/// `--continuous` flag (that is a later phase's feature decision, not a protocol correction),
/// so there is no way for a `wh set rt` call to ask for continuous mode; but a key can already
/// be in that state from the vendor's own web UI, and `wh set rt --keys w --set 0.5` is a
/// sensitivity change, not a request to also turn continuous off. Writing `Rt` unconditionally
/// would do exactly that silently, on every sensitivity tweak, which is the same class of data
/// loss chunk 3 fixes for `rt_off_records`. Reading the current MODE before deciding what to
/// write, the same read-modify-write that already preserves `advanced` and `high`, costs
/// nothing extra here since that read already happens.
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

/// Build the [mode] records to turn rapid trigger off on `usages`, preserving each key's
/// advanced-mode nibble. Reads current MODE per key but sends nothing else.
///
/// Only touches keys that actually have RT on (`TouchMode::Rt` or `RtContinuous`): those get
/// rewritten to `TouchMode::Single` (nibble 1, per-key actuation point), never to `Global`
/// (nibble 0). A key already in `Global`, `Single`, or an `Unknown` state is left exactly as
/// read; a key with no RT to turn off has nothing for `set rt --off` to do to its mode, and gets
/// no record at all (whole-branch review): the recomputed value equals what was just read, and a
/// key with nothing to change gets nothing written, rather than a MODE record whose value is
/// identical to the one already on the board. This matters at scale: `wh set rt --keys all --off`
/// against a board with only a handful of RT keys used to write one record per selected key
/// regardless, most of them a no-op; the returned `Vec` (and so `set_rt_off`'s write, and
/// `verify_rt_off`'s readback and its reported count) now reflects only the keys that actually
/// change.
///
/// Measured on the real device (`captures/rt-off-w.jsonl`, task 19b chunk 3): turning RT off
/// wrote MODE nibble 1, not 0. Nibble 0 means "follow the global travel setting" instead of this
/// key's own layout `0x04` value; the original reasoning here was that writing it would silently
/// make a per-key actuation point inert, but that was never measured, and a later hardware check
/// (`docs/tasks.md`, the touch-nibble-0 actuation test) found a key at nibble 0 honouring its own
/// per-key actuation point, the opposite of what that reasoning predicted. What nibble 0 actually
/// does to a key's `0x04` value is unmeasured either way. This function writes nibble 1 anyway,
/// because that is what the vendor was observed writing in this exact situation, and matching it
/// costs nothing regardless of which belief about nibble 0 turns out to be right. But that
/// capture covers exactly one transition, an RT key with an AP going to Single; no capture
/// anywhere in the ten scenarios shows a nibble-0 (Global) key being turned "off" into nibble 1,
/// and doing that unconditionally (the first cut of this function did) detaches every non-RT key
/// on the board from the global travel setting on a plain `wh set rt --keys all --off`, a second
/// data-loss bug of the same shape chunk 3 fixed. Restricting the rewrite to keys that were
/// actually `Rt`/`RtContinuous` closes both that case and the `Unknown(n)` case at once, rather
/// than special-casing `Global` alone: a key without RT, in whatever state, has no RT to turn
/// off.
pub fn rt_off_records<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
) -> Result<Vec<KeyRecord>, DeviceError> {
    let mut records = Vec::new();
    for &u in usages {
        let cur_value = read_layout_value(s, u, layout::MODE)?;
        let cur_mode = Mode::from_value(cur_value);
        let touch = match cur_mode.touch {
            TouchMode::Rt | TouchMode::RtContinuous => TouchMode::Single,
            other => other,
        };
        let mode = Mode {
            touch,
            advanced: cur_mode.advanced,
            high: cur_mode.high,
        };
        let new_value = mode.value();
        // Skip the record entirely when nothing would change (review, whole-branch pass): a key
        // that was not `Rt`/`RtContinuous` recomputes to the exact value it was just read as, and
        // sending it anyway means writing a MODE value nobody has ever observed the vendor send
        // in this situation, nibble 0 (`Global`) included, which `docs/protocol.md` documents as
        // something `wh` does not write. The vendor was never once observed writing nibble 0
        // across 1224 captured frames; sending it unconditionally here, on every non-RT key of
        // every `wh set rt --keys all --off`, contradicted that. Same reasoning as not sending
        // SAVE: do not write a byte the vendor was never observed writing.
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

/// Enable RT on `usages` with the given sensitivities (preserves advanced-key nibble).
/// Returns the exact records that were written (one MODE/RT_PRESS/RT_RELEASE triple per key,
/// in `usages` order), so a caller that needs to verify the write can compare against what was
/// actually sent, advanced nibble and high byte included, rather than only the touch mode and
/// the two sensitivities.
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

/// Disable RT (touch mode -> Single, per-key actuation point), preserving the advanced
/// nibble. Returns the exact records that were written, in `usages` order, the same reason
/// `set_rt` returns its records: a caller verifying the write needs to compare against what
/// was actually sent, advanced nibble and high byte included, not just the touch mode. Not
/// necessarily one record per key in `usages`: a key with nothing to change (see
/// `rt_off_records`) contributes no record at all, so the caller's own reporting reflects how
/// many keys actually changed, not how many were selected.
pub fn set_rt_off<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
) -> Result<Vec<KeyRecord>, DeviceError> {
    let records = rt_off_records(s, usages)?;
    write_records(s, &records)?;
    Ok(records)
}

/// Build the [ap] records to set `usages`' actuation point (layout DB0).
/// Reads nothing (unlike `rt_records`/`rt_off_records`, AP has no other
/// nibble to preserve), so it needs no session; the CLI's dry-run path can
/// call this directly instead of re-deriving the record shape itself.
pub fn ap_records(usages: &[u8], depth: Um) -> Vec<KeyRecord> {
    usages
        .iter()
        .map(|&u| KeyRecord {
            key: u,
            layout: layout::AP,
            value: depth.0,
        })
        .collect()
}

/// Per-key actuation point (layout DB0).
pub fn set_ap<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
    depth: Um,
) -> Result<(), DeviceError> {
    write_records(s, &ap_records(usages, depth))
}

pub fn device_info<T: Transport>(s: &mut Session<T>) -> Result<cmds::DeviceInfo, DeviceError> {
    let payload = s.roundtrip(&cmds::sync())?;
    cmds::parse_sync(&payload).map_err(|e| DeviceError::Decode(e.to_string()))
}

pub fn global_travel<T: Transport>(s: &mut Session<T>) -> Result<cmds::GlobalTravel, DeviceError> {
    let payload = s.roundtrip(&cmds::read_global_travel())?;
    cmds::parse_global_travel(&payload).map_err(|e| DeviceError::Decode(e.to_string()))
}

/// The board's currently active profile, already validated (see `wh_proto::cmds::ProfileNumber`
/// and `parse_profile`, the seam where the wire's own zero-based index becomes this type). Read
/// only, task 19b group B: profile *select* is documented in the brief but deliberately not
/// implemented, since nothing in Phase 1 needs to change the active profile.
///
/// A reply naming an index the board's four measured profiles could never produce surfaces as
/// `DeviceError::ProfileOutOfRange`, kept distinct from `DeviceError::Decode` (a reply that does
/// not look like a profile reply at all), so a caller that wants to degrade gracefully on the
/// former while still hard-failing on the latter can match on the specific variant.
pub fn profile<T: Transport>(s: &mut Session<T>) -> Result<cmds::ProfileNumber, DeviceError> {
    let payload = s.roundtrip(&cmds::read_profile())?;
    cmds::parse_profile(&payload).map_err(|e| match e {
        cmds::DecodeError::ProfileOutOfRange(idx) => DeviceError::ProfileOutOfRange(idx),
        other => DeviceError::Decode(other.to_string()),
    })
}

/// Write a whole snapshot back to the board: global travel first, then every per-key record
/// (via `write_records`, which skips the batch entirely when `records` is empty). Global travel
/// goes first so a partial restore that fails partway through the per-key batch still leaves the
/// board's overall travel consistent with what the caller intended, rather than a mix of old
/// per-key values against a new global travel.
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
    /// Builds a reply frame the way the real device sends it: with the high
    /// bit set on the command byte (see `wh_proto::frame::REPLY_BIT`), so
    /// fixtures built through this helper are faithful to the wire.
    fn rf(cmd: u8, payload: &[u8]) -> [u8; 64] {
        wh_proto::frame::frame(cmd | wh_proto::frame::REPLY_BIT, payload).unwrap()
    }

    /// Script a full read_matrix: 3 DEFKEY roundtrips, each row pair carrying
    /// its own distinct usages, so a pass requires reading and collecting
    /// from every row rather than only the first.
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
        // expected frames: write [mode, rtp, rtr] per key (one batch), then nothing else. If
        // set_rt sent a SAVE order afterwards, ReplayTransport would reject it against this
        // exhausted script.
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
        // Five keys, each with a different current MODE byte (so each has a
        // different advanced nibble to preserve, including 0x1 and 0xF), to
        // pin that a multi-key call keeps every key's own nibble rather than
        // reusing the first key's. The script ends right after the write batch(es); if set_rt
        // sent a SAVE order afterwards, ReplayTransport would reject it against this exhausted
        // script.
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

    /// The design decision behind `rt_records`' touch-mode choice: a key that already carries
    /// `RtContinuous` (nibble 4, the vendor UI's continuous-mode toggle, which the CLI has no
    /// flag for) must keep that variant when `wh set rt` only changes the sensitivity. Forcing
    /// plain `Rt` unconditionally here would silently turn continuous off on every sensitivity
    /// tweak, the same class of data loss chunk 3 fixes for `rt_off_records`; there is no way for
    /// this call to ask for continuous mode back once lost, since `wh` has no `--continuous`
    /// flag (a later phase's feature decision). Current mode byte 0x48: touch RtContinuous(4),
    /// advanced nibble 8.
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

    /// The other half of the same design decision: a key that is not already RT-enabled at all
    /// (touch mode `Global` here) gets plain `Rt` (nibble 3, continuous off), not
    /// `RtContinuous`, since `wh` has no way to ask for continuous mode on a key that never had
    /// it. Only an already-continuous key keeps that variant; enabling RT fresh always starts
    /// from continuous off, matching the measured default (chunk 2).
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
    /// `KeyRecord.value`'s upper 8 bits, distinct from the low byte carrying the touch and
    /// advanced nibbles) must survive a read-modify-write intact. Reply lo `0x21` (touch
    /// nibble `0x2` -> `Unknown(2)`, never observed on the wire, advanced nibble `0x1`), hi
    /// `0x02`: `rt_records` forces the touch nibble to `Rt` (`0x3`) while preserving the
    /// advanced nibble (`0x1`) and the high byte (`0x02`), giving `0x0231`.
    ///
    /// That expected value is hand-written as a literal below rather than built by calling
    /// `Mode { .. }.value()` again the way `rt_records` itself does: a test that reconstructs
    /// its expectation through the same method under test would inherit any bug in that method
    /// identically on both sides and assert nothing. This is the same shape of gap that let
    /// the pre-fix `Mode` truncate its high byte silently: see `wh-proto`'s
    /// `mode_round_trips_the_full_16_bit_value_including_a_non_zero_high_byte`, which pins
    /// `Mode` itself the same way, and this test's sibling below for `rt_off_records`.
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
        // current mode byte 0x37: touch Rt(3), advanced nibble 7. rt_off_records must write
        // touch Single(1), not Global(0): see rt_off_records' own doc comment for why. In short,
        // `captures/rt-off-w.jsonl` shows the vendor writing nibble 1, not 0, in this exact
        // transition, and matching that observed behaviour is the reason, not a measured effect
        // of writing nibble 0, which remains untested.
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

    /// The `rt_off_records` sibling of
    /// `rt_records_preserves_the_high_byte_of_mode_across_a_read_modify_write` above: reply lo
    /// `0x37` (touch `Rt`, advanced nibble `0x7`), hi `0x02`. `rt_off_records` forces the touch
    /// nibble to `Single` (`0x1`) while preserving the advanced nibble (`0x7`) and the high
    /// byte (`0x02`), giving `0x0217`, hand-written for the same reason: an expectation built
    /// by calling `Mode { .. }.value()` again would share any bug in that method with the code
    /// under test instead of catching it.
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

    /// Chunk 3's own regression test: on a key with a per-key actuation point (touch mode
    /// already `Single`, i.e. not even RT-enabled from this call's perspective), turning "RT
    /// off" again must leave it at `Single`, not coerce it to `Global`. Since whole-branch
    /// review, "leave it at `Single`" means no record at all, not a record that echoes the value
    /// already on the board (see `rt_off_records`' own doc comment): this key's recomputed MODE
    /// value equals what was just read, so nothing is written. A regression that coerced the key
    /// to `Global` instead would still produce a record, with the wrong value, so this still
    /// catches that: it distinguishes "correctly left as `Single`, nothing to write" from
    /// "wrongly rewritten to `Global`, something (wrong) to write".
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

    /// Two regressions this test guards, from two different fix rounds, both against the same
    /// real fixture: every one of the 68 real per-key MODE values read from the device in
    /// `captures/initial-load.jsonl` (extracted with `layout == 0x08` from the KEY-reply frames
    /// in that capture; every one is a key that has never had RT on, confirmed by the assertion
    /// above), replayed through `rt_off_records`, exactly what `wh set rt --keys all --off`
    /// sends.
    ///
    /// The first cut of this function detached all 58 nibble-0 keys among these from the global
    /// travel setting by rewriting their MODE to nibble 1 unconditionally; task 19b chunk 3 fixed
    /// that by restricting the rewrite to keys that were actually `Rt`/`RtContinuous`. A later
    /// whole-branch review found the second half of the same shape still present: this function
    /// went on to push a record for every one of these 68 keys anyway, carrying their own
    /// unchanged value right back at them, 58 of them nibble 0, the exact value
    /// `docs/protocol.md` documents as one `wh` never writes. Since no key here has anything to
    /// change, the correct output is not "68 records, each equal to what was read" but no
    /// records at all: nothing for `set_rt_off` to write, and nothing for `verify_rt_off` to
    /// read back and report as changed.
    #[test]
    fn rt_off_records_leaves_every_real_non_rt_key_from_initial_load_unchanged() {
        // (key, MODE value), verbatim from captures/initial-load.jsonl.
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
        // Script ends right after the write batch: if set_rt_off sent a SAVE order afterwards,
        // ReplayTransport would reject it against this exhausted script.
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        set_rt_off(&mut s, &[0x1A]).unwrap();
        assert!(s.into_inner().finished());
    }

    #[test]
    fn set_ap_writes_ap_records_and_sends_no_save() {
        let recs = vec![KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 1500,
        }];
        let batch = cmds::write_key_records(&recs);
        let mut lines = Vec::new();
        for f in &batch {
            lines.push(l("out", f));
            lines.push(l("in", &rf(cmds::cmd::KEY, &[0x01])));
        }
        // Script ends right after the write batch: if set_ap sent a SAVE order afterwards,
        // ReplayTransport would reject it against this exhausted script.
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        set_ap(&mut s, &[0x1A], Um(1500)).unwrap();
        assert!(s.into_inner().finished());
    }

    #[test]
    fn ap_records_builds_records_without_a_session() {
        let recs = ap_records(&[0x04, 0x05], Um(1200));
        assert_eq!(
            recs,
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
    }

    #[test]
    fn write_records_uses_roundtrip_many_so_mid_batch_failure_reports_progress() {
        // 16 usages -> 16 AP records -> encoder splits into a 14-record and a
        // 2-record frame. Reply only to the first frame, so the second write
        // frame goes unanswered: this must surface as a Batch error with
        // partial-progress detail, not a bare Timeout, proving write_records
        // goes through roundtrip_many rather than a hand-rolled loop.
        let usages: Vec<u8> = (0x04u8..0x14).collect();
        let records = ap_records(&usages, Um(1500));
        let frames = cmds::write_key_records(&records);
        assert_eq!(frames.len(), 2);
        let lines = [
            l("out", &frames[0]),
            l("in", &rf(cmds::cmd::KEY, &[0x01])),
            l("out", &frames[1]),
            // no reply for frames[1]: script ends here
        ];
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
        // An empty script: if anything at all reached the wire for an empty selection,
        // ReplayTransport would reject the unexpected send. Note (review round 1, minor 7): this
        // pins the resulting behaviour, not specifically the `is_empty` early return in
        // `write_records` - `write_key_records(&[])` already yields no frames on its own, so
        // this test would still pass with that guard deleted. It is kept because the guard
        // documents the no-op case explicitly rather than relying on that encoder behaviour by
        // coincidence.
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
    fn read_key_settings_reads_four_layouts() {
        // Press and release are deliberately distinct (500 vs 650, not the same value twice)
        // and both are asserted below: equal values, or asserting only one of the two fields,
        // can't catch the pair being swapped anywhere between the wire reply and `KeySettings`.
        let mut lines = Vec::new();
        for (lid, val) in [
            (layout::AP, 1200u16),
            (layout::MODE, 0x30),
            (layout::RT_PRESS, 500),
            (layout::RT_RELEASE, 650),
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

    /// A reply that parses fine as a profile reply, but names an index the board's four measured
    /// profiles could never produce, must surface as `ProfileOutOfRange`, not `Decode`: this is
    /// what lets a caller distinguish "the reply was garbled" from "the reply named an impossible
    /// profile" and degrade only for the latter (review, task 20 step 4c).
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

    /// `restore_all` writes the global travel DB record first, then the per-key batch(es), and
    /// sends no SAVE order afterwards, mirroring `set_rt`/`set_ap`'s own script shape above (DB
    /// write, key batches, no SAVE) so a restore looks like one coherent write on the wire rather
    /// than a pile of independent calls.
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
        // Script ends right after the key batches: if restore_all sent a SAVE order afterwards,
        // ReplayTransport would reject it against this exhausted script.

        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        restore_all(&mut s, &global, &recs).unwrap();
        assert!(s.into_inner().finished());
    }

    #[test]
    fn restore_all_skips_key_batch_when_there_are_no_records() {
        // Only the global travel write should reach the wire; an empty `records` produces no
        // key batch frames at all (there is no SAVE to skip any more, chunk 4). As with
        // `write_records_sends_nothing_when_there_are_no_records` above (review round 1, minor
        // 7), this pins the resulting wire behaviour rather than the presence of a specific
        // early-return branch in `write_records`.
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
