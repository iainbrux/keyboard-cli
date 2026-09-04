//! The `wh keyset` command tree. Every handler here reads the board's live membership first;
//! `wh` caches no device state, and allocation is max plus one over live membership, so a stale
//! view could hand out an index a key already holds.

use anyhow::{bail, Result};
use std::io::Write;
use wh_device::keyset::{self, Global, Keyset, Kind, Membership};
use wh_device::ops;
use wh_device::session::Session;
use wh_device::transport::Transport;
use wh_proto::cmds::{layout, KeyRecord};
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
/// silently.
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

/// The value a create or delete resolved to write: `--value`, or `--press`/`--release`, or the
/// board's global. Carried alongside the plan so `announce_steal` and `announce_delete` can show
/// what each affected member is about to lose it for.
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
) -> Result<keyset::WritePlan> {
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
    Ok(plan)
}

/// Changes an existing keyset's value across every member. Membership is untouched: the keyset
/// keeps its index and its member list.
pub(crate) fn set_value<T: Transport>(
    s: &mut Session<T>,
    kind: Kind,
    index: u16,
    value: Option<Um>,
    rt: Option<(Um, Um)>,
) -> Result<keyset::WritePlan> {
    let m = keyset::read_membership(s, kind)?;
    let ks = resolve_index(&keyset::group(&m), index)?;
    let change = match kind {
        Kind::Ap => keyset::Change::ap(value.ok_or_else(|| {
            anyhow::anyhow!("pass --value to say what this keyset's actuation point becomes")
        })?),
        Kind::Rt => {
            let (p, r) = rt.ok_or_else(|| {
                anyhow::anyhow!(
                    "pass --press and --release, or --value to set both, to say what this \
                     keyset's rapid trigger sensitivity becomes"
                )
            })?;
            keyset::Change::rt_on(p, r)
        }
    };
    Ok(keyset::plan(s, &ks.members, &change, None)?)
}

/// Deletes a keyset: its members return to the global value and their membership is cleared.
/// Announces every member's prior value and what replaces it before writing, since a delete
/// overwrites all of them with a global the operator may never have typed. Values go out first
/// and membership last, which is `plan`'s own ordering.
pub(crate) fn delete<T: Transport>(
    out: &mut impl Write,
    s: &mut Session<T>,
    kind: Kind,
    index: u16,
    value: Option<Um>,
    rt: Option<(Um, Um)>,
) -> Result<keyset::WritePlan> {
    let m = keyset::read_membership(s, kind)?;
    let ks = resolve_index(&keyset::group(&m), index)?;
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
            (keyset::Change::rt_off(p, r), Target::Rt(p, r))
        }
    };
    let cleared = keyset::KeysetIndex::clear(kind);
    let plan = keyset::plan(s, &ks.members, &change, Some(cleared))?;
    announce_delete(out, kind, index, target, &plan)?;
    Ok(plan)
}

/// Takes named keys out of whatever keyset each is in, returning them to the global value and
/// leaving every other member of those keysets untouched. The vendor sends the ordinary per-key
/// template for the removed key alone, ending in one `0xFF = 0` record, and writes nothing for
/// the members that stay (`docs/keysets.md`). A keyset that loses its last member simply ceases
/// to exist, so there is no emptying case to handle here.
pub(crate) fn remove<T: Transport>(
    out: &mut impl Write,
    s: &mut Session<T>,
    kind: Kind,
    usages: &[u8],
    value: Option<Um>,
    rt: Option<(Um, Um)>,
) -> Result<keyset::WritePlan> {
    let m = keyset::read_membership(s, kind)?;
    let sets = keyset::group(&m);
    let leaving: Vec<(u16, u8)> = usages
        .iter()
        .filter_map(|&u| {
            sets.iter()
                .find(|k| k.members.contains(&u))
                .map(|k| (k.index, u))
        })
        .collect();
    if leaving.is_empty() {
        bail!(
            "none of those keys is in an {} keyset; nothing to remove",
            kind_name(kind)
        );
    }
    // The same two branches `delete` uses, because the vendor sends the same template for a
    // single-key removal as for a whole-keyset delete. Rt writes touch nibble 1 and the global
    // sensitivity, and deliberately does not touch layout 0x04: the removed key keeps its own
    // actuation point (`ks-remove-one-rt`).
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
            (keyset::Change::rt_off(p, r), Target::Rt(p, r))
        }
    };
    let moving: Vec<u8> = leaving.iter().map(|&(_, u)| u).collect();
    let free: Vec<u8> = usages
        .iter()
        .copied()
        .filter(|u| !moving.contains(u))
        .collect();
    let cleared = keyset::KeysetIndex::clear(kind);
    let plan = keyset::plan(s, &moving, &change, Some(cleared))?;
    announce_remove(out, kind, &leaving, &free, target, &plan)?;
    Ok(plan)
}

/// Announces a remove's effect on every key that actually left a keyset, and names any selected
/// key that was already free so that case is never silent: a removal that dropped a free key
/// wordlessly would leave the operator thinking it moved when it never held anything to leave.
/// `free` is derived from `usages` rather than `plan.before()`, since `plan` is built only over
/// the leaving keys and never reads a free one at all.
fn announce_remove(
    out: &mut impl Write,
    kind: Kind,
    leaving: &[(u16, u8)],
    free: &[u8],
    target: Target,
    plan: &keyset::WritePlan,
) -> std::io::Result<()> {
    for &(index, u) in leaving {
        let prior = plan
            .before()
            .iter()
            .find(|ks| ks.usage == u)
            .expect("leaving keys were the selection plan was built over");
        let prior_value = value_display(kind, prior);
        writeln!(
            out,
            "{}: removing {} from keyset {index}, {prior_value} to {}",
            kind_name(kind),
            key_label(u),
            target.display()
        )?;
    }
    if !free.is_empty() {
        let names: Vec<String> = free.iter().map(|&u| key_label(u)).collect();
        writeln!(
            out,
            "{}: {} were already in no keyset, left alone",
            kind_name(kind),
            names.join(",")
        )?;
    }
    Ok(())
}

/// Announces a delete's effect on every member before it writes: what each currently holds and
/// what it is about to become. Reuses `describe_member`'s vocabulary, the same one `announce_steal`
/// uses when a create takes a member from another keyset, since a delete is the same kind of loss.
fn announce_delete(
    out: &mut impl Write,
    kind: Kind,
    index: u16,
    target: Target,
    plan: &keyset::WritePlan,
) -> std::io::Result<()> {
    writeln!(
        out,
        "{} keyset {index}: deleting, returning members to {}",
        kind_name(kind),
        target.display()
    )?;
    for before in plan.before() {
        writeln!(out, "  {}", describe_member(kind, plan, before.usage))?;
    }
    Ok(())
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

/// What `wh set ap` should do about membership for `usages`. What is measured is only that a
/// capture wrote no `0xFF` record over a value change; that the selection behind it happened to
/// equal one whole keyset, the one case `Keep` now covers, is inferred, not itself read back
/// (`docs/keysets.md`). No capture shows the vendor splitting a keyset either: what was observed
/// is its UI copying a mixed selection into a new one.
#[derive(Debug, PartialEq)]
pub(crate) enum ApMembership {
    /// Leave membership alone: the selection is exactly one keyset's members, so it keeps its
    /// index. A selection of free keys does not come here; it allocates, since a key holding a
    /// value of its own belongs to a keyset.
    Keep,
    /// Move every selected key into a newly allocated keyset, taking these members from these
    /// existing keysets.
    Split {
        index: keyset::KeysetIndex,
        losing: Vec<(u16, Vec<u8>)>,
    },
}

/// Errors if `m` isn't `Kind::Ap` membership, the same guard `global_ap`/`global_rt` and `plan`
/// apply: the one caller today always passes `Kind::Ap`, so this is a latent contract hole rather
/// than a live bug, but a future caller passing a rapid trigger `Membership` would otherwise get
/// `Keep` back silently and wrongly, since `losing_members` would find nothing to lose against an
/// actuation point selection it was never built over.
pub(crate) fn ap_membership_for(m: &Membership, usages: &[u8]) -> Result<ApMembership> {
    if m.kind() != Kind::Ap {
        bail!(
            "ap_membership_for requires an actuation point membership, got {:?}",
            m.kind()
        );
    }
    let sets = keyset::group(m);
    let losing = losing_members(&sets, usages);
    if losing.len() == 1 {
        let (index, taken) = &losing[0];
        let whole = sets
            .iter()
            .find(|k| k.index == *index)
            .is_some_and(|k| k.members.len() == taken.len());
        if whole && taken.len() == usages.len() {
            return Ok(ApMembership::Keep);
        }
    }
    Ok(ApMembership::Split {
        index: keyset::next_index(m)?,
        losing,
    })
}

/// Confirms `plan`'s resolved actuation point target, for every key it covers, equals `depth`,
/// the operator's own requested value. `verify_write` only ever compares the board against what
/// `plan` sent: an internally consistent check that a conversion bug in `Change::ap` or `plan`
/// itself, one that sent the wrong value everywhere, would pass cleanly, since the board would
/// then agree with the very number the bug produced. This is the one place that number is
/// checked against a source independent of `plan`.
pub(crate) fn confirm_ap_target(plan: &keyset::WritePlan, depth: Um) -> Result<()> {
    for before in plan.before() {
        let sent = plan
            .value_records()
            .iter()
            .find(|r| r.key == before.usage && r.layout == layout::AP)
            .map(|r| Um(r.value));
        let target = sent.unwrap_or(before.ap);
        if target != depth {
            bail!(
                "internal error: plan resolved {} to {:.2}mm, not the {:.2}mm requested",
                key_label(before.usage),
                target.to_mm(),
                depth.to_mm()
            );
        }
    }
    Ok(())
}

/// Announces which existing keysets lose members to a create, what they lose it for, and which
/// free keys (in no keyset at all) are being enrolled alongside them: a create overwrites every
/// affected member's value with the target rather than carrying its own in, and a selection that
/// mixes stolen members with free keys moves the free ones too, so both halves need saying before
/// either happens. Naming only the losing keysets, as an earlier version did, described half the
/// change: a selection of an entire keyset plus one free key silently enrolled that key with
/// nothing printed about it at all.
///
/// Takes `plan` whole rather than a separate key list, so a member `losing` names, or a free key
/// this derives from `plan.before()`, can never be looked up against settings from a different
/// selection: every usage `losing` carries came from `losing_members` filtering against the same
/// `usages` `plan` was built over, so `plan.before()` always has an entry for it.
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
    let mut taken_flat: Vec<u8> = Vec::new();
    for (index, taken) in losing {
        let parts: Vec<String> = taken
            .iter()
            .map(|&u| describe_member(kind, plan, u))
            .collect();
        writeln!(out, "  keyset {index} loses {}", parts.join(","))?;
        taken_flat.extend(taken.iter().copied());
    }
    let free: Vec<u8> = plan
        .before()
        .iter()
        .map(|b| b.usage)
        .filter(|u| !taken_flat.contains(u))
        .collect();
    if !free.is_empty() {
        let parts: Vec<String> = free
            .iter()
            .map(|&u| describe_member(kind, plan, u))
            .collect();
        writeln!(out, "  enrolling free key(s) {}", parts.join(","))?;
    }
    Ok(())
}

/// One member's line, describing what a create's steal, a create's plain enrollment of a free
/// key, or a delete's own write does to it. Three cases, since a value record can be present
/// without the value it carries actually moving: `plan` echoes a key's own value back unchanged
/// whenever anything else about it (its touch mode, say) changes, so "a record exists" is not
/// "the value changes".
///
/// - The value itself moves: "w at 2.00mm", its prior value, about to be overwritten.
/// - Nothing at all was written for it (`plan`'s skip rule): "w (keeps 2.00mm, index only)".
/// - The value stays but the touch mode moves: "w (keeps 2.00mm, mode Global to Single)", naming
///   the thing that actually changes rather than implying nothing did.
/// - Something else was written but neither the value nor the touch mode moved:
///   "w (keeps 2.00mm)".
fn describe_member(kind: Kind, plan: &keyset::WritePlan, u: u8) -> String {
    let prior = plan
        .before()
        .iter()
        .find(|ks| ks.usage == u)
        .expect("callers only ever name a usage plan was built over");
    let value = value_display(kind, prior);
    let name = key_label(u);
    if value_moves(kind, plan, prior, u) {
        format!("{name} at {value}")
    } else if let Some(change) = mode_change(plan, prior, u) {
        format!("{name} (keeps {value}, {change})")
    } else if plan.value_records().iter().any(|r| r.key == u) {
        format!("{name} (keeps {value})")
    } else {
        format!("{name} (keeps {value}, index only)")
    }
}

/// The touch mode transition a record `plan` sent for `u` represents, when the touch nibble it
/// carries actually differs from `prior`'s: "mode Global to Single". The only place in `wh` that
/// names a touch mode to the operator; `dump` prints `on`/`off` and a raw `mode_raw` instead. An
/// unknown nibble prints Rust tuple-variant syntax, `mode Unknown(7) to Rt`, matching `ops::rt_records`.
fn mode_change(plan: &keyset::WritePlan, prior: &ops::KeySettings, u: u8) -> Option<String> {
    let sent_mode = plan
        .value_records()
        .iter()
        .find(|r| r.key == u && r.layout == layout::MODE)
        .map(|r| r.value)?;
    let new_touch = wh_proto::cmds::Mode::from_value(sent_mode).touch;
    let prior_touch = prior.mode.touch;
    (new_touch != prior_touch).then(|| format!("mode {prior_touch:?} to {new_touch:?}"))
}

/// Whether the value `kind` reports (AP for `Kind::Ap`, press/release for `Kind::Rt`) actually
/// differs from `prior`, read off the record `plan` sent for it rather than assumed from whether
/// a record exists at all: `plan` always echoes a key's unchanged value back in the same bundle
/// as an unrelated change, such as a touch mode promotion.
fn value_moves(kind: Kind, plan: &keyset::WritePlan, prior: &ops::KeySettings, u: u8) -> bool {
    let sent = |layout_id: u8| {
        plan.value_records()
            .iter()
            .find(|r| r.key == u && r.layout == layout_id)
            .map(|r| r.value)
    };
    match kind {
        Kind::Ap => sent(layout::AP).is_some_and(|v| v != prior.ap.0),
        Kind::Rt => {
            sent(layout::RT_PRESS).is_some_and(|v| v != prior.rt_press.0)
                || sent(layout::RT_RELEASE).is_some_and(|v| v != prior.rt_release.0)
        }
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

/// Whether a raw MODE value has rapid trigger on, for annotating a wanted value held as a bare
/// `u16` rather than a parsed `KeySettings`. Matches `ops::KeySettings::rt_enabled`'s nibble set,
/// including nibble 2 (`RtGlobal`): task 2.12 fixed a real bug where `wh` reported rapid trigger
/// off on a board where it was on for every key, because an earlier version of this check missed
/// that nibble.
fn mode_rt_on(mode_raw: u16) -> bool {
    matches!(
        wh_proto::cmds::Mode::from_value(mode_raw).touch,
        wh_proto::cmds::TouchMode::RtGlobal
            | wh_proto::cmds::TouchMode::Rt
            | wh_proto::cmds::TouchMode::RtContinuous
    )
}

/// The one fault line a mode mismatch contributes, or `None` when the board agrees, annotated
/// with rapid trigger state on both sides. Split out from `verify_write_as` so a test can read it
/// back directly: `report_verification` writes fault lines to real process stderr, which a test
/// cannot capture. `run.rs`'s `verify_rt`/`verify_rt_off` build their own mode-fault wording
/// inline and so cannot be pinned this precisely; this function is what makes that possible here.
fn mode_fault(got: u16, want: u16) -> Option<String> {
    if got == want {
        return None;
    }
    Some(format!(
        "mode {got:#06x} (rt {}), wanted mode {want:#06x} (rt {})",
        if mode_rt_on(got) { "on" } else { "off" },
        if mode_rt_on(want) { "on" } else { "off" },
    ))
}

/// Re-reads every key `plan` touched and confirms it holds every value `plan` computed for it:
/// MODE, AP, RT_PRESS, RT_RELEASE, and both keyset memberships, every one of them checked against
/// what `plan` actually sent for that field or, where it sent nothing, against what was read from
/// the key before the write. Matches `verify_rt`'s own rule. Reads the board back rather than
/// trusting the write's echo, the same way every other write path in `wh` verifies.
///
/// Takes no separate key list: `plan.before()` covers every key `plan` was built over, in order.
/// Every field is checked unconditionally rather than picked by `kind`: an earlier version chose
/// AP or press/release from the caller's `kind`, and a caller passing the wrong one still passed,
/// since `plan` always sends all four value layouts for a changed key, the other kind's fields
/// echoed back unchanged. `kind` survives only to name the ap or rt half of `op`'s label; nothing
/// here is selected by it any more.
pub(crate) fn verify_write<T: Transport>(
    out: &mut impl Write,
    s: &mut Session<T>,
    kind: Kind,
    op: &str,
    plan: &keyset::WritePlan,
) -> Result<()> {
    verify_write_as(out, s, &format!("{} keyset {op}", kind_name(kind)), plan)
}

/// The body of `verify_write`, taking the exact label to report rather than assembling one from
/// `kind`/`op`: `wh set ap` needs a label that says what actually happened (a plain depth change,
/// or a keyset split), not the generic "kind keyset op" shape every keyset subcommand shares.
/// `verify_write` itself is the thin wrapper every keyset subcommand still uses.
pub(crate) fn verify_write_as<T: Transport>(
    out: &mut impl Write,
    s: &mut Session<T>,
    what: &str,
    plan: &keyset::WritePlan,
) -> Result<()> {
    let mut bad = Vec::new();
    let mut usages = Vec::new();
    for before in plan.before() {
        let u = before.usage;
        usages.push(u);
        let ks = wh_device::ops::read_key_settings(s, u)?;
        let mut faults = Vec::new();

        let sent = |records: &[KeyRecord], layout_id: u8| {
            records
                .iter()
                .find(|r| r.key == u && r.layout == layout_id)
                .map(|r| r.value)
        };
        let sent_value = |layout_id: u8| sent(plan.value_records(), layout_id);
        let sent_membership = |layout_id: u8| sent(plan.membership_records(), layout_id);

        let want_ap_keyset = sent_membership(layout::KEYSET_AP).unwrap_or(before.ap_keyset);
        if ks.ap_keyset != want_ap_keyset {
            faults.push(format!(
                "ap keyset {}, wanted {want_ap_keyset}",
                ks.ap_keyset
            ));
        }
        let want_rt_keyset = sent_membership(layout::KEYSET_RT).unwrap_or(before.rt_keyset);
        if ks.rt_keyset != want_rt_keyset {
            faults.push(format!(
                "rt keyset {}, wanted {want_rt_keyset}",
                ks.rt_keyset
            ));
        }

        let want_mode = sent_value(layout::MODE).unwrap_or_else(|| before.mode.value());
        if let Some(line) = mode_fault(ks.mode.value(), want_mode) {
            faults.push(line);
        }

        let want_ap = Um(sent_value(layout::AP).unwrap_or(before.ap.0));
        if ks.ap != want_ap {
            faults.push(format!(
                "ap {:.2}mm, wanted {:.2}mm",
                ks.ap.to_mm(),
                want_ap.to_mm()
            ));
        }

        let want_press = Um(sent_value(layout::RT_PRESS).unwrap_or(before.rt_press.0));
        let want_release = Um(sent_value(layout::RT_RELEASE).unwrap_or(before.rt_release.0));
        if ks.rt_press != want_press || ks.rt_release != want_release {
            faults.push(format!(
                "press {:.2}mm release {:.2}mm, wanted press {:.2}mm release {:.2}mm",
                ks.rt_press.to_mm(),
                ks.rt_release.to_mm(),
                want_press.to_mm(),
                want_release.to_mm()
            ));
        }

        if !faults.is_empty() {
            bad.push(format!(
                "{}: board reports {}",
                key_label(u),
                faults.join("; ")
            ));
        }
    }
    crate::run::report_verification(out, what, &usages, &bad)
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
        assert!(msg.contains("the board has 1, 3"), "got: {msg}");
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

    // -- verify_write: a membership-drift acceptance case --

    use wh_device::replay::{hex, ReplayTransport};

    fn l(dir: &str, b: &[u8; 64]) -> String {
        format!("{{\"dir\":\"{dir}\",\"hex\":\"{}\"}}", hex(b))
    }
    fn rf(cmd: u8, payload: &[u8]) -> [u8; 64] {
        wh_proto::frame::frame(cmd | wh_proto::frame::REPLY_BIT, payload).unwrap()
    }
    fn read_reply(usage: u8, lid: u8, val: u16) -> Vec<String> {
        vec![
            l("out", &wh_proto::cmds::read_key_layout(usage, lid)),
            l(
                "in",
                &rf(
                    wh_proto::cmds::cmd::KEY,
                    &[0x00, usage, lid, (val & 0xFF) as u8, (val >> 8) as u8],
                ),
            ),
        ]
    }
    #[allow(clippy::too_many_arguments)]
    fn settings_script(
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
            lines.extend(read_reply(usage, lid, val));
        }
        lines
    }

    /// The scenario `wh keyset set` builds in practice: a plan that writes no membership record
    /// at all, over a key whose `ap_keyset` drifts between the pre-write read and the readback.
    /// Nothing in the plan asked for that field to move, so the fallback to `before` is what
    /// catches it; skipping the membership check entirely whenever `plan` wrote none, the way an
    /// earlier version did, would have missed this.
    #[test]
    fn verify_write_catches_a_membership_drift_on_a_plan_with_no_membership_records() {
        let usage = 0x1Au8;
        // plan()'s own read: ap already at the target, so nothing at all is written for this key.
        let mut lines = settings_script(usage, 2000, 0x18, 100, 150, 0, 0);
        // verify_write's readback: ap_keyset drifted to 7, though the plan never touched it.
        lines.extend(settings_script(usage, 2000, 0x18, 100, 150, 7, 0));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());

        let change = keyset::Change::ap(Um(2000));
        let plan = keyset::plan(&mut s, &[usage], &change, None).unwrap();
        assert!(
            plan.membership_records().is_empty(),
            "this test's whole point is a plan that writes no membership"
        );

        let mut out = Vec::new();
        let err = verify_write(&mut out, &mut s, Kind::Ap, "create", &plan).unwrap_err();
        assert!(
            err.to_string().contains("readback mismatch on 1 key(s)"),
            "got: {err}"
        );
    }

    // -- ap_membership_for --

    /// The `ops::read_matrix` script for up to three usages, one per row-pair column, matching
    /// `ops::read_matrix`'s own reporting order.
    fn matrix_lines(usages: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        for (i, &row) in [0u8, 2, 4].iter().enumerate() {
            let req = wh_proto::cmds::read_defkey_rows(row, row + 1);
            let mut payload = vec![0u8; 45];
            payload[1] = row;
            if let Some(&u) = usages.get(i) {
                payload[2] = u;
            }
            payload[23] = row + 1;
            lines.push(l("out", &req));
            lines.push(l("in", &rf(wh_proto::cmds::cmd::DEFKEY, &payload)));
        }
        lines
    }

    /// `taken.len() == usages.len()` is not redundant with `whole`, even though `taken` is always
    /// a subset of `usages`: a selection can fully consume one keyset (`whole` true) while also
    /// naming a free key that keyset never held, and that extra key must still force a split.
    /// Weakening the guard to `<=` (always true, since `taken` can never exceed `usages`) makes
    /// this case wrongly report `Keep`, and nothing else in this crate's test suite catches it:
    /// the whole workspace passes under that mutation without this test.
    #[test]
    fn ap_membership_for_splits_when_a_whole_keyset_rides_along_with_a_free_key() {
        // w (0x1A) and a (0x04) fully make up ap keyset 1; x (0x10) is free.
        let mut lines = matrix_lines(&[0x1A, 0x04, 0x10]);
        lines.extend(read_reply(0x1A, layout::KEYSET_AP, 1));
        lines.extend(read_reply(0x04, layout::KEYSET_AP, 1));
        lines.extend(read_reply(0x10, layout::KEYSET_AP, 0));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let m = keyset::read_membership(&mut s, Kind::Ap).unwrap();

        let got = ap_membership_for(&m, &[0x1A, 0x04, 0x10]).unwrap();
        match got {
            ApMembership::Split { index, losing } => {
                assert_eq!(index.value(), 2, "got: {index:?}");
                assert_eq!(losing, vec![(1u16, vec![0x1A, 0x04])], "got: {losing:?}");
            }
            ApMembership::Keep => {
                panic!("a free key riding along with a fully-consumed keyset must still split")
            }
        }
    }

    /// The ruling: a selection where every key is free must still allocate a keyset, where
    /// it previously returned `Keep` and wrote no membership at all. Pins the allocated index and
    /// the empty losing list, not merely that a `Split` came back: a rewrite that allocated the
    /// wrong index, or invented a losing keyset, would pass a bare variant check.
    #[test]
    fn ap_membership_for_creates_a_keyset_when_every_selected_key_is_free() {
        // w (0x1A) and a (0x04) are both free; the board has no keysets at all.
        let mut lines = matrix_lines(&[0x1A, 0x04]);
        lines.extend(read_reply(0x1A, layout::KEYSET_AP, 0));
        lines.extend(read_reply(0x04, layout::KEYSET_AP, 0));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let m = keyset::read_membership(&mut s, Kind::Ap).unwrap();

        let got = ap_membership_for(&m, &[0x1A, 0x04]).unwrap();
        match got {
            ApMembership::Split { index, losing } => {
                assert_eq!(
                    index.value(),
                    1,
                    "first keyset on an empty board: {index:?}"
                );
                assert!(losing.is_empty(), "no keyset loses anything: {losing:?}");
            }
            ApMembership::Keep => panic!("an all-free selection must now create a keyset"),
        }
    }

    /// The mirror case, unchanged by the ruling above: a selection that is exactly one keyset's
    /// members keeps that keyset's index rather than allocating a new one. Without this, deleting
    /// the `losing.is_empty()` early return could be over-generalised into deleting the whole
    /// `Keep` arm, and every value change would churn a fresh keyset index.
    #[test]
    fn ap_membership_for_keeps_the_index_when_the_selection_is_exactly_one_keyset() {
        // w (0x1A) and a (0x04) are keyset 1, and nothing else is selected.
        let mut lines = matrix_lines(&[0x1A, 0x04]);
        lines.extend(read_reply(0x1A, layout::KEYSET_AP, 1));
        lines.extend(read_reply(0x04, layout::KEYSET_AP, 1));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let m = keyset::read_membership(&mut s, Kind::Ap).unwrap();

        assert_eq!(
            ap_membership_for(&m, &[0x1A, 0x04]).unwrap(),
            ApMembership::Keep,
            "a selection that is exactly one keyset must keep its index"
        );
    }

    /// `ap_membership_for` must refuse a rapid trigger `Membership`, the same guard every other
    /// kind-sensitive function in this module applies, rather than silently returning `Keep`: a
    /// caller that passed the wrong kind would otherwise overwrite a keyset's shared value across
    /// members the operator never selected, with no error to say why.
    #[test]
    fn ap_membership_for_rejects_a_rapid_trigger_membership() {
        // Built through a scripted read the same way `read_membership` builds one: `Membership`
        // has no public constructor, so there is no way to hand this function the wrong kind
        // other than through a real read.
        let mut lines = matrix_lines(&[0x1A]);
        lines.extend(read_reply(0x1A, layout::KEYSET_RT, 0));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let rt_m = keyset::read_membership(&mut s, Kind::Rt).unwrap();

        let err = ap_membership_for(&rt_m, &[0x1A]).unwrap_err();
        assert!(
            err.to_string()
                .contains("requires an actuation point membership"),
            "got: {err}"
        );
    }

    // -- mode_fault: the rt-state annotation --

    /// Silent when the board agrees, the same shape the old `wh set ap` verifier's own mode-fault
    /// case had, before that whole path was replaced by `keyset::plan` and this one.
    #[test]
    fn mode_fault_is_silent_when_the_board_agrees() {
        assert_eq!(mode_fault(0x38, 0x38), None);
    }

    /// Names rapid trigger state on both sides, not just the bare hex values: a bare `mode
    /// 0x0018, wanted mode 0x0038` does not tell the operator that the fault is rapid trigger
    /// silently turning off, the most safety-relevant thing this tool can report. The old `wh set
    /// ap` verifier, replaced by `keyset::plan` and this module, carried the same annotation.
    #[test]
    fn mode_fault_names_rapid_trigger_state_on_both_sides() {
        let line = mode_fault(0x18, 0x38).expect("0x18 != 0x38 must fault");
        assert_eq!(
            line, "mode 0x0018 (rt off), wanted mode 0x0038 (rt on)",
            "got: {line}"
        );
    }

    /// The pre-write nibble is reported through `mode_rt_on`, which must include nibble 2
    /// (`RtGlobal`) as rapid trigger on: task 2.12 exists because an earlier rapid-trigger check
    /// elsewhere in this codebase missed exactly that nibble, reporting rapid trigger off on a
    /// board where it was on for every key.
    #[test]
    fn mode_fault_names_the_global_rapid_trigger_nibble_as_rt_on() {
        let line = mode_fault(0x10, 0x20).expect("0x10 != 0x20 must fault");
        assert_eq!(
            line, "mode 0x0010 (rt off), wanted mode 0x0020 (rt on)",
            "got: {line}"
        );
    }

    // -- confirm_ap_target --

    /// The one place `plan`'s resolved actuation point target is checked against a value `plan`
    /// itself never computed: the operator's own request. A `Change::ap` and `plan` that agreed
    /// with each other but disagreed with `depth` would otherwise pass `verify_write` cleanly,
    /// since that only ever compares the board against what `plan` sent.
    #[test]
    fn confirm_ap_target_catches_a_plan_that_resolved_to_the_wrong_depth() {
        let lines = settings_script(0x1A, 2000, 0x18, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let change = keyset::Change::ap(Um(2500));
        let plan = keyset::plan(&mut s, &[0x1A], &change, None).unwrap();

        // Correct: the plan really did resolve to 2500um (2.50mm), matching what was requested.
        confirm_ap_target(&plan, Um(2500)).unwrap();
        // Wrong: an independently-known value the plan never produced.
        let err = confirm_ap_target(&plan, Um(1200)).unwrap_err();
        assert!(
            err.to_string()
                .contains("plan resolved w to 2.50mm, not the 1.20mm requested"),
            "got: {err}"
        );
    }
}
