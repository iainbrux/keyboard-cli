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
    /// A SYNC-specific decode failure, naming the field that failed instead of the opaque
    /// `Shape` message. Only `parse_sync` uses this; every other decoder still uses `Shape`.
    #[error("SYNC reply: {0}")]
    Identity(&'static str),
    /// A wire profile index (`ProfileNumber::from_wire_index`) past the board's four measured
    /// profiles, e.g. a misbehaving device echoing its own request byte `0xFF` back. Kept
    /// distinct from `Shape`: the reply parsed fine as a profile reply, only the index inside
    /// it is one the board could never actually report.
    #[error("profile index {0} is out of range: the board has 4 profiles (wire index 0..=3)")]
    ProfileOutOfRange(u8),
    /// A one-based profile number (`ProfileNumber::from_one_based`) outside `1..=4`. Kept
    /// separate from `ProfileOutOfRange`: the two constructors validate different conventions
    /// (a live wire index versus a stored one-based number), and the error should name which.
    #[error("profile {0} is out of range: the board has 4 profiles, numbered 1..=4")]
    ProfileNumberOutOfRange(u8),
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
    /// Actuation point keyset index. Read as 1 for w,a,s,d and 2 for esc, matching the two
    /// keysets the vendor UI showed. Never observed being written, so do not write it.
    pub const KEYSET_AP: u8 = 0xFF;
    /// Rapid trigger keyset membership. Written 1 on create (`captures/rt-on-w-0.5.jsonl`) and
    /// 0 on delete (`captures/rt-off-w.jsonl`).
    pub const KEYSET_RT: u8 = 0xFE;
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

/// Layout_Mode: touch mode in the high nibble of the low byte, advanced-key mode in the low
/// nibble (recdata.getLayoutModelRecdata). Nibble values measured across 1224 captured frames:
/// 0 = follow global, 1 = per-key actuation, 3 = rapid trigger, 4 = rapid trigger continuous.
/// Nibble 2 never appeared on the wire, so it stays folded into `Unknown`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TouchMode {
    Global,       // 0x0
    Single,       // 0x1
    RtGlobal,     // 0x2, rapid trigger following the global settings
    Rt,           // 0x3, own settings, continuous off
    RtContinuous, // 0x4, own settings, continuous on
    Unknown(u8),  // any other nibble; preserved so read-modify-write is lossless
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode {
    pub touch: TouchMode,
    pub advanced: u8,
    /// The high byte (bits 8..16) of the raw Layout_Mode value, preserved verbatim: `touch` and
    /// `advanced` only ever read the low byte, but `wh-cli`'s `dump`/`restore` round trip relies
    /// on `from_value`/`value` being a lossless identity. Set to 0 for a fresh `Mode`; carry it
    /// forward from `Mode::from_value(cur).high` on any read-modify-write.
    pub high: u8,
}

impl Mode {
    pub fn from_value(v: u16) -> Self {
        let b = (v & 0xFF) as u8;
        let nibble = (b >> 4) & 0x0F;
        let touch = match nibble {
            0x0 => TouchMode::Global,
            0x1 => TouchMode::Single,
            0x2 => TouchMode::RtGlobal,
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
            TouchMode::RtGlobal => 0x2,
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
    // ORDER_TYPE_SAVING_PARAMETER. Kept as protocol vocabulary but never sent: the vendor
    // configurator never sends this order across 1224 captured frames covering ten scenarios
    // and five complete write sequences. Do not wire it into a write path without measuring
    // what it actually does on this firmware.
    pub const SAVE: u8 = 0x02;
    pub const FACTORY_RESET: u8 = 0x11; // not exposed in CLI; documented only
    pub const PRECISION: u8 = 0x25;
    pub const KEYBOARD_NAME: u8 = 0x26;
    pub const POLLING: u8 = 0x50;
    pub const PROFILE: u8 = 0x70;
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

/// The board's own bound: the K-001 has four profiles, wire index `0..=3`, measured.
const MAX_WIRE_INDEX: u8 = 3;

/// A validated profile index, stored wire-native as the board's own zero-based index.
///
/// Two constructors for two conventions that are not interchangeable. `from_wire_index` takes
/// the zero-based wire index and is the only one `parse_profile` calls. `from_one_based` takes
/// a plain `1..=4` number, so a caller holding one (a TOML snapshot field, a test) never has to
/// subtract 1 by hand, which underflows on 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProfileNumber(u8);

impl ProfileNumber {
    /// Converts the wire's own zero-based index, rejecting anything past the board's four
    /// measured profiles: a device reporting `0xFE` or `0xFF` is not one to trust the profile
    /// of.
    pub fn from_wire_index(idx: u8) -> Result<Self, DecodeError> {
        if idx > MAX_WIRE_INDEX {
            return Err(DecodeError::ProfileOutOfRange(idx));
        }
        Ok(Self(idx))
    }

    /// Converts a plain one-based number (`1..=4`), rejecting `0` and anything past the board's
    /// four measured profiles. See the type doc for why this exists alongside `from_wire_index`.
    pub fn from_one_based(n: u8) -> Result<Self, DecodeError> {
        if n == 0 || n > MAX_WIRE_INDEX + 1 {
            return Err(DecodeError::ProfileNumberOutOfRange(n));
        }
        Ok(Self(n - 1))
    }

    /// The wire's own zero-based index, for a caller that needs to send it back (a future
    /// profile-select encoder) rather than display it.
    pub fn wire_index(self) -> u8 {
        self.0
    }

    /// The one-based number, for storing into a TOML snapshot or any other UI-facing text.
    /// `Display` (below) covers every other use, printing the same number.
    pub fn one_based(self) -> u8 {
        self.0 + 1
    }
}

impl std::fmt::Display for ProfileNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.one_based())
    }
}

/// Read the board's active profile: cmd 0x00, sub-order `order::PROFILE` (0x70), arg 0xFF.
/// `cmd_order` lays out `[order_id, ...h_args, 0xFF, 0xFF]`, so passing `0xFF` produces the
/// measured `[0x70, 0xFF, 0xFF, 0xFF]` payload.
pub fn read_profile() -> [u8; REPORT_LEN] {
    cmd_order(order::PROFILE, &[0xFF]).expect("4 bytes")
}

/// Reply payload `[status, sub-order, index, 0xff]`. Returns the wire's own zero-based profile
/// index at `payload[2]`, validated here, the sole place a wire byte becomes a `ProfileNumber`.
/// Rejects a short payload, a reply whose `payload[1]` is not `order::PROFILE` (a reply to a
/// different sub-order must not be misread as a profile index), and an index past the four
/// measured profiles.
pub fn parse_profile(payload: &[u8]) -> Result<ProfileNumber, DecodeError> {
    if payload.len() < 3 {
        return Err(DecodeError::Short(payload.len()));
    }
    if payload[1] != order::PROFILE {
        return Err(DecodeError::Shape);
    }
    ProfileNumber::from_wire_index(payload[2])
}

/// Select the active profile: cmd 0x00, sub-order `order::PROFILE` (0x70), argument the wire's
/// own zero-based index. Measured byte-for-byte in `captures/profile-switch.jsonl` (checksum
/// included): the select frame is identical in shape to `cmd_order(order::PROFILE, &[idx])`, the
/// same padding `read_profile` sends with `0xFF` in the argument slot instead of a real index.
/// The ack that follows carries no reliable confirmation of which profile actually landed, so
/// callers must re-read with `profile()` rather than trust it.
pub fn select_profile(p: ProfileNumber) -> [u8; REPORT_LEN] {
    cmd_order(order::PROFILE, &[p.wire_index()]).expect("1 byte")
}

pub fn sync() -> [u8; REPORT_LEN] {
    frame(cmd::SYNC, &[1, 2, 3, 4, 0xFF, 0xFF]).expect("6 bytes")
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    pub serial: String,
    pub firmware: String,
}

/// recdata.getCmdSyncRecdata: serial and firmware are length-prefixed on the wire, not fixed
/// width. `payload[8]` is the serial's declared length, serial starts at `payload[9]`; the
/// firmware's declared length follows immediately after (at `payload[9 + serial_len]`), with the
/// firmware string starting one byte later. Measured against the real device's 60-byte SYNC
/// reply (`captures/initial-load.jsonl`, frames 1 and 3).
///
/// A declared length past the end of `payload` is `DecodeError::Identity`, not `Short`: the
/// payload itself may be the right size, only the declared length is bogus. An empty or
/// non-printable-ASCII serial/firmware is also `Identity`, never a silent empty or garbled
/// success: `wh backup` must not lose the board's identity, and `wh dump` must not let a
/// misbehaving device smuggle control bytes into the terminal through these fields.
pub fn parse_sync(payload: &[u8]) -> Result<DeviceInfo, DecodeError> {
    if payload.len() < 9 {
        return Err(DecodeError::Short(payload.len()));
    }
    // Each caller supplies its own two static messages directly, so adding a third field can't
    // silently mis-attribute to the wrong one via a stale match arm.
    //
    // Order matters: trim to the first NUL, then trailing 0xFF padding, then reject any
    // non-printable-ASCII byte, then trim outer whitespace. The wire pads a declared length with
    // a NUL plus slack, and the payload's tail is 0xFF-padded; a tab or other control byte inside
    // the declared length is corruption, not padding, so `"\tABC\t"` is rejected rather than
    // trimmed to `"ABC"`.
    let clean = |b: &[u8],
                 non_ascii_msg: &'static str,
                 empty_msg: &'static str|
     -> Result<String, DecodeError> {
        let nul_end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        let b = &b[..nul_end];
        let ff_end = b.iter().rposition(|&c| c != 0xFF).map_or(0, |i| i + 1);
        let b = &b[..ff_end];
        if !b.iter().all(|&c| c.is_ascii_graphic() || c == b' ') {
            return Err(DecodeError::Identity(non_ascii_msg));
        }
        // `b` is now known to be printable ASCII, so this can never fail.
        let s = std::str::from_utf8(b)
            .expect("printable ASCII is always valid UTF-8")
            .trim()
            .to_string();
        if s.is_empty() {
            return Err(DecodeError::Identity(empty_msg));
        }
        Ok(s)
    };

    let serial_len = payload[8] as usize;
    let serial_start = 9usize;
    let serial_end = serial_start
        .checked_add(serial_len)
        .filter(|&end| end <= payload.len())
        .ok_or(DecodeError::Identity(
            "serial length prefix overruns the reply",
        ))?;
    let serial = clean(
        &payload[serial_start..serial_end],
        "serial contains a non-printable byte",
        "serial is empty",
    )?;

    if serial_end >= payload.len() {
        return Err(DecodeError::Short(payload.len()));
    }
    let firmware_len = payload[serial_end] as usize;
    let firmware_start = serial_end + 1;
    let firmware_end = firmware_start
        .checked_add(firmware_len)
        .filter(|&end| end <= payload.len())
        .ok_or(DecodeError::Identity(
            "firmware length prefix overruns the reply",
        ))?;
    let firmware = clean(
        &payload[firmware_start..firmware_end],
        "firmware contains a non-printable byte",
        "firmware is empty",
    )?;

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

    /// Nibble 2 is rapid trigger following the global settings, measured 2026-08-29: turning
    /// GLOBAL RAPID TRIGGER on wrote it to every key outside a rapid trigger keyset. It must not
    /// alias `Rt` (nibble 3), which is the same feature with the key's own sensitivity.
    #[test]
    fn mode_nibble_2_is_rapid_trigger_from_the_global_settings() {
        let m = Mode::from_value(0x23);
        assert_eq!(m.touch, TouchMode::RtGlobal);
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

    /// The high byte of a 16-bit Layout_Mode value must survive a round trip through `Mode`
    /// unmodified: `wh-cli`'s `dump`/`restore` depends on this being a true identity.
    #[test]
    fn mode_round_trips_the_full_16_bit_value_including_a_non_zero_high_byte() {
        let v = 0x0221u16; // high byte 0x02, touch nibble 0x2 (RtGlobal), advanced nibble 0x1
        let m = Mode::from_value(v);
        assert_eq!(m.high, 0x02);
        assert_eq!(m.touch, TouchMode::RtGlobal);
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

        let f2 = cmd_order(order::PROFILE, &[0x01]).unwrap();
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

    /// The measured frame, checksum included: `5c 04 00 <crc> 70 ff ff ff`.
    #[test]
    fn read_profile_matches_the_measured_frame() {
        let f = read_profile();
        assert_eq!(&f[..8], &[0x5C, 0x04, 0x00, 0x94, 0x70, 0xFF, 0xFF, 0xFF]);
        assert!(f[8..].iter().all(|&b| b == 0));
    }

    /// The measured select frames from `captures/profile-switch.jsonl`, checksum included:
    /// `5c 04 00 94 70 01 ff ff` (select profile 2) and `5c 04 00 94 70 00 ff ff` (select
    /// profile 1). Byte-for-byte identical to `cmd_order(order::PROFILE, &[idx])`, not the
    /// two-byte-then-zero-padded shape a naive reading of the capture might suggest: the
    /// trailing `0xFF 0xFF` is on the wire in both directions.
    #[test]
    fn select_profile_matches_the_measured_frames() {
        let f = select_profile(ProfileNumber::from_one_based(2).unwrap());
        assert_eq!(f[1], 4, "declared length");
        assert_eq!(f[2], cmd::CMD);
        assert_eq!(f[3], 0x94, "checksum measured on the wire");
        assert_eq!(&f[4..8], &[0x70, 0x01, 0xFF, 0xFF]);
        assert!(
            f[8..].iter().all(|&b| b == 0),
            "nothing beyond the declared length"
        );

        let f1 = select_profile(ProfileNumber::from_one_based(1).unwrap());
        assert_eq!(f1[3], 0x94, "checksum measured on the wire");
        assert_eq!(&f1[4..8], &[0x70, 0x00, 0xFF, 0xFF]);
    }

    #[test]
    fn parse_profile_reads_the_zero_based_index_from_the_real_replies() {
        assert_eq!(
            parse_profile(&[0x00, 0x70, 0x00, 0xFF])
                .unwrap()
                .wire_index(),
            0
        );
        assert_eq!(
            parse_profile(&[0x00, 0x70, 0x01, 0xFF])
                .unwrap()
                .wire_index(),
            1
        );
    }

    #[test]
    fn parse_profile_rejects_a_payload_too_short_to_hold_the_index() {
        assert_eq!(
            parse_profile(&[0x00, 0x70]).unwrap_err(),
            DecodeError::Short(2)
        );
    }

    #[test]
    fn parse_profile_rejects_a_reply_to_a_different_sub_order() {
        // payload[1] is 0x50 (order::POLLING), not order::PROFILE: a reply to a different
        // sub-order landing here must be rejected, not misread as profile index 0x00.
        assert_eq!(
            parse_profile(&[0x00, 0x50, 0x00, 0xFF]).unwrap_err(),
            DecodeError::Shape
        );
    }

    #[test]
    fn parse_profile_rejects_an_index_the_board_cannot_report() {
        // 0xFE is past the board's four measured profiles (wire index 0..=3): a garbled or
        // misbehaving reply must not decode into a plausible-looking but meaningless profile.
        assert_eq!(
            parse_profile(&[0x00, 0x70, 0xFE, 0xFF]).unwrap_err(),
            DecodeError::ProfileOutOfRange(0xFE)
        );
    }

    #[test]
    fn profile_number_from_wire_index_converts_zero_based_to_one_based() {
        assert_eq!(ProfileNumber::from_wire_index(0).unwrap().one_based(), 1);
        assert_eq!(ProfileNumber::from_wire_index(3).unwrap().one_based(), 4);
    }

    #[test]
    fn profile_number_from_one_based_accepts_the_full_range_without_underflowing() {
        assert_eq!(ProfileNumber::from_one_based(1).unwrap().one_based(), 1);
        assert_eq!(ProfileNumber::from_one_based(4).unwrap().one_based(), 4);
        assert_eq!(ProfileNumber::from_one_based(1).unwrap().wire_index(), 0);
    }

    #[test]
    fn profile_number_from_one_based_rejects_zero_and_anything_past_four() {
        assert_eq!(
            ProfileNumber::from_one_based(0).unwrap_err(),
            DecodeError::ProfileNumberOutOfRange(0)
        );
        assert_eq!(
            ProfileNumber::from_one_based(5).unwrap_err(),
            DecodeError::ProfileNumberOutOfRange(5)
        );
    }

    /// The two constructors take the same argument type but different conventions: pinned here
    /// as a behavioural difference, not just a doc comment. The same input, `1`, means "profile
    /// 2" through `from_wire_index` (it is a wire index) and "profile 1" through `from_one_based`
    /// (it is already the one-based number).
    #[test]
    fn profile_number_constructors_disagree_on_the_same_input_by_design() {
        assert_eq!(ProfileNumber::from_wire_index(1).unwrap().one_based(), 2);
        assert_eq!(ProfileNumber::from_one_based(1).unwrap().one_based(), 1);
    }

    #[test]
    fn profile_number_from_wire_index_rejects_an_index_the_board_cannot_report() {
        assert_eq!(
            ProfileNumber::from_wire_index(0xFE).unwrap_err(),
            DecodeError::ProfileOutOfRange(0xFE)
        );
        assert_eq!(
            ProfileNumber::from_wire_index(0xFF).unwrap_err(),
            DecodeError::ProfileOutOfRange(0xFF)
        );
        // The two wire indices `saturating_add(1)` would otherwise collapse into the same 255
        // must stay distinct all the way to the error a caller sees.
        assert_ne!(
            ProfileNumber::from_wire_index(0xFE).unwrap_err(),
            ProfileNumber::from_wire_index(0xFF).unwrap_err()
        );
    }

    #[test]
    fn profile_number_display_prints_the_one_based_number() {
        let p = ProfileNumber::from_wire_index(1).unwrap();
        assert_eq!(p.to_string(), "2");
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
    /// `captures/initial-load.jsonl` (frames 1 and 3). The wire declares a 16-byte firmware
    /// field (`App_V1.1.046000`), not the ten bytes a fixed-offset slice would read.
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

    /// A full 60-byte SYNC reply (what the device actually sends) with only the serial length
    /// byte corrupted: an overrunning length prefix is a bogus declaration, not a too-short
    /// payload, so this is `DecodeError::Identity`, not `Short`. The message names SYNC and the
    /// field, not just "unexpected reply shape".
    #[test]
    fn parse_sync_rejects_a_serial_length_prefix_that_overruns_the_payload() {
        let mut payload = vec![0u8; 60];
        payload[8] = 0xFF; // declares a serial far longer than the payload has room for
        let err = parse_sync(&payload).unwrap_err();
        assert_eq!(
            err,
            DecodeError::Identity("serial length prefix overruns the reply")
        );
        assert!(err.to_string().contains("SYNC"));
        assert!(err.to_string().contains("serial"));
    }

    #[test]
    fn parse_sync_rejects_a_firmware_length_prefix_that_overruns_the_payload() {
        let mut payload = vec![0u8; 60];
        payload[8] = 4; // serial: 4 bytes, well within the payload
        payload[9..13].copy_from_slice(b"1234");
        payload[13] = 0xFF; // firmware length prefix declares far more than remains
        let err = parse_sync(&payload).unwrap_err();
        assert_eq!(
            err,
            DecodeError::Identity("firmware length prefix overruns the reply")
        );
        assert!(err.to_string().contains("SYNC"));
        assert!(err.to_string().contains("firmware"));
    }

    /// A serial that declares exactly to the end of the payload leaves no byte for the
    /// firmware's own length prefix. This is a genuinely too-short payload, not a bogus
    /// declaration, so `Short`, not `Identity`, is the right diagnosis here, unlike the overrun
    /// cases above.
    #[test]
    fn parse_sync_rejects_a_serial_that_leaves_no_room_for_the_firmware_length_prefix() {
        let mut payload = vec![0u8; 20];
        payload[8] = 11; // 9 + 11 == 20 == payload.len(): no byte left for the firmware prefix
                         // non-empty serial filling all 11 bytes: exercises the inter-string
                         // guard, not the separate empty-serial rejection above
        payload[9..20].copy_from_slice(b"ABCDEFGHIJK");
        assert_eq!(
            parse_sync(&payload).unwrap_err(),
            DecodeError::Short(payload.len())
        );
    }

    /// Proves the parse follows the declared length prefix, not a hardcoded constant: a shorter
    /// serial (6 bytes, not 16) shifts where the firmware prefix and string are read from, and
    /// both still decode correctly. Uses a full 60-byte payload, the real device's actual reply
    /// size.
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

    /// A zero-length serial is a `DecodeError`, not `Ok(DeviceInfo { serial: "", .. })`: `wh
    /// backup` must never silently write a snapshot that has lost the board's identity.
    #[test]
    fn parse_sync_rejects_an_empty_serial() {
        let mut payload = vec![0u8; 60];
        payload[8] = 0; // serial length: 0
        payload[9] = 10; // firmware length prefix, right after the zero-length serial
        payload[10..20].copy_from_slice(b"V1.2.3.456");
        let err = parse_sync(&payload).unwrap_err();
        assert_eq!(err, DecodeError::Identity("serial is empty"));
        assert!(err.to_string().contains("SYNC"));
        assert!(err.to_string().contains("serial"));
    }

    /// The firmware sibling of the empty-serial test above.
    #[test]
    fn parse_sync_rejects_an_empty_firmware() {
        let mut payload = vec![0u8; 60];
        payload[8] = 16;
        payload[9..25].copy_from_slice(b"SN0123456789ABCD");
        payload[25] = 0; // firmware length: 0
        let err = parse_sync(&payload).unwrap_err();
        assert_eq!(err, DecodeError::Identity("firmware is empty"));
        assert!(err.to_string().contains("SYNC"));
        assert!(err.to_string().contains("firmware"));
    }

    /// A serial carrying a BEL and an ANSI escape sequence (`\x07\x1b[31m...`) must be a
    /// `DecodeError`: `wh dump`'s non-JSON path writes `serial` straight to the terminal, so a
    /// misbehaving device must not smuggle control bytes into it.
    #[test]
    fn parse_sync_rejects_control_bytes_in_the_serial() {
        let mut payload = vec![0u8; 60];
        let serial = b"SN\x07\x1b[31mEVIL\"";
        payload[8] = serial.len() as u8;
        payload[9..9 + serial.len()].copy_from_slice(serial);
        let fw_len_pos = 9 + serial.len();
        payload[fw_len_pos] = 10;
        payload[fw_len_pos + 1..fw_len_pos + 11].copy_from_slice(b"V1.2.3.456");
        let err = parse_sync(&payload).unwrap_err();
        assert_eq!(
            err,
            DecodeError::Identity("serial contains a non-printable byte")
        );
        assert!(err.to_string().contains("SYNC"));
        assert!(err.to_string().contains("serial"));
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
        let err = parse_sync(&payload).unwrap_err();
        assert_eq!(
            err,
            DecodeError::Identity("firmware contains a non-printable byte")
        );
        assert!(err.to_string().contains("SYNC"));
        assert!(err.to_string().contains("firmware"));
    }

    /// The printable-ASCII check runs before `.trim()`, so a tab is corruption, not whitespace
    /// to tidy, and rejects the whole string rather than trimming to its non-tab content. Space
    /// padding, by contrast, is still trimmed.
    #[test]
    fn parse_sync_rejects_a_tab_padded_serial_but_still_trims_space_padding() {
        let mut payload = vec![0u8; 60];
        let tab_padded = b"\tABC\t";
        payload[8] = tab_padded.len() as u8;
        payload[9..9 + tab_padded.len()].copy_from_slice(tab_padded);
        let fw_len_pos = 9 + tab_padded.len();
        payload[fw_len_pos] = 10;
        payload[fw_len_pos + 1..fw_len_pos + 11].copy_from_slice(b"V1.2.3.456");
        assert_eq!(
            parse_sync(&payload).unwrap_err(),
            DecodeError::Identity("serial contains a non-printable byte")
        );

        let mut payload = vec![0u8; 60];
        let space_padded = b" ABC ";
        payload[8] = space_padded.len() as u8;
        payload[9..9 + space_padded.len()].copy_from_slice(space_padded);
        let fw_len_pos = 9 + space_padded.len();
        payload[fw_len_pos] = 10;
        payload[fw_len_pos + 1..fw_len_pos + 11].copy_from_slice(b"V1.2.3.456");
        assert_eq!(parse_sync(&payload).unwrap().serial, "ABC");
    }

    /// The two keyset layouts. `0xFE` has direct write evidence (1 on rapid trigger create, 0 on
    /// delete). `0xFF` correlates with the actuation point keysets the vendor UI showed but has
    /// never been observed being written, so nothing here may write it.
    #[test]
    fn keyset_layout_ids() {
        assert_eq!(layout::KEYSET_AP, 0xFF);
        assert_eq!(layout::KEYSET_RT, 0xFE);
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
