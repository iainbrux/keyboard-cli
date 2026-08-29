//! Key-selection grammar shared by every `--keys` flag.

use crate::keys::{builtin_group, suggestions, usage_for_name};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SelectError {
    #[error("empty item in selector (double comma?)")]
    Empty,
    #[error("descending range '{0}'")]
    Descending(String),
    #[error("bad range end '{0}'")]
    BadRange(String),
    #[error("unknown key or group '{0}'{1}")]
    Unknown(String, String),
    #[error("'{0}' is not a key on this device")]
    NotOnDevice(String),
    #[error(
        "'{0}' is both a key name and a stored group; rename the group (see `wh keys group`) to use it in a selector"
    )]
    AmbiguousWithGroup(String),
}

#[derive(Debug, Clone, PartialEq)]
enum Item {
    All,
    Name(String),
    Range(u8, u8),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
    /// (exclude?, item), applied left to right
    items: Vec<(bool, Item)>,
}

impl Selector {
    pub fn parse(s: &str) -> Result<Self, SelectError> {
        let mut items = Vec::new();
        for raw in s.split(',') {
            let raw = raw.trim();
            if raw.is_empty() {
                return Err(SelectError::Empty);
            }
            let (neg, body) = match raw.strip_prefix('!') {
                Some(rest) => (true, rest.trim()),
                None => (false, raw),
            };
            if body.is_empty() {
                return Err(SelectError::Empty);
            }
            if body.eq_ignore_ascii_case("all") {
                items.push((neg, Item::All));
                continue;
            }
            // Range only when both sides resolve as key names ("f1-f12", "a-z").
            // A bare name wins over a range interpretation ("kpminus" etc.).
            if usage_for_name(body).is_none() {
                if let Some((lhs, rhs)) = body.split_once('-') {
                    if let (Some(a), Some(b)) = (usage_for_name(lhs), usage_for_name(rhs)) {
                        if a > b {
                            return Err(SelectError::Descending(body.to_string()));
                        }
                        items.push((neg, Item::Range(a, b)));
                        continue;
                    }
                    // If exactly one side is a valid key name, the other side
                    // is a typo'd range end. If NEITHER side is a valid key
                    // name, fall through to Item::Name so a hyphenated group
                    // name (e.g. "my-fps") can still resolve.
                    if usage_for_name(lhs).is_some() != usage_for_name(rhs).is_some() {
                        return Err(SelectError::BadRange(body.to_string()));
                    }
                }
            }
            items.push((neg, Item::Name(body.to_ascii_lowercase())));
        }
        Ok(Selector { items })
    }

    /// Resolve to usages, ordered by first inclusion, filtered to `universe`.
    pub fn resolve(
        &self,
        universe: &[u8],
        user_groups: &HashMap<String, Vec<u8>>,
    ) -> Result<Vec<u8>, SelectError> {
        let in_universe = |u: &u8| universe.contains(u);
        let mut out: Vec<u8> = Vec::new();
        for (neg, item) in &self.items {
            let expanded: Vec<u8> = match item {
                Item::All => universe.to_vec(),
                Item::Range(a, b) => (*a..=*b).filter(in_universe).collect(),
                Item::Name(n) => {
                    if let Some(u) = usage_for_name(n) {
                        // A name that resolves as a key AND names a stored user group is
                        // ambiguous, not a case where the key silently wins: a group saved
                        // before this name became a key (e.g. task 19b added "ap"/"rt"/"play"/
                        // "light" as board-function key names) would otherwise be silently
                        // repointed at a single board key the next time the selector runs,
                        // with no warning and exit 0. Refuse rather than resolve either way.
                        // Key names still win over every name that is *not* also a stored
                        // group; only this collision is rejected.
                        if user_groups.contains_key(n) {
                            return Err(SelectError::AmbiguousWithGroup(n.clone()));
                        }
                        // A *positively* named key is a user assertion that the
                        // key exists on this device: absence is an error, not
                        // a silent filter (unlike groups, ranges, and `all`).
                        // Excluding a key that isn't present is a harmless
                        // no-op, so negated names are exempt from this check.
                        if !in_universe(&u) {
                            if *neg {
                                vec![]
                            } else {
                                return Err(SelectError::NotOnDevice(n.clone()));
                            }
                        } else {
                            vec![u]
                        }
                    } else if let Some(g) = builtin_group(n) {
                        g.into_iter().filter(in_universe).collect()
                    } else if let Some(g) = user_groups.get(n) {
                        g.iter().copied().filter(in_universe).collect()
                    } else {
                        let hint = match suggestions(n).first() {
                            Some(s) => format!(" (did you mean '{s}'?)"),
                            None => String::new(),
                        };
                        return Err(SelectError::Unknown(n.clone(), hint));
                    }
                }
            };
            if *neg {
                out.retain(|u| !expanded.contains(u));
            } else {
                for u in expanded {
                    if !out.contains(&u) {
                        out.push(u);
                    }
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn uni() -> Vec<u8> {
        // small universe: a,s,d,w,space,f1,f2
        vec![0x04, 0x16, 0x07, 0x1A, 0x2C, 0x3A, 0x3B]
    }

    #[test]
    fn plain_list() {
        let sel = Selector::parse("w,a,s,d").unwrap();
        assert_eq!(
            sel.resolve(&uni(), &HashMap::new()).unwrap(),
            vec![0x1A, 0x04, 0x16, 0x07]
        );
    }

    #[test]
    fn groups_builtin_and_user() {
        let mut groups = HashMap::new();
        groups.insert("fps".to_string(), vec![0x1A, 0x2C]);
        let sel = Selector::parse("fps,f1").unwrap();
        assert_eq!(
            sel.resolve(&uni(), &groups).unwrap(),
            vec![0x1A, 0x2C, 0x3A]
        );
        let sel2 = Selector::parse("wasd").unwrap();
        assert_eq!(
            sel2.resolve(&uni(), &HashMap::new()).unwrap(),
            vec![0x1A, 0x04, 0x16, 0x07]
        );
    }

    #[test]
    fn all_and_negation() {
        let sel = Selector::parse("all,!space").unwrap();
        let r = sel.resolve(&uni(), &HashMap::new()).unwrap();
        assert!(!r.contains(&0x2C));
        assert_eq!(r.len(), 6);
    }

    #[test]
    fn ranges() {
        let sel = Selector::parse("f1-f2").unwrap();
        assert_eq!(
            sel.resolve(&uni(), &HashMap::new()).unwrap(),
            vec![0x3A, 0x3B]
        );
        // range keys outside universe are silently filtered
        let sel2 = Selector::parse("a-z").unwrap();
        let r = sel2.resolve(&uni(), &HashMap::new()).unwrap();
        assert_eq!(r, vec![0x04, 0x07, 0x16, 0x1A]); // a,d,s,w in usage order
    }

    #[test]
    fn errors_are_helpful() {
        let e = Selector::parse("w,,d").unwrap_err().to_string();
        assert!(e.contains("empty"));
        let sel = Selector::parse("shft").unwrap();
        let e2 = sel
            .resolve(&uni(), &HashMap::new())
            .unwrap_err()
            .to_string();
        assert!(e2.contains("shft"));
        let e3 = Selector::parse("z-a").unwrap_err().to_string();
        assert!(e3.contains("descending"));
    }

    #[test]
    fn named_key_absent_from_universe_errors() {
        // "b" is a valid key name but not present in the test universe.
        let sel = Selector::parse("b").unwrap();
        let err = sel.resolve(&uni(), &HashMap::new()).unwrap_err();
        assert_eq!(err, SelectError::NotOnDevice("b".to_string()));
        assert!(err.to_string().contains("b"));
    }

    #[test]
    fn group_with_absent_key_filters_silently() {
        // A group (user or builtin) containing a key absent from the universe
        // is a query, not an assertion: it filters silently rather than erroring.
        let mut groups = HashMap::new();
        groups.insert(
            "myset".to_string(),
            vec![0x05 /* b, absent */, 0x1A /* w, present */],
        );
        let sel = Selector::parse("myset").unwrap();
        assert_eq!(sel.resolve(&uni(), &groups).unwrap(), vec![0x1A]);
    }

    #[test]
    fn range_typo_diagnostics_are_symmetric() {
        let e = Selector::parse("nonsense-f1").unwrap_err();
        assert!(matches!(e, SelectError::BadRange(ref s) if s == "nonsense-f1"));
        // A hyphenated non-key string still parses as a plain name so that
        // user groups like "my-fps" can resolve.
        let mut groups = HashMap::new();
        groups.insert("my-fps".to_string(), vec![0x1A, 0x2C]);
        let sel = Selector::parse("my-fps").unwrap();
        assert_eq!(sel.resolve(&uni(), &groups).unwrap(), vec![0x1A, 0x2C]);
    }

    #[test]
    fn negated_absent_key_is_a_harmless_noop() {
        // "f12" is a valid key name but absent from the test universe (no F-row
        // past f2, mirroring a 65% board). Excluding it should be a silent
        // no-op, not an error: only a *positive* named key is an assertion
        // that the key exists.
        let sel = Selector::parse("all,!f12").unwrap();
        let r = sel.resolve(&uni(), &HashMap::new()).unwrap();
        assert_eq!(r, uni());
    }

    #[test]
    fn positively_named_absent_key_still_errors() {
        // A universe with no dedicated `w` key: the positive name "w" is
        // still an assertion the key exists, so it must still error.
        let no_w = vec![0x04, 0x16, 0x07, 0x2C, 0x3A, 0x3B]; // a,s,d,space,f1,f2
        let sel = Selector::parse("w").unwrap();
        let err = sel.resolve(&no_w, &HashMap::new()).unwrap_err();
        assert_eq!(err, SelectError::NotOnDevice("w".to_string()));
    }

    /// The regression this fix exists for (review round 1, chunk 7): a user group named "rt"
    /// saved before task 19b added "rt" as a board-function key name. Reproduces the reviewer's
    /// exact measurement: `groups = {"rt": [w,a,s,d]}`, `universe` includes both wasd and the new
    /// board-function usages. A tool that writes to hardware must refuse this selector rather
    /// than silently resolve it as either reading (the key, dropping wasd; or the group, ignoring
    /// the key), since either choice writes somewhere other than what an old, still-valid group
    /// definition asked for.
    #[test]
    fn a_name_that_is_both_a_key_and_a_stored_group_is_refused_not_silently_resolved() {
        let mut groups = HashMap::new();
        groups.insert(
            "rt".to_string(),
            vec![0x1A, 0x04, 0x16, 0x07], // w, a, s, d: legal before "rt" became a key name
        );
        let universe = vec![0x04, 0x07, 0x16, 0x1A, 0xD6, 0xFA, 0xFB, 0xFC];
        let sel = Selector::parse("rt").unwrap();
        let err = sel.resolve(&universe, &groups).unwrap_err();
        assert_eq!(err, SelectError::AmbiguousWithGroup("rt".to_string()));
        assert!(err.to_string().contains("rt"));

        // The negated form must be refused the same way, not silently treated as a no-op or as
        // excluding the group.
        let sel_neg = Selector::parse("!rt").unwrap();
        let err_neg = sel_neg.resolve(&universe, &groups).unwrap_err();
        assert_eq!(err_neg, SelectError::AmbiguousWithGroup("rt".to_string()));
    }

    /// Key names must keep winning for every name that is *not* also a stored group: this fix
    /// must not invert the precedence wholesale, only refuse the specific collision above.
    #[test]
    fn a_key_name_with_no_same_named_group_still_resolves_as_the_key() {
        let groups = HashMap::new(); // no stored group named "rt" here
        let universe = vec![0x04, 0x07, 0x16, 0x1A, 0xFB];
        let sel = Selector::parse("rt").unwrap();
        assert_eq!(sel.resolve(&universe, &groups).unwrap(), vec![0xFB]);
    }

    #[test]
    fn negated_typo_still_errors_as_unknown() {
        // A typo inside a negation is still a typo: SelectError::Unknown,
        // never SelectError::NotOnDevice.
        let sel = Selector::parse("!nonsense").unwrap();
        let err = sel.resolve(&uni(), &HashMap::new()).unwrap_err();
        assert!(matches!(err, SelectError::Unknown(ref n, _) if n == "nonsense"));
    }
}
