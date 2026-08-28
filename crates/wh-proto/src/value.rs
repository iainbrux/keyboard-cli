//! Travel values. Device speaks micrometres (mm × 1000) as LE u16.

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ValueError {
    #[error("{0}mm is out of range ({1}mm to {2}mm)")]
    OutOfRange(f64, f64, f64),
    #[error("not a finite number")]
    NotFinite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Um(pub u16);

impl Um {
    pub fn from_mm(mm: f64, min_mm: f64, max_mm: f64) -> Result<Self, ValueError> {
        if !mm.is_finite() {
            return Err(ValueError::NotFinite);
        }
        if mm < min_mm || mm > max_mm {
            return Err(ValueError::OutOfRange(mm, min_mm, max_mm));
        }
        let um = (mm * 1000.0).round();
        if !(0.0..=65535.0).contains(&um) {
            return Err(ValueError::OutOfRange(mm, min_mm, max_mm));
        }
        Ok(Um(um as u16))
    }
    pub fn to_mm(self) -> f64 {
        self.0 as f64 / 1000.0
    }
    pub fn to_le(self) -> [u8; 2] {
        self.0.to_le_bytes()
    }
    pub fn from_le(lo: u8, hi: u8) -> Self {
        Um(u16::from_le_bytes([lo, hi]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_mm_converts_and_bounds() {
        assert_eq!(Um::from_mm(0.5, 0.0, 4.0).unwrap(), Um(500));
        assert_eq!(Um::from_mm(4.0, 0.0, 4.0).unwrap(), Um(4000));
        assert!(Um::from_mm(4.01, 0.0, 4.0).is_err());
        assert!(Um::from_mm(-0.1, 0.0, 4.0).is_err());
        assert!(Um::from_mm(f64::NAN, 0.0, 4.0).is_err());
    }

    #[test]
    fn from_mm_rejects_values_that_would_overflow_or_underflow_u16() {
        // mm * 1000 exceeds u16::MAX (65535) but is within the (permissive)
        // caller-supplied bounds, so the range check must catch it, not the cast.
        assert!(Um::from_mm(100.0, 0.0, 200.0).is_err());
        // Negative mm within a negative min bound would otherwise saturate to 0.
        assert!(Um::from_mm(-0.5, -1.0, 4.0).is_err());
    }

    #[test]
    fn le_bytes_roundtrip() {
        let v = Um(500);
        assert_eq!(v.to_le(), [0xF4, 0x01]);
        assert_eq!(Um::from_le(0xF4, 0x01), v);
        assert_eq!(v.to_mm(), 0.5);
    }
}
