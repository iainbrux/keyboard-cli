//! Command dispatch. Every command in the `wh` tree, read and write alike, runs through here.

use crate::cli::{Cli, Cmd, KeysWhat, SetWhat};
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::io::Write;
use wh_config::store::Store;
use wh_device::ops;
use wh_device::session::Session;
use wh_device::transport::Transport;
use wh_proto::cmds::{self, layout, KeyRecord};
use wh_proto::select::Selector;
use wh_proto::value::Um;

pub fn run(cli: Cli) -> Result<()> {
    // Opened once, here, regardless of which command runs: `Store::open` only resolves a
    // path, it does not touch disk, so it is cheap even for commands that never read it.
    // Keeping the one call site at the top means every command function below has to take
    // a `&Store` rather than reaching for the user's real config directory a second time.
    let store = Store::open()?;
    match cli.cmd {
        Cmd::Keys { what } => keys(what, &store),
        Cmd::Dump { json } => dump(json),
        Cmd::Get { what } => get(what, &store),
        Cmd::Set { what } => set(what, &store),
        Cmd::Backup { to } => backup(to, &store),
        Cmd::Restore { file, last } => restore(file, last, &store),
        Cmd::Selftest => selftest(),
    }
}

/// Treats `WH_REPLAY=` (present but empty, distinct from unset) the same as unset, rather
/// than as a request to read a file literally named the empty string. `env::var` returns
/// `Ok(String::new())` for that case, which would otherwise surface as a confusing I/O error
/// instead of falling back to the real device.
fn non_empty_replay_path(raw: Result<String, std::env::VarError>) -> Option<String> {
    raw.ok().filter(|p| !p.is_empty())
}

/// Open the real device on Windows, or a replay script when WH_REPLAY is set to a non-empty
/// path.
fn with_session<R>(f: impl FnOnce(&mut Session<Box<dyn Transport>>) -> Result<R>) -> Result<R> {
    let t: Box<dyn Transport> =
        if let Some(path) = non_empty_replay_path(std::env::var("WH_REPLAY")) {
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

/// A key's display name, falling back to its hex usage code (e.g. `"0x50"`) when it isn't in
/// `wh_proto::keys::TABLE`. Shared by `dump` and `get` so a board with unnamed usages prints
/// the same, distinguishable label in both, rather than `get` collapsing every unnamed key to
/// an indistinguishable literal `"?"`.
fn key_label(usage: u8) -> String {
    wh_proto::keys::name_for_usage(usage)
        .map(str::to_string)
        .unwrap_or_else(|| format!("0x{usage:02X}"))
}

fn snapshot_from_device<T: Transport>(s: &mut Session<T>) -> Result<wh_config::snapshot::Snapshot> {
    let info = ops::device_info(s)?;
    let global = ops::global_travel(s)?;
    let matrix = ops::read_matrix(s)?;
    let mut keys = Vec::new();
    for usage in matrix {
        let ks = ops::read_key_settings(s, usage)?;
        keys.push(wh_config::snapshot::KeyToml {
            name: key_label(usage),
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
    // Locked once, here, and moved into the closure below: `writeln!`'s `Result` (unlike
    // `println!`, which panics on a write failure) lets a reader that stops early, e.g. `wh
    // dump | head -1`, surface as an ordinary `Err` carrying an `io::Error`, which `main`
    // recognises and exits on quietly instead of reporting as a real failure.
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    with_session(|s| {
        let snap = snapshot_from_device(s)?;
        if json {
            writeln!(out, "{}", serde_json::to_string_pretty(&snap)?)?;
        } else {
            writeln!(out, "{} (fw {})", snap.serial, snap.firmware)?;
            writeln!(
                out,
                "global: travel {:.2}mm, dead {:.2}/{:.2}mm",
                snap.global.travel_mm, snap.global.press_dead_mm, snap.global.release_dead_mm
            )?;
            writeln!(
                out,
                "{:<12} {:>6} {:>4} {:>8} {:>8}",
                "key", "ap", "rt", "press", "release"
            )?;
            for k in &snap.keys {
                writeln!(
                    out,
                    "{:<12} {:>4.2}mm {:>4} {:>6.2}mm {:>6.2}mm",
                    k.name,
                    k.ap_mm,
                    if k.rt { "on" } else { "off" },
                    k.rt_press_mm,
                    k.rt_release_mm
                )?;
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

/// Resolves `arg`'s selector against every key `wh_proto::keys::TABLE` knows about, rather
/// than the live board's matrix, so `--dry-run` can preview the exact frames a write would
/// send without opening a session at all: no device required, no report sent, not even a
/// read. This is the same static-table universe `group()` below already resolves new group
/// definitions against for the same reason (defining a group needs no attached board either).
/// The cost is that it cannot tell a key genuinely on this board from one only known to the
/// protocol; that check happens for real at write time, through `resolve_keys`.
fn resolve_keys_offline(arg: &crate::cli::KeysArg, store: &Store) -> Result<Vec<u8>> {
    if arg.pick {
        bail!("--pick needs the live board and cannot be combined with --dry-run");
    }
    let keys = arg
        .keys
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("no key selector given: pass --keys or --pick"))?;
    let sel = Selector::parse(keys)?;
    let universe: Vec<u8> = wh_proto::keys::TABLE.iter().map(|&(_, u)| u).collect();
    let usages = sel.resolve(&universe, &store.groups()?)?;
    if usages.is_empty() {
        bail!("selector matches no keys");
    }
    Ok(usages)
}

fn get(what: crate::cli::GetWhat, store: &Store) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    with_session(|s| {
        let (arg, show_rt) = match &what {
            crate::cli::GetWhat::Rt(a) => (a, true),
            crate::cli::GetWhat::Ap(a) => (a, false),
        };
        for usage in resolve_keys(s, arg, store)? {
            let ks = ops::read_key_settings(s, usage)?;
            let name = key_label(usage);
            if show_rt {
                writeln!(
                    out,
                    "{name}: rt {} press {:.2}mm release {:.2}mm",
                    if ks.rt_enabled() { "on" } else { "off" },
                    ks.rt_press.to_mm(),
                    ks.rt_release.to_mm()
                )?;
            } else {
                writeln!(out, "{name}: ap {:.2}mm", ks.ap.to_mm())?;
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
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "keys:")?;
    for (name, usage) in wh_proto::keys::TABLE {
        writeln!(out, "  {name:<12} 0x{usage:02X}")?;
    }
    writeln!(
        out,
        "\nbuiltin groups: {}",
        wh_proto::keys::BUILTIN_GROUPS.join(", ")
    )?;
    writeln!(
        out,
        "selector keyword: all (every key on the board, not a stored group)"
    )?;
    let groups = store.groups()?;
    if !groups.is_empty() {
        writeln!(out, "user groups:")?;
        // HashMap iteration order is unspecified and would otherwise vary between runs, so
        // sort by name for stable, diffable output.
        let mut sorted: Vec<_> = groups.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        for (name, usages) in sorted {
            let names: Vec<_> = usages
                .iter()
                .filter_map(|&u| wh_proto::keys::name_for_usage(u))
                .collect();
            writeln!(out, "  {name:<12} {}", names.join(","))?;
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
    writeln!(
        std::io::stdout().lock(),
        "group '{name}' = {} keys",
        usages.len()
    )?;
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

/// Writes one line to stderr, best-effort, matching the pattern `main.rs` already uses for its
/// own final error line: a closed stderr (`wh ... 2>/dev/null`, or a pipe reader that closed
/// both descriptors) must not panic over something that is, at most, informational.
/// `writeln!`'s `Result` is deliberately discarded here, unlike every stdout write in this
/// module, which propagate their `io::Error` so a closed stdout still exits cleanly through
/// `main.rs`'s broken-pipe check.
fn best_effort_eprintln(msg: &str) {
    let _ = writeln!(std::io::stderr(), "{msg}");
}

/// Converts a snapshot's millimetre field into a device `Um`, routing every float that will
/// ever reach the board through `Um::from_mm`'s finite-and-in-range check. `as u16` on a float
/// saturates rather than panicking or wrapping, so a hand-edited or stale snapshot's `99.0mm`
/// would otherwise become `65535` um (65mm) and go straight to the keyboard; this is the one
/// conversion path every millimetre value in this crate is required to go through instead.
fn mm(v: f64) -> Result<Um> {
    Ok(Um::from_mm(v, 0.0, 4.0)?)
}

fn auto_backup<T: Transport>(s: &mut Session<T>, store: &Store) -> Result<()> {
    let snap = snapshot_from_device(s)?;
    let path = store.save_backup(&snap.to_toml()?)?;
    best_effort_eprintln(&format!("(backed up to {})", path.display()));
    Ok(())
}

/// Prints the exact reports `--dry-run` would otherwise send, plus the SAVE frame that never
/// follows them, to `out`. Propagates a write failure rather than swallowing it (unlike
/// `best_effort_eprintln`): this is the dry-run path's only output, and it is the one most
/// likely to be piped into a pager (`wh set ap --keys all --set 1.2 --dry-run | less`), so a
/// closed reader has to surface as an ordinary `io::Error` that `main.rs` recognises as a
/// broken pipe, not a panic.
fn print_frames(out: &mut impl Write, frames: &[[u8; 64]]) -> Result<()> {
    for f in frames {
        writeln!(out, "{}", wh_device::replay::hex(f))?;
    }
    let save = cmds::cmd_order(cmds::order::SAVE, &[])?;
    writeln!(
        out,
        "dry run, nothing sent; save-to-flash frame {} would follow",
        wh_device::replay::hex(&save)
    )?;
    Ok(())
}

/// Shared tail of every write command's readback verification: `bad` is one already-formatted
/// line per key (or field) that failed to match, built by the caller so each verifier can name
/// exactly what differed (press vs release vs the enabled flag, in `verify_rt`'s case) rather
/// than this shared code guessing which field to report.
fn report_verification(
    out: &mut impl Write,
    what: &str,
    usages: &[u8],
    bad: &[String],
) -> Result<()> {
    if bad.is_empty() {
        writeln!(out, "{what}: {} keys verified", usages.len())?;
        return Ok(());
    }
    for line in bad {
        best_effort_eprintln(&format!("  {line}"));
    }
    bail!(
        "readback mismatch on {} key(s), backup retained, use `wh restore --last` to roll back",
        bad.len()
    )
}

fn verify_rt<T: Transport>(
    out: &mut impl Write,
    s: &mut Session<T>,
    usages: &[u8],
    press: Um,
    release: Um,
) -> Result<()> {
    let mut bad = Vec::new();
    for &u in usages {
        let ks = ops::read_key_settings(s, u)?;
        let name = key_label(u);
        if !ks.rt_enabled() {
            bad.push(format!(
                "{name}: rt not enabled, wanted press {:.2}mm release {:.2}mm",
                press.to_mm(),
                release.to_mm()
            ));
        } else if ks.rt_press != press || ks.rt_release != release {
            // Both actual values are reported, not just the one field that first differs, so
            // a press-only or release-only mismatch is visible against its own wanted value
            // rather than being reported next to the field that was actually correct.
            bad.push(format!(
                "{name}: board reports press {:.2}mm release {:.2}mm, wanted press {:.2}mm release {:.2}mm",
                ks.rt_press.to_mm(),
                ks.rt_release.to_mm(),
                press.to_mm(),
                release.to_mm()
            ));
        }
    }
    report_verification(
        out,
        &format!(
            "rt press {:.2}mm release {:.2}mm",
            press.to_mm(),
            release.to_mm()
        ),
        usages,
        &bad,
    )
}

fn verify_rt_off<T: Transport>(
    out: &mut impl Write,
    s: &mut Session<T>,
    usages: &[u8],
) -> Result<()> {
    let mut bad = Vec::new();
    for &u in usages {
        let ks = ops::read_key_settings(s, u)?;
        if ks.rt_enabled() {
            bad.push(format!("{}: rt still enabled", key_label(u)));
        }
    }
    report_verification(out, "rt off", usages, &bad)
}

fn set(what: SetWhat, store: &Store) -> Result<()> {
    match what {
        SetWhat::Rt {
            keys,
            set,
            press,
            release,
            off,
            dry_run,
        } => {
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            with_session(|s| {
                let usages = resolve_keys(s, &keys, store)?;
                if off {
                    if dry_run {
                        let records = ops::rt_off_records(s, &usages)?;
                        return print_frames(&mut out, &cmds::write_key_records(&records));
                    }
                    auto_backup(s, store)?;
                    ops::set_rt_off(s, &usages)?;
                    verify_rt_off(&mut out, s, &usages)
                } else {
                    let base = set.ok_or_else(|| {
                        anyhow::anyhow!("--set, --press/--release, or --off required")
                    })?;
                    let p = mm(press.unwrap_or(base))?;
                    let r = mm(release.unwrap_or(base))?;
                    if dry_run {
                        let records = ops::rt_records(s, &usages, p, r)?;
                        return print_frames(&mut out, &cmds::write_key_records(&records));
                    }
                    auto_backup(s, store)?;
                    ops::set_rt(s, &usages, p, r)?;
                    verify_rt(&mut out, s, &usages, p, r)
                }
            })
        }
        SetWhat::Ap { keys, set, dry_run } => {
            let depth = mm(set)?;
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            if dry_run {
                // No session at all: `ap_records` needs no prior device state (unlike RT,
                // which preserves the advanced-mode nibble), so the whole preview, selector
                // resolution included, can run with nothing attached and nothing sent.
                let usages = resolve_keys_offline(&keys, store)?;
                let records = ops::ap_records(&usages, depth);
                return print_frames(&mut out, &cmds::write_key_records(&records));
            }
            with_session(|s| {
                let usages = resolve_keys(s, &keys, store)?;
                auto_backup(s, store)?;
                ops::set_ap(s, &usages, depth)?;
                let mut bad = Vec::new();
                for &u in &usages {
                    let ks = ops::read_key_settings(s, u)?;
                    if ks.ap != depth {
                        bad.push(format!(
                            "{}: board reports {:.2}mm, wanted {:.2}mm",
                            key_label(u),
                            ks.ap.to_mm(),
                            depth.to_mm()
                        ));
                    }
                }
                report_verification(
                    &mut out,
                    &format!("ap {:.2}mm", depth.to_mm()),
                    &usages,
                    &bad,
                )
            })
        }
    }
}

fn backup(to: Option<std::path::PathBuf>, store: &Store) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    with_session(|s| {
        let snap = snapshot_from_device(s)?;
        let text = snap.to_toml()?;
        match to {
            Some(p) => {
                std::fs::write(&p, &text)?;
                writeln!(out, "wrote {}", p.display())?;
            }
            None => {
                let path = store.save_backup(&text)?;
                writeln!(out, "wrote {}", path.display())?;
            }
        }
        Ok(())
    })
}

/// One snapshot key, with every millimetre field already validated through `mm()`. Built once
/// by `validate_restore_keys` before `restore` opens a session, so a bad snapshot is refused
/// before a single frame is sent, and reused for both `restore_records` (what to write) and
/// `verify_restore` (what the readback must match), so the two can never drift apart on what
/// "restored" means.
struct RestoreKey {
    usage: u8,
    ap: Um,
    mode_raw: u16,
    rt_press: Um,
    rt_release: Um,
}

fn validate_restore_keys(snap: &wh_config::snapshot::Snapshot) -> Result<Vec<RestoreKey>> {
    snap.keys
        .iter()
        .map(|k| {
            let ap = mm(k.ap_mm).with_context(|| {
                format!("key '{}' (usage {:#04x}): actuation point", k.name, k.usage)
            })?;
            let rt_press = mm(k.rt_press_mm)
                .with_context(|| format!("key '{}' (usage {:#04x}): rt press", k.name, k.usage))?;
            let rt_release = mm(k.rt_release_mm).with_context(|| {
                format!("key '{}' (usage {:#04x}): rt release", k.name, k.usage)
            })?;
            Ok(RestoreKey {
                usage: k.usage,
                ap,
                // mode_raw stays a verbatim u16: round-tripping the raw mode value, advanced
                // nibble included, is the entire point of this field, so unlike the three
                // millimetre fields above it does not go through `mm()`.
                mode_raw: k.mode_raw,
                rt_press,
                rt_release,
            })
        })
        .collect()
}

fn restore_records(keys: &[RestoreKey]) -> Vec<KeyRecord> {
    let mut records = Vec::with_capacity(keys.len() * 4);
    for k in keys {
        records.push(KeyRecord {
            key: k.usage,
            layout: layout::AP,
            value: k.ap.0,
        });
        records.push(KeyRecord {
            key: k.usage,
            layout: layout::MODE,
            value: k.mode_raw,
        });
        records.push(KeyRecord {
            key: k.usage,
            layout: layout::RT_PRESS,
            value: k.rt_press.0,
        });
        records.push(KeyRecord {
            key: k.usage,
            layout: layout::RT_RELEASE,
            value: k.rt_release.0,
        });
    }
    records
}

fn snap_to_global(snap: &wh_config::snapshot::Snapshot) -> Result<cmds::GlobalTravel> {
    Ok(cmds::GlobalTravel {
        travel: mm(snap.global.travel_mm).context("global travel")?,
        press_dead: mm(snap.global.press_dead_mm).context("global press dead zone")?,
        release_dead: mm(snap.global.release_dead_mm).context("global release dead zone")?,
    })
}

fn verify_restore<T: Transport>(
    out: &mut impl Write,
    s: &mut Session<T>,
    keys: &[RestoreKey],
) -> Result<()> {
    let mut bad = Vec::new();
    let usages: Vec<u8> = keys.iter().map(|k| k.usage).collect();
    for k in keys {
        let ks = ops::read_key_settings(s, k.usage)?;
        if ks.ap != k.ap
            || ks.rt_press != k.rt_press
            || ks.rt_release != k.rt_release
            || ks.mode.value() != k.mode_raw
        {
            bad.push(format!(
                "{}: board reports ap {:.2}mm press {:.2}mm release {:.2}mm mode {:#06x}, \
                 wanted ap {:.2}mm press {:.2}mm release {:.2}mm mode {:#06x}",
                key_label(k.usage),
                ks.ap.to_mm(),
                ks.rt_press.to_mm(),
                ks.rt_release.to_mm(),
                ks.mode.value(),
                k.ap.to_mm(),
                k.rt_press.to_mm(),
                k.rt_release.to_mm(),
                k.mode_raw,
            ));
        }
    }
    report_verification(out, "restore", &usages, &bad)
}

fn restore(file: Option<std::path::PathBuf>, last: bool, store: &Store) -> Result<()> {
    if file.is_some() && last {
        bail!("pass a snapshot file or --last, not both");
    }
    let text = match (file, last) {
        (Some(p), _) => std::fs::read_to_string(p)?,
        (None, true) => store.load_backup(None)?,
        (None, false) => bail!("give a snapshot file or --last"),
    };
    let snap = wh_config::snapshot::Snapshot::from_toml(&text)?;
    // Every value that will reach the board is validated, and the global travel and per-key
    // write records are built, before a session is ever opened: a bad snapshot is refused
    // before a single frame is sent, not after.
    let global = snap_to_global(&snap)?;
    let keys = validate_restore_keys(&snap)?;
    let records = restore_records(&keys);

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    with_session(|s| {
        // Unlike `set`, which is scoped to the keys the caller selected, `restore` overwrites
        // every key in the snapshot: an auto-backup here is the only way back if the file
        // named on the command line turns out to be the wrong one, or a stale one.
        auto_backup(s, store)?;
        ops::restore_all(s, &global, &records)?;
        writeln!(
            out,
            "restored {} keys from snapshot ({})",
            snap.keys.len(),
            snap.taken_at
        )?;
        verify_restore(&mut out, s, &keys)
    })
}

fn selftest() -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    with_session(|s| {
        let info = ops::device_info(s)?;
        writeln!(out, "device: {} fw {}", info.serial, info.firmware)?;
        let g = ops::global_travel(s)?;
        writeln!(
            out,
            "global travel: {:.2}mm, rewriting identical value",
            g.travel.to_mm()
        )?;
        // Deliberately no SAVE: this has to be a true no-op on flash, proving only that a
        // write reaches the device and reads back correctly, not that a save cycle works.
        // If a future change adds one here, it turns every selftest run into an unwanted
        // flash-wear cycle on the user's only keyboard.
        s.roundtrip(&cmds::write_global_travel(
            g.travel,
            g.press_dead,
            g.release_dead,
        ))?;
        let g2 = ops::global_travel(s)?;
        if g2 != g {
            bail!("selftest FAILED: readback {:?} != {:?}", g2, g);
        }
        writeln!(out, "selftest OK: write path verified with a no-op write")?;
        Ok(())
    })
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

    #[test]
    fn empty_wh_replay_value_is_treated_as_unset() {
        // WH_REPLAY="" (present but empty) must not be read as "open a file named the empty
        // string"; it should fall back exactly like an absent variable does.
        assert_eq!(non_empty_replay_path(Ok(String::new())), None);
        assert_eq!(
            non_empty_replay_path(Err(std::env::VarError::NotPresent)),
            None
        );
        assert_eq!(
            non_empty_replay_path(Ok("script.jsonl".to_string())),
            Some("script.jsonl".to_string())
        );
    }

    #[test]
    fn key_label_falls_back_to_hex_for_an_unnamed_usage() {
        let (unnamed, _) = two_usages_absent_from_table();
        assert_eq!(key_label(unnamed), format!("0x{unnamed:02X}"));
    }
}
