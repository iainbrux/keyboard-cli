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
}
