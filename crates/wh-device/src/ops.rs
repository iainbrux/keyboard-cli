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
    let mut get = |lid: u8| -> Result<u16, DeviceError> {
        let payload = s.roundtrip(&cmds::read_key_layout(usage, lid))?;
        let rec =
            cmds::parse_key_reply(&payload).map_err(|e| DeviceError::Decode(e.to_string()))?;
        Ok(rec.value)
    };
    let ap = Um(get(layout::AP)?);
    let mode = Mode::from_value(get(layout::MODE)?);
    let rt_press = Um(get(layout::RT_PRESS)?);
    let rt_release = Um(get(layout::RT_RELEASE)?);
    Ok(KeySettings { usage, ap, mode, rt_press, rt_release })
}

fn write_and_save<T: Transport>(
    s: &mut Session<T>,
    records: &[KeyRecord],
) -> Result<(), DeviceError> {
    for f in cmds::write_key_records(records) {
        s.roundtrip(&f)?;
    }
    s.roundtrip(&cmds::cmd_order(cmds::order::SAVE, &[])?)?;
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
        let payload = s.roundtrip(&cmds::read_key_layout(u, layout::MODE))?;
        let cur =
            cmds::parse_key_reply(&payload).map_err(|e| DeviceError::Decode(e.to_string()))?;
        let mode = Mode { touch: TouchMode::Rt, advanced: Mode::from_value(cur.value).advanced };
        records.push(KeyRecord { key: u, layout: layout::MODE, value: mode.value() });
        records.push(KeyRecord { key: u, layout: layout::RT_PRESS, value: press.0 });
        records.push(KeyRecord { key: u, layout: layout::RT_RELEASE, value: release.0 });
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
        let payload = s.roundtrip(&cmds::read_key_layout(u, layout::MODE))?;
        let cur =
            cmds::parse_key_reply(&payload).map_err(|e| DeviceError::Decode(e.to_string()))?;
        let mode =
            Mode { touch: TouchMode::Global, advanced: Mode::from_value(cur.value).advanced };
        records.push(KeyRecord { key: u, layout: layout::MODE, value: mode.value() });
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

/// Per-key actuation point (layout DB0).
pub fn set_ap<T: Transport>(
    s: &mut Session<T>,
    usages: &[u8],
    depth: Um,
) -> Result<(), DeviceError> {
    let records: Vec<KeyRecord> = usages
        .iter()
        .map(|&u| KeyRecord { key: u, layout: layout::AP, value: depth.0 })
        .collect();
    write_and_save(s, &records)
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
    use wh_proto::cmds::{self, layout, KeyRecord};
    use wh_proto::value::Um;

    fn l(dir: &str, b: &[u8; 64]) -> String {
        format!("{{\"dir\":\"{dir}\",\"hex\":\"{}\"}}", hex(b))
    }
    fn rf(cmd: u8, payload: &[u8]) -> [u8; 64] {
        wh_proto::frame::frame(cmd, payload).unwrap()
    }

    /// Script a full read_matrix: 3 DEFKEY roundtrips; only row0 has keys 'w'@5, 'a'@6.
    fn matrix_script() -> Vec<String> {
        let mut lines = Vec::new();
        for (a, b) in [(0u8, 1u8), (2, 3), (4, 5)] {
            let req = cmds::read_defkey_rows(a, b);
            let mut payload = vec![0u8; 45];
            payload[1] = a;
            payload[23] = b;
            if a == 0 {
                payload[2 + 5] = 0x1A; // w
                payload[2 + 6] = 0x04; // a
            }
            lines.push(l("out", &req));
            lines.push(l("in", &rf(cmds::cmd::DEFKEY, &payload)));
        }
        lines
    }

    #[test]
    fn read_matrix_collects_usages() {
        let script = matrix_script().join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&script).unwrap());
        let m = read_matrix(&mut s).unwrap();
        assert_eq!(m, vec![0x1A, 0x04]);
    }

    #[test]
    fn read_matrix_maps_decode_failure_to_decode_error_not_timeout() {
        let req = cmds::read_defkey_rows(0, 1);
        let bad_reply = rf(cmds::cmd::DEFKEY, &[0x00]); // too short to decode
        let lines = [l("out", &req), l("in", &bad_reply)].join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let err = read_matrix(&mut s).unwrap_err();
        assert!(matches!(err, DeviceError::Decode(_)), "expected Decode, got {err:?}");
    }

    #[test]
    fn set_rt_writes_mode_and_both_sensitivities_then_saves() {
        // expected frames: write [mode, rtp, rtr] per key (one batch), then SAVE order
        let recs = vec![
            KeyRecord { key: 0x1A, layout: layout::MODE, value: 0x20 },
            KeyRecord { key: 0x1A, layout: layout::RT_PRESS, value: 500 },
            KeyRecord { key: 0x1A, layout: layout::RT_RELEASE, value: 500 },
        ];
        let batch = cmds::write_key_records(&recs);
        let save = cmds::cmd_order(cmds::order::SAVE, &[]).unwrap();
        let mut lines = vec![
            // set_rt first reads current mode to preserve the advanced nibble
            l("out", &cmds::read_key_layout(0x1A, layout::MODE)),
            l("in", &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0x00, 0x00])),
        ];
        for f in &batch {
            lines.push(l("out", f));
            lines.push(l("in", &rf(cmds::cmd::KEY, &[0x01])));
        }
        lines.push(l("out", &save));
        lines.push(l("in", &rf(cmds::cmd::CMD, &[0x00, cmds::order::SAVE, 0x01])));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());

        set_rt(&mut s, &[0x1A], Um(500), Um(500)).unwrap();
        assert!(s.into_inner().finished());
    }

    #[test]
    fn rt_records_preserves_advanced_nibble_from_unknown_touch_mode() {
        // current mode byte 0x53: high nibble 5 (Unknown touch mode), low nibble 3 (advanced)
        let lines = [l("out", &cmds::read_key_layout(0x1A, layout::MODE)),
            l("in", &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0x53, 0x00]))]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let recs = rt_records(&mut s, &[0x1A], Um(500), Um(600)).unwrap();
        assert_eq!(
            recs,
            vec![
                KeyRecord { key: 0x1A, layout: layout::MODE, value: 0x23 },
                KeyRecord { key: 0x1A, layout: layout::RT_PRESS, value: 500 },
                KeyRecord { key: 0x1A, layout: layout::RT_RELEASE, value: 600 },
            ]
        );
        assert!(s.into_inner().finished());
    }

    #[test]
    fn rt_off_records_sets_touch_mode_global_preserving_advanced_nibble() {
        // current mode byte 0x27: touch Rt(2), advanced nibble 7
        let lines = [l("out", &cmds::read_key_layout(0x1A, layout::MODE)),
            l("in", &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0x27, 0x00]))]
        .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let recs = rt_off_records(&mut s, &[0x1A]).unwrap();
        assert_eq!(recs, vec![KeyRecord { key: 0x1A, layout: layout::MODE, value: 0x07 }]);
        assert!(s.into_inner().finished());
    }

    #[test]
    fn set_rt_off_writes_mode_global_then_saves() {
        let rec = KeyRecord { key: 0x1A, layout: layout::MODE, value: 0x00 };
        let batch = cmds::write_key_records(&[rec]);
        let save = cmds::cmd_order(cmds::order::SAVE, &[]).unwrap();
        let mut lines = vec![
            l("out", &cmds::read_key_layout(0x1A, layout::MODE)),
            l("in", &rf(cmds::cmd::KEY, &[0x00, 0x1A, layout::MODE, 0x20, 0x00])),
        ];
        for f in &batch {
            lines.push(l("out", f));
            lines.push(l("in", &rf(cmds::cmd::KEY, &[0x01])));
        }
        lines.push(l("out", &save));
        lines.push(l("in", &rf(cmds::cmd::CMD, &[0x00, cmds::order::SAVE, 0x01])));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        set_rt_off(&mut s, &[0x1A]).unwrap();
        assert!(s.into_inner().finished());
    }

    #[test]
    fn set_ap_writes_ap_records_then_saves() {
        let recs = vec![KeyRecord { key: 0x1A, layout: layout::AP, value: 1500 }];
        let batch = cmds::write_key_records(&recs);
        let save = cmds::cmd_order(cmds::order::SAVE, &[]).unwrap();
        let mut lines = Vec::new();
        for f in &batch {
            lines.push(l("out", f));
            lines.push(l("in", &rf(cmds::cmd::KEY, &[0x01])));
        }
        lines.push(l("out", &save));
        lines.push(l("in", &rf(cmds::cmd::CMD, &[0x00, cmds::order::SAVE, 0x01])));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        set_ap(&mut s, &[0x1A], Um(1500)).unwrap();
        assert!(s.into_inner().finished());
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
                &rf(cmds::cmd::KEY, &[0x00, 0x1A, lid, (val & 0xFF) as u8, (val >> 8) as u8]),
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
        let lines = [l("out", &cmds::read_key_layout(0x1A, layout::AP)), l("in", &bad_reply)]
            .join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let err = read_key_settings(&mut s, 0x1A).unwrap_err();
        assert!(matches!(err, DeviceError::Decode(_)), "expected Decode, got {err:?}");
    }

    #[test]
    fn device_info_reads_sync_reply() {
        let mut payload = vec![0u8; 60];
        payload[9..25].copy_from_slice(b"SN0123456789ABCD");
        payload[26..36].copy_from_slice(b"V1.2.3.456");
        let lines = [l("out", &cmds::sync()), l("in", &rf(cmds::cmd::SYNC, &payload))]
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
        assert!(matches!(err, DeviceError::Decode(_)), "expected Decode, got {err:?}");
    }

    #[test]
    fn global_travel_reads_reply() {
        let payload = [0x00, 0, 0, 0xF4, 0x01, 0xC8, 0x00, 0x64, 0x00];
        let lines = [l("out", &cmds::read_global_travel()),
            l("in", &rf(cmds::cmd::DB, &payload))]
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
        let lines =
            [l("out", &cmds::read_global_travel()), l("in", &bad_reply)].join("\n");
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
        let err = global_travel(&mut s).unwrap_err();
        assert!(matches!(err, DeviceError::Decode(_)), "expected Decode, got {err:?}");
    }
}
