//! The user-facing JSON snapshot of a board's settings. TOML is still read, by extension, for
//! backups written before the format changed; nothing writes it any more.

use serde::{Deserialize, Serialize};
use std::path::Path;
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
    /// Missing entirely from a snapshot file, JSON or pre-JSON TOML alike, deserializes to
    /// `None`, so old backups still parse. Goes through `crate::profile`'s bridge functions since `wh-proto` carries no
    /// serde dependency.
    #[serde(
        default,
        with = "crate::profile",
        skip_serializing_if = "Option::is_none"
    )]
    pub profile: Option<ProfileNumber>,
    /// What took this snapshot: `manual` for `wh backup`, or `auto: <command>` for the backup
    /// every write takes first. `None` means a snapshot from before this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    pub global: GlobalToml,
    pub keys: Vec<KeyToml>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalToml {
    /// The configurator's `"MM" CUSTOM VALUE`, the step size for its `< >` controls, not the
    /// global actuation point (`docs/keysets.md`), which is what every key outside a keyset holds
    /// in layout `0x04`. The alias is the name this field had before it was corrected: real
    /// backups on disk spell it that way and must keep restoring.
    #[serde(alias = "travel_mm")]
    pub custom_value_mm: f64,
    /// Informational only, the dead zones the board reported when the snapshot was taken, which
    /// every measured read reports as `0`. `wh restore` sends the 200 every measured vendor write
    /// carries instead, so editing either of these changes nothing that reaches the wire.
    pub press_dead_mm: f64,
    pub release_dead_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyToml {
    pub name: String,
    pub usage: u8,
    pub ap_mm: f64,
    /// Informational only, derived from `mode_raw` when the snapshot was taken. `wh restore`
    /// never reads it, and `wh dump` builds its own snapshot from a live read rather than a
    /// stored file, so hand-editing a stored file's `rt` changes nothing any command does or
    /// prints. Edit `mode_raw` (its low byte's high nibble) to change what restores.
    pub rt: bool,
    pub rt_press_mm: f64,
    pub rt_release_mm: f64,
    /// Raw Layout_Mode value, restored verbatim so advanced-key modes survive.
    pub mode_raw: u16,
    /// Keyset membership as read from layouts 0xFF and 0xFE. `0` is the value read for keys
    /// outside any keyset. `None` means the field is absent, which is what a snapshot taken
    /// before these fields existed deserialises to, and is not the same thing as `Some(0)`, a
    /// live read that found the key outside any keyset: `wh restore` must tell the two apart, or
    /// an old snapshot would assert "no keyset" for every key and dissolve whatever the board
    /// actually holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ap_keyset: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rt_keyset: Option<u16>,
}

/// Which parser a snapshot file needs. JSON is what `wh backup` writes; TOML is read so backups
/// taken before the format change still restore.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("JSON parse: {0}")]
    Json(#[from] serde_json::Error),
    #[error("TOML parse: {0}")]
    Toml(#[from] toml::de::Error),
}

impl Snapshot {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
    pub fn from_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }
    /// Parses by file extension: `.toml` for a pre-JSON backup, JSON for anything else.
    pub fn from_file_text(path: &Path, text: &str) -> Result<Self, ParseError> {
        if path.extension().is_some_and(|e| e == "toml") {
            Ok(Self::from_toml(text)?)
        } else {
            Ok(Self::from_json(text)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Snapshot {
        Snapshot {
            firmware: "V1.2.3".into(),
            serial: "SN1".into(),
            taken_at: "2026-08-28T12:00:00Z".into(),
            profile: Some(ProfileNumber::from_wire_index(0).unwrap()),
            origin: None,
            global: GlobalToml {
                custom_value_mm: 2.0,
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
                ap_keyset: Some(0),
                rt_keyset: Some(0),
            }],
        }
    }

    #[test]
    fn snapshot_json_round_trips() {
        let snap = sample();
        let back = Snapshot::from_json(&snap.to_json().unwrap()).unwrap();
        assert_eq!(back, snap);
    }

    /// The global record's first field is the configurator's `"MM" CUSTOM VALUE`, not the global
    /// actuation point, so a new snapshot must spell it `custom_value_mm`.
    #[test]
    fn snapshot_json_spells_custom_value_mm() {
        let text = sample().to_json().unwrap();
        assert!(
            text.contains("custom_value_mm") && !text.contains("travel_mm"),
            "a new snapshot must spell custom_value_mm: {text}"
        );
    }

    /// The ruling behind the alias: real backups on the operator's disk spell this field
    /// `travel_mm`, one of which proved a destroy-and-restore hardware test, so a rename that
    /// stopped them loading would be a real loss. Asserts the value that arrives, not just that
    /// the parse succeeded, so the old name has to reach the field rather than merely be tolerated.
    #[test]
    fn snapshot_json_spelling_the_old_travel_mm_still_loads() {
        let old = r#"{"firmware":"V1","serial":"S","taken_at":"t",
            "global":{"travel_mm":2.0,"press_dead_mm":0.2,"release_dead_mm":0.2},"keys":[]}"#;
        let snap = Snapshot::from_json(old).unwrap();
        assert_eq!(snap.global.custom_value_mm, 2.0);
    }

    /// A `None` profile is omitted from the JSON rather than written as `null`, so a snapshot with
    /// unknown provenance reads the same way it did as TOML.
    #[test]
    fn snapshot_json_omits_an_absent_profile() {
        let mut snap = sample();
        snap.profile = None;
        let text = snap.to_json().unwrap();
        assert!(!text.contains("profile"), "profile must be absent: {text}");
        assert_eq!(Snapshot::from_json(&text).unwrap().profile, None);
    }

    /// A `None` keyset field is omitted rather than written as `null`, matching `profile`'s own
    /// omission above: a snapshot that never recorded membership must round-trip as one that never
    /// recorded it, not as one that explicitly recorded "absent".
    #[test]
    fn snapshot_json_omits_absent_keyset_fields() {
        let mut snap = sample();
        snap.keys[0].ap_keyset = None;
        snap.keys[0].rt_keyset = None;
        let text = snap.to_json().unwrap();
        assert!(
            !text.contains("ap_keyset"),
            "ap_keyset must be absent: {text}"
        );
        assert!(
            !text.contains("rt_keyset"),
            "rt_keyset must be absent: {text}"
        );
        let back = Snapshot::from_json(&text).unwrap();
        assert_eq!(back.keys[0].ap_keyset, None);
        assert_eq!(back.keys[0].rt_keyset, None);
    }

    /// A Phase 1 backup is TOML. `from_file_text` must pick the parser from the extension so those
    /// files still restore after the format change.
    #[test]
    fn from_file_text_reads_a_toml_backup_by_extension() {
        let toml_text = r#"
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
        let snap =
            Snapshot::from_file_text(Path::new("00000000000000000001.000000000.toml"), toml_text)
                .unwrap();
        assert_eq!(snap.serial, "SN1");
        assert_eq!(snap.keys.len(), 1);
    }

    #[test]
    fn from_file_text_reads_a_json_backup_by_extension() {
        let snap = sample();
        let text = snap.to_json().unwrap();
        let back =
            Snapshot::from_file_text(Path::new("00000000000000000001.000000000.json"), &text)
                .unwrap();
        assert_eq!(back, snap);
    }

    /// An unrecognised extension is parsed as JSON. Asserting a valid JSON body parses, rather than
    /// that a bad body fails, is what makes this discriminate: a TOML default would reject it.
    #[test]
    fn from_file_text_defaults_to_json_for_an_unknown_extension() {
        let snap = sample();
        let text = snap.to_json().unwrap();
        let back = Snapshot::from_file_text(Path::new("snap.bak"), &text).unwrap();
        assert_eq!(back, snap);
    }

    /// The shape of a real pre-existing snapshot file: no `profile` key anywhere. Written by
    /// hand, not produced by any serializer, so the test proves the parser accepts it on its
    /// own.
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

    /// Origin distinguishes a deliberate `wh backup` from the automatic one every write takes
    /// first. A snapshot without the field still loads.
    #[test]
    fn snapshot_origin_round_trips_and_defaults_to_none() {
        let mut snap = sample();
        snap.origin = Some("auto: set rt".into());
        let back = Snapshot::from_json(&snap.to_json().unwrap()).unwrap();
        assert_eq!(back.origin.as_deref(), Some("auto: set rt"));

        let without = r#"{"firmware":"V1","serial":"S","taken_at":"t",
            "global":{"travel_mm":2.0,"press_dead_mm":0.2,"release_dead_mm":0.2},"keys":[]}"#;
        assert_eq!(Snapshot::from_json(without).unwrap().origin, None);
    }

    /// A snapshot JSON body with neither `ap_keyset` nor `rt_keyset` must still deserialise:
    /// snapshots taken before these fields existed have no way to carry them.
    #[test]
    fn snapshot_json_with_no_keyset_fields_still_parses() {
        let text = r#"{
  "firmware": "V1.2.3",
  "serial": "SN1",
  "taken_at": "2026-08-28T12:00:00Z",
  "global": { "travel_mm": 2.0, "press_dead_mm": 0.2, "release_dead_mm": 0.2 },
  "keys": [
    {
      "name": "w",
      "usage": 26,
      "ap_mm": 1.2,
      "rt": true,
      "rt_press_mm": 0.5,
      "rt_release_mm": 0.5,
      "mode_raw": 32
    }
  ]
}"#;
        let snap = Snapshot::from_json(text).unwrap();
        assert_eq!(snap.keys[0].ap_keyset, None);
        assert_eq!(snap.keys[0].rt_keyset, None);
    }
}
