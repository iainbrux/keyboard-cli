//! The user-facing TOML snapshot of a board's settings.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub firmware: String,
    pub serial: String,
    pub taken_at: String, // RFC3339, informational
    pub global: GlobalToml,
    pub keys: Vec<KeyToml>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalToml {
    pub travel_mm: f64,
    pub press_dead_mm: f64,
    pub release_dead_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyToml {
    pub name: String,
    pub usage: u8,
    pub ap_mm: f64,
    pub rt: bool,
    pub rt_press_mm: f64,
    pub rt_release_mm: f64,
    /// Raw Layout_Mode value, restored verbatim so advanced-key modes survive.
    pub mode_raw: u16,
}

impl Snapshot {
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
    pub fn from_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_toml_roundtrip() {
        let snap = Snapshot {
            firmware: "V1.2.3".into(),
            serial: "SN1".into(),
            taken_at: "2026-08-28T12:00:00Z".into(),
            global: GlobalToml {
                travel_mm: 2.0,
                press_dead_mm: 0.2,
                release_dead_mm: 0.2,
            },
            keys: vec![KeyToml {
                name: "w".into(),
                usage: 0x1A,
                ap_mm: 1.2,
                rt: true,
                rt_press_mm: 0.5,
                rt_release_mm: 0.5,
                mode_raw: 0x20,
            }],
        };
        let text = snap.to_toml().unwrap();
        let back = Snapshot::from_toml(&text).unwrap();
        assert_eq!(back, snap);
    }
}
