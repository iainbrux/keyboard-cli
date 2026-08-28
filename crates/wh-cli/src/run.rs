//! Command dispatch. `keys list`, `keys group`, `dump` and `get` work end to end here; the
//! write commands are stubbed until Task 16 wires them up over the same `Session` plumbing.

use crate::cli::{Cli, Cmd, KeysWhat};
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use wh_config::store::Store;
use wh_device::ops;
use wh_device::session::Session;
use wh_device::transport::Transport;
use wh_proto::select::Selector;

pub fn run(cli: Cli) -> Result<()> {
    // Opened once, here, regardless of which command runs: `Store::open` only resolves a
    // path, it does not touch disk, so it is cheap even for commands that never read it.
    // Keeping the one call site at the top means `resolve_keys` and friends have to take a
    // `&Store` rather than reaching for the user's real config directory a second time.
    let store = Store::open()?;
    match cli.cmd {
        Cmd::Keys { what } => keys(what, &store),
        Cmd::Dump { json } => dump(json),
        Cmd::Get { what } => get(what, &store),
        other => device_cmd(other),
    }
}

/// Open the real device on Windows, or a replay script when WH_REPLAY is set.
fn with_session<R>(f: impl FnOnce(&mut Session<Box<dyn Transport>>) -> Result<R>) -> Result<R> {
    let t: Box<dyn Transport> = if let Ok(path) = std::env::var("WH_REPLAY") {
        let text = std::fs::read_to_string(&path).context("reading WH_REPLAY script")?;
        Box::new(wh_device::replay::ReplayTransport::from_jsonl(&text)?)
    } else {
        #[cfg(windows)]
        {
            Box::new(wh_device::hid::HidTransport::open()?)
        }
        #[cfg(not(windows))]
        {
            bail!(
                "the keyboard is attached to the Windows host: run the Windows build \
                 (bin/wh), or set WH_REPLAY=<capture.jsonl> to use a replay script instead"
            );
        }
    };
    let mut s = Session::new(t);
    f(&mut s)
}

fn snapshot_from_device<T: Transport>(s: &mut Session<T>) -> Result<wh_config::snapshot::Snapshot> {
    let info = ops::device_info(s)?;
    let global = ops::global_travel(s)?;
    let matrix = ops::read_matrix(s)?;
    let mut keys = Vec::new();
    for usage in matrix {
        let ks = ops::read_key_settings(s, usage)?;
        keys.push(wh_config::snapshot::KeyToml {
            name: wh_proto::keys::name_for_usage(usage)
                .map(str::to_string)
                .unwrap_or_else(|| format!("0x{usage:02X}")),
            usage,
            ap_mm: ks.ap.to_mm(),
            rt: ks.rt_enabled(),
            rt_press_mm: ks.rt_press.to_mm(),
            rt_release_mm: ks.rt_release.to_mm(),
            mode_raw: ks.mode.value(),
        });
    }
    Ok(wh_config::snapshot::Snapshot {
        firmware: info.firmware,
        serial: info.serial,
        taken_at: httpdate_now()?,
        global: wh_config::snapshot::GlobalToml {
            travel_mm: global.travel.to_mm(),
            press_dead_mm: global.press_dead.to_mm(),
            release_dead_mm: global.release_dead.to_mm(),
        },
        keys,
    })
}

/// Returns the current time as an RFC3339 UTC timestamp, e.g. `"2026-08-28T12:00:00Z"`, the
/// shape `wh-config`'s `Snapshot::taken_at` documents and its own roundtrip test uses. A
/// `unix:<secs>` string would technically be "informational" too, but this field exists so a
/// human can pick the right backup out of twenty others during a recovery, and a raw epoch
/// count is not that.
///
/// Implemented inline with the standard days-from-civil algorithm rather than by adding a
/// date crate: wh-cli cross-compiles for Windows, and a new dependency has to earn surviving
/// that build. See `civil_from_days` for the algorithm itself.
fn httpdate_now() -> Result<String> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock is set before the Unix epoch")?
        .as_secs();
    Ok(rfc3339_from_unix_secs(secs))
}

fn rfc3339_from_unix_secs(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Howard Hinnant's days-from-civil / civil-from-days algorithm: converts a day count since
/// the Unix epoch (1970-01-01) into a proleptic-Gregorian (year, month, day). See
/// http://howardhinnant.github.io/date_algorithms.html for the derivation; this is a direct
/// port, not a reinvention, chosen because it is exact for every day this side of year 0 and
/// needs no lookup tables.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn dump(json: bool) -> Result<()> {
    with_session(|s| {
        let snap = snapshot_from_device(s)?;
        if json {
            println!("{}", serde_json::to_string_pretty(&snap)?);
        } else {
            println!("{} (fw {})", snap.serial, snap.firmware);
            println!(
                "global: travel {:.2}mm, dead {:.2}/{:.2}mm",
                snap.global.travel_mm, snap.global.press_dead_mm, snap.global.release_dead_mm
            );
            println!(
                "{:<12} {:>6} {:>4} {:>8} {:>8}",
                "key", "ap", "rt", "press", "release"
            );
            for k in &snap.keys {
                println!(
                    "{:<12} {:>5.2}m {:>4} {:>7.2}m {:>7.2}m",
                    k.name,
                    k.ap_mm,
                    if k.rt { "on" } else { "off" },
                    k.rt_press_mm,
                    k.rt_release_mm
                );
            }
        }
        Ok(())
    })
}

fn resolve_keys<T: Transport>(
    s: &mut Session<T>,
    arg: &crate::cli::KeysArg,
    store: &Store,
) -> Result<Vec<u8>> {
    let universe = ops::read_matrix(s)?;
    if arg.pick {
        return crate::picker::pick(&universe);
    }
    // Clap's `required_unless_present = "pick"` makes this unreachable today, but that
    // guarantee lives in cli.rs, a different file from here, so a later change to the clap
    // attributes should not be able to turn a missing selector into a crash.
    let keys = arg
        .keys
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("no key selector given: pass --keys or --pick"))?;
    let sel = Selector::parse(keys)?;
    let usages = sel.resolve(&universe, &store.groups()?)?;
    if usages.is_empty() {
        bail!("selector matches no keys on this board");
    }
    Ok(usages)
}

fn get(what: crate::cli::GetWhat, store: &Store) -> Result<()> {
    with_session(|s| {
        let (arg, show_rt) = match &what {
            crate::cli::GetWhat::Rt(a) => (a, true),
            crate::cli::GetWhat::Ap(a) => (a, false),
        };
        for usage in resolve_keys(s, arg, store)? {
            let ks = ops::read_key_settings(s, usage)?;
            let name = wh_proto::keys::name_for_usage(usage).unwrap_or("?");
            if show_rt {
                println!(
                    "{name}: rt {} press {:.2}mm release {:.2}mm",
                    if ks.rt_enabled() { "on" } else { "off" },
                    ks.rt_press.to_mm(),
                    ks.rt_release.to_mm()
                );
            } else {
                println!("{name}: ap {:.2}mm", ks.ap.to_mm());
            }
        }
        Ok(())
    })
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

    #[test]
    fn rfc3339_at_the_unix_epoch() {
        assert_eq!(rfc3339_from_unix_secs(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn rfc3339_on_a_leap_day() {
        // 2020-02-29T00:00:00Z; 2020 is a leap year, so this date exists at all only if the
        // days-from-civil arithmetic gets the leap rule right.
        assert_eq!(
            rfc3339_from_unix_secs(1_582_934_400),
            "2020-02-29T00:00:00Z"
        );
    }

    #[test]
    fn rfc3339_crosses_a_month_boundary_right_after_a_leap_day() {
        // The day immediately after the leap day above: 2020-03-01T00:00:00Z. An off-by-one
        // in the leap-year handling would show up here as 2020-02-30 or 2020-03-02, not as an
        // obviously wrong year, which is exactly the kind of mistake that stays invisible
        // until someone reads a backup filed under the wrong day.
        assert_eq!(
            rfc3339_from_unix_secs(1_583_020_800),
            "2020-03-01T00:00:00Z"
        );
    }

    #[test]
    fn rfc3339_carries_the_time_of_day() {
        // 2026-08-28T12:34:56Z, so the test also pins that hours/minutes/seconds are not
        // silently dropped or truncated to midnight.
        assert_eq!(
            rfc3339_from_unix_secs(1_787_920_496),
            "2026-08-28T12:34:56Z"
        );
    }
}
