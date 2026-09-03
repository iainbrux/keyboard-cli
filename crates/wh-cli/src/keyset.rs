//! The `wh keyset` command tree. Every handler here reads the board's live membership first;
//! `wh` caches no device state, and allocation is max plus one over live membership, so a stale
//! view could hand out an index a key already holds.

use anyhow::{bail, Result};
use std::io::Write;
use wh_device::keyset::{self, Global, Keyset, Kind, Membership};
use wh_device::ops;
use wh_device::session::Session;
use wh_device::transport::Transport;
use wh_proto::cmds::layout;
use wh_proto::value::Um;

use crate::cli::KeysetKindArg;
use crate::run::key_label;

pub(crate) fn kind_of(arg: KeysetKindArg) -> Kind {
    match arg {
        KeysetKindArg::Ap => Kind::Ap,
        KeysetKindArg::Rt => Kind::Rt,
    }
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Ap => "ap",
        Kind::Rt => "rt",
    }
}

/// The keyset holding `index`, or an error naming what is actually there. A caller that let a
/// missing index through would allocate nothing and write membership to no keys, succeeding
/// silently. Tested directly below; still unreached outside a test build until `set` or `delete`
/// land and call it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn resolve_index(sets: &[Keyset], index: u16) -> Result<Keyset> {
    if index == 0 {
        bail!("0 is not a keyset index; it is the value a key outside every keyset holds");
    }
    match sets.iter().find(|k| k.index == index) {
        Some(k) => Ok(k.clone()),
        None if sets.is_empty() => bail!("no keysets of this kind exist on the board"),
        None => {
            let live: Vec<String> = sets.iter().map(|k| k.index.to_string()).collect();
            bail!("no keyset {index}; the board has {}", live.join(", "))
        }
    }
}

/// The board's global actuation point, or an error the operator can act on. `wh` never picks a
/// winner when the keys outside every keyset disagree: a majority vote would write a value
/// nobody typed over every member of the keyset being created.
pub(crate) fn global_ap_or_bail<T: Transport>(
    s: &mut Session<T>,
    m: &Membership,
    flag: &str,
) -> Result<Um> {
    match keyset::global_ap(s, m)? {
        Global::Agreed(v) => Ok(v),
        Global::Split(counts) => bail!("{}", split_message("actuation point", &counts, flag)),
        Global::NoneOutsideAKeyset => bail!(
            "every key on the board is in a keyset, so there is no global actuation point to \
             read; pass {flag} to say which value to use"
        ),
    }
}

/// The board's global rapid trigger sensitivity, or an error the operator can act on. Same
/// refusal as `global_ap_or_bail`, over the rapid trigger layout instead.
pub(crate) fn global_rt_or_bail<T: Transport>(
    s: &mut Session<T>,
    m: &Membership,
    flag: &str,
) -> Result<(Um, Um)> {
    match keyset::global_rt(s, m)? {
        Global::Agreed(v) => Ok(v),
        Global::Split(counts) => {
            let shown: Vec<(String, usize)> = counts
                .iter()
                .map(|((p, r), n)| (format!("{:.2}/{:.2}mm", p.to_mm(), r.to_mm()), *n))
                .collect();
            bail!(
                "{}",
                split_message_str("rapid trigger sensitivity", &shown, flag)
            )
        }
        Global::NoneOutsideAKeyset => bail!(
            "every key on the board is in a keyset, so there is no global rapid trigger \
             sensitivity to read; pass {flag} to say which value to use"
        ),
    }
}

fn split_message(what: &str, counts: &[(Um, usize)], flag: &str) -> String {
    let shown: Vec<(String, usize)> = counts
        .iter()
        .map(|(v, n)| (format!("{:.2}mm", v.to_mm()), *n))
        .collect();
    split_message_str(what, &shown, flag)
}

fn split_message_str(what: &str, shown: &[(String, usize)], flag: &str) -> String {
    let parts: Vec<String> = shown
        .iter()
        .map(|(v, n)| format!("{n} key(s) at {v}"))
        .collect();
    format!(
        "the keys outside every keyset disagree on the global {what} ({}), so there is no one \
         global value to use; pass {flag} to say which",
        parts.join(", ")
    )
}

/// Lists one kind's keysets with their members and value. Every member is read and compared: an
/// agreeing keyset prints its one value, a disagreeing one names each distinct value and which
/// keys hold it, matching the refusal `global_ap` and `global_rt` already apply to the board's
/// global value.
pub(crate) fn list<T: Transport>(
    out: &mut impl Write,
    s: &mut Session<T>,
    kind: Kind,
) -> Result<()> {
    let m = keyset::read_membership(s, kind)?;
    let sets = keyset::group(&m);
    if sets.is_empty() {
        writeln!(out, "{} keysets: none", kind_name(kind))?;
        return Ok(());
    }
    writeln!(out, "{} keysets:", kind_name(kind))?;
    for ks in &sets {
        let line = keyset_line(s, kind, ks)?;
        writeln!(out, "  {} {}", ks.index, line)?;
    }
    Ok(())
}

/// One keyset's value column: every member read individually, then compared. `Kind::Ap` reads
/// one layout per member, `Kind::Rt` reads two; neither reads the four layouts `list` doesn't
/// print, unlike `ops::read_key_settings` would.
fn keyset_line<T: Transport>(s: &mut Session<T>, kind: Kind, ks: &Keyset) -> Result<String> {
    match kind {
        Kind::Ap => {
            let mut values = Vec::with_capacity(ks.members.len());
            for &u in &ks.members {
                values.push((u, Um(ops::read_layout_value(s, u, layout::AP)?)));
            }
            agreement_line(&values, |v: Um| format!("{:.2}mm", v.to_mm()))
        }
        Kind::Rt => {
            let mut values = Vec::with_capacity(ks.members.len());
            for &u in &ks.members {
                let press = Um(ops::read_layout_value(s, u, layout::RT_PRESS)?);
                let release = Um(ops::read_layout_value(s, u, layout::RT_RELEASE)?);
                values.push((u, (press, release)));
            }
            agreement_line(&values, |(p, r): (Um, Um)| {
                format!("{:.2}/{:.2}mm", p.to_mm(), r.to_mm())
            })
        }
    }
}

/// Renders one keyset's members and their values: every member's name if they agree, or, if they
/// do not, each distinct value with the names of the keys holding it. Never a single member's
/// value passed off as the whole keyset's, which is exactly the defect this replaces. Errors on
/// an empty slice rather than indexing it: `keyset::group` never builds a memberless `Keyset`
/// today, but that invariant lives in a different crate, not here.
fn agreement_line<V: PartialEq + Copy>(
    members: &[(u8, V)],
    fmt: impl Fn(V) -> String,
) -> Result<String> {
    let first = match members.first() {
        Some(&(_, v)) => v,
        None => bail!("keyset has no members to compare"),
    };
    if members.iter().all(|&(_, v)| v == first) {
        let names: Vec<String> = members.iter().map(|&(u, _)| key_label(u)).collect();
        return Ok(format!("{}  {}", fmt(first), names.join(",")));
    }
    let mut groups: Vec<(V, Vec<u8>)> = Vec::new();
    for &(u, v) in members {
        match groups.iter_mut().find(|(gv, _)| *gv == v) {
            Some((_, us)) => us.push(u),
            None => groups.push((v, vec![u])),
        }
    }
    let parts: Vec<String> = groups
        .into_iter()
        .map(|(v, us)| {
            let names: Vec<String> = us.iter().map(|&u| key_label(u)).collect();
            format!("{} at {}", names.join(","), fmt(v))
        })
        .collect();
    Ok(format!("disagree: {}", parts.join(", ")))
}

/// The value a create resolved to write: `--value`, or `--press`/`--release`, or the board's
/// global. Carried alongside the plan so `announce_steal` can show what a stolen member is about
/// to lose it for.
#[derive(Clone, Copy)]
pub(crate) enum Target {
    Ap(Um),
    Rt(Um, Um),
}

impl Target {
    fn display(self) -> String {
        match self {
            Target::Ap(v) => format!("{:.2}mm", v.to_mm()),
            Target::Rt(p, r) => format!("{:.2}/{:.2}mm", p.to_mm(), r.to_mm()),
        }
    }
}

/// Creates a keyset over `usages` at the global value, or at an explicit one. Announces which
/// existing keysets lose members first: a create overwrites its members' values with the global
/// rather than carrying them in, so the operator sees what is about to go.
pub(crate) fn create<T: Transport>(
    out: &mut impl Write,
    s: &mut Session<T>,
    kind: Kind,
    usages: &[u8],
    value: Option<Um>,
    rt: Option<(Um, Um)>,
) -> Result<CreatePlan> {
    let m = keyset::read_membership(s, kind)?;
    let index = keyset::next_index(&m)?;
    let (change, target) = match kind {
        Kind::Ap => {
            let v = match value {
                Some(v) => v,
                None => global_ap_or_bail(s, &m, "--value")?,
            };
            (keyset::Change::ap(v), Target::Ap(v))
        }
        Kind::Rt => {
            let (p, r) = match rt {
                Some(v) => v,
                None => global_rt_or_bail(s, &m, "--press and --release")?,
            };
            (keyset::Change::rt_on(p, r), Target::Rt(p, r))
        }
    };
    let losing = losing_members(&keyset::group(&m), usages);
    // Built before the announcement, not after: `before()` is what lets the announcement show
    // each stolen member's own pre-write value, and it costs no extra reads since `plan` sends
    // them anyway. `plan` itself only reads; nothing here has written to the board yet.
    let plan = keyset::plan(s, usages, &change, Some(index))?;
    announce_steal(out, kind, &losing, index.value(), target, &plan)?;
    Ok(CreatePlan { index, plan })
}

/// Existing keysets that would lose members to a create over `usages`, as (index, the members it
/// loses). In `group`'s own order, which is ascending by index. Every returned member is one of
/// `usages`, since `taken` is built by filtering each keyset's own members against it, which is
/// what lets `announce_steal` find each one in `plan.before()` unconditionally.
fn losing_members(sets: &[Keyset], usages: &[u8]) -> Vec<(u16, Vec<u8>)> {
    sets.iter()
        .filter_map(|ks| {
            let taken: Vec<u8> = ks
                .members
                .iter()
                .copied()
                .filter(|u| usages.contains(u))
                .collect();
            (!taken.is_empty()).then_some((ks.index, taken))
        })
        .collect()
}

/// Announces which existing keysets lose members to a create, and what they lose it for: a
/// create overwrites a stolen member's value with the target rather than carrying its own in, so
/// this line is the operator's only warning before that happens.
///
/// Takes `plan` whole rather than a separate key list, so a member `losing` names can never be
/// looked up against settings from a different selection: every usage `losing` carries came from
/// `losing_members` filtering against the same `usages` `plan` was built over, so `plan.before()`
/// always has an entry for it.
pub(crate) fn announce_steal(
    out: &mut impl Write,
    kind: Kind,
    losing: &[(u16, Vec<u8>)],
    new_index: u16,
    target: Target,
    plan: &keyset::WritePlan,
) -> std::io::Result<()> {
    writeln!(
        out,
        "{} keyset {new_index}: creating at {}",
        kind_name(kind),
        target.display()
    )?;
    for (index, taken) in losing {
        let parts: Vec<String> = taken
            .iter()
            .map(|&u| describe_loss(kind, plan, u))
            .collect();
        writeln!(out, "  keyset {index} loses {}", parts.join(","))?;
    }
    Ok(())
}

/// One stolen key's line: its prior value, when the write is actually about to change it, or a
/// note that it keeps that value and only its index moves, when `plan`'s skip rule found nothing
/// to write for it. A key already at the target value loses nothing but its old index, and
/// saying it "loses w at 2.00mm" would claim the opposite of what is about to happen.
fn describe_loss(kind: Kind, plan: &keyset::WritePlan, u: u8) -> String {
    let prior = plan
        .before()
        .iter()
        .find(|ks| ks.usage == u)
        .expect("losing_members only ever names a usage plan was built over");
    let value = value_display(kind, prior);
    let name = key_label(u);
    if plan.value_records().iter().any(|r| r.key == u) {
        format!("{name} at {value}")
    } else {
        format!("{name} (keeps {value}, index only)")
    }
}

/// One key's current value, formatted the way `kind` reports it: `Kind::Ap`'s bare millimetres,
/// `Kind::Rt`'s press/release pair.
fn value_display(kind: Kind, ks: &ops::KeySettings) -> String {
    match kind {
        Kind::Ap => format!("{:.2}mm", ks.ap.to_mm()),
        Kind::Rt => format!("{:.2}/{:.2}mm", ks.rt_press.to_mm(), ks.rt_release.to_mm()),
    }
}

/// A create's index and the plan that writes it, returned together so a caller can both apply
/// the plan and verify against the exact index it carries.
pub(crate) struct CreatePlan {
    pub index: keyset::KeysetIndex,
    pub plan: keyset::WritePlan,
}

/// Re-reads every key `plan` touched and confirms it holds both the new index and the value
/// `plan` computed for it, the same way `verify_ap`/`verify_rt` check a plain `wh set` write:
/// against what `plan` actually sent, or, for a key the skip rule left untouched, against what
/// was read from it before the write (which the skip rule only fires when that already equals
/// the target). Reads the board back rather than trusting the write's echo, the same way every
/// other write path in `wh` verifies.
///
/// Takes no separate key list: `plan.before()` covers every key `plan` was built over, in order,
/// so the keys checked here can never drift from the keys `apply` actually wrote. A caller that
/// passed a key list of its own alongside `plan` could hand the two different views of the
/// selection; indexing one by a position from the other is exactly what panicked, after the
/// write, on a length mismatch, and would have verified a key against another key's settings on
/// an order mismatch.
pub(crate) fn verify_create<T: Transport>(
    out: &mut impl Write,
    s: &mut Session<T>,
    kind: Kind,
    want: u16,
    plan: &keyset::WritePlan,
) -> Result<()> {
    let mut bad = Vec::new();
    let mut usages = Vec::new();
    for before in plan.before() {
        let u = before.usage;
        usages.push(u);
        let ks = wh_device::ops::read_key_settings(s, u)?;
        let mut faults = Vec::new();

        let got_membership = match kind {
            Kind::Ap => ks.ap_keyset,
            Kind::Rt => ks.rt_keyset,
        };
        if got_membership != want {
            faults.push(format!("keyset {got_membership}, wanted {want}"));
        }

        let sent = |layout_id: u8| {
            plan.value_records()
                .iter()
                .find(|r| r.key == u && r.layout == layout_id)
                .map(|r| r.value)
        };
        let want_mode = sent(layout::MODE).unwrap_or_else(|| before.mode.value());
        if ks.mode.value() != want_mode {
            faults.push(format!(
                "mode {:#06x}, wanted mode {want_mode:#06x}",
                ks.mode.value()
            ));
        }

        match kind {
            Kind::Ap => {
                let want_ap = Um(sent(layout::AP).unwrap_or(before.ap.0));
                if ks.ap != want_ap {
                    faults.push(format!(
                        "ap {:.2}mm, wanted {:.2}mm",
                        ks.ap.to_mm(),
                        want_ap.to_mm()
                    ));
                }
            }
            Kind::Rt => {
                let want_press = Um(sent(layout::RT_PRESS).unwrap_or(before.rt_press.0));
                let want_release = Um(sent(layout::RT_RELEASE).unwrap_or(before.rt_release.0));
                if ks.rt_press != want_press || ks.rt_release != want_release {
                    faults.push(format!(
                        "press {:.2}mm release {:.2}mm, wanted press {:.2}mm release {:.2}mm",
                        ks.rt_press.to_mm(),
                        ks.rt_release.to_mm(),
                        want_press.to_mm(),
                        want_release.to_mm()
                    ));
                }
            }
        }

        if !faults.is_empty() {
            bad.push(format!(
                "{}: board reports {}",
                key_label(u),
                faults.join("; ")
            ));
        }
    }
    // `wh restore` does not yet write keyset membership (only the four value layouts), so a
    // rollback after a mismatch here restores values but leaves membership as this write left it.
    let what = format!("{} keyset create", kind_name(kind));
    crate::run::report_verification(out, &what, &usages, &bad)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ks(index: u16, members: &[u8]) -> Keyset {
        Keyset {
            index,
            members: members.to_vec(),
        }
    }

    #[test]
    fn resolve_index_refuses_zero() {
        let err = resolve_index(&[ks(1, &[0x1A])], 0).unwrap_err();
        assert!(
            err.to_string().contains("0 is not a keyset index"),
            "got: {err}"
        );
    }

    #[test]
    fn resolve_index_names_the_live_indices_when_the_one_asked_for_is_missing() {
        let sets = [ks(1, &[0x1A]), ks(3, &[0x04])];
        let err = resolve_index(&sets, 2).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no keyset 2"), "got: {msg}");
        assert!(msg.contains('1') && msg.contains('3'), "got: {msg}");
    }

    #[test]
    fn resolve_index_says_the_board_has_no_keysets_of_this_kind_when_empty() {
        let err = resolve_index(&[], 1).unwrap_err();
        assert!(
            err.to_string()
                .contains("no keysets of this kind exist on the board"),
            "got: {err}"
        );
    }

    /// Pins the wording `global_ap_or_bail` depends on: each distinct value named with how many
    /// keys hold it, and the flag that resolves the disagreement, since `wh` refuses rather than
    /// voting and this string is the only place that decision reaches the operator.
    #[test]
    fn split_message_names_each_value_its_count_and_the_resolving_flag() {
        let msg = split_message(
            "actuation point",
            &[(Um(2000), 2), (Um(1000), 1)],
            "--value",
        );
        assert!(msg.contains("2 key(s) at 2.00mm"), "got: {msg}");
        assert!(msg.contains("1 key(s) at 1.00mm"), "got: {msg}");
        assert!(msg.contains("pass --value to say which"), "got: {msg}");
    }
}
