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
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TouchMode {
    Global, // 0x0
    Single, // 0x1
    Rt,     // 0x2
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode {
    pub touch: TouchMode,
    pub advanced: u8,
}

impl Mode {
    pub fn from_value(v: u16) -> Self {
        let b = (v & 0xFF) as u8;
        let touch = match (b >> 4) & 0x0F {
            0x1 => TouchMode::Single,
            0x2 => TouchMode::Rt,
            _ => TouchMode::Global,
        };
        Mode { touch, advanced: b & 0x0F }
    }
    pub fn value(self) -> u16 {
        let t = match self.touch {
            TouchMode::Global => 0x0u8,
            TouchMode::Single => 0x1,
            TouchMode::Rt => 0x2,
        };
        ((t << 4) | (self.advanced & 0x0F)) as u16
    }
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
        assert_eq!(g, GlobalTravel { travel: Um(500), press_dead: Um(200), release_dead: Um(100) });
    }

    #[test]
    fn parse_global_travel_rejects_short() {
        assert!(parse_global_travel(&[0x00, 0, 0]).is_err());
    }

    #[test]
    fn key_record_batches_of_14_with_rw_prefix() {
        let recs: Vec<KeyRecord> = (0u8..20)
            .map(|i| KeyRecord { key: 0x04 + i, layout: layout::RT_PRESS, value: 500 })
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
            KeyRecord { key: 0x1A, layout: layout::RT_PRESS, value: 500 }
        );
    }

    #[test]
    fn mode_nibbles() {
        let m = Mode::from_value(0x23);
        assert_eq!(m.touch, TouchMode::Rt); // high nibble 2
        assert_eq!(m.advanced, 0x03);
        assert_eq!(m.value(), 0x23);
        let g = Mode { touch: TouchMode::Global, advanced: 0x03 };
        assert_eq!(g.value(), 0x03);
    }
}
