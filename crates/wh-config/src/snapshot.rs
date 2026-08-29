//! The user-facing TOML snapshot of a board's settings.

use serde::{Deserialize, Serialize};
use wh_proto::cmds::ProfileNumber;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub firmware: String,
    pub serial: String,
    pub taken_at: String, // RFC3339, informational
    /// The board's active profile at the moment this snapshot was taken. `ProfileNumber` (not a
    /// bare `u8`) so the UI's one-based numbering can never be mixed up with the wire's own
    /// zero-based index at a call site (task 19b group B, review round 1 finding 2; the type
    /// itself moved into `wh-proto` at task 20 step 4c, see its own doc comment there). `None`
    /// means this snapshot predates profile recording, so its provenance is unknown, not that the
    /// board has no active profile (every board always has one). Absent entirely from a
    /// snapshot's TOML deserializes to `None` (serde's own behaviour for a missing `Option`
    /// field), so backups taken before this field existed still parse. `wh-proto` carries no
    /// serde dependency, so this field goes through `crate::profile`'s bridge functions
    /// (`#[serde(with = "...")]`) rather than a direct impl on `ProfileNumber`.
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
    /// Informational only, derived from `mode_raw` at the moment the snapshot was taken
    /// (whole-branch review): `wh restore` never reads this field, since it writes `mode_raw`
    /// back verbatim. Hand-editing `rt` in a snapshot file changes what `dump`-style tooling
    /// would print about that key, not what `wh restore` writes to the board; to actually change
    /// whether a key restores with rapid trigger on, edit `mode_raw` (the touch nibble, the high
    /// nibble of its low byte) instead.
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

    /// A snapshot taken before profile recording existed serializes with `profile: None`, which
    /// omits the `profile` key from the TOML entirely (task 19b group B: `None` is the "provenance
    /// unknown" case, not "no active profile"). Round-tripped both ways here, not just parsed:
    /// `to_toml` must not emit a `profile` line for `None` (it would otherwise emit something
    /// TOML has no syntax for, since TOML has no null), and `from_toml` on the resulting text
    /// must come back with `profile` still absent, not defaulted to `Some(0)` or any other value.
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

    /// The literal shape an operator's real, pre-existing snapshot file takes: no `profile` key
    /// anywhere, written by hand here (not round-tripped through `to_toml`) so this test does not
    /// depend on the serializer's own behaviour for `None` to prove the parser accepts it.
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
