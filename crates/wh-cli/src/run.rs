//! Command dispatch. Every command in the `wh` tree, read and write alike, runs through here.

use crate::cli::{BackupsWhat, Cli, Cmd, KeysWhat, SetWhat};
use crate::keyset::KeysetOp;
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
    // Opened once here: `Store::open` only resolves a path, so it is cheap even for commands
    // that never read it. Every command that needs one takes this `&Store` instead of reaching
    // for the config directory again; `Dump`, `Profile`, and `Selftest` touch no config at all.
    let store = Store::open()?;
    match cli.cmd {
        Cmd::Keys { what } => keys(what, &store),
        Cmd::Backups { what } => backups(what, &store),
        Cmd::Dump { table } => dump(table),
        Cmd::Get { what } => get(what, &store),
        Cmd::Set { what } => set(what, &store),
        Cmd::Backup { to } => backup(to, &store),
        Cmd::Restore { file, last } => restore(file, last, &store),
        Cmd::Profile { number } => profile_cmd(number),
        Cmd::Selftest => selftest(),
        Cmd::Keyset { what } => keyset_cmd(what, &store),
        Cmd::Socd { what } => socd_cmd(what, &store),
    }
}

/// Treats `WH_REPLAY=` (present but empty) the same as unset, rather than as a request to
/// read a file named the empty string, which would otherwise surface as a confusing I/O
/// error instead of falling back to the real device.
fn non_empty_replay_path(raw: Result<String, std::env::VarError>) -> Option<String> {
    raw.ok().filter(|p| !p.is_empty())
}

/// Open the real device on Windows, or a replay script when WH_REPLAY is set to a non-empty
/// path.
///
/// Announces which transport it opened, on stderr, once the transport is actually ready:
/// `bin/wh` must propagate `WH_REPLAY` across the WSL/Windows boundary, or this function
/// silently opens the real keyboard instead. This line is the backstop that makes a run
/// quietly hitting real hardware never silent, regardless of what carries the variable.
/// Kept off stdout so `dump`'s parseable JSON output stays clean.
fn with_session<R>(f: impl FnOnce(&mut Session<Box<dyn Transport>>) -> Result<R>) -> Result<R> {
    let t: Box<dyn Transport> =
        if let Some(path) = non_empty_replay_path(std::env::var("WH_REPLAY")) {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading WH_REPLAY script from {path}"))?;
            let t = wh_device::replay::ReplayTransport::from_jsonl(&text)?;
            best_effort_eprintln(&format!("transport: replay ({path})"));
            Box::new(t)
        } else {
            #[cfg(windows)]
            {
                let t = wh_device::hid::HidTransport::open()?;
                best_effort_eprintln("transport: hardware (real keyboard)");
                Box::new(t)
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
/// `wh_proto::keys::TABLE`. Shared by `dump`, `get`, and `picker` so an unnamed usage prints
/// the same label everywhere. Delegates to `wh_proto::keys::label`, which `wh-device`'s own
/// errors also use: two copies of this would drift the first time the fallback form changed.
pub(crate) fn key_label(usage: u8) -> String {
    wh_proto::keys::label(usage)
}

/// "key" for 1, "keys" for anything else, so a count-carrying sentence never reads "1 keys".
/// Shared rather than three separate inline copies (`report_verification`, `restore`'s summary
/// line, and `wh set ap --base`'s own announcement), the same reason `ap_mode_clause` was
/// extracted in `keyset.rs`.
fn key_or_keys(n: usize) -> &'static str {
    if n == 1 {
        "key"
    } else {
        "keys"
    }
}

/// Renders a raw keyset value for display: `0`, the value read outside any keyset, as `-`,
/// anything else as its decimal index verbatim, since whether the wire value is a boolean or an
/// index is unmeasured.
fn keyset_display(v: u16) -> String {
    if v == 0 {
        "-".to_string()
    } else {
        v.to_string()
    }
}

/// The `" keyset N"` / `" keyset none"` suffix `wh get ap`/`wh get rt` appends, from the raw
/// keyset value: `0`, the value read outside any keyset, prints as "none"; anything else prints
/// as its decimal index verbatim.
fn keyset_suffix(v: u16) -> String {
    if v == 0 {
        " keyset none".to_string()
    } else {
        format!(" keyset {v}")
    }
}

fn snapshot_from_device<T: Transport>(s: &mut Session<T>) -> Result<wh_config::snapshot::Snapshot> {
    let info = ops::device_info(s)?;
    // The profile is a value in `0..=3` on the wire and nothing else, so an index outside that
    // is an error and every caller stops: `dump`, `backup` and every write command's
    // auto-backup alike. This is the only place `wh` could ever have produced a snapshot with
    // no profile, and it no longer does; `Snapshot::profile` stays `Option` for files on disk.
    let profile = Some(ops::profile(s).context("reading the board's active profile")?);
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
            // Always `Some`: this is a live read, never a stale or absent value, so `wh
            // restore` from this snapshot can always tell "no keyset" apart from "unknown".
            ap_keyset: Some(ks.ap_keyset),
            rt_keyset: Some(ks.rt_keyset),
        });
    }
    Ok(wh_config::snapshot::Snapshot {
        firmware: info.firmware,
        serial: info.serial,
        taken_at: httpdate_now()?,
        profile,
        // Set by the caller once it knows why this snapshot was taken (`backup` or
        // `auto_backup`); `dump` never assigns one, since it saves nothing to disk.
        origin: None,
        global: wh_config::snapshot::GlobalToml {
            custom_value_mm: global.travel.to_mm(),
            press_dead_mm: global.press_dead.to_mm(),
            release_dead_mm: global.release_dead.to_mm(),
        },
        keys,
    })
}

/// Returns the current time as an RFC3339 UTC timestamp, e.g. `"2026-08-28T12:00:00Z"`, the
/// shape `Snapshot::taken_at` expects. This field exists so a human can pick the right backup
/// out of twenty others, not just a raw epoch count.
///
/// Implemented inline rather than with a date crate: wh-cli cross-compiles for Windows, and a
/// new dependency has to earn surviving that build. See `civil_from_days` for the algorithm.
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

/// Howard Hinnant's days-from-civil algorithm: converts a day count since the Unix epoch
/// into a proleptic-Gregorian (year, month, day). See
/// http://howardhinnant.github.io/date_algorithms.html; exact for every day this side of
/// year 0, with no lookup tables needed.
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

fn dump(table: bool) -> Result<()> {
    // `writeln!`'s `Result` (unlike `println!`, which panics) lets an early-closing reader,
    // e.g. `wh dump | head -1`, surface as an `io::Error` that `main` recognises and exits on
    // quietly.
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    with_session(|s| {
        let snap = snapshot_from_device(s)?;
        if !table {
            writeln!(out, "{}", snap.to_json()?)?;
        } else {
            writeln!(out, "{} (fw {})", snap.serial, snap.firmware)?;
            // `snapshot_from_device` a few lines up always records a profile, since an index
            // outside `0..=3` stops the read, so `None` cannot be reached from here. Matched
            // rather than unwrapped so a future caller cannot turn this into a panic.
            match snap.profile {
                Some(profile) => writeln!(out, "profile {profile}")?,
                None => writeln!(out, "profile unrecorded")?,
            }
            // "custom value", not "travel": this is the configurator's `"MM" CUSTOM VALUE`, the
            // step size for its controls, and the global actuation point is not in this record.
            writeln!(
                out,
                "global: custom value {:.2}mm, dead {:.2}/{:.2}mm",
                snap.global.custom_value_mm, snap.global.press_dead_mm, snap.global.release_dead_mm
            )?;
            writeln!(
                out,
                "{:<12} {:>6} {:>4} {:>4} {:>8} {:>8} {:>4}",
                "key", "ap", "apks", "rt", "press", "release", "rtks"
            )?;
            for k in &snap.keys {
                // `.unwrap_or(0)` is safe here, not a silent fallback: `snap` came from
                // `snapshot_from_device` a few lines up in this same function, which always
                // sets `Some`. A stored snapshot with an absent field never reaches this table.
                writeln!(
                    out,
                    "{:<12} {:>4.2}mm {:>4} {:>4} {:>6.2}mm {:>6.2}mm {:>4}",
                    k.name,
                    k.ap_mm,
                    keyset_display(k.ap_keyset.unwrap_or(0)),
                    if k.rt { "on" } else { "off" },
                    k.rt_press_mm,
                    k.rt_release_mm,
                    keyset_display(k.rt_keyset.unwrap_or(0))
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
    // Unreachable today thanks to clap's `required_unless_present = "pick"` in cli.rs, but a
    // later change there should not be able to turn a missing selector into a crash here.
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
                    "{name}: rt {} press {:.2}mm release {:.2}mm{}",
                    if ks.rt_enabled() { "on" } else { "off" },
                    ks.rt_press.to_mm(),
                    ks.rt_release.to_mm(),
                    keyset_suffix(ks.rt_keyset)
                )?;
            } else {
                writeln!(
                    out,
                    "{name}: ap {:.2}mm{}",
                    ks.ap.to_mm(),
                    keyset_suffix(ks.ap_keyset)
                )?;
            }
        }
        Ok(())
    })
}

fn keys(what: KeysWhat, store: &Store) -> Result<()> {
    match what {
        KeysWhat::List => list_keys(store),
        KeysWhat::Group { name, selector } => group(store, &name, &selector),
        KeysWhat::Ungroup { name } => ungroup(store, &name),
        KeysWhat::Rename { old, new } => rename(store, &old, &new),
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
            // A usage with no `TABLE` entry is still listed, as hex, not dropped: reading a
            // stale group's members off this output is the operator's only recovery route
            // when a group fails to resolve, and under-reporting here would send them to
            // recreate an incomplete group.
            let names: Vec<_> = usages.iter().map(|&u| key_label(u)).collect();
            writeln!(out, "  {name:<12} {}", names.join(","))?;
        }
    }
    Ok(())
}

/// Refuses a name that could not be a usable group name: one already taken by a key or
/// builtin group, or one the selector grammar would not read back as a plain name. Shared
/// by `group` (on create) and `rename` (on the new name), so a later addition to the
/// disallowed set only needs to change here.
fn check_group_name_usable(name: &str) -> Result<()> {
    if wh_proto::keys::usage_for_name(name).is_some()
        || wh_proto::keys::builtin_group(name).is_some()
    {
        bail!("'{name}' is already a key or builtin group name");
    }
    if looks_like_hex_form(name) {
        bail!("'{name}' looks like a hex usage code (e.g. `0x01`); pick a different name");
    }
    if !group_name_is_reachable(name) {
        bail!(
            "'{name}' cannot be used as a group name: the selector grammar would not read it \
             back as a plain name (for example it looks like a range, 'all', a negation, or a \
             list), so the group would be unreachable once saved"
        );
    }
    Ok(())
}

/// Whether `name` has the `0x01`-style shape the selector grammar reads back as a single
/// usage code, matching `Selector::parse`'s own hex prefix check.
fn looks_like_hex_form(name: &str) -> bool {
    name.strip_prefix("0x")
        .or_else(|| name.strip_prefix("0X"))
        .is_some_and(|hex| !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()))
}

fn group(store: &Store, name: &str, selector: &str) -> Result<()> {
    // The selector grammar lowercases a bare name before looking it up (see
    // Selector::resolve), so storing under a mixed-case spelling would make it unreachable.
    // Normalize once here, where the group is created.
    let name = name.to_ascii_lowercase();
    check_group_name_usable(&name)?;
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

/// Deletes a group by name directly, never through `Selector::parse`, so a group whose name
/// collides with a key name (refused as a selector) can still be removed. No naming guard
/// applies here: removing is always safe.
fn ungroup(store: &Store, name: &str) -> Result<()> {
    let name = name.to_ascii_lowercase();
    if !store.remove_group(&name)? {
        bail!("no such group: '{name}'");
    }
    writeln!(std::io::stdout().lock(), "removed group '{name}'")?;
    Ok(())
}

/// Renames a group by name directly, never through `Selector::parse`. The new name gets the
/// same usability guard `group` applies on create, so a rename cannot recreate the problem
/// `ungroup` exists to recover from.
fn rename(store: &Store, old: &str, new: &str) -> Result<()> {
    let old = old.to_ascii_lowercase();
    let new = new.to_ascii_lowercase();
    check_group_name_usable(&new)?;
    store.rename_group(&old, &new)?;
    writeln!(std::io::stdout().lock(), "renamed group '{old}' to '{new}'")?;
    Ok(())
}

fn backups(what: BackupsWhat, store: &Store) -> Result<()> {
    match what {
        BackupsWhat::List => list_backups(store),
    }
}

/// Lists every stored backup, oldest first. Each backup is parsed on its own, and a file that
/// fails to parse prints a warning naming it, on stderr, rather than aborting the whole listing:
/// one corrupt file must not hide every other backup that still reads fine.
fn list_backups(store: &Store) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for path in store.list_backups()? {
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                best_effort_eprintln(&format!(
                    "warning: could not read backup {}: {e}",
                    path.display()
                ));
                continue;
            }
        };
        match wh_config::snapshot::Snapshot::from_file_text(&path, &text) {
            Ok(snap) => {
                let origin = snap.origin.as_deref().unwrap_or("unknown");
                let profile = snap
                    .profile
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "unknown".into());
                writeln!(
                    out,
                    "{}  {origin}  profile {profile}  {}",
                    snap.taken_at,
                    path.display()
                )?;
            }
            Err(e) => {
                best_effort_eprintln(&format!(
                    "warning: could not parse backup {}: {e}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

/// Reports whether `name` (already lowercased) would resolve back to the group stored under it
/// if later typed as a bare `--keys` token.
///
/// Asks the grammar itself rather than hand-listing special shapes: parse `name`, then resolve
/// it against a two-element sentinel universe `[a, b]` where only `a` is bound under `name` in
/// a throwaway group map. Only a plain, non-negated name resolves to exactly `[a]`; a range or
/// list resolves to real usages, `all` resolves to the whole `[a, b]` (why the universe needs
/// two elements: one element would make `all` indistinguishable from a group hit), and a
/// negation can only shrink an empty accumulator.
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

/// Picks two usage bytes absent from `wh_proto::keys::TABLE`, at runtime rather than
/// hardcoded so the choice can't rot as the table grows. Used as sentinels in
/// `group_name_is_reachable`'s probe universe.
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

/// Writes one line to stderr, best-effort: a closed stderr (`wh ... 2>/dev/null`) must not
/// panic over something merely informational. Unlike stdout writes across this crate, the
/// `Result` is discarded rather than propagated. `pub(crate)` since `keyset.rs` calls it too.
pub(crate) fn best_effort_eprintln(msg: &str) {
    let _ = writeln!(std::io::stderr(), "{msg}");
}

/// Converts a snapshot's millimetre field into a device `Um`, through `Um::from_mm`'s
/// finite-and-in-range check. A bare `as u16` on a float saturates instead of erroring, so a
/// hand-edited `99.0mm` would silently become `65535` um and go straight to the keyboard;
/// every millimetre value in this crate must go through this conversion instead.
fn mm(v: f64) -> Result<Um> {
    Ok(Um::from_mm(v, 0.0, 4.0)?)
}

/// Resolves `wh keyset create rt`'s three flags into the `Option<(Um, Um)>` shape
/// `keyset::create` takes: `--press`/`--release` override a `--value` base, exactly as
/// `wh set rt`'s `--press`/`--release` override `--set`. `None` only when none of the three
/// were given at all, so `keyset::create` falls back to the board's global instead.
fn resolve_rt_override(
    value: Option<Um>,
    press: Option<Um>,
    release: Option<Um>,
) -> Result<Option<(Um, Um)>> {
    if value.is_none() && press.is_none() && release.is_none() {
        return Ok(None);
    }
    let p = press
        .or(value)
        .ok_or_else(|| anyhow::anyhow!("--release given without --press or --value"))?;
    let r = release
        .or(value)
        .ok_or_else(|| anyhow::anyhow!("--press given without --release or --value"))?;
    Ok(Some((p, r)))
}

/// Why a backup was taken, one variant per command family that takes one plus the manual `wh
/// backup`. The reason was a `&str` before, so a copy-pasted call site could label a `restore`
/// backup a `keyset delete`, and nothing failed: the label is only ever written and displayed,
/// never acted on, so no readback can catch it.
#[derive(Clone, Copy)]
enum BackupReason {
    SetAp,
    SetRt,
    SetMm,
    KeysetCreate,
    KeysetSet,
    KeysetDelete,
    KeysetRemove,
    SocdPair,
    SocdUnpair,
    Restore,
    /// `wh backup`, the only one an operator asks for directly, hence no `auto: ` prefix.
    Manual,
}

impl BackupReason {
    /// The `Snapshot::origin` this reason writes, verbatim. These strings are already on disk in
    /// operators' existing backups and are what `wh backups list` and `--last` print, so they are
    /// a compatibility surface: changing one would make old and new backups of the same command
    /// read as different things.
    fn origin(self) -> &'static str {
        match self {
            BackupReason::SetAp => "auto: set ap",
            BackupReason::SetRt => "auto: set rt",
            BackupReason::SetMm => "auto: set mm",
            BackupReason::KeysetCreate => "auto: keyset create",
            BackupReason::KeysetSet => "auto: keyset set",
            BackupReason::KeysetDelete => "auto: keyset delete",
            BackupReason::KeysetRemove => "auto: keyset remove",
            BackupReason::SocdPair => "auto: socd pair",
            BackupReason::SocdUnpair => "auto: socd unpair",
            BackupReason::Restore => "auto: restore",
            BackupReason::Manual => "manual",
        }
    }
}

/// Takes and saves an auto-backup, recording `reason` (e.g. `SetRt`) as its origin. `restore`
/// reads the board's profile through its own separate `ops::profile` call rather than off this
/// function's returned snapshot, so a future `--no-backup` flag or a best-effort backup here
/// cannot silently drop the profile safety check.
fn auto_backup<T: Transport>(
    s: &mut Session<T>,
    store: &Store,
    reason: BackupReason,
) -> Result<()> {
    let mut snap = snapshot_from_device(s)?;
    snap.origin = Some(reason.origin().to_string());
    let path = store.save_backup(&snap.to_json()?)?;
    best_effort_eprintln(&format!("(backed up to {})", path.display()));
    Ok(())
}

/// Prints exactly the reports a real run would send, and nothing else, since an operator
/// compares this output against captures by eye. Propagates a write failure (unlike
/// `best_effort_eprintln`): this output is likely piped into a pager, so a closed reader must
/// surface as a broken pipe, not a panic.
fn print_frames(out: &mut impl Write, frames: &[[u8; 64]]) -> Result<()> {
    for f in frames {
        writeln!(out, "{}", wh_device::replay::hex(f))?;
    }
    writeln!(out, "dry run, no writes sent")?;
    Ok(())
}

/// Shared tail of every write command's readback verification. `bad` is one already-formatted
/// line per mismatch, built by the caller so each verifier can name exactly what differed.
pub(crate) fn report_verification(
    out: &mut impl Write,
    what: &str,
    usages: &[u8],
    bad: &[String],
) -> Result<()> {
    if bad.is_empty() {
        let n = usages.len();
        writeln!(out, "{what}: {n} {} verified", key_or_keys(n))?;
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

/// Verifies an RT-on write by comparing the full MODE value against what `ops::rt_records`
/// computed, advanced nibble and high byte included, not just the touch nibble: a firmware
/// that clears the advanced nibble on a mode change must show up here as a mismatch.
///
/// Derives the key list from `records` rather than taking a separate `usages` parameter, so
/// the two can never disagree.
fn verify_rt<T: Transport>(
    out: &mut impl Write,
    s: &mut Session<T>,
    press: Um,
    release: Um,
    records: &[KeyRecord],
) -> Result<()> {
    let mut bad = Vec::new();
    let mut usages = Vec::new();
    for r in records.iter().filter(|r| r.layout == layout::MODE) {
        let u = r.key;
        let want_mode = r.value;
        usages.push(u);
        let ks = ops::read_key_settings(s, u)?;
        let name = key_label(u);
        if ks.mode.value() != want_mode {
            bad.push(format!(
                "{name}: board reports mode {:#06x} (rt {}), wanted mode {:#06x} (rt on, \
                 advanced nibble and high byte preserved)",
                ks.mode.value(),
                if ks.rt_enabled() { "on" } else { "off" },
                want_mode,
            ));
        } else if ks.rt_press != press || ks.rt_release != release {
            // Both actual values are reported, not just the one that first differs, so a
            // press-only mismatch is visible against its own wanted value.
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
        &usages,
        &bad,
    )
}

/// What `wh set rt` asked for, resolved once up front into a shape where "on" and "off"
/// cannot disagree with themselves, unlike a bare `off: bool` plus a separate
/// `Option<(Um, Um)>` could.
enum RtAction {
    /// `--off`, carrying the `--press`/`--release` the keys are reset to. `None` means read the
    /// board's base instead, from the keys outside every rapid trigger keyset that the selection
    /// leaves behind, which `crate::keyset::rt_off` does and refuses over.
    Off {
        rt: Option<(Um, Um)>,
    },
    On {
        press: Um,
        release: Um,
    },
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
            // Validated before a session opens, like `Ap`'s `mm(set)?` below: a malformed
            // value is refused before `resolve_keys` sends any DEFKEY roundtrips.
            let action = if off {
                // Both or neither, never one: `--off` resets both sensitivities together, and a
                // half-given override would have to read the other half from the very base
                // reading whose disagreement is the reason these flags were reached for.
                let rt = match (press, release) {
                    (None, None) => None,
                    (Some(p), Some(r)) => Some((mm(p)?, mm(r)?)),
                    _ => bail!(
                        "--off resets both sensitivities, so pass --press and --release together \
                         or neither; with neither, both come from the keys outside every rapid \
                         trigger keyset that this selection leaves behind"
                    ),
                };
                RtAction::Off { rt }
            } else {
                let base = set.ok_or_else(|| {
                    anyhow::anyhow!(
                        "--set is required unless --off is given; --press and --release override \
                         a --set base, or name the value --off resets to, and cannot be used alone"
                    )
                })?;
                RtAction::On {
                    press: mm(press.unwrap_or(base))?,
                    release: mm(release.unwrap_or(base))?,
                }
            };
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            // A separate locked stderr for `--off`'s whole-board confirmation, matching
            // `KeysetWhat::Remove`'s own split: the per-key announcement stays on stdout since it
            // is data someone may pipe, while the prompt is a diagnostic.
            let stderr = std::io::stderr();
            let mut prompt_out = stderr.lock();
            let stdin = std::io::stdin();
            let mut input = stdin.lock();
            with_session(|s| {
                // `resolve_keys` always reads the live matrix, dry run or not: a preview
                // against keys the board lacks would be meaningless, and `--pick` needs a
                // live board regardless. Only writes are skipped below.
                let usages = resolve_keys(s, &keys, store)?;
                match action {
                    RtAction::Off { rt } => {
                        // Routed through `keyset::plan`, like every other write but `set rt`'s
                        // own enable path: the vendor's per-key rapid trigger off resets the
                        // sensitivities and clears `0xFE` as well as moving the touch nibble, and
                        // `ops::rt_off_records`, which sends MODE alone, is the divergent one.
                        // `rt_off` only reads, so a dry run can build the same plan and stop.
                        // `!dry_run` is what tells it whether to run the whole-board
                        // confirmation, the same argument `keyset::remove` takes.
                        let plan = crate::keyset::rt_off(
                            &mut out,
                            &mut prompt_out,
                            s,
                            &usages,
                            rt,
                            !dry_run,
                            &mut input,
                        )?;
                        if dry_run {
                            return print_frames(&mut out, &plan.frames());
                        }
                        auto_backup(s, store, BackupReason::SetRt)?;
                        wh_device::keyset::apply(s, &plan)?;
                        crate::keyset::verify_write_as(&mut out, s, "rt off", &plan)
                    }
                    RtAction::On { press, release } => {
                        if dry_run {
                            let records = ops::rt_records(s, &usages, press, release)?;
                            return print_frames(&mut out, &cmds::write_key_records(&records));
                        }
                        auto_backup(s, store, BackupReason::SetRt)?;
                        let records = ops::set_rt(s, &usages, press, release)?;
                        verify_rt(&mut out, s, press, release, &records)
                    }
                }
            })
        }
        SetWhat::Ap {
            keys,
            pick,
            set,
            base,
            dry_run,
        } => {
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            match base {
                Some(base_mm) => {
                    // `--base` names the board, not a selection: it never touches `keys`/`pick`,
                    // never reads or writes a `0xFF` membership record, and so never reaches
                    // `confirm_whole_board_ap_set` at all, unlike `--keys all` below.
                    let depth = mm(base_mm)?;
                    with_session(|s| {
                        let m = wh_device::keyset::read_membership(s, wh_device::keyset::Kind::Ap)?;
                        let free: Vec<u8> = m
                            .entries()
                            .iter()
                            .filter(|&&(_, v)| v == 0)
                            .map(|&(u, _)| u)
                            .collect();
                        if free.is_empty() {
                            bail!(
                                "every key on the board is in a keyset, so there is no key \
                                 outside a keyset to write; use `wh keyset set ap` to change a \
                                 keyset's own value instead"
                            );
                        }
                        let plan = wh_device::keyset::plan(
                            s,
                            &free,
                            &wh_device::keyset::Change::ap(depth),
                            None,
                        )?;
                        // Every branch names the actuation point explicitly, so "already
                        // there" never reads as a claim about the whole write; `ap_mode_clause`
                        // says what else moved. "Nothing written" is `plan.is_empty()`, never
                        // `moved_values == 0`: a mode-only promotion still sends a full bundle.
                        let moved_values = crate::keyset::ap_value_moved_count(&plan);
                        let total = free.len();
                        let mode_clause = crate::keyset::ap_mode_clause(&plan);
                        let target_mm = depth.to_mm();
                        let n_keys = key_or_keys(total);
                        let move_clause = if plan.is_empty() {
                            // No key gets any record at all: every free key already sits at the
                            // target and none of them was on touch nibble 0 to promote, so
                            // `mode_clause` is always empty here too.
                            format!(
                                "the actuation point outside every keyset already matches \
                                 {target_mm:.2}mm on {total} {n_keys}, nothing written"
                            )
                        } else if moved_values == total {
                            format!(
                                "the actuation point outside every keyset moves to \
                                 {target_mm:.2}mm on {total} {n_keys}"
                            )
                        } else if moved_values == 0 {
                            // `plan` is not empty (checked above), so every key that got a
                            // record here got one because its touch mode moved, not its AP:
                            // `mode_clause` below is guaranteed non-empty in this branch.
                            "no key outside a keyset changes its actuation point".to_string()
                        } else {
                            format!(
                                "the actuation point outside every keyset moves to \
                                 {target_mm:.2}mm on {moved_values} of {total} {n_keys}; the \
                                 actuation point on the rest already matches {target_mm:.2}mm"
                            )
                        };
                        writeln!(
                            out,
                            "ap base: {move_clause}, keysets untouched{mode_clause}"
                        )?;
                        if dry_run {
                            return print_frames(&mut out, &plan.frames());
                        }
                        auto_backup(s, store, BackupReason::SetAp)?;
                        wh_device::keyset::apply(s, &plan)?;
                        crate::keyset::verify_write_as(
                            &mut out,
                            s,
                            &format!("ap base {:.2}mm", depth.to_mm()),
                            &plan,
                        )
                    })
                }
                None => {
                    // clap enforces `required_unless_present = "base"` on `set`, so reaching
                    // this branch (`base` absent) guarantees `set` is `Some`.
                    let depth = mm(set.expect("clap requires --set whenever --base is absent"))?;
                    let keys = crate::cli::KeysArg { keys, pick };
                    // A separate locked stderr for the whole-board confirmation, matching
                    // `KeysetWhat::Remove`'s own split: the per-key announcement below stays on
                    // stdout since it is data someone may pipe, while the prompt is a diagnostic.
                    let stderr = std::io::stderr();
                    let mut prompt_out = stderr.lock();
                    let stdin = std::io::stdin();
                    let mut input = stdin.lock();
                    with_session(|s| {
                        let usages = resolve_keys(s, &keys, store)?;
                        // `kind` is taken from `change`, the same binding that builds the plan
                        // below, rather than written out again as `Kind::Ap`: the membership read
                        // and the plan then cannot end up on different layouts. `announce_steal`
                        // takes no kind at all now, reading it off the `Target` it prints.
                        let change = wh_device::keyset::Change::ap(depth);
                        let kind = change.kind();
                        let m = wh_device::keyset::read_membership(s, kind)?;
                        let membership = crate::keyset::ap_membership_for(&m, &usages)?;
                        let index = match &membership {
                            crate::keyset::ApMembership::Keep => None,
                            crate::keyset::ApMembership::Split { index, .. } => Some(*index),
                        };
                        let plan = wh_device::keyset::plan(s, &usages, &change, index)?;
                        // Built after `plan`, matching `remove`: only `plan` knows how many keys
                        // the promotion off touch nibble 0 actually moves. Decides its own
                        // whole-board and `Keep` triggers from `m`/`usages` rather than trusting
                        // this arm's own `membership`, and no-ops for anything but a whole-board
                        // `Split`.
                        if !dry_run {
                            crate::keyset::confirm_whole_board_ap_set(
                                &mut prompt_out,
                                &m,
                                &usages,
                                &plan,
                                depth,
                                &mut input,
                            )?;
                        }
                        let what = ap_write_label(&mut out, &membership, &plan, depth)?;
                        if dry_run {
                            return print_frames(&mut out, &plan.frames());
                        }
                        auto_backup(s, store, BackupReason::SetAp)?;
                        wh_device::keyset::apply(s, &plan)?;
                        crate::keyset::verify_write_as(&mut out, s, &what, &plan)
                    })
                }
            }
        }
        SetWhat::Mm { value, dry_run } => {
            // Validated before a session opens: a malformed value is refused before a single
            // frame is sent. The 0 to 4mm bound is `mm()`'s actuation-point range, borrowed rather
            // than measured for this setting: every measured vendor write to this record carries
            // 0.10 to 0.65mm, so 4mm is a reused bound here, not a defended one.
            let target = mm(value)?;
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            with_session(|s| {
                // The read is what makes the announcement below honest, and costs one roundtrip.
                let old = ops::global_travel(s)?;
                if old.travel == target {
                    // Skips the write outright rather than taking one and reporting it a no-op,
                    // matching `--base`'s own "already matches" vocabulary. Whether the vendor
                    // itself writes on a no-op set is unmeasured; this is `wh`'s own choice, made
                    // to avoid an avoidable 200/200 dead-zone write while that question stays open
                    // (`docs/backlog.md`).
                    writeln!(
                        out,
                        "mm custom value already matches {:.2}mm, nothing written",
                        target.to_mm()
                    )?;
                    return Ok(());
                }
                writeln!(
                    out,
                    "mm custom value: {:.2}mm -> {:.2}mm",
                    old.travel.to_mm(),
                    target.to_mm()
                )?;
                let frame =
                    cmds::write_global_travel(target, VENDOR_PRESS_DEAD, VENDOR_RELEASE_DEAD);
                if dry_run {
                    return print_frames(&mut out, &[frame]);
                }
                auto_backup(s, store, BackupReason::SetMm)?;
                s.roundtrip(&frame)?;
                // The dead zones read back as 0 on every measured board regardless of what was
                // written (14 reads across 7 captures, `docs/keysets.md`), so only the travel
                // field is a meaningful readback check.
                let readback = ops::global_travel(s)?;
                if readback.travel != target {
                    bail!(
                        "mm custom value mismatch: board reports {:.2}mm, wanted {:.2}mm; \
                         backup retained, use `wh restore --last` to roll back",
                        readback.travel.to_mm(),
                        target.to_mm()
                    );
                }
                writeln!(out, "mm custom value: verified")?;
                Ok(())
            })
        }
    }
}

/// Confirms `plan` resolved to `depth`, then builds the label `verify_write_as` reports and, for
/// a split, prints `announce_steal` first. Split out of `set`'s `SetWhat::Ap` arm so a test can
/// hand it a `plan` and `depth` that disagree: through the real `Change::ap(depth)` construction
/// that arm itself uses, the two can never actually diverge, since both come from the same value,
/// so this is the only way to prove `confirm_ap_target` is wired into the write path at all, not
/// only correct on its own.
fn ap_write_label(
    out: &mut impl Write,
    membership: &crate::keyset::ApMembership,
    plan: &wh_device::keyset::WritePlan,
    depth: Um,
) -> Result<String> {
    // Cross-checked against `depth` independently of whatever `plan` itself computed: see
    // `confirm_ap_target`'s own doc for why `verify_write_as`'s board-vs-sent check alone cannot
    // catch a conversion bug that sends the wrong value everywhere.
    crate::keyset::confirm_ap_target(plan, depth)?;
    // The label names what actually happened, not a generic "keyset op": a selection that keeps
    // its membership never touched a keyset at all, and must not claim it did, while a split did
    // create one and says which.
    Ok(match membership {
        crate::keyset::ApMembership::Keep => format!("ap {:.2}mm", depth.to_mm()),
        crate::keyset::ApMembership::Split { index, losing } => {
            crate::keyset::announce_steal(
                out,
                losing,
                index.value(),
                crate::keyset::Target::Ap(depth),
                plan,
            )?;
            format!("ap keyset {} at {:.2}mm", index.value(), depth.to_mm())
        }
    })
}

/// Reads the active profile with no argument, or selects one with `1..=4`. A select takes no
/// automatic backup, unlike every write command above: this is a mode switch, not a settings
/// write. Snapshots are recorded per-profile, which is exactly what makes `wh restore` refuse
/// once the board is on a different profile than the one a snapshot was taken from.
fn profile_cmd(number: Option<u8>) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    with_session(|s| match number {
        None => {
            let p = ops::profile(s)?;
            writeln!(out, "profile {p}")?;
            Ok(())
        }
        Some(n) => {
            let target = cmds::ProfileNumber::from_one_based(n)?;
            let confirmed = ops::set_profile(s, target)?;
            writeln!(out, "profile {confirmed} selected")?;
            writeln!(
                out,
                "note: snapshots are per-profile; `wh restore` refuses to write a snapshot \
                 taken on a different profile than the one the board is currently on"
            )?;
            Ok(())
        }
    })
}

/// `Create`/`Set`/`Delete`/`Remove` are named explicitly, not matched by `_`, so a renamed or
/// added `KeysetWhat` variant is a compile error here rather than a silent "not yet implemented".
/// Decided before `with_session` opens the device, since the vendor HID collection is exclusive.
fn keyset_cmd(what: crate::cli::KeysetWhat, store: &Store) -> Result<()> {
    use crate::cli::KeysetWhat;
    match what {
        KeysetWhat::List { kind } => {
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            with_session(|s| match kind {
                Some(k) => crate::keyset::list(&mut out, s, crate::keyset::kind_of(k)),
                None => {
                    crate::keyset::list(&mut out, s, wh_device::keyset::Kind::Ap)?;
                    crate::keyset::list(&mut out, s, wh_device::keyset::Kind::Rt)
                }
            })
        }
        KeysetWhat::Create {
            kind,
            keys,
            value,
            press,
            release,
            dry_run,
        } => {
            let kind = crate::keyset::kind_of(kind);
            // Refused up front, before any flag is even converted: `--press`/`--release` mean
            // nothing on an actuation point create, and silently ignoring them would let a typo'd
            // command believe it set a sensitivity that was never used.
            if kind == wh_device::keyset::Kind::Ap && (press.is_some() || release.is_some()) {
                bail!(
                    "--press and --release apply to `wh keyset create rt`; pass --value for an \
                     actuation point keyset"
                );
            }
            let value = value.map(mm).transpose()?;
            let rt = match kind {
                wh_device::keyset::Kind::Ap => None,
                wh_device::keyset::Kind::Rt => {
                    let press = press.map(mm).transpose()?;
                    let release = release.map(mm).transpose()?;
                    resolve_rt_override(value, press, release)?
                }
            };
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            // A separate locked stderr for the whole-board confirmation prompt, matching
            // `KeysetWhat::Remove`'s own split: the per-key announcement stays on stdout since it
            // is data someone may pipe, while the prompt is a diagnostic.
            let stderr = std::io::stderr();
            let mut prompt_out = stderr.lock();
            let stdin = std::io::stdin();
            let mut input = stdin.lock();
            with_session(|s| {
                let usages = resolve_keys(s, &keys, store)?;
                // `!dry_run` is what keeps the confirmation off a preview: this arm calls
                // `create` before its own `dry_run` check, so the flag travels in rather than
                // guarding the call, the same shape `remove`'s `will_write` uses.
                let plan = crate::keyset::create(
                    &mut out,
                    &mut prompt_out,
                    s,
                    kind,
                    &usages,
                    value,
                    rt,
                    !dry_run,
                    &mut input,
                )?;
                if dry_run {
                    return print_frames(&mut out, &plan.frames());
                }
                auto_backup(s, store, BackupReason::KeysetCreate)?;
                wh_device::keyset::apply(s, &plan)?;
                crate::keyset::verify_write(&mut out, s, kind, KeysetOp::Create, &plan)
            })
        }
        KeysetWhat::Set {
            kind,
            index,
            value,
            press,
            release,
            dry_run,
        } => {
            let kind = crate::keyset::kind_of(kind);
            // Same refusal as `create`: rapid trigger flags mean nothing on an actuation point
            // operation, so a typo'd flag must be refused, not silently ignored.
            if kind == wh_device::keyset::Kind::Ap && (press.is_some() || release.is_some()) {
                bail!(
                    "--press and --release apply to `wh keyset set rt`; pass --value for an \
                     actuation point keyset"
                );
            }
            let value = value.map(mm).transpose()?;
            let rt = match kind {
                wh_device::keyset::Kind::Ap => None,
                wh_device::keyset::Kind::Rt => {
                    let press = press.map(mm).transpose()?;
                    let release = release.map(mm).transpose()?;
                    resolve_rt_override(value, press, release)?
                }
            };
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            with_session(|s| {
                let plan = crate::keyset::set_value(s, kind, index, value, rt)?;
                if dry_run {
                    return print_frames(&mut out, &plan.frames());
                }
                auto_backup(s, store, BackupReason::KeysetSet)?;
                wh_device::keyset::apply(s, &plan)?;
                crate::keyset::verify_write(&mut out, s, kind, KeysetOp::Set, &plan)
            })
        }
        KeysetWhat::Delete {
            kind,
            index,
            value,
            press,
            release,
            dry_run,
        } => {
            let kind = crate::keyset::kind_of(kind);
            if kind == wh_device::keyset::Kind::Ap && (press.is_some() || release.is_some()) {
                bail!(
                    "--press and --release apply to `wh keyset delete rt`; pass --value for an \
                     actuation point keyset"
                );
            }
            let value = value.map(mm).transpose()?;
            let rt = match kind {
                wh_device::keyset::Kind::Ap => None,
                wh_device::keyset::Kind::Rt => {
                    let press = press.map(mm).transpose()?;
                    let release = release.map(mm).transpose()?;
                    resolve_rt_override(value, press, release)?
                }
            };
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            with_session(|s| {
                let plan = crate::keyset::delete(&mut out, s, kind, index, value, rt)?;
                if dry_run {
                    return print_frames(&mut out, &plan.frames());
                }
                auto_backup(s, store, BackupReason::KeysetDelete)?;
                wh_device::keyset::apply(s, &plan)?;
                crate::keyset::verify_write(&mut out, s, kind, KeysetOp::Delete, &plan)
            })
        }
        KeysetWhat::Remove {
            kind,
            keys,
            dry_run,
        } => {
            let kind = crate::keyset::kind_of(kind);
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            // A separate locked stderr for the whole-board confirmation prompt: the per-key
            // announcement above stays on stdout since it is data someone may pipe, while the
            // prompt is a diagnostic, matching this binary's own `transport:` line.
            let stderr = std::io::stderr();
            let mut prompt_out = stderr.lock();
            let stdin = std::io::stdin();
            let mut input = stdin.lock();
            with_session(|s| {
                let usages = resolve_keys(s, &keys, store)?;
                let plan = crate::keyset::remove(
                    &mut out,
                    &mut prompt_out,
                    s,
                    kind,
                    &usages,
                    !dry_run,
                    &mut input,
                )?;
                if dry_run {
                    return print_frames(&mut out, &plan.frames());
                }
                auto_backup(s, store, BackupReason::KeysetRemove)?;
                wh_device::keyset::apply(s, &plan)?;
                crate::keyset::verify_write(&mut out, s, kind, KeysetOp::Remove, &plan)
            })
        }
    }
}

fn socd_cmd(what: crate::cli::SocdWhat, store: &Store) -> Result<()> {
    use crate::cli::SocdWhat;
    match what {
        SocdWhat::List => {
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            with_session(|s| crate::socd::list(&mut out, s))
        }
        SocdWhat::Pair {
            key_a,
            key_b,
            priority,
            dry_run,
        } => {
            // Resolved before a session opens: two different known keys, and a priority naming
            // one of them or `last-input`. A typo costs no device roundtrip and no backup.
            let pair = crate::socd::resolve_pair(&key_a, &key_b, &priority)?;
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            with_session(|s| {
                crate::socd::check_unpaired(&wh_device::socd::read_socd(s)?, pair)?;
                crate::socd::announce_pair(&mut out, pair)?;
                if dry_run {
                    return print_frames(&mut out, &[wh_proto::socd::write_pair(pair)]);
                }
                auto_backup(s, store, BackupReason::SocdPair)?;
                wh_device::socd::write_socd_pair(s, pair)?;
                crate::socd::report_pair(&mut out, pair)
            })
        }
        SocdWhat::Unpair { keys, dry_run } => {
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            with_session(|s| {
                let live = wh_device::socd::read_socd(s)?;
                let targets = crate::socd::resolve_unpair(&live, &keys)?;
                // Every plan is built, and every announcement made, before the first write: a
                // named key that turns out not to be paired must stop the command with nothing
                // sent, however many pairs came before it in the list.
                let mut plans = Vec::with_capacity(targets.len());
                for pair in targets {
                    let plan = wh_device::socd::plan_remove(s, pair)?;
                    crate::socd::announce_unpair(&mut out, &plan)?;
                    plans.push(plan);
                }
                if dry_run {
                    let frames: Vec<[u8; 64]> = plans.iter().flat_map(|p| p.frames()).collect();
                    return print_frames(&mut out, &frames);
                }
                auto_backup(s, store, BackupReason::SocdUnpair)?;
                for plan in &plans {
                    wh_device::socd::remove_socd_pair(s, plan)?;
                    crate::socd::report_unpair(&mut out, plan)?;
                }
                Ok(())
            })
        }
    }
}

fn backup(to: Option<std::path::PathBuf>, store: &Store) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    with_session(|s| {
        let mut snap = snapshot_from_device(s)?;
        snap.origin = Some(BackupReason::Manual.origin().to_string());
        let text = snap.to_json()?;
        match to {
            Some(p) => {
                std::fs::write(&p, &text)
                    .with_context(|| format!("writing backup to {}", p.display()))?;
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

/// One snapshot key, with every millimetre field already validated through `mm()`. Built
/// once, before `restore` opens a session, and reused by both `restore_records` and
/// `verify_restore` so the two can never drift on what "restored" means.
struct RestoreKey {
    usage: u8,
    ap: Um,
    mode_raw: u16,
    rt_press: Um,
    rt_release: Um,
    /// `None` when the snapshot predates keyset recording: distinct from `Some(0)`, a live read
    /// that found the key outside any keyset. `restore_membership_records` and `verify_restore`
    /// both key off this to leave a key's membership alone rather than assert `0` for it.
    ap_keyset: Option<u16>,
    rt_keyset: Option<u16>,
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
                // mode_raw stays a verbatim u16, unlike the millimetre fields above:
                // round-tripping the raw value is the entire point of this field.
                mode_raw: k.mode_raw,
                rt_press,
                rt_release,
                ap_keyset: k.ap_keyset,
                rt_keyset: k.rt_keyset,
            })
        })
        .collect()
}

/// Membership records for a restore, actuation point first then rapid trigger, each built
/// through `KeysetIndex::restoring` so an index from a snapshot can never be mistaken for one
/// allocation produced.
///
/// A key recorded at `Some(0)` still gets a record: skipping it would be incoherent with
/// `verify_restore` below, which would then read the board's real, live index back, find it
/// disagreeing with the snapshot's `0`, and fail a restore that never touched that key on
/// purpose. The write is otherwise unconditional and unverified against the vendor's own
/// per-operation rules, which is safe only because `verify_restore` re-reads every layout
/// afterwards, so a firmware side effect here surfaces as a reported mismatch, not silent drift.
/// A key recorded at `None`, meaning the snapshot predates these fields, gets no record at all:
/// `restore` cannot assert a membership it was never told.
fn restore_membership_records(keys: &[RestoreKey]) -> Result<Vec<KeyRecord>> {
    use wh_device::keyset::{KeysetIndex, Kind};
    let ap: Vec<(u8, KeysetIndex)> = keys
        .iter()
        .filter_map(|k| {
            k.ap_keyset
                .map(|v| (k.usage, KeysetIndex::restoring(Kind::Ap, v)))
        })
        .collect();
    let rt: Vec<(u8, KeysetIndex)> = keys
        .iter()
        .filter_map(|k| {
            k.rt_keyset
                .map(|v| (k.usage, KeysetIndex::restoring(Kind::Rt, v)))
        })
        .collect();
    let mut out = wh_device::keyset::membership_records_for_restore(&ap)?;
    out.extend(wh_device::keyset::membership_records_for_restore(&rt)?);
    Ok(out)
}

/// How many keys `restore_membership_records` left untouched because the snapshot predates
/// keyset recording, ap and rt counted separately since a hand-edited file could carry one field
/// and not the other. `restore` prints this to stderr so the gap reads as a reported limitation,
/// not as the silent success it was before this existed.
fn restore_membership_skip_counts(keys: &[RestoreKey]) -> (usize, usize) {
    let ap = keys.iter().filter(|k| k.ap_keyset.is_none()).count();
    let rt = keys.iter().filter(|k| k.rt_keyset.is_none()).count();
    (ap, rt)
}

/// `k.mode_raw` is written verbatim, including touch nibble `0` if the snapshot recorded it,
/// which puts the key back on global travel: the one place in `wh` that writes that touch nibble,
/// which the vendor has never been observed writing (`docs/keysets.md`). Membership restore can
/// also write other unobserved values, such as keyset index `7`; this is about the nibble only.
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

/// The dead zones every measured vendor `cmd 0x29` write carries. Measured 2026-09-05 over every
/// `cmd 0x29` frame in `captures/`: three vendor writes, at three different travel values, all
/// carrying 200 and 200, against 14 read requests whose replies all report `0` for both. Whether
/// 200 is a fixed constant or a user setting sitting at its default is **not** established, see
/// `docs/backlog.md`. We send it because it is the only value the vendor has been seen to write.
const VENDOR_PRESS_DEAD: Um = Um(200);
const VENDOR_RELEASE_DEAD: Um = Um(200);

/// The record's first value comes from the snapshot's `custom_value_mm`; the dead zones do not, and
/// are the two values above. The snapshot's own recorded dead zones stay a faithful record of what
/// the board reported, read by nothing.
fn snap_to_global(snap: &wh_config::snapshot::Snapshot) -> Result<cmds::GlobalTravel> {
    Ok(cmds::GlobalTravel {
        travel: mm(snap.global.custom_value_mm).context("global custom value")?,
        press_dead: VENDOR_PRESS_DEAD,
        release_dead: VENDOR_RELEASE_DEAD,
    })
}

/// Re-reads every restored key and confirms it holds every value the snapshot recorded. Every
/// comparison, ap, rt press, rt release, mode, and both keyset memberships, is its own `if`
/// pushing its own fault line, not one bundled condition: a bug that drops exactly one of them
/// (mode, say, the field `mode_raw` exists so advanced-key modes survive a round trip) must fail
/// on that comparison alone, not hide behind five others still catching the same key. A key whose
/// snapshot had no recorded membership (`RestoreKey::ap_keyset` or `rt_keyset` is `None`) gets no
/// membership comparison at all: comparing it against a fabricated `0` is exactly the defect
/// `restore_membership_records` avoids by not writing one.
///
/// It re-reads the **snapshot's** usages, not the board's, which only verifies anything because
/// `check_restore_matrix` has already refused any snapshot carrying a usage the board does not
/// have. Drop that refusal and this function reports a phantom usage verified.
fn verify_restore<T: Transport>(
    out: &mut impl Write,
    s: &mut Session<T>,
    keys: &[RestoreKey],
) -> Result<()> {
    let mut bad = Vec::new();
    let usages: Vec<u8> = keys.iter().map(|k| k.usage).collect();
    for k in keys {
        let ks = ops::read_key_settings(s, k.usage)?;
        let mut faults = Vec::new();

        if ks.ap != k.ap {
            faults.push(format!(
                "ap {:.2}mm, wanted {:.2}mm",
                ks.ap.to_mm(),
                k.ap.to_mm()
            ));
        }
        if ks.rt_press != k.rt_press {
            faults.push(format!(
                "rt press {:.2}mm, wanted {:.2}mm",
                ks.rt_press.to_mm(),
                k.rt_press.to_mm()
            ));
        }
        if ks.rt_release != k.rt_release {
            faults.push(format!(
                "rt release {:.2}mm, wanted {:.2}mm",
                ks.rt_release.to_mm(),
                k.rt_release.to_mm()
            ));
        }
        if ks.mode.value() != k.mode_raw {
            faults.push(format!(
                "mode {:#06x}, wanted {:#06x}",
                ks.mode.value(),
                k.mode_raw
            ));
        }
        if let Some(want) = k.ap_keyset {
            if ks.ap_keyset != want {
                faults.push(format!("ap keyset {}, wanted {want}", ks.ap_keyset));
            }
        }
        if let Some(want) = k.rt_keyset {
            if ks.rt_keyset != want {
                faults.push(format!("rt keyset {}, wanted {want}", ks.rt_keyset));
            }
        }

        if !faults.is_empty() {
            bad.push(format!(
                "{}: board reports {}",
                key_label(k.usage),
                faults.join("; ")
            ));
        }
    }
    report_verification(out, "restore", &usages, &bad)
}

/// The restore-time profile safety check: `wh backup` records no provenance beyond global
/// travel and per-key settings, so restoring a snapshot taken on one profile while the board
/// sits on another silently overwrites the wrong profile, and `restore`'s own readback
/// verification cannot catch it, since it reads back exactly what it just wrote.
/// `snap_profile` is what the snapshot recorded; `board_profile` is the board's current
/// profile, both `wh_proto::cmds::ProfileNumber` rather than a bare `u8`, since the wire's
/// zero-based index and the UI's one-based number are different things.
///
/// Three cases, and the two refusals are worded apart rather than sharing a tail, so an
/// operator (and a test) can tell which one fired:
/// - recorded and matching: proceed.
/// - recorded and differing: refuse. Restoring would overwrite the profile in front of you
///   with another one's settings, which is the more dangerous mistake of the two.
/// - not recorded: refuse. Nothing can establish which profile the settings belong to, and
///   asserting one on the operator's behalf is the same mistake with the blame moved.
fn check_restore_profile(
    snap_profile: Option<wh_proto::cmds::ProfileNumber>,
    board_profile: wh_proto::cmds::ProfileNumber,
) -> Result<()> {
    match snap_profile {
        Some(p) if p == board_profile => Ok(()),
        Some(p) => bail!(
            "snapshot was taken on profile {p} but the board is on profile {board_profile}; \
             restoring would silently overwrite profile {board_profile}'s settings with profile \
             {p}'s. Switch the board to profile {p} first, or restore to the profile you \
             actually intend; there is no override for this refusal"
        ),
        None => bail!(
            "snapshot has no recorded profile, so whether it belongs to the board's current \
             profile (profile {board_profile}) cannot be verified. This build never writes such \
             a snapshot: a board reporting a profile index it does not recognise fails the read \
             outright. Take a fresh snapshot on this board"
        ),
    }
}

/// How many missing key names the refusal below lists before it starts counting instead. A
/// snapshot from a foreign matrix can miss all 68 keys, and 68 names is not a message.
const MISSING_KEY_LIST_CAP: usize = 8;

/// The restore-time matrix safety check: every usage the snapshot carries must be a key the
/// board actually has. A snapshot from a different matrix would otherwise write values and
/// membership to usages this board has no key for, and `verify_restore` would then read those
/// same usages back and report them verified, since it re-reads exactly what it wrote. The harm
/// is that second part, not the write: `wh` would be telling the operator it checked something
/// it did not.
///
/// The converse is not a fault: a snapshot covering fewer keys than the board leaves the rest
/// alone, which is what `restore` has always done.
fn check_restore_matrix(keys: &[RestoreKey], board: &[u8]) -> Result<()> {
    // Deduped, in the snapshot's own order, the same shape `ops::read_matrix` uses: a
    // hand-edited file can list one key twice, and counting entries rather than keys would both
    // miscount and name the key twice. `wh backup` cannot produce that, since it builds from
    // `read_matrix`, which dedupes already.
    let mut missing: Vec<u8> = Vec::new();
    for u in keys.iter().map(|k| k.usage) {
        if !board.contains(&u) && !missing.contains(&u) {
            missing.push(u);
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    let n = missing.len();
    let mut list = missing
        .iter()
        .take(MISSING_KEY_LIST_CAP)
        .map(|&u| key_label(u))
        .collect::<Vec<String>>()
        .join(", ");
    if n > MISSING_KEY_LIST_CAP {
        list.push_str(&format!(", and {} more", n - MISSING_KEY_LIST_CAP));
    }
    bail!(
        "snapshot has {n} {} this board does not have ({list}), so it does not belong to this \
         board's key matrix; restoring it would write to usages this board has no key for and \
         then report them verified, having read back exactly what it wrote. Take a fresh \
         snapshot on this board",
        key_or_keys(n)
    )
}

fn restore(file: Option<std::path::PathBuf>, last: bool, store: &Store) -> Result<()> {
    if file.is_some() && last {
        bail!("pass a snapshot file or --last, not both");
    }
    let (path, text) = match (file, last) {
        (Some(p), _) => {
            let text = std::fs::read_to_string(&p)
                .with_context(|| format!("reading snapshot from {}", p.display()))?;
            (p, text)
        }
        (None, true) => store.load_backup(None)?,
        (None, false) => bail!("give a snapshot file or --last"),
    };
    let snap = wh_config::snapshot::Snapshot::from_file_text(&path, &text)
        .with_context(|| format!("parsing snapshot {}", path.display()))?;
    // `--last` means "the most recent snapshot, whatever took it", auto or manual alike: it does
    // not change meaning here, only visibility does. Naming what was picked, and its origin, is
    // the whole fix for a real session where that surprised an operator expecting a manual backup.
    if last {
        best_effort_eprintln(&format!(
            "--last picked {} (origin: {})",
            path.display(),
            snap.origin.as_deref().unwrap_or("unknown")
        ));
    }
    // Every value is validated and the write records built before a session ever opens, so a
    // bad snapshot is refused before a single frame is sent.
    let global = snap_to_global(&snap)?;
    let keys = validate_restore_keys(&snap)?;
    let records = restore_records(&keys);
    let membership = restore_membership_records(&keys)?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    with_session(|s| {
        // Read independently of `auto_backup` below, deliberately: coupling this to
        // `auto_backup`'s own returned snapshot would let a future `--no-backup` flag or a
        // best-effort backup silently drop this safety check with nothing failing to compile.
        let board_profile = ops::profile(s).context("reading the board's active profile")?;
        check_restore_profile(snap.profile, board_profile)?;
        // Three roundtrips on every restore that did not cost them before. Worth it because the
        // failure it catches is not the bad write but the false verification: `verify_restore`
        // reads back the snapshot's own usages, so without this it can report keys verified on a
        // board that does not have them.
        let board_matrix = ops::read_matrix(s).context("reading the board's key matrix")?;
        check_restore_matrix(&keys, &board_matrix)?;
        // Past every refusal that stops this restore before it writes anything: only from here
        // is it true that a key's membership is (about to be) left as the board already has it,
        // rather than describing a restore that never ran.
        let (skipped_ap, skipped_rt) = restore_membership_skip_counts(&keys);
        if skipped_ap > 0 || skipped_rt > 0 {
            best_effort_eprintln(&format!(
                "note: snapshot has no recorded actuation point keyset for {skipped_ap} key(s) \
                 and no recorded rapid trigger keyset for {skipped_rt} key(s) (it predates \
                 keyset recording); leaving those keys' membership on the board as it already is"
            ));
        }
        // Unlike `set`, scoped to selected keys, `restore` overwrites every key in the
        // snapshot: this auto-backup is the only way back if the named file turns out to be
        // the wrong one.
        auto_backup(s, store, BackupReason::Restore)?;
        ops::restore_all(s, &global, &records, &membership)?;
        // Printed only after verification passes, so stdout never claims success on a run
        // where stderr reports a mismatch.
        verify_restore(&mut out, s, &keys)?;
        let n = snap.keys.len();
        writeln!(
            out,
            "restored {n} {} from snapshot ({})",
            key_or_keys(n),
            snap.taken_at
        )?;
        Ok(())
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
            "global custom value: {:.2}mm, rewriting the record with the values just read",
            g.travel.to_mm()
        )?;
        // Deliberately no SAVE: it would turn every selftest run into a flash-wear cycle, and
        // this proves only that a write reaches the device and reads back. The two dead zones
        // go back out as they were read, `0` on every measured read: if they are a user setting
        // the board does not report (`docs/backlog.md`), that is not the no-op it looks like.
        s.roundtrip(&cmds::write_global_travel(
            g.travel,
            g.press_dead,
            g.release_dead,
        ))?;
        let g2 = ops::global_travel(s)?;
        if g2 != g {
            bail!("selftest FAILED: readback {:?} != {:?}", g2, g);
        }
        writeln!(
            out,
            "selftest OK: write path verified by rewriting the values just read"
        )?;
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

    /// Every origin string, pinned literally. Operators' existing backup files already hold these
    /// exact words and `wh backups list` prints them, so a rename here would silently split one
    /// command's history into two labels; this test is what makes that a deliberate act.
    #[test]
    fn every_backup_reason_renders_its_persisted_origin_string() {
        assert_eq!(BackupReason::SetAp.origin(), "auto: set ap");
        assert_eq!(BackupReason::SetRt.origin(), "auto: set rt");
        assert_eq!(BackupReason::SetMm.origin(), "auto: set mm");
        assert_eq!(BackupReason::KeysetCreate.origin(), "auto: keyset create");
        assert_eq!(BackupReason::KeysetSet.origin(), "auto: keyset set");
        assert_eq!(BackupReason::KeysetDelete.origin(), "auto: keyset delete");
        assert_eq!(BackupReason::KeysetRemove.origin(), "auto: keyset remove");
        assert_eq!(BackupReason::SocdPair.origin(), "auto: socd pair");
        assert_eq!(BackupReason::SocdUnpair.origin(), "auto: socd unpair");
        assert_eq!(BackupReason::Restore.origin(), "auto: restore");
        assert_eq!(BackupReason::Manual.origin(), "manual");
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
        // Pins that rejection means nothing was persisted, not just that some unrelated
        // error string satisfied the substring check above.
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

    /// `"wasd,all"` parses as a builtin-group reference filtered to nothing, followed by
    /// `Item::All`; under a one-element probe universe that combination would resolve to
    /// exactly the sentinel, indistinguishable from a genuine group hit. The two-element
    /// universe in `group_name_is_reachable` rules this out.
    #[test]
    fn comma_shaped_name_combining_all_is_rejected() {
        let dir = test_dir("comma-all");
        let store = Store::at(dir.clone());
        let err = keys(group_cmd("wasd,all", "w,a"), &store).unwrap_err();
        assert!(!err.to_string().is_empty());
        assert!(store.groups().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A name that reads back as a hex usage code (e.g. `0x01`) would be unreachable as a
    /// group: the selector grammar would resolve it against that literal usage, never the
    /// stored group. `group` must refuse it, the same class as a key-name collision.
    #[test]
    fn hex_shaped_group_name_is_rejected() {
        let dir = test_dir("hex-shaped");
        let store = Store::at(dir.clone());
        let err = keys(group_cmd("0x01", "w,a"), &store).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("0x01"),
            "unexpected error: {err}"
        );
        assert!(store.groups().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `0x00` is the one hex-shaped name the reachability probe alone would let through by
    /// coincidence, since its sentinel universe happens to include usage `0x00`: the explicit
    /// hex-shape guard is what actually closes this, not the generic reachability check.
    #[test]
    fn hex_shaped_group_name_zero_is_rejected() {
        let dir = test_dir("hex-shaped-zero");
        let store = Store::at(dir.clone());
        let err = keys(group_cmd("0x00", "w,a"), &store).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("0x00"),
            "unexpected error: {err}"
        );
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

    /// The group most worth deleting is exactly the one whose name collides with a key, since
    /// that collision is what makes it unusable as a selector. `ungroup` must take the name
    /// directly rather than through `Selector::parse`, or this exact case would fail with the
    /// same ambiguity error that made the group unreachable in the first place.
    #[test]
    fn ungroup_deletes_a_group_whose_name_collides_with_a_key() {
        let dir = test_dir("ungroup-key-collision");
        let store = Store::at(dir.clone());
        // The store itself allows this; only the CLI's create-time guard would refuse it.
        store.set_group("rt", &[0x1A]).unwrap();
        keys(
            KeysWhat::Ungroup {
                name: "rt".to_string(),
            },
            &store,
        )
        .unwrap();
        assert!(store.groups().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ungroup_reports_no_such_group() {
        let dir = test_dir("ungroup-missing");
        let store = Store::at(dir.clone());
        let err = keys(
            KeysWhat::Ungroup {
                name: "ghost".to_string(),
            },
            &store,
        )
        .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("ghost"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_moves_members_under_the_new_name() {
        let dir = test_dir("rename-basic");
        let store = Store::at(dir.clone());
        keys(group_cmd("fps", "w,a"), &store).unwrap();
        keys(
            KeysWhat::Rename {
                old: "fps".to_string(),
                new: "quiver".to_string(),
            },
            &store,
        )
        .unwrap();
        let groups = store.groups().unwrap();
        assert!(!groups.contains_key("fps"));
        assert!(groups.contains_key("quiver"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Rename must apply the same create-time guard as `group`: a new name that is a key or a
    /// builtin group name must be refused, not silently accepted and left unreachable again.
    #[test]
    fn rename_refuses_a_new_name_that_is_a_key() {
        let dir = test_dir("rename-into-key");
        let store = Store::at(dir.clone());
        keys(group_cmd("fps", "w,a"), &store).unwrap();
        let err = keys(
            KeysWhat::Rename {
                old: "fps".to_string(),
                new: "rt".to_string(),
            },
            &store,
        )
        .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("rt"));
        // Refusal must not clobber the original group.
        assert!(store.groups().unwrap().contains_key("fps"));
        let _ = std::fs::remove_dir_all(&dir);
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
        // The day right after the leap day above. An off-by-one in the leap-year handling
        // would show up as 2020-02-30 or 2020-03-02, not an obviously wrong year.
        assert_eq!(
            rfc3339_from_unix_secs(1_583_020_800),
            "2020-03-01T00:00:00Z"
        );
    }

    #[test]
    fn rfc3339_carries_the_time_of_day() {
        // Pins that hours/minutes/seconds are not silently dropped or truncated to midnight.
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

    /// Builds the one-based `ProfileNumber` `n` (e.g. `pn(2)` is the UI's "profile 2") via
    /// `from_one_based`: `from_wire_index(n - 1)` would underflow-panic on `pn(0)`, and
    /// `from_wire_index(n)` would silently mean a different profile than `pn`'s name promises.
    fn pn(n: u8) -> wh_proto::cmds::ProfileNumber {
        wh_proto::cmds::ProfileNumber::from_one_based(n).unwrap()
    }

    #[test]
    fn restore_profile_check_proceeds_on_a_match() {
        check_restore_profile(Some(pn(2)), pn(2)).unwrap();
    }

    /// The two refusals must not converge on one sentence: an operator reading only the tail
    /// they might share could not tell which one fired, and neither could a test.
    #[test]
    fn restore_profile_check_refuses_a_mismatch_and_an_unrecorded_profile_differently() {
        let mismatch = check_restore_profile(Some(pn(1)), pn(2))
            .unwrap_err()
            .to_string();
        assert!(
            mismatch.contains("snapshot was taken on profile 1 but the board is on profile 2"),
            "the mismatch refusal should name both profiles: {mismatch}"
        );

        let unrecorded = check_restore_profile(None, pn(2)).unwrap_err().to_string();
        assert!(
            unrecorded.contains("snapshot has no recorded profile"),
            "an unrecorded profile is refused in its own words: {unrecorded}"
        );
        assert!(
            !unrecorded.contains("was taken on profile"),
            "the two refusals must stay distinguishable: {unrecorded}"
        );
    }

    /// `--force` is gone, and so are both of the things its message said. "predates profile
    /// recording" was never true of a released `wh` (the field landed before the first tag),
    /// and naming a flag the operator cannot pass is a defect in its own right.
    #[test]
    fn restore_profile_refusal_names_neither_the_removed_flag_nor_the_dead_cause() {
        let msg = check_restore_profile(None, pn(2)).unwrap_err().to_string();
        assert!(
            !msg.contains("--force") && !msg.contains("predates"),
            "unexpected refusal text: {msg}"
        );
    }

    fn restore_key(usage: u8) -> RestoreKey {
        RestoreKey {
            usage,
            ap: Um(1200),
            mode_raw: 0x0010,
            rt_press: Um(500),
            rt_release: Um(600),
            ap_keyset: Some(0),
            rt_keyset: Some(0),
        }
    }

    /// A snapshot covering fewer keys than the board is not a mismatch: the rest are left alone,
    /// which is what `restore` has always done and what this check must not start refusing.
    #[test]
    fn restore_matrix_check_accepts_a_snapshot_covering_part_of_the_board() {
        let keys = [restore_key(0x1A)];
        check_restore_matrix(&keys, &[0x1A, 0x04, 0x16]).unwrap();
    }

    /// A named usage reads as its name, so the operator sees the key rather than a code.
    #[test]
    fn restore_matrix_check_names_a_missing_key() {
        let keys = [restore_key(0x1A), restore_key(0x16)];
        let msg = check_restore_matrix(&keys, &[0x1A, 0x04])
            .unwrap_err()
            .to_string();
        assert!(
            msg.contains("snapshot has 1 key this board does not have (s)"),
            "unexpected refusal: {msg}"
        );
        assert!(
            msg.contains("Take a fresh snapshot on this board"),
            "the refusal must say what the operator can do: {msg}"
        );
        assert!(
            !msg.contains("--"),
            "the refusal must not name a flag, there is not one: {msg}"
        );
    }

    /// A hand-edited snapshot can list one key twice. `wh backup` cannot produce that, since it
    /// builds from `read_matrix`, which dedupes, but the refusal still has to count keys rather
    /// than entries: "2 keys this board does not have (s, s)" is wrong twice over.
    #[test]
    fn restore_matrix_check_counts_a_duplicated_missing_usage_once() {
        let keys = [restore_key(0x16), restore_key(0x16)];
        let msg = check_restore_matrix(&keys, &[0x1A])
            .unwrap_err()
            .to_string();
        assert!(
            msg.contains("snapshot has 1 key this board does not have (s)"),
            "a usage listed twice is still one key: {msg}"
        );
    }

    /// An unnamed usage reads as its hex code rather than being dropped from the list, the same
    /// `key_label` fallback every other listing here uses.
    #[test]
    fn restore_matrix_check_renders_an_unnamed_missing_usage_as_hex() {
        let (unnamed, _) = two_usages_absent_from_table();
        let keys = [restore_key(unnamed)];
        let msg = check_restore_matrix(&keys, &[0x1A])
            .unwrap_err()
            .to_string();
        assert!(
            msg.contains(&format!("0x{unnamed:02X}")),
            "unexpected refusal: {msg}"
        );
    }

    /// A whole foreign matrix must not print 68 key names: the list is capped and the remainder
    /// counted, while the total still names every missing key in the count.
    #[test]
    fn restore_matrix_check_caps_the_listed_keys_and_counts_the_rest() {
        // Ten keys, none on a board holding only 'w': two more than the cap.
        let usages = [0x04u8, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D];
        let keys: Vec<RestoreKey> = usages.iter().map(|&u| restore_key(u)).collect();
        let msg = check_restore_matrix(&keys, &[0x1A])
            .unwrap_err()
            .to_string();
        assert!(
            msg.contains("snapshot has 10 keys this board does not have"),
            "the count must cover every missing key, not just the listed ones: {msg}"
        );
        assert!(msg.contains("and 2 more"), "unexpected refusal: {msg}");
        // 'a' through 'h' are listed; 'i' and 'j', the ninth and tenth, are counted instead.
        assert!(
            msg.contains("(a, b, c, d, e, f, g, h, and 2 more)"),
            "the cap must list the first eight and count the rest: {msg}"
        );
    }

    #[test]
    fn key_label_falls_back_to_hex_for_an_unnamed_usage() {
        let (unnamed, _) = two_usages_absent_from_table();
        assert_eq!(key_label(unnamed), format!("0x{unnamed:02X}"));
    }

    // -- ap_write_label: proves confirm_ap_target is actually wired in --

    use wh_device::replay::ReplayTransport;

    fn ap_write_label_l(dir: &str, b: &[u8; 64]) -> String {
        format!(
            "{{\"dir\":\"{dir}\",\"hex\":\"{}\"}}",
            wh_device::replay::hex(b)
        )
    }
    fn ap_write_label_rf(cmd: u8, payload: &[u8]) -> [u8; 64] {
        wh_proto::frame::frame(cmd | wh_proto::frame::REPLY_BIT, payload).unwrap()
    }
    #[allow(clippy::too_many_arguments)]
    fn ap_write_label_settings_script(
        usage: u8,
        ap: u16,
        mode: u16,
        rt_press: u16,
        rt_release: u16,
        ap_keyset: u16,
        rt_keyset: u16,
    ) -> Vec<String> {
        let mut lines = Vec::new();
        for (lid, val) in [
            (layout::AP, ap),
            (layout::MODE, mode),
            (layout::RT_PRESS, rt_press),
            (layout::RT_RELEASE, rt_release),
            (layout::KEYSET_AP, ap_keyset),
            (layout::KEYSET_RT, rt_keyset),
        ] {
            lines.push(ap_write_label_l("out", &cmds::read_key_layout(usage, lid)));
            lines.push(ap_write_label_l(
                "in",
                &ap_write_label_rf(
                    cmds::cmd::KEY,
                    &[0x00, usage, lid, (val & 0xFF) as u8, (val >> 8) as u8],
                ),
            ));
        }
        lines
    }

    /// Deleting `ap_write_label`'s call to `confirm_ap_target` leaves every other test in this
    /// crate green, since `set`'s `SetWhat::Ap` arm always builds `plan` and this check from the
    /// same `depth`, so the two can never disagree through the real path. This test bypasses that
    /// by building `plan` from `Change::ap(2.50mm)` and then confirming it against a different,
    /// independently-chosen depth (1.20mm), the only way to prove the call is wired in here at
    /// all, not merely correct in isolation (already pinned in `keyset.rs`).
    #[test]
    fn ap_write_label_bails_when_the_plan_disagrees_with_the_requested_depth() {
        let lines = ap_write_label_settings_script(0x1A, 2000, 0x18, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = wh_device::keyset::Change::ap(Um(2500));
        let plan = wh_device::keyset::plan(&mut s, &[0x1A], &change, None).unwrap();
        let membership = crate::keyset::ApMembership::Keep;

        let mut out = Vec::new();
        let err = ap_write_label(&mut out, &membership, &plan, Um(1200)).unwrap_err();
        assert!(
            err.to_string()
                .contains("plan resolved w to 2.50mm, not the 1.20mm requested"),
            "got: {err}"
        );
    }
}
