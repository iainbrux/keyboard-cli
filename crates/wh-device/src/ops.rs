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

/// Write `records` in batches and, only if every batch reached the device,
/// persist them with SAVE. A no-op selection (empty `records`) returns
/// immediately without writing anything or burning a flash-save cycle.
///
/// Uses `roundtrip_many` rather than a hand-rolled send loop so a mid-batch
/// failure reports how many frames already reached the device (`DeviceError::Batch`)
/// instead of a bare timeout. A SAVE failure is wrapped separately in
/// `DeviceError::NotPersisted`, because at that point the writes already
/// landed on the board (a read-back would see them) but a power cycle would
/// revert them since they were never flushed to flash.
fn write_and_save<T: Transport>(
    s: &mut Session<T>,
    records: &[KeyRecord],
) -> Result<(), DeviceError> {
    if records.is_empty() {
        return Ok(());
    }
    let frames = cmds::write_key_records(records);
    let applied = frames.len();
    s.roundtrip_many(&frames)?;
    s.roundtrip(&cmds::cmd_order(cmds::order::SAVE, &[])?)
        .map_err(|source| DeviceError::NotPersisted {
            applied,
            source: Box::new(source),
        })?;
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

/// Build the [mode] records to switch `usages` back to per-key actuation point mode
/// (`TouchMode::Single`, nibble 1), preserving each key's advanced-mode nibble. Reads current
/// MODE per key but sends nothing else.
///
/// Writes `Single`, not `Global` (nibble 0): measured on the real device (`captures/rt-off-w.jsonl`,
/// task 19b chunk 3), turning RT off wrote MODE nibble 1, not 0. `Global` means "ignore this
/// key's AP register and follow the global travel setting", so on a key that had a per-key
/// actuation point configured, writing nibble 0 here would silently discard it: the AP register
/// itself is untouched, but with the touch mode set to `Global` the board ignores it. Writing
/// `Single` instead is what actually turns RT off while keeping that key's own actuation point in
/// effect, which is the sole reason a per-key AP would have been set in the first place.
pub fn rt_off_records<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
) -> Result<Vec<KeyRecord>, DeviceError> {
    let mut records = Vec::new();
    for &u in usages {
        let cur_value = read_layout_value(s, u, layout::MODE)?;
        let cur_mode = Mode::from_value(cur_value);
        let mode = Mode {
            touch: TouchMode::Single,
            advanced: cur_mode.advanced,
            high: cur_mode.high,
        };
        records.push(KeyRecord {
            key: u,
            layout: layout::MODE,
            value: mode.value(),
        });
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
    write_and_save(s, &records)?;
    Ok(records)
}

/// Disable RT (touch mode -> Single, per-key actuation point), preserving the advanced
/// nibble. Returns the exact
/// records that were written (one MODE record per key, in `usages` order), the same reason
/// `set_rt` returns its records: a caller verifying the write needs to compare against what
/// was actually sent, advanced nibble and high byte included, not just the touch mode.
pub fn set_rt_off<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
) -> Result<Vec<KeyRecord>, DeviceError> {
    let records = rt_off_records(s, usages)?;
    write_and_save(s, &records)?;
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
    write_and_save(s, &ap_records(usages, depth))
}

pub fn device_info<T: Transport>(s: &mut Session<T>) -> Result<cmds::DeviceInfo, DeviceError> {
    let payload = s.roundtrip(&cmds::sync())?;
    cmds::parse_sync(&payload).map_err(|e| DeviceError::Decode(e.to_string()))
}

pub fn global_travel<T: Transport>(s: &mut Session<T>) -> Result<cmds::GlobalTravel, DeviceError> {
    let payload = s.roundtrip(&cmds::read_global_travel())?;
    cmds::parse_global_travel(&payload).map_err(|e| DeviceError::Decode(e.to_string()))
}

/// Write a whole snapshot back to the board: global travel first, then every per-key record,
/// then SAVE (via `write_and_save`, which also skips SAVE when `records` is empty). Global
/// travel goes first so a partial restore that fails partway through the per-key batch still
/// leaves the board's overall travel consistent with what the caller intended, rather than a
/// mix of old per-key values against a new global travel.
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
    write_and_save(s, records)
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
    fn set_rt_writes_mode_and_both_sensitivities_then_saves() {
        // expected frames: write [mode, rtp, rtr] per key (one batch), then SAVE order
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
        let save = cmds::cmd_order(cmds::order::SAVE, &[]).unwrap();
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
        lines.push(l("out", &save));
        lines.push(l(
            "in",
            &rf(cmds::cmd::CMD, &[0x00, cmds::order::SAVE, 0x01]),
        ));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());

        set_rt(&mut s, &[0x1A], Um(500), Um(500)).unwrap();
        assert!(s.into_inner().finished());
    }

    #[test]
    fn set_rt_over_five_keys_preserves_each_advanced_nibble_and_saves_once() {
        // Five keys, each with a different current MODE byte (so each has a
        // different advanced nibble to preserve, including 0x1 and 0xF), to
        // pin that a multi-key call keeps every key's own nibble rather than
        // reusing the first key's, and that the resulting 15 records (3 per
        // key) still produce exactly one SAVE regardless of how many 0x23
        // frames the encoder splits them into.
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
        let save = cmds::cmd_order(cmds::order::SAVE, &[]).unwrap();
        lines.push(l("out", &save));
        lines.push(l(
            "in",
            &rf(cmds::cmd::CMD, &[0x00, cmds::order::SAVE, 0x01]),
        ));

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
        // touch Single(1), not Global(0): see rt_off_records' own doc comment, and chunk 3 of
        // task 19b, for why nibble 0 would silently discard this key's per-key actuation point.
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
    /// off" again must not silently coerce it to `Global` and thereby drop that AP. This is the
    /// exact data-loss shape measured on the real device in `captures/rt-off-w.jsonl`.
    #[test]
    fn rt_off_records_writes_single_not_global_so_the_per_key_actuation_point_survives() {
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
            vec![KeyRecord {
                key: 0x1A,
                layout: layout::MODE,
                value: 0x18,
            }],
            "touch mode must stay Single (nibble 1), not fall back to Global (nibble 0)"
        );
        assert!(s.into_inner().finished());
    }

    #[test]
    fn set_rt_off_writes_mode_single_then_saves() {
        let rec = KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x10,
        };
        let batch = cmds::write_key_records(&[rec]);
        let save = cmds::cmd_order(cmds::order::SAVE, &[]).unwrap();
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
        lines.push(l("out", &save));
        lines.push(l(
            "in",
            &rf(cmds::cmd::CMD, &[0x00, cmds::order::SAVE, 0x01]),
        ));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        set_rt_off(&mut s, &[0x1A]).unwrap();
        assert!(s.into_inner().finished());
    }

    #[test]
    fn set_ap_writes_ap_records_then_saves() {
        let recs = vec![KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 1500,
        }];
        let batch = cmds::write_key_records(&recs);
        let save = cmds::cmd_order(cmds::order::SAVE, &[]).unwrap();
        let mut lines = Vec::new();
        for f in &batch {
            lines.push(l("out", f));
            lines.push(l("in", &rf(cmds::cmd::KEY, &[0x01])));
        }
        lines.push(l("out", &save));
        lines.push(l(
            "in",
            &rf(cmds::cmd::CMD, &[0x00, cmds::order::SAVE, 0x01]),
        ));
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
    fn write_and_save_uses_roundtrip_many_so_mid_batch_failure_reports_progress() {
        // 16 usages -> 16 AP records -> encoder splits into a 14-record and a
        // 2-record frame. Reply only to the first frame, so the second write
        // frame goes unanswered: this must surface as a Batch error with
        // partial-progress detail, not a bare Timeout, proving write_and_save
        // now goes through roundtrip_many rather than a hand-rolled loop.
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
    fn save_failure_after_successful_writes_is_reported_as_not_persisted() {
        let rec = KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 1500,
        };
        let batch = cmds::write_key_records(&[rec]);
        let mut lines = Vec::new();
        for f in &batch {
            lines.push(l("out", f));
            lines.push(l("in", &rf(cmds::cmd::KEY, &[0x01])));
        }
        let save = cmds::cmd_order(cmds::order::SAVE, &[]).unwrap();
        lines.push(l("out", &save));
        // no reply for SAVE: the write succeeded but the save never confirms
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let err = set_ap(&mut s, &[0x1A], Um(1500)).unwrap_err();
        match err {
            DeviceError::NotPersisted { applied, source } => {
                assert_eq!(applied, 1);
                assert!(
                    matches!(*source, DeviceError::Timeout),
                    "expected Timeout source, got {source:?}"
                );
            }
            other => panic!("expected NotPersisted, got {other:?}"),
        }
    }

    #[test]
    fn write_and_save_skips_save_when_there_are_no_records() {
        // An empty script: if write_and_save sent anything at all (even the
        // SAVE order) with no usages selected, ReplayTransport would reject
        // the unexpected send.
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
        payload[9..25].copy_from_slice(b"SN0123456789ABCD");
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

    /// `restore_all` writes the global travel DB record first, then the per-key batch(es),
    /// then SAVE, mirroring `set_rt`/`set_ap`'s own script shape above (DB write, key
    /// batches, SAVE) so a restore looks like one coherent write on the wire rather than a
    /// pile of independent calls.
    #[test]
    fn restore_all_writes_global_travel_then_key_batches_then_saves() {
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
        let save = cmds::cmd_order(cmds::order::SAVE, &[]).unwrap();

        let mut lines = vec![
            l("out", &db_write),
            l("in", &rf(cmds::cmd::DB, &[0x01, 0, 0])),
        ];
        for f in &batches {
            lines.push(l("out", f));
            lines.push(l("in", &rf(cmds::cmd::KEY, &[0x01])));
        }
        lines.push(l("out", &save));
        lines.push(l(
            "in",
            &rf(cmds::cmd::CMD, &[0x00, cmds::order::SAVE, 0x01]),
        ));

        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        restore_all(&mut s, &global, &recs).unwrap();
        assert!(s.into_inner().finished());
    }

    #[test]
    fn restore_all_skips_key_batch_and_save_when_there_are_no_records() {
        // Only the global travel write should reach the wire; no records means no key batch
        // and, per write_and_save, no SAVE either.
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
