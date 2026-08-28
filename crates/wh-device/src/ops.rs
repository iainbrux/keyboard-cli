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
    pub fn rt_enabled(&self) -> bool {
        self.mode.touch == TouchMode::Rt
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
        let mode = Mode {
            touch: TouchMode::Rt,
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

/// Build the [mode] records to switch `usages` back to Global touch mode,
/// preserving each key's advanced-mode nibble. Reads current MODE per key but
/// sends nothing else.
pub fn rt_off_records<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
) -> Result<Vec<KeyRecord>, DeviceError> {
    let mut records = Vec::new();
    for &u in usages {
        let cur_value = read_layout_value(s, u, layout::MODE)?;
        let cur_mode = Mode::from_value(cur_value);
        let mode = Mode {
            touch: TouchMode::Global,
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
pub fn set_rt<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
    press: Um,
    release: Um,
) -> Result<(), DeviceError> {
    let records = rt_records(s, usages, press, release)?;
    write_and_save(s, &records)
}

/// Disable RT (touch mode -> Global), preserving the advanced nibble.
pub fn set_rt_off<T: Transport>(s: &mut Session<T>, usages: &[u8]) -> Result<(), DeviceError> {
    let records = rt_off_records(s, usages)?;
    write_and_save(s, &records)
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
    fn rf(cmd: u8, payload: &[u8]) -> [u8; 64] {
        wh_proto::frame::frame(cmd, payload).unwrap()
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
                    value: 0x23
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
    fn rt_off_records_sets_touch_mode_global_preserving_advanced_nibble() {
        // current mode byte 0x27: touch Rt(2), advanced nibble 7
        let lines = [
            l("out", &cmds::read_key_layout(0x1A, layout::MODE)),
            l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0x27, 0x00]),
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
                value: 0x07
            }]
        );
        assert!(s.into_inner().finished());
    }

    #[test]
    fn set_rt_off_writes_mode_global_then_saves() {
        let rec = KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x00,
        };
        let batch = cmds::write_key_records(&[rec]);
        let save = cmds::cmd_order(cmds::order::SAVE, &[]).unwrap();
        let mut lines = vec![
            l("out", &cmds::read_key_layout(0x1A, layout::MODE)),
            l(
                "in",
                &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0x20, 0x00]),
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
        let mut lines = Vec::new();
        for (lid, val) in [
            (layout::AP, 1200u16),
            (layout::MODE, 0x20),
            (layout::RT_PRESS, 500),
            (layout::RT_RELEASE, 500),
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
}
