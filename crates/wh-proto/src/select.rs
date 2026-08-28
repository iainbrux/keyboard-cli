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
                    if usage_for_name(lhs).is_some() {
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
                        vec![u].into_iter().filter(in_universe).collect()
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
        assert_eq!(sel.resolve(&uni(), &HashMap::new()).unwrap(), vec![0x1A, 0x04, 0x16, 0x07]);
    }

    #[test]
    fn groups_builtin_and_user() {
        let mut groups = HashMap::new();
        groups.insert("fps".to_string(), vec![0x1A, 0x2C]);
        let sel = Selector::parse("fps,f1").unwrap();
        assert_eq!(sel.resolve(&uni(), &groups).unwrap(), vec![0x1A, 0x2C, 0x3A]);
        let sel2 = Selector::parse("wasd").unwrap();
        assert_eq!(sel2.resolve(&uni(), &HashMap::new()).unwrap(), vec![0x1A, 0x04, 0x16, 0x07]);
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
        assert_eq!(sel.resolve(&uni(), &HashMap::new()).unwrap(), vec![0x3A, 0x3B]);
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
        let e2 = sel.resolve(&uni(), &HashMap::new()).unwrap_err().to_string();
        assert!(e2.contains("shft"));
        let e3 = Selector::parse("z-a").unwrap_err().to_string();
        assert!(e3.contains("descending"));
    }
}
