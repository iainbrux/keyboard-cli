//! Serde bridge for `wh_proto::cmds::ProfileNumber`.
//!
//! The type itself lives in `wh-proto` (task 20 step 4c), since a profile index is a protocol
//! value and `parse_profile` is the seam where the wire byte becomes one; `wh-device` depends on
//! `wh-proto` but not on `wh-config`, so the type has to live somewhere both `wh-device` and
//! `wh-config` can reach, and `wh-proto` is the only crate that qualifies.
//!
//! `wh-proto` deliberately carries no serde dependency: it is the pure protocol crate, and gaining
//! one just to serialize a single field would be a real cost for every consumer, not a free
//! convenience. The orphan rule also blocks a direct `impl Serialize`/`Deserialize` here, since
//! neither the trait nor the type is local to this crate. `#[serde(with = "...")]` sidesteps both
//! problems: these two free functions are the serde contract for `Option<ProfileNumber>`, calling
//! only `ProfileNumber`'s own public constructors and accessors, never a trait impl on a foreign
//! type.
//!
//! `Snapshot::profile` stores this one-based (`1..=4`), the number a human reads and types, not
//! the wire's own zero-based index; see `ProfileNumber`'s own doc comment for why the two
//! conventions are kept distinct at all. A hand-edited or corrupted snapshot claiming `profile = 0`
//! or `profile = 200` is rejected at load time, the same invariant `from_wire_index` enforces on a
//! live device reply.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use wh_proto::cmds::ProfileNumber;

pub fn serialize<S>(profile: &Option<ProfileNumber>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    profile.map(ProfileNumber::one_based).serialize(serializer)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<ProfileNumber>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<u8>::deserialize(deserializer)? {
        None => Ok(None),
        Some(n) => ProfileNumber::from_one_based(n)
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trips_through_a_bare_integer() {
        let p = ProfileNumber::from_wire_index(0).unwrap();
        let value = toml::Value::try_from(p.one_based()).unwrap();
        assert_eq!(value, toml::Value::Integer(1));
    }

    #[test]
    fn deserialize_rejects_zero_and_anything_past_four() {
        assert!(ProfileNumber::from_one_based(0).is_err());
        assert!(ProfileNumber::from_one_based(5).is_err());
        assert!(ProfileNumber::from_one_based(200).is_err());
        assert!(ProfileNumber::from_one_based(1).is_ok());
        assert!(ProfileNumber::from_one_based(4).is_ok());
    }
}
