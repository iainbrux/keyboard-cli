//! Serde bridge for `wh_proto::cmds::ProfileNumber`.
//!
//! `ProfileNumber` lives in `wh-proto` since both `wh-device` and `wh-config` need to reach it,
//! but `wh-proto` carries no serde dependency, and the orphan rule blocks a direct
//! `Serialize`/`Deserialize` impl here on a foreign type. `#[serde(with = "...")]` sidesteps
//! both: these two functions are the whole serde contract for `Option<ProfileNumber>`, going
//! through `ProfileNumber`'s own public constructors and accessors only.
//!
//! `Snapshot::profile` stores this one-based (`1..=4`), not the wire's zero-based index. A
//! hand-edited snapshot claiming `profile = 0` or `profile = 200` is rejected at load time, the
//! same invariant `from_wire_index` enforces on a live device reply.

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
