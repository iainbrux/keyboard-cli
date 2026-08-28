//! Report framing for the Sparklink Playjoy protocol.
//! Port of research/proto/package/src/utils/index.ts (computeCRC / createProtocol).

pub const HEAD: u8 = 0x5C;
pub const REPORT_LEN: usize = 64;
pub const CMD_FAIL: u8 = 0xFF;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum FrameError {
    #[error("payload too long: {0} bytes (max 60)")]
    TooLong(usize),
    #[error("bad magic byte 0x{0:02X} (expected 0x5C)")]
    BadMagic(u8),
    #[error("bad length {0}")]
    BadLength(u8),
    #[error("checksum mismatch: got 0x{got:02X}, want 0x{want:02X}")]
    BadChecksum { got: u8, want: u8 },
    #[error("device reported failure, code 0x{0:02X}")]
    DeviceFail(u8),
    #[error("report shorter than 64 bytes: {0}")]
    Short(usize),
}

pub fn checksum(len: u8, cmd: u8, payload: &[u8]) -> u8 {
    let mut crc = 0x35u8
        .wrapping_add(HEAD)
        .wrapping_add(len)
        .wrapping_add(cmd);
    if let Some(&last) = payload.last() {
        crc = crc.wrapping_add(last);
    }
    crc
}

pub fn frame(cmd: u8, payload: &[u8]) -> Result<[u8; REPORT_LEN], FrameError> {
    if payload.len() > REPORT_LEN - 4 {
        return Err(FrameError::TooLong(payload.len()));
    }
    let len = payload.len() as u8;
    let mut out = [0u8; REPORT_LEN];
    out[0] = HEAD;
    out[1] = len;
    out[2] = cmd;
    out[3] = checksum(len, cmd, payload);
    out[4..4 + payload.len()].copy_from_slice(payload);
    Ok(out)
}

#[derive(Debug, PartialEq)]
pub struct Reply<'a> {
    pub cmd: u8,
    pub payload: &'a [u8],
}

pub fn parse(report: &[u8]) -> Result<Reply<'_>, FrameError> {
    if report.len() < REPORT_LEN {
        return Err(FrameError::Short(report.len()));
    }
    if report[0] != HEAD {
        return Err(FrameError::BadMagic(report[0]));
    }
    let len = report[1] as usize;
    if len > REPORT_LEN - 4 {
        return Err(FrameError::BadLength(report[1]));
    }
    let payload = &report[4..4 + len];
    let want = checksum(report[1], report[2], payload);
    if report[3] != want {
        return Err(FrameError::BadChecksum {
            got: report[3],
            want,
        });
    }
    if report[2] == CMD_FAIL {
        return Err(FrameError::DeviceFail(*payload.first().unwrap_or(&0)));
    }
    Ok(Reply {
        cmd: report[2],
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_matches_known_global_travel_frame() {
        // 5C 0F 29 C9: decoded vendor template, payload ends 0x00
        let payload = [0x01, 0, 0, 0xF4, 0x01, 0xC8, 0, 0xC8, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(checksum(0x0F, 0x29, &payload), 0xC9);
    }

    #[test]
    fn checksum_pins_last_payload_byte_term() {
        // Literal expectations so deleting the payload-last-byte term would fail:
        // 0x35 + 0x5C + 0x02 + 0x29 + 0x02 (last byte) = 0x1BE -> 0xBE
        assert_eq!(checksum(0x02, 0x29, &[0x01, 0x02]), 0xBE);
        // Empty payload: no last-byte term is added.
        // 0x35 + 0x5C + 0x00 + 0x29 = 0xBA
        assert_eq!(checksum(0x00, 0x29, &[]), 0xBA);
    }

    #[test]
    fn frame_lays_out_header_and_pads_to_64() {
        let f = frame(0x29, &[0x01, 0x02]).unwrap();
        assert_eq!(f.len(), 64);
        assert_eq!(&f[..4], &[0x5C, 0x02, 0x29, 0xBE]);
        assert_eq!(&f[4..6], &[0x01, 0x02]);
        assert!(f[6..].iter().all(|&b| b == 0));
    }

    #[test]
    fn frame_with_empty_payload_is_all_zero_body() {
        let f = frame(0x29, &[]).unwrap();
        assert_eq!(&f[..4], &[0x5C, 0x00, 0x29, 0xBA]);
        assert!(f[4..].iter().all(|&b| b == 0));
    }

    #[test]
    fn frame_rejects_oversize_payload() {
        assert!(frame(0x29, &[0u8; 61]).is_err());
    }

    #[test]
    fn parse_roundtrips_and_validates() {
        let f = frame(0x23, &[0x00, 0x1A, 0x14, 0xF4, 0x01]).unwrap();
        let r = parse(&f).unwrap();
        assert_eq!(r.cmd, 0x23);
        assert_eq!(r.payload, &[0x00, 0x1A, 0x14, 0xF4, 0x01]);

        let mut bad = f;
        bad[3] ^= 0xFF;
        assert!(matches!(parse(&bad), Err(FrameError::BadChecksum { .. })));
        bad[0] = 0x00;
        assert!(matches!(parse(&bad), Err(FrameError::BadMagic(0))));
    }

    #[test]
    fn parse_fail_cmd_is_surfaced() {
        let f = frame(0xFF, &[0x01]).unwrap();
        assert!(matches!(parse(&f), Err(FrameError::DeviceFail(_))));
    }
}
