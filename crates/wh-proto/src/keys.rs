//! Key name ↔ USB HID keyboard usage mapping.

pub const TABLE: &[(&str, u8)] = &[
    ("a", 0x04),
    ("b", 0x05),
    ("c", 0x06),
    ("d", 0x07),
    ("e", 0x08),
    ("f", 0x09),
    ("g", 0x0A),
    ("h", 0x0B),
    ("i", 0x0C),
    ("j", 0x0D),
    ("k", 0x0E),
    ("l", 0x0F),
    ("m", 0x10),
    ("n", 0x11),
    ("o", 0x12),
    ("p", 0x13),
    ("q", 0x14),
    ("r", 0x15),
    ("s", 0x16),
    ("t", 0x17),
    ("u", 0x18),
    ("v", 0x19),
    ("w", 0x1A),
    ("x", 0x1B),
    ("y", 0x1C),
    ("z", 0x1D),
    ("1", 0x1E),
    ("2", 0x1F),
    ("3", 0x20),
    ("4", 0x21),
    ("5", 0x22),
    ("6", 0x23),
    ("7", 0x24),
    ("8", 0x25),
    ("9", 0x26),
    ("0", 0x27),
    ("enter", 0x28),
    ("esc", 0x29),
    ("backspace", 0x2A),
    ("tab", 0x2B),
    ("space", 0x2C),
    ("minus", 0x2D),
    ("equals", 0x2E),
    ("lbracket", 0x2F),
    ("rbracket", 0x30),
    ("backslash", 0x31),
    ("semicolon", 0x33),
    ("quote", 0x34),
    ("grave", 0x35),
    ("comma", 0x36),
    ("period", 0x37),
    ("slash", 0x38),
    ("capslock", 0x39),
    ("f1", 0x3A),
    ("f2", 0x3B),
    ("f3", 0x3C),
    ("f4", 0x3D),
    ("f5", 0x3E),
    ("f6", 0x3F),
    ("f7", 0x40),
    ("f8", 0x41),
    ("f9", 0x42),
    ("f10", 0x43),
    ("f11", 0x44),
    ("f12", 0x45),
    ("printscreen", 0x46),
    ("scrolllock", 0x47),
    ("pause", 0x48),
    ("insert", 0x49),
    ("home", 0x4A),
    ("pageup", 0x4B),
    ("delete", 0x4C),
    ("end", 0x4D),
    ("pagedown", 0x4E),
    ("right", 0x4F),
    ("left", 0x50),
    ("down", 0x51),
    ("up", 0x52),
    ("numlock", 0x53),
    ("kpslash", 0x54),
    ("kpstar", 0x55),
    ("kpminus", 0x56),
    ("kpplus", 0x57),
    ("kpenter", 0x58),
    ("kp1", 0x59),
    ("kp2", 0x5A),
    ("kp3", 0x5B),
    ("kp4", 0x5C),
    ("kp5", 0x5D),
    ("kp6", 0x5E),
    ("kp7", 0x5F),
    ("kp8", 0x60),
    ("kp9", 0x61),
    ("kp0", 0x62),
    ("kpdot", 0x63),
    ("iso", 0x64),
    ("menu", 0x65),
    ("lctrl", 0xE0),
    ("lshift", 0xE1),
    ("lalt", 0xE2),
    ("lgui", 0xE3),
    ("rctrl", 0xE4),
    ("rshift", 0xE5),
    ("ralt", 0xE6),
    ("rgui", 0xE7),
];

pub fn usage_for_name(name: &str) -> Option<u8> {
    let n = name.to_ascii_lowercase();
    TABLE.iter().find(|(k, _)| *k == n).map(|&(_, u)| u)
}

pub fn name_for_usage(usage: u8) -> Option<&'static str> {
    TABLE.iter().find(|&&(_, u)| u == usage).map(|&(k, _)| k)
}

/// The names `builtin_group` recognizes. Kept as the single source of truth so a caller
/// that needs to list, print, or check membership against the builtin groups never has to
/// maintain a second copy that can drift the moment a group is added or renamed.
pub const BUILTIN_GROUPS: &[&str] = &["wasd", "arrows", "mods"];

pub fn builtin_group(name: &str) -> Option<Vec<u8>> {
    let lname = name.to_ascii_lowercase();
    if !BUILTIN_GROUPS.contains(&lname.as_str()) {
        return None;
    }
    let names: &[&str] = match lname.as_str() {
        "wasd" => &["w", "a", "s", "d"],
        "arrows" => &["up", "down", "left", "right"],
        "mods" => &[
            "lctrl", "lshift", "lalt", "lgui", "rctrl", "rshift", "ralt", "rgui",
        ],
        _ => unreachable!("BUILTIN_GROUPS and builtin_group's match are out of sync"),
    };
    Some(names.iter().map(|n| usage_for_name(n).unwrap()).collect())
}

/// Cheap edit-distance suggestions for error messages.
pub fn suggestions(input: &str) -> Vec<&'static str> {
    let n = input.to_ascii_lowercase();
    let mut scored: Vec<(usize, &'static str)> = TABLE
        .iter()
        .map(|&(k, _)| (levenshtein(&n, k), k))
        .filter(|&(d, _)| d <= 2)
        .collect();
    scored.sort();
    scored.into_iter().take(5).map(|(_, k)| k).collect()
}

fn levenshtein(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_usage_roundtrip() {
        assert_eq!(usage_for_name("w"), Some(0x1A));
        assert_eq!(usage_for_name("W"), Some(0x1A)); // case-insensitive
        assert_eq!(usage_for_name("f12"), Some(0x45));
        assert_eq!(usage_for_name("lshift"), Some(0xE1));
        assert_eq!(usage_for_name("nosuchkey"), None);
        assert_eq!(name_for_usage(0x1A), Some("w"));
        assert_eq!(name_for_usage(0x2C), Some("space"));
    }

    #[test]
    fn builtin_groups() {
        assert_eq!(builtin_group("wasd"), Some(vec![0x1A, 0x04, 0x16, 0x07]));
        assert_eq!(builtin_group("arrows"), Some(vec![0x52, 0x51, 0x50, 0x4F]));
        assert_eq!(builtin_group("mods").map(|v| v.len()), Some(8));
        assert_eq!(builtin_group("none"), None);
    }

    #[test]
    fn builtin_groups_const_matches_builtin_group_fn() {
        // Every name BUILTIN_GROUPS claims to recognize must actually resolve, and no other
        // name may sneak past the guard and still resolve. This is what keeps the two in sync
        // once someone adds a group without updating both spots.
        for &name in BUILTIN_GROUPS {
            assert!(
                builtin_group(name).is_some(),
                "BUILTIN_GROUPS lists '{name}' but builtin_group does not recognize it"
            );
        }
        assert_eq!(builtin_group("not-a-builtin-group"), None);
    }

    #[test]
    fn near_matches_for_typos() {
        let s = suggestions("shft");
        assert!(s.contains(&"lshift") || s.contains(&"rshift"));
    }
}
