//! The UI's one-based profile number, kept distinct from the wire's own zero-based index.

use serde::{Deserialize, Serialize};

/// The board's own bound: the K-001 has four profiles, wire index `0..=3` (task 19b group B,
/// measured).
const MAX_WIRE_INDEX: u8 = 3;

/// The UI's one-based profile number (wire index 0 is "profile 1"). A bare `u8` let the wire's
/// zero-based index and this one-based number be substituted for each other and still compile:
/// review round 1 on task 19b group B found that the natural, wrong call
/// `check_restore_profile(snap.profile, ops::profile(s)?, force)` type-checked and silently
/// inverted the restore safety check this task exists to add. Every place a profile number
/// crosses from the wire into `wh-config`/`wh-cli` (`Snapshot::profile`, `dump`, `backup`,
/// `restore`'s safety check) does so through this type instead, so that call fails to compile.
///
/// `from_wire_index` is the sole conversion point, and it is also where an index the board could
/// never actually report is rejected, rather than silently accepted as a valid-looking but
/// meaningless profile number (the same review round: a misbehaving device echoing back its own
/// request byte, `0xFF`, must not be trusted as "profile 256", and must be distinguishable from
/// a device replying `0xFE`, which a bare `saturating_add(1)` collapsed into the same `255`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProfileNumber(u8);

/// The reason a value could not become a `ProfileNumber`: either a wire index past the board's
/// four real profiles, or (via `Deserialize`, on a hand-edited or corrupted snapshot file) a
/// one-based number outside `1..=4`.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProfileNumberError {
    #[error(
        "board reported profile index {0}, but the board only has 4 profiles (wire index 0..=3)"
    )]
    WireIndexOutOfRange(u8),
    #[error("profile {0} is out of range: the board has 4 profiles, numbered 1..=4")]
    OneBasedOutOfRange(u8),
}

impl ProfileNumber {
    /// Converts the wire's own zero-based index, exactly what `ops::profile` returns, into the
    /// UI's one-based number. Rejects anything past the board's four measured profiles instead
    /// of accepting it: a device reporting an index like `0xFE` or `0xFF` is not a device whose
    /// snapshot provenance should be trusted at all.
    ///
    /// Contrast `from_ui_number`, below, which takes a number that is already one-based. The two
    /// are not interchangeable despite both taking a bare `u8` (review round 3, important 2: a
    /// call site that used one where it meant the other, e.g. `from_wire_index(user_profile)`
    /// where `user_profile` is already the UI's "profile 2", would silently add one and produce
    /// profile 3, and nothing in the argument's type stops that). Call this one only for a value
    /// that came straight off the wire, never for one that already has the UI's own numbering.
    pub fn from_wire_index(idx: u8) -> Result<Self, ProfileNumberError> {
        if idx > MAX_WIRE_INDEX {
            return Err(ProfileNumberError::WireIndexOutOfRange(idx));
        }
        Ok(Self(idx + 1))
    }

    /// Converts a plain one-based number (`1..=4`) directly, rejecting `0` and anything past the
    /// board's four measured profiles the same way `from_wire_index` rejects an impossible wire
    /// index. Exists so a caller that already has a one-based number in hand (`Deserialize`
    /// below, a test building "profile 1", or a future `--profile 2` CLI argument) never has to
    /// fake a wire index by subtracting 1 from it and risking an underflow panic on the boundary
    /// value 0 (review round 2, minor 4): the whole point of this type is to remove that boundary
    /// arithmetic, not relocate it to call sites.
    ///
    /// Contrast `from_wire_index`, above, which takes the wire's own zero-based index. Wrong
    /// usage example, the one this naming exists to make obvious rather than a bug someone finds
    /// later:
    /// ```
    /// # use wh_config::profile::ProfileNumber;
    /// # let user_profile = 2u8; // already one-based, e.g. from a --profile 2 argument
    /// // WRONG: from_wire_index always adds one, so this produces profile 3, not profile 2.
    /// let p = ProfileNumber::from_wire_index(user_profile);
    /// assert_eq!(p.unwrap().one_based(), 3);
    /// // RIGHT:
    /// let p = ProfileNumber::from_ui_number(user_profile);
    /// assert_eq!(p.unwrap().one_based(), 2);
    /// ```
    pub fn from_ui_number(n: u8) -> Result<Self, ProfileNumberError> {
        if n == 0 || n > MAX_WIRE_INDEX + 1 {
            return Err(ProfileNumberError::OneBasedOutOfRange(n));
        }
        Ok(Self(n))
    }

    /// The one-based number as a plain integer, for storing into a TOML snapshot. Display
    /// (below) covers every other use, printing the same number.
    pub fn one_based(self) -> u8 {
        self.0
    }
}

impl std::fmt::Display for ProfileNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for ProfileNumber {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProfileNumber {
    /// Validated the same way `from_wire_index` validates a live read: a snapshot's `profile`
    /// field is stored one-based (`1..=4`), so a hand-edited or corrupted file claiming
    /// `profile = 0` or `profile = 200` must not silently become a `ProfileNumber` that carries
    /// that value, the same invariant `from_wire_index` enforces on a live device reply.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let n = u8::deserialize(deserializer)?;
        Self::from_ui_number(n).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_wire_index_converts_zero_based_to_one_based() {
        assert_eq!(ProfileNumber::from_wire_index(0).unwrap().one_based(), 1);
        assert_eq!(ProfileNumber::from_wire_index(3).unwrap().one_based(), 4);
    }

    #[test]
    fn from_ui_number_accepts_the_full_range_without_underflowing() {
        assert_eq!(ProfileNumber::from_ui_number(1).unwrap().one_based(), 1);
        assert_eq!(ProfileNumber::from_ui_number(4).unwrap().one_based(), 4);
    }

    #[test]
    fn from_ui_number_rejects_zero_and_anything_past_four() {
        assert_eq!(
            ProfileNumber::from_ui_number(0).unwrap_err(),
            ProfileNumberError::OneBasedOutOfRange(0)
        );
        assert_eq!(
            ProfileNumber::from_ui_number(5).unwrap_err(),
            ProfileNumberError::OneBasedOutOfRange(5)
        );
    }

    /// Review round 3, important 2: the two constructors take the same argument type but
    /// different conventions, and this is the exact confusion the rename exists to make
    /// unreachable by mistake. Pinned here as a behavioural difference, not just a doc comment:
    /// the same input, `1`, means "profile 2" through `from_wire_index` (it is a wire index) and
    /// "profile 1" through `from_ui_number` (it is already the UI's own number).
    #[test]
    fn from_wire_index_and_from_ui_number_disagree_on_the_same_input_by_design() {
        assert_eq!(ProfileNumber::from_wire_index(1).unwrap().one_based(), 2);
        assert_eq!(ProfileNumber::from_ui_number(1).unwrap().one_based(), 1);
    }

    #[test]
    fn from_wire_index_rejects_an_index_the_board_cannot_report() {
        assert_eq!(
            ProfileNumber::from_wire_index(0xFE).unwrap_err(),
            ProfileNumberError::WireIndexOutOfRange(0xFE)
        );
        assert_eq!(
            ProfileNumber::from_wire_index(0xFF).unwrap_err(),
            ProfileNumberError::WireIndexOutOfRange(0xFF)
        );
    }

    /// The two wire indices `saturating_add(1)` used to collapse into the same `255` (review
    /// round 1, minor 3) must stay distinct all the way to the error a caller sees.
    #[test]
    fn distinct_out_of_range_wire_indices_produce_distinct_errors() {
        let a = ProfileNumber::from_wire_index(0xFE).unwrap_err();
        let b = ProfileNumber::from_wire_index(0xFF).unwrap_err();
        assert_ne!(a, b);
    }

    #[test]
    fn display_prints_the_one_based_number() {
        let p = ProfileNumber::from_wire_index(1).unwrap();
        assert_eq!(p.to_string(), "2");
    }

    #[test]
    fn serde_round_trips_through_a_bare_integer() {
        let p = ProfileNumber::from_wire_index(0).unwrap();
        let value = toml::Value::try_from(p).unwrap();
        assert_eq!(value, toml::Value::Integer(1));
        let back: ProfileNumber = value.try_into().unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn deserialize_rejects_zero_and_anything_past_four() {
        assert!(toml::Value::Integer(0).try_into::<ProfileNumber>().is_err());
        assert!(toml::Value::Integer(5).try_into::<ProfileNumber>().is_err());
        assert!(toml::Value::Integer(200)
            .try_into::<ProfileNumber>()
            .is_err());
        assert!(toml::Value::Integer(1).try_into::<ProfileNumber>().is_ok());
        assert!(toml::Value::Integer(4).try_into::<ProfileNumber>().is_ok());
    }
}
