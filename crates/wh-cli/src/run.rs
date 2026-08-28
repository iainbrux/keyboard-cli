//! Command dispatch. `keys list` and `keys group` work end to end here; every command that
//! needs the physical keyboard is stubbed until Tasks 15 and 16 wire up a real `Session`.

use crate::cli::{Cli, Cmd, KeysWhat};
use anyhow::{bail, Result};
use std::collections::HashMap;
use wh_config::store::Store;
use wh_proto::select::Selector;

pub fn run(cli: Cli) -> Result<()> {
    match cli.cmd {
        Cmd::Keys { what } => {
            let store = Store::open()?;
            keys(what, &store)
        }
        other => device_cmd(other),
    }
}

fn keys(what: KeysWhat, store: &Store) -> Result<()> {
    match what {
        KeysWhat::List => list_keys(store),
        KeysWhat::Group { name, selector } => group(store, &name, &selector),
    }
}

fn list_keys(store: &Store) -> Result<()> {
    println!("keys:");
    for (name, usage) in wh_proto::keys::TABLE {
        println!("  {name:<12} 0x{usage:02X}");
    }
    println!(
        "\nbuiltin groups: {}",
        wh_proto::keys::BUILTIN_GROUPS.join(", ")
    );
    println!("selector keyword: all (every key on the board, not a stored group)");
    let groups = store.groups()?;
    if !groups.is_empty() {
        println!("user groups:");
        // HashMap iteration order is unspecified and would otherwise vary between runs, so
        // sort by name for stable, diffable output.
        let mut sorted: Vec<_> = groups.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        for (name, usages) in sorted {
            let names: Vec<_> = usages
                .iter()
                .filter_map(|&u| wh_proto::keys::name_for_usage(u))
                .collect();
            println!("  {name:<12} {}", names.join(","));
        }
    }
    Ok(())
}

fn group(store: &Store, name: &str, selector: &str) -> Result<()> {
    // The selector grammar always lowercases a bare name before looking it up in the user
    // group map (see Selector::resolve), so a group stored under its literal, possibly mixed
    // case, spelling would be unreachable by anything but an exact-case retype. Normalize here
    // once, at the one place a group is created, so storage and lookup agree.
    let name = name.to_ascii_lowercase();
    if wh_proto::keys::usage_for_name(&name).is_some()
        || wh_proto::keys::builtin_group(&name).is_some()
    {
        bail!("'{name}' is already a key or builtin group name");
    }
    if !group_name_is_reachable(&name) {
        bail!(
            "'{name}' cannot be used as a group name: the selector grammar would not read it \
             back as a plain name (for example it looks like a range, 'all', a negation, or a \
             list), so the group would be unreachable once saved"
        );
    }
    let sel = Selector::parse(selector)?;
    // resolve against the full static table (device not needed to define a group)
    let universe: Vec<u8> = wh_proto::keys::TABLE.iter().map(|&(_, u)| u).collect();
    let usages = sel.resolve(&universe, &store.groups()?)?;
    if usages.is_empty() {
        bail!("selector resolves to no keys");
    }
    store.set_group(&name, &usages)?;
    println!("group '{name}' = {} keys", usages.len());
    Ok(())
}

/// Reports whether `name` (already lowercased) would resolve back to the group stored under it
/// if it were later typed as a bare `--keys` token.
///
/// Rather than hand listing the shapes the grammar treats specially (a blacklist that drifts
/// the moment `Selector::parse` changes), this asks the grammar itself: parse `name` as a
/// selector, then resolve it against a throwaway two-element universe `[a, b]` holding one
/// sentinel usage `a` that is bound, in a throwaway group map, only under the exact key
/// `store.set_group` would use. If the result is exactly `[a]`, the grammar read `name` as
/// nothing but a single, non-negated plain name, the only shape that ever reaches the user
/// group lookup at all. Any other reading either resolves to something else or fails to parse,
/// and both count as unreachable:
/// - a range or a plain list of real key names resolves to real key usages, never `[a]`;
/// - `Item::All` resolves to the *whole* universe, `[a, b]`, not `[a]` alone, which is exactly
///   why the universe needs two elements: a one-element universe made `all` indistinguishable
///   from a genuine group hit, since both would resolve to the same single sentinel;
/// - a negated item can only ever shrink an already-empty accumulator, so it can never produce
///   `[a]` either.
fn group_name_is_reachable(name: &str) -> bool {
    let sel = match Selector::parse(name) {
        Ok(sel) => sel,
        Err(_) => return false,
    };
    let (a, b) = two_usages_absent_from_table();
    let universe = [a, b];
    let mut probe: HashMap<String, Vec<u8>> = HashMap::new();
    probe.insert(name.to_string(), vec![a]);
    matches!(sel.resolve(&universe, &probe), Ok(v) if v == [a])
}

/// Picks two usage bytes that do not appear anywhere in `wh_proto::keys::TABLE`, chosen at
/// runtime rather than hardcoded so the choice can never rot as the table grows. Used as
/// sentinels in `group_name_is_reachable`'s probe universe: since neither value is a real key,
/// neither can collide with one and mask a false positive.
fn two_usages_absent_from_table() -> (u8, u8) {
    let mut free = (0u8..=u8::MAX).filter(|&u| wh_proto::keys::name_for_usage(u).is_none());
    let a = free
        .next()
        .expect("wh_proto::keys::TABLE occupies every u8 usage code, no sentinel is free");
    let b = free.next().expect(
        "wh_proto::keys::TABLE occupies all but one u8 usage code, no second sentinel is free",
    );
    (a, b)
}

fn device_cmd(_cmd: Cmd) -> Result<()> {
    bail!("this command needs the keyboard and is not wired up yet")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::KeysWhat;

    fn test_dir(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("wh-cli-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    fn group_cmd(name: &str, selector: &str) -> KeysWhat {
        KeysWhat::Group {
            name: name.to_string(),
            selector: selector.to_string(),
        }
    }

    #[test]
    fn range_shaped_group_name_is_rejected() {
        let dir = test_dir("range-shaped");
        let store = Store::at(dir.clone());
        let err = keys(group_cmd("f1-fps", "w"), &store).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("f1-fps")
                || err.to_string().to_lowercase().contains("range"),
            "unexpected error: {err}"
        );
        // Pins that rejection actually means nothing was persisted, not merely that some
        // unrelated error string happened to satisfy the substring check above.
        assert!(store.groups().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn all_is_rejected_as_a_group_name() {
        let dir = test_dir("all-name");
        let store = Store::at(dir.clone());
        let err = keys(group_cmd("all", "w"), &store).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("all"),
            "unexpected error: {err}"
        );
        assert!(store.groups().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn negation_shaped_group_name_is_rejected() {
        let dir = test_dir("negation-name");
        let store = Store::at(dir.clone());
        let err = keys(group_cmd("!fps", "w"), &store).unwrap_err();
        assert!(!err.to_string().is_empty());
        assert!(store.groups().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Regression test for the hole a single-element probe universe left open: `"wasd,all"`
    /// parses as a builtin-group reference filtered to nothing, followed by `Item::All`, and
    /// under a one-element probe universe that combination resolved to exactly the sentinel,
    /// indistinguishable from a genuine group hit. A plain list like `"w,a"` would not have
    /// caught this, since it was already rejected before this fix (it never reaches the user
    /// group lookup either).
    #[test]
    fn comma_shaped_name_combining_all_is_rejected() {
        let dir = test_dir("comma-all");
        let store = Store::at(dir.clone());
        let err = keys(group_cmd("wasd,all", "w,a"), &store).unwrap_err();
        assert!(!err.to_string().is_empty());
        assert!(store.groups().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plain_legal_group_name_is_still_accepted() {
        let dir = test_dir("plain-legal");
        let store = Store::at(dir.clone());
        keys(group_cmd("fps", "w,a,s,d"), &store).unwrap();
        let groups = store.groups().unwrap();
        assert!(groups.contains_key("fps"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn other_legal_names_are_accepted() {
        for name in ["my-fps", "gaming", "left_hand", "grp1"] {
            let dir = test_dir(&format!("legal-{name}"));
            let store = Store::at(dir.clone());
            keys(group_cmd(name, "w,a"), &store)
                .unwrap_or_else(|e| panic!("expected '{name}' to be accepted, got: {e}"));
            assert!(store.groups().unwrap().contains_key(name));
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
