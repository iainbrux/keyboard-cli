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
        for (name, usages) in &groups {
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
/// selector, then resolve it against a throwaway universe holding one sentinel usage that is
/// bound, in a throwaway group map, only under the exact key `store.set_group` would use. If
/// the result is exactly that sentinel, the grammar read `name` as nothing but a single,
/// non-negated plain name, the only shape that ever reaches the user group lookup at all. Any
/// other reading (a range, a list, a negation) either resolves to something else or fails to
/// parse, and both count as unreachable.
///
/// `all` is checked directly up front because it is genuinely indistinguishable from a
/// reachable single name under this probe: `Item::All` also resolves to "the whole universe",
/// which is exactly what a matching sentinel probe looks like too.
fn group_name_is_reachable(name: &str) -> bool {
    if name.eq_ignore_ascii_case("all") {
        return false;
    }
    let sel = match Selector::parse(name) {
        Ok(sel) => sel,
        Err(_) => return false,
    };
    // 0xFF is not a real HID keyboard usage anywhere in wh_proto::keys::TABLE (the table tops
    // out at 0xE7), so it can never collide with an actual key and mask a false positive.
    const SENTINEL: u8 = 0xFF;
    let universe = [SENTINEL];
    let mut probe: HashMap<String, Vec<u8>> = HashMap::new();
    probe.insert(name.to_string(), vec![SENTINEL]);
    matches!(sel.resolve(&universe, &probe), Ok(v) if v == [SENTINEL])
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
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn negation_shaped_group_name_is_rejected() {
        let dir = test_dir("negation-name");
        let store = Store::at(dir.clone());
        let err = keys(group_cmd("!fps", "w"), &store).unwrap_err();
        assert!(!err.to_string().is_empty());
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
}
