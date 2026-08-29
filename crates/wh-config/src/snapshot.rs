//! The user-facing TOML snapshot of a board's settings.

use serde::{Deserialize, Serialize};
use wh_proto::cmds::ProfileNumber;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub firmware: String,
    pub serial: String,
    pub taken_at: String, // RFC3339, informational
    /// The board's active profile when this snapshot was taken. `ProfileNumber`, not a bare
    /// `u8`, so the UI's one-based numbering can never be confused with the wire's zero-based
    /// index. `None` means the snapshot's profile provenance is unknown (it predates profile
    /// recording), never that the board had no active profile: every board always has one.
    /// Missing entirely from a snapshot's TOML deserializes to `None`, so old backups still
    /// parse. Goes through `crate::profile`'s bridge functions since `wh-proto` carries no
    /// serde dependency.
    #[serde(default, with = "crate::profile")]
    pub profile: Option<ProfileNumber>,
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
    /// Informational only, derived from `mode_raw` when the snapshot was taken. `wh restore`
    /// writes `mode_raw` back verbatim and never reads this field, so hand-editing `rt` changes
    /// only what tooling prints about the key, not what gets restored. To change whether a key
    /// restores with rapid trigger on, edit `mode_raw` (its low byte's high nibble) instead.
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
            profile: Some(ProfileNumber::from_wire_index(0).unwrap()),
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

    /// A `None` profile (provenance unknown) serializes with the `profile` key omitted, since
    /// TOML has no null. Round-tripped both ways: `to_toml` must not emit a `profile` line, and
    /// `from_toml` on the result must come back with `profile` still absent.
    #[test]
    fn snapshot_with_no_profile_round_trips_with_the_field_absent() {
        let snap = Snapshot {
            firmware: "V1.2.3".into(),
            serial: "SN1".into(),
            taken_at: "2026-08-28T12:00:00Z".into(),
            profile: None,
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
        assert!(
            !text.contains("profile"),
            "a None profile must not appear in the TOML at all: {text}"
        );
        let back = Snapshot::from_toml(&text).unwrap();
        assert_eq!(back.profile, None);
        assert_eq!(back, snap);
    }

    /// The shape of a real pre-existing snapshot file: no `profile` key anywhere. Written by
    /// hand, not round-tripped through `to_toml`, so the test proves the parser accepts it
    /// independent of the serializer.
    #[test]
    fn snapshot_toml_with_no_profile_key_at_all_still_parses() {
        let text = r#"
firmware = "V1.2.3"
serial = "SN1"
taken_at = "2026-08-28T12:00:00Z"

[global]
travel_mm = 2.0
press_dead_mm = 0.2
release_dead_mm = 0.2

[[keys]]
name = "w"
usage = 26
ap_mm = 1.2
rt = true
rt_press_mm = 0.5
rt_release_mm = 0.5
mode_raw = 32
"#;
        let snap = Snapshot::from_toml(text).unwrap();
        assert_eq!(snap.profile, None);
    }
}
