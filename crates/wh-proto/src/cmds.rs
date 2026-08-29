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

/// recdata.getCmdSyncRecdata: both the serial and the firmware string are length-prefixed on
/// the wire, not fixed-width. `p[8]` is the serial's declared length, with the string itself
/// starting at `p[9]`; the firmware's declared length follows immediately after the serial (at
/// `p[9 + serial_len]`), with that string starting one byte later. Measured against the real
/// device's 60-byte SYNC reply (task 19b chunk 6, `initial-load.jsonl` frames 1 and 3): the old
/// code read the firmware from a hardcoded `payload[26..36]`, which took 10 bytes where the wire
/// actually declares 16, truncating it.
///
/// A declared length that runs past the end of `payload` is a `DecodeError::Shape` (the payload
/// itself may be exactly the right size; it is the declared length that is bogus, so `Short` -
/// which reports the payload's own length - would misdiagnose it), never a panic or a silent
/// truncation: this parses whatever the device sends back, including a device in a bad state.
///
/// A cleaned serial or firmware string that comes back empty, or that contains a byte outside
/// printable ASCII, is also a `DecodeError::Shape`, not a successful empty/garbled identity:
/// a truncated or corrupted reply must not make `wh backup` succeed with a snapshot that has
/// silently lost the identity of the board it came from (review round 1, chunk 6), and a
/// misbehaving device must not be able to smuggle control bytes (e.g. an ANSI escape) into
/// `wh dump`'s terminal output through `serial`/`firmware` (review round 1, chunk 7's sibling
/// finding on this same function).
pub fn parse_sync(payload: &[u8]) -> Result<DeviceInfo, DecodeError> {
    if payload.len() < 9 {
        return Err(DecodeError::Short(payload.len()));
    }
    let clean = |b: &[u8]| -> Result<String, DecodeError> {
        // Trim at the first NUL, then trim trailing 0xFF padding, then trim surrounding
        // whitespace, in that order: the wire pads a string's declared length with a NUL
        // terminator plus any remaining slack, and the payload's own tail is 0xFF-padded.
        let nul_end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        let b = &b[..nul_end];
        let ff_end = b.iter().rposition(|&c| c != 0xFF).map_or(0, |i| i + 1);
        let b = &b[..ff_end];
        if !b.iter().all(|&c| c.is_ascii_graphic() || c == b' ') {
            return Err(DecodeError::Shape);
        }
        // `b` is now known to be printable ASCII, so this can never fail.
        let s = std::str::from_utf8(b)
            .expect("printable ASCII is always valid UTF-8")
            .trim()
            .to_string();
        if s.is_empty() {
            return Err(DecodeError::Shape);
        }
        Ok(s)
    };

    let serial_len = payload[8] as usize;
    let serial_start = 9usize;
    let serial_end = serial_start
        .checked_add(serial_len)
        .filter(|&end| end <= payload.len())
        .ok_or(DecodeError::Shape)?;
    let serial = clean(&payload[serial_start..serial_end])?;

    if serial_end >= payload.len() {
        return Err(DecodeError::Short(payload.len()));
    }
    let firmware_len = payload[serial_end] as usize;
    let firmware_start = serial_end + 1;
    let firmware_end = firmware_start
        .checked_add(firmware_len)
        .filter(|&end| end <= payload.len())
        .ok_or(DecodeError::Shape)?;
    let firmware = clean(&payload[firmware_start..firmware_end])?;

    Ok(DeviceInfo { serial, firmware })
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
        payload[8] = 16; // serial length prefix
        payload[9..25].copy_from_slice(b"SN0123456789ABCD");
        payload[25] = 10; // firmware length prefix
        payload[26..36].copy_from_slice(b"V1.2.3.456");
        let info = parse_sync(&payload).unwrap();
        assert_eq!(info.serial, "SN0123456789ABCD");
        assert_eq!(info.firmware, "V1.2.3.456");
    }

    /// The real device's own 60-byte SYNC reply payload, captured twice identically in
    /// `initial-load.jsonl` (frames 1 and 3, task 19b chunk 6). This is the test that matters:
    /// the old hardcoded `payload[26..36]` firmware slice produced `App_V1.1.0`, ten bytes where
    /// the wire actually declares sixteen (`App_V1.1.046000`).
    #[test]
    fn parse_sync_reads_the_real_device_reply() {
        let hex =
            "00140802468002001033343833313431333933453033353032104170705f56312e312e30343630303\
                    00000417567203230203230323600ffffffffff";
        let payload: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let info = parse_sync(&payload).unwrap();
        assert_eq!(info.serial, "3483141393E03502");
        assert_eq!(info.firmware, "App_V1.1.046000");
    }

    /// Review round 1, minor 5: an overrunning length prefix is a bogus *declaration*, not a
    /// too-short *payload* - this one is a full, real-size 60-byte SYNC reply, exactly what the
    /// device actually sends (review round 1, important 3, on moving these off artificially
    /// tiny payloads), with only its serial length byte corrupted. `DecodeError::Shape`, not
    /// `Short`, is the honest diagnosis: `Short(60)` would claim the payload itself was too
    /// small, which is false.
    #[test]
    fn parse_sync_rejects_a_serial_length_prefix_that_overruns_the_payload() {
        let mut payload = vec![0u8; 60];
        payload[8] = 0xFF; // declares a serial far longer than the payload has room for
        assert_eq!(parse_sync(&payload).unwrap_err(), DecodeError::Shape);
    }

    #[test]
    fn parse_sync_rejects_a_firmware_length_prefix_that_overruns_the_payload() {
        let mut payload = vec![0u8; 60];
        payload[8] = 4; // serial: 4 bytes, well within the payload
        payload[9..13].copy_from_slice(b"1234");
        payload[13] = 0xFF; // firmware length prefix declares far more than remains
        assert_eq!(parse_sync(&payload).unwrap_err(), DecodeError::Shape);
    }

    /// The guard between the two length-prefixed strings (`if serial_end >= payload.len()`) has
    /// no test of its own without this one (review round 1, minor 4): a serial that declares
    /// exactly to the end of the payload leaves no byte for the firmware's own length prefix.
    /// This *is* a too-short payload (there is no bogus declaration here, just not enough bytes
    /// to hold a second string at all), so `Short`, not `Shape`, is the right diagnosis, unlike
    /// the two overrun cases above.
    #[test]
    fn parse_sync_rejects_a_serial_that_leaves_no_room_for_the_firmware_length_prefix() {
        let mut payload = vec![0u8; 20];
        payload[8] = 11; // 9 + 11 == 20 == payload.len(): no byte left for the firmware prefix
                         // A real, non-empty serial filling every one of those 11 bytes, so this
                         // test exercises the guard between the two strings, not the separate
                         // empty-serial rejection above.
        payload[9..20].copy_from_slice(b"ABCDEFGHIJK");
        assert_eq!(
            parse_sync(&payload).unwrap_err(),
            DecodeError::Short(payload.len())
        );
    }

    /// Proves the parse follows the declared length prefix rather than a hardcoded constant: a
    /// shorter serial (6 bytes, not 16) shifts where the firmware length prefix and firmware
    /// string are read from, and both still come back correctly. A full 60-byte payload (review
    /// round 1, important 3): the real device's replies are always this size, so the test no
    /// longer needs an artificially tiny one just to exercise a short serial.
    #[test]
    fn parse_sync_follows_a_shorter_declared_serial_length_not_a_constant() {
        let mut payload = vec![0u8; 60];
        payload[8] = 6; // serial length
        payload[9..15].copy_from_slice(b"ABC123");
        payload[15] = 5; // firmware length prefix, right after the 6-byte serial
        payload[16..21].copy_from_slice(b"V9.9.");
        let info = parse_sync(&payload).unwrap();
        assert_eq!(info.serial, "ABC123");
        assert_eq!(info.firmware, "V9.9.");
    }

    /// Review round 1, important 3: lowering the length floor from 36 to 9 (chunk 6) must not
    /// let a truncated or bad-state reply decode as a well-formed, empty identity. A zero-length
    /// serial is a `DecodeError`, not `Ok(DeviceInfo { serial: "", .. })`: `wh backup` must never
    /// silently write a snapshot that has lost the identity of the board it came from.
    #[test]
    fn parse_sync_rejects_an_empty_serial() {
        let mut payload = vec![0u8; 60];
        payload[8] = 0; // serial length: 0
        payload[9] = 10; // firmware length prefix, right after the zero-length serial
        payload[10..20].copy_from_slice(b"V1.2.3.456");
        assert_eq!(parse_sync(&payload).unwrap_err(), DecodeError::Shape);
    }

    /// The firmware sibling of the empty-serial test above.
    #[test]
    fn parse_sync_rejects_an_empty_firmware() {
        let mut payload = vec![0u8; 60];
        payload[8] = 16;
        payload[9..25].copy_from_slice(b"SN0123456789ABCD");
        payload[25] = 0; // firmware length: 0
        assert_eq!(parse_sync(&payload).unwrap_err(), DecodeError::Shape);
    }

    /// Review round 1, minor 6: the brief's premise that the current code already rejected
    /// non-ASCII/non-printable bytes was false; this implements that rejection. A serial
    /// carrying a BEL and an ANSI escape sequence (`\x07\x1b[31m...`) must be a `DecodeError`,
    /// not a successfully parsed string: `wh dump`'s non-JSON path writes `serial` straight to
    /// the terminal, so a misbehaving device must not be able to smuggle control bytes into an
    /// operator's terminal through it.
    #[test]
    fn parse_sync_rejects_control_bytes_in_the_serial() {
        let mut payload = vec![0u8; 60];
        let serial = b"SN\x07\x1b[31mEVIL\"";
        payload[8] = serial.len() as u8;
        payload[9..9 + serial.len()].copy_from_slice(serial);
        let fw_len_pos = 9 + serial.len();
        payload[fw_len_pos] = 10;
        payload[fw_len_pos + 1..fw_len_pos + 11].copy_from_slice(b"V1.2.3.456");
        assert_eq!(parse_sync(&payload).unwrap_err(), DecodeError::Shape);
    }

    /// The firmware sibling: raw bytes outside printable ASCII (not just invalid UTF-8, which
    /// `from_utf8_lossy` would have silently replaced rather than rejected) must also be
    /// refused.
    #[test]
    fn parse_sync_rejects_non_ascii_bytes_in_the_firmware() {
        let mut payload = vec![0u8; 60];
        payload[8] = 16;
        payload[9..25].copy_from_slice(b"SN0123456789ABCD");
        let firmware = [0x80, 0x81, 0x82, b'V', b'1'];
        payload[25] = firmware.len() as u8;
        payload[26..26 + firmware.len()].copy_from_slice(&firmware);
        assert_eq!(parse_sync(&payload).unwrap_err(), DecodeError::Shape);
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
