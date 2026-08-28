//! Typed command encoders/decoders.
//! Port of research/proto/package/src/controller/*.ts + utils/pack.ts + utils/recdata.ts.

use crate::frame::{frame, REPORT_LEN};
use crate::value::Um;

pub mod cmd {
    pub const CMD: u8 = 0x00;
    pub const SYNC: u8 = 0x01;
    pub const KEY: u8 = 0x23;
    pub const DB: u8 = 0x29;
    pub const DEFKEY: u8 = 0x2B;
}

pub const RW_READ: u8 = 0x00;
pub const RW_WRITE: u8 = 0x01;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DecodeError {
    #[error("reply payload too short: {0} bytes")]
    Short(usize),
    #[error("unexpected reply shape")]
    Shape,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalTravel {
    pub travel: Um,
    pub press_dead: Um,
    pub release_dead: Um,
}

/// DBDataPack: [rw, tick, tick, travel(2), press(2), release(2), 6×0]
pub fn write_global_travel(travel: Um, press: Um, release: Um) -> [u8; REPORT_LEN] {
    let mut p = vec![RW_WRITE, 0, 0];
    p.extend(travel.to_le());
    p.extend(press.to_le());
    p.extend(release.to_le());
    p.extend([0u8; 6]);
    frame(cmd::DB, &p).expect("fixed size")
}

pub fn read_global_travel() -> [u8; REPORT_LEN] {
    let mut p = vec![RW_READ, 0, 0];
    p.extend([0u8; 12]);
    frame(cmd::DB, &p).expect("fixed size")
}

/// recdata.getGlobalTouchTravelRecdata: values at payload[3..9].
pub fn parse_global_travel(payload: &[u8]) -> Result<GlobalTravel, DecodeError> {
    if payload.len() < 9 {
        return Err(DecodeError::Short(payload.len()));
    }
    Ok(GlobalTravel {
        travel: Um::from_le(payload[3], payload[4]),
        press_dead: Um::from_le(payload[5], payload[6]),
        release_dead: Um::from_le(payload[7], payload[8]),
    })
}

pub mod layout {
    pub const AP: u8 = 0x04; // Layout_DB0
    pub const MODE: u8 = 0x08; // Layout_Mode
    pub const RT_PRESS: u8 = 0x14; // Layout_RTP
    pub const RT_RELEASE: u8 = 0x15; // Layout_RTR
}

/// MaxPack from constants/byte.ts.
pub const MAX_RECORDS_PER_REPORT: usize = 14;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyRecord {
    pub key: u8,    // USB HID keyboard usage
    pub layout: u8, // layout::* id
    pub value: u16,
}

/// KeyDataPack batches, <=14 records per report, rw-prefixed.
pub fn write_key_records(records: &[KeyRecord]) -> Vec<[u8; REPORT_LEN]> {
    records
        .chunks(MAX_RECORDS_PER_REPORT)
        .map(|chunk| {
            let mut p = Vec::with_capacity(1 + chunk.len() * 4);
            p.push(RW_WRITE);
            for r in chunk {
                p.push(r.key);
                p.push(r.layout);
                p.extend(r.value.to_le_bytes());
            }
            frame(cmd::KEY, &p).expect("<=57 bytes")
        })
        .collect()
}

/// cmdLayout with rw=read: single [key, layout, 0, 0] record.
pub fn read_key_layout(key: u8, layout_id: u8) -> [u8; REPORT_LEN] {
    frame(cmd::KEY, &[RW_READ, key, layout_id, 0, 0]).expect("5 bytes")
}

/// Reply payload [rw, key, layout, lo, hi] (recdata.getSingleTravelRecdata).
pub fn parse_key_reply(payload: &[u8]) -> Result<KeyRecord, DecodeError> {
    if payload.len() < 5 {
        return Err(DecodeError::Short(payload.len()));
    }
    Ok(KeyRecord {
        key: payload[1],
        layout: payload[2],
        value: u16::from_le_bytes([payload[3], payload[4]]),
    })
}

/// Layout_Mode value: touch mode in the high nibble of the low byte,
/// advanced-key mode in the low nibble (recdata.getLayoutModelRecdata).
///
/// The nibble values were measured against the real device across 1224 captured frames
/// (task 19b): 0 = follow global, 1 = per-key actuation point, 3 = per-key rapid trigger with
/// continuous off, 4 = the same with continuous on. Nibble 2 never appeared on the wire in any
/// capture; it is left folded into `Unknown` rather than given its own variant, since nothing
/// observed writes it and there is nothing to name it after.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TouchMode {
    Global,       // 0x0
    Single,       // 0x1
    Rt,           // 0x3, continuous off
    RtContinuous, // 0x4, continuous on
    Unknown(u8),  // any other nibble (0x2 included); preserved so read-modify-write is lossless
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode {
    pub touch: TouchMode,
    pub advanced: u8,
    /// The high byte (bits 8..16) of the raw 16-bit Layout_Mode value, preserved verbatim.
    /// Nothing in this protocol interprets these bits, `touch`/`advanced` only ever read the
    /// low byte, but `from_value`/`value` are relied on elsewhere (the CLI's `dump`/`restore`
    /// round trip) to be a lossless identity on a value nobody has modified, and silently
    /// clearing this byte on every pass through `Mode` would make that untrue. Set to 0 when
    /// building a `Mode` from scratch, i.e. one with no prior device value to preserve; carry
    /// it forward from `Mode::from_value(cur).high` on any read-modify-write.
    pub high: u8,
}

impl Mode {
    pub fn from_value(v: u16) -> Self {
        let b = (v & 0xFF) as u8;
        let nibble = (b >> 4) & 0x0F;
        let touch = match nibble {
            0x0 => TouchMode::Global,
            0x1 => TouchMode::Single,
            0x3 => TouchMode::Rt,
            0x4 => TouchMode::RtContinuous,
            n => TouchMode::Unknown(n),
        };
        Mode {
            touch,
            advanced: b & 0x0F,
            high: (v >> 8) as u8,
        }
    }
    pub fn value(self) -> u16 {
        let t = match self.touch {
            TouchMode::Global => 0x0u8,
            TouchMode::Single => 0x1,
            TouchMode::Rt => 0x3,
            TouchMode::RtContinuous => 0x4,
            TouchMode::Unknown(n) => n & 0x0F,
        };
        let low = (t << 4) | (self.advanced & 0x0F);
        ((self.high as u16) << 8) | low as u16
    }
}

pub mod order {
    pub const PROTOCOL_VERSION: u8 = 0x01;
    // ORDER_TYPE_SAVING_PARAMETER. Kept as protocol vocabulary, but never sent by `wh-device`:
    // across 1224 captured frames covering nine scenarios and five complete write sequences
    // (task 19b), the vendor web configurator never sends this order. Do not wire it back into
    // a write path without first measuring what it actually does on this firmware.
    pub const SAVE: u8 = 0x02;
    pub const FACTORY_RESET: u8 = 0x11; // not exposed in CLI; documented only
    pub const PRECISION: u8 = 0x25;
    pub const KEYBOARD_NAME: u8 = 0x26;
    pub const POLLING: u8 = 0x50;
    pub const CONFIG: u8 = 0x70;
}

/// `h_args` is caller-supplied and unbounded, unlike the other encoders in
/// this module, so overlong input is reported rather than panicking.
pub fn cmd_order(
    order_id: u8,
    h_args: &[u8],
) -> Result<[u8; REPORT_LEN], crate::frame::FrameError> {
    let mut p = vec![order_id];
    p.extend_from_slice(h_args);
    p.extend([0xFF, 0xFF]);
    frame(cmd::CMD, &p)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Precision {
    pub step: Um,
    pub min: Um,
    pub max: Um,
}

/// recdata.getCmdRecdata ORDER_TYPE_PRECISION_STROKE branch.
pub fn parse_precision(payload: &[u8]) -> Result<Precision, DecodeError> {
    if payload.len() < 7 || payload[1] != order::PRECISION {
        return Err(DecodeError::Shape);
    }
    Ok(Precision {
        step: Um(payload[2] as u16),
        min: Um::from_le(payload[3], payload[4]),
        max: Um::from_le(payload[5], payload[6]),
    })
}

pub fn sync() -> [u8; REPORT_LEN] {
    frame(cmd::SYNC, &[1, 2, 3, 4, 0xFF, 0xFF]).expect("6 bytes")
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    pub serial: String,
    pub firmware: String,
}

/// recdata.getCmdSyncRecdata: SN at bytes 9..25, firmware at 26..36.
pub fn parse_sync(payload: &[u8]) -> Result<DeviceInfo, DecodeError> {
    if payload.len() < 36 {
        return Err(DecodeError::Short(payload.len()));
    }
    let clean = |b: &[u8]| {
        String::from_utf8_lossy(b)
            .trim_end_matches('\0')
            .to_string()
    };
    Ok(DeviceInfo {
        serial: clean(&payload[9..25]),
        firmware: clean(&payload[26..36]),
    })
}

pub const MATRIX_ROWS: u8 = 6;
pub const MATRIX_COLS: usize = 21;

pub fn read_defkey_rows(row_a: u8, row_b: u8) -> [u8; REPORT_LEN] {
    frame(cmd::DEFKEY, &[RW_READ, row_a, row_b]).expect("3 bytes")
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefKeyRow {
    pub row: u8,
    /// (col, hid usage) for non-zero cells
    pub keys: Vec<(u8, u8)>,
}

/// recdata.getDefKeyRecdata: [rw, rowA, 21 usages, rowB, 21 usages].
pub fn parse_defkey(payload: &[u8]) -> Result<[DefKeyRow; 2], DecodeError> {
    if payload.len() < 45 {
        return Err(DecodeError::Short(payload.len()));
    }
    let row_at = |row_idx: usize, data_idx: usize| DefKeyRow {
        row: payload[row_idx],
        keys: payload[data_idx..data_idx + MATRIX_COLS]
            .iter()
            .enumerate()
            .filter(|(_, &u)| u != 0)
            .map(|(c, &u)| (c as u8, u))
            .collect(),
    };
    Ok([row_at(1, 2), row_at(23, 24)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Um;

    #[test]
    fn write_global_travel_matches_vendor_template() {
        // Template: 5C 0F 29 C9 01 00 00 <lo> <hi> C8 00 C8 00 + 6 zero bytes
        let f = write_global_travel(Um(500), Um(200), Um(200));
        assert_eq!(&f[..4], &[0x5C, 0x0F, 0x29, 0xC9]);
        assert_eq!(&f[4..13], &[0x01, 0, 0, 0xF4, 0x01, 0xC8, 0x00, 0xC8, 0x00]);
        assert!(f[13..19].iter().all(|&b| b == 0)); // dbn filler
    }

    #[test]
    fn read_global_travel_is_read_flagged() {
        let f = read_global_travel();
        assert_eq!(f[2], cmd::DB);
        assert_eq!(f[4], RW_READ);
    }

    #[test]
    fn parse_global_travel_reads_le_at_payload_3() {
        // payload: [rw, t0, t1, glo_lo, glo_hi, dp_lo, dp_hi, dr_lo, dr_hi]
        let payload = [0x00, 0, 0, 0xF4, 0x01, 0xC8, 0x00, 0x64, 0x00];
        let g = parse_global_travel(&payload).unwrap();
        assert_eq!(
            g,
            GlobalTravel {
                travel: Um(500),
                press_dead: Um(200),
                release_dead: Um(100)
            }
        );
    }

    #[test]
    fn parse_global_travel_rejects_short() {
        assert!(parse_global_travel(&[0x00, 0, 0]).is_err());
    }

    #[test]
    fn key_record_batches_of_14_with_rw_prefix() {
        let recs: Vec<KeyRecord> = (0u8..20)
            .map(|i| KeyRecord {
                key: 0x04 + i,
                layout: layout::RT_PRESS,
                value: 500,
            })
            .collect();
        let frames = write_key_records(&recs);
        assert_eq!(frames.len(), 2); // 14 + 6
                                     // first frame: len = 1 + 14*4 = 57 = 0x39, the vendor batch template
        assert_eq!(frames[0][1], 0x39);
        assert_eq!(frames[0][2], cmd::KEY);
        assert_eq!(frames[0][4], RW_WRITE);
        assert_eq!(&frames[0][5..9], &[0x04, layout::RT_PRESS, 0xF4, 0x01]);
        // second frame: 1 + 6*4 = 25
        assert_eq!(frames[1][1], 25);
    }

    #[test]
    fn read_key_layout_is_single_record_read() {
        let f = read_key_layout(0x1A, layout::AP); // 'w'
        assert_eq!(f[1], 5); // rw + one record
        assert_eq!(&f[4..9], &[RW_READ, 0x1A, layout::AP, 0x00, 0x00]);
    }

    #[test]
    fn parse_key_reply_reads_record() {
        let payload = [0x00, 0x1A, layout::RT_PRESS, 0xF4, 0x01];
        assert_eq!(
            parse_key_reply(&payload).unwrap(),
            KeyRecord {
                key: 0x1A,
                layout: layout::RT_PRESS,
                value: 500
            }
        );
    }

    #[test]
    fn mode_nibbles() {
        let m = Mode::from_value(0x33);
        assert_eq!(m.touch, TouchMode::Rt); // high nibble 3, measured against the real device
        assert_eq!(m.advanced, 0x03);
        assert_eq!(m.value(), 0x33);
        let g = Mode {
            touch: TouchMode::Global,
            advanced: 0x03,
            high: 0,
        };
        assert_eq!(g.value(), 0x03);
    }

    /// Nibble 4, measured against the real device (`rt-continuous-toggle.jsonl`: MODE 0x0048
    /// with continuous on, 0x0038 with it off, everything else held constant).
    #[test]
    fn mode_nibble_4_is_rt_continuous() {
        let m = Mode::from_value(0x48);
        assert_eq!(m.touch, TouchMode::RtContinuous);
        assert_eq!(m.advanced, 0x08);
        assert_eq!(m.value(), 0x48);
    }

    /// Nibble 2 never appeared in any of 1224 captured frames (task 19b); it must stay folded
    /// into `Unknown` rather than aliasing `Rt`, or a read-modify-write on a key in this state
    /// would silently coerce it to `Rt`'s wire value instead of leaving it alone.
    #[test]
    fn mode_nibble_2_is_never_observed_and_stays_unknown() {
        let m = Mode::from_value(0x23);
        assert_eq!(m.touch, TouchMode::Unknown(0x2));
        assert_eq!(m.advanced, 0x03);
        assert_eq!(m.value(), 0x23);
    }

    #[test]
    fn mode_unknown_touch_nibble_round_trips_losslessly() {
        let m = Mode::from_value(0x53);
        assert_eq!(m.touch, TouchMode::Unknown(0x5));
        assert_eq!(m.advanced, 0x03);
        assert_eq!(m.value(), 0x53);
    }

    /// `from_value` used to truncate to `v & 0xFF` before `value()` rebuilt a `u16`, so any
    /// high byte the device actually sent (a real 16-bit Layout_Mode value, not merely a byte
    /// with 8 spare bits) was silently cleared on every pass through `Mode`. `wh-cli`'s
    /// `dump`/`restore` round trip depends on this being a true identity for a value nobody
    /// has modified.
    #[test]
    fn mode_round_trips_the_full_16_bit_value_including_a_non_zero_high_byte() {
        let v = 0x0221u16; // high byte 0x02, touch nibble 0x2 (Unknown, never observed), advanced nibble 0x1
        let m = Mode::from_value(v);
        assert_eq!(m.high, 0x02);
        assert_eq!(m.touch, TouchMode::Unknown(0x2));
        assert_eq!(m.advanced, 0x01);
        assert_eq!(m.value(), v);
    }

    #[test]
    fn cmd_order_layout() {
        // CMDPack: [order, ...h_args, 0xFF, 0xFF]
        let f = cmd_order(order::SAVE, &[]).unwrap();
        assert_eq!(f[2], cmd::CMD);
        assert_eq!(f[1], 3);
        assert_eq!(&f[4..7], &[0x02, 0xFF, 0xFF]);

        let f2 = cmd_order(order::CONFIG, &[0x01]).unwrap();
        assert_eq!(&f2[4..8], &[0x70, 0x01, 0xFF, 0xFF]);
    }

    #[test]
    fn cmd_order_rejects_oversize_h_args() {
        // payload = 1 + h_args.len() + 2 must stay within frame()'s 60-byte cap.
        assert!(cmd_order(order::SAVE, &[0u8; 58]).is_err());
    }

    #[test]
    fn parse_precision_reply() {
        // payload: [status, order=0x25, precision_um, min lo, min hi, max lo, max hi]
        let payload = [0x00, 0x25, 10, 0x00, 0x00, 0xA0, 0x0F];
        let p = parse_precision(&payload).unwrap();
        assert_eq!(
            p,
            Precision {
                step: Um(10),
                min: Um(0),
                max: Um(4000)
            }
        );
    }

    #[test]
    fn sync_request_and_reply() {
        let f = sync();
        assert_eq!(f[2], cmd::SYNC);
        assert_eq!(&f[4..10], &[1, 2, 3, 4, 0xFF, 0xFF]);

        let mut payload = vec![0u8; 60];
        payload[9..25].copy_from_slice(b"SN0123456789ABCD");
        payload[26..36].copy_from_slice(b"V1.2.3.456");
        let info = parse_sync(&payload).unwrap();
        assert_eq!(info.serial, "SN0123456789ABCD");
        assert_eq!(info.firmware, "V1.2.3.456");
    }

    #[test]
    fn defkey_request_and_reply() {
        let f = read_defkey_rows(2, 3);
        assert_eq!(f[2], cmd::DEFKEY);
        assert_eq!(&f[4..7], &[RW_READ, 2, 3]);

        // payload: [rw, rowA, 21 usages, rowB, 21 usages]
        let mut payload = vec![0u8; 45];
        payload[1] = 2;
        payload[2] = 0x04; // col 0 = 'a'
        payload[23] = 3;
        payload[24 + 5] = 0x1A; // row 3 col 5 = 'w'
        let rows = parse_defkey(&payload).unwrap();
        assert_eq!(rows[0].row, 2);
        assert_eq!(rows[0].keys.len(), 1);
        assert_eq!(rows[0].keys[0], (0, 0x04));
        assert_eq!(rows[1].row, 3);
        assert_eq!(rows[1].keys.len(), 1);
        assert_eq!(rows[1].keys[0], (5, 0x1A));
    }
}
