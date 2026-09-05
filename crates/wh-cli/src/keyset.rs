//! The `wh keyset` command tree. Every handler here reads the board's live membership first;
//! `wh` caches no device state, and allocation is max plus one over live membership, so a stale
//! view could hand out an index a key already holds.

use anyhow::{bail, Result};
use std::io::{BufRead, Write};
use wh_device::keyset::{self, Global, Keyset, Kind, Membership};
use wh_device::ops;
use wh_device::session::Session;
use wh_device::transport::Transport;
use wh_proto::cmds::{layout, KeyRecord, TouchMode};
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

/// The distinct values and their counts, rendered for a refusal: "3 key(s) at 0.10mm, 1 key(s) at
/// 0.50mm", in the order given, which every caller takes from `Global::Split` and so is already
/// descending by count. Shared by all three disagreement messages, which differ in what they
/// advise and not in how they list what disagreed.
fn value_counts_list(shown: &[(String, usize)]) -> String {
    shown
        .iter()
        .map(|(v, n)| format!("{n} key(s) at {v}"))
        .collect::<Vec<String>>()
        .join(", ")
}

fn split_message_str(what: &str, shown: &[(String, usize)], flag: &str) -> String {
    format!(
        "the keys outside every keyset disagree on the global {what} ({}), so there is no one \
         global value to use; pass {flag} to say which",
        value_counts_list(shown)
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

/// The value an operation resolved to write, whatever its source: `create`/`delete` from
/// `--value`/`--press`/`--release` or the board's global, `remove` from the base read excluding
/// its own selection, or, `Kind::Ap` only, `NO_SIGNAL_BASE` when nothing is left to read;
/// `remove_base_rt` refuses in that case instead, so a `Target::Rt` never comes from the constant.
/// Carried alongside the plan so `announce_steal`, `announce_delete` and `announce_remove` can show
/// what each affected member is about to move to or lose it for.
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

/// Where the value an announcement names came from. `announce_remove` says "returning w to X" and
/// "w already at X", both of which read as a claim that X is a destination the board defines. That
/// is true of one of these three sources and of neither other, so the source travels with the
/// value rather than being inferred at the line that prints it.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum TargetSource {
    /// Read from the keys outside every keyset, which is the board's own base.
    BoardBase,
    /// `NO_SIGNAL_BASE`: a chosen default reached when no key was left to read a base from, not a
    /// reading. `Kind::Ap` only; the rapid trigger side refuses instead.
    InventedBase,
    /// Named by the operator, `wh set rt --off --press/--release`. The board never held it and
    /// nothing was read to reach it.
    Operator,
}

impl TargetSource {
    /// The parenthetical each line appends straight after the value, empty for a real reading
    /// since that is the case the sentences are already written for.
    fn suffix(self) -> &'static str {
        match self {
            TargetSource::BoardBase => "",
            TargetSource::InventedBase => {
                " (no key outside a keyset to read a base from, using the default)"
            }
            TargetSource::Operator => " (from --press/--release, not the board's base)",
        }
    }
}

/// The `Change` a reset writes, over a value the caller has already resolved. `delete`, `remove`
/// and `rt_off` send the same per-key template, measured in `ks-delete-rt`, `ks-remove-one-rt` and
/// `rt-off-w`, so a correction to that template has to land on all three at once and this is what
/// makes it land. `create` is not a caller: its rapid trigger arm builds `Change::rt_on`, the
/// opposite template.
///
/// Resolves nothing and reads no device, deliberately. The three callers agree on the template and
/// disagree on how they reach the value: `delete` and `rt_off` refuse on `NoneOutsideAKeyset` and
/// name `--value`/`--press`/`--release` as the way out, while `remove`, which has no such flags,
/// falls back to `NO_SIGNAL_BASE` for the actuation point and refuses for rapid trigger. A helper
/// that resolved the value as well would hand one command the other's behaviour.
///
/// The two arms do different things to touch nibble 0, and only the actuation point arm moves it.
/// `Target::Ap` uses `Change::ap`, not `Change::ap_keeping_touch`, so a key at nibble 0 ("follow
/// global travel") is promoted to nibble 1, a per-key pinned actuation point, matching the
/// vendor's own measured behaviour on an actuation point change (`ks-value-ap`);
/// `ap_keeping_touch` exists for an operation that must never move a key off global travel, and
/// resetting a key to the base is not that operation. `Target::Rt` uses `Change::rt_off`, whose
/// `TouchChange::Off` sends nibbles 2, 3 and 4 to 1 and leaves a key at nibble 0 exactly where it
/// is: a key following global travel with rapid trigger already off has no rapid trigger to turn
/// off, and `plan` emits no MODE record for it.
fn reset_change(target: Target) -> keyset::Change {
    match target {
        Target::Ap(v) => keyset::Change::ap(v),
        Target::Rt(p, r) => keyset::Change::rt_off(p, r),
    }
}

/// Creates a keyset over `usages` at the global value, or at an explicit one. Announces which
/// existing keysets lose members first: a create overwrites its members' values with the global
/// rather than carrying them in, so the operator sees what is about to go.
///
/// `will_write` must agree with whether the caller goes on to send the returned plan to the
/// device: pass `false` only when the caller's own next step is printing the plan and stopping,
/// since the whole-matrix confirmation below is skipped whenever it is `false`. `prompt_out` is
/// separate from `out`, the announcement writer, for the reason `remove`'s own signature splits
/// them: the prompt is a diagnostic and belongs on stderr, while the announcement is data
/// someone may pipe.
#[allow(clippy::too_many_arguments)]
pub(crate) fn create<T: Transport>(
    out: &mut impl Write,
    prompt_out: &mut impl Write,
    s: &mut Session<T>,
    kind: Kind,
    usages: &[u8],
    value: Option<Um>,
    rt: Option<(Um, Um)>,
    will_write: bool,
    input: &mut impl BufRead,
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
    if will_write {
        confirm_whole_board_create(
            prompt_out,
            kind,
            &m,
            usages,
            index.value(),
            target,
            &plan,
            input,
        )?;
    }
    announce_steal(out, kind, &losing, index.value(), target, &plan)?;
    Ok(plan)
}

/// The typed confirmation guarding a create whose resolved selection covers every key in the
/// board's matrix: every key moves into the one freshly allocated index, so every existing keyset
/// of this kind loses all of its members and ceases to exist. The same destruction
/// `confirm_whole_board_remove` and `confirm_whole_board_ap_set` guard, reached by a third route,
/// so this reuses `crate::confirm::confirm` rather than a third acceptance check.
///
/// Called after `plan` is built and before `announce_steal`, matching `remove`: the mode clause
/// is read off the plan, which is the only thing that knows how many touch nibbles actually move,
/// and a refusal announces nothing at all. `--dry-run` never reaches here, since it writes
/// nothing to confirm.
///
/// Computes its own trigger from `m` and `usages`, matching `confirm_whole_board_ap_set`, rather
/// than trusting a caller to have checked first: both "this selects every key on the board" and
/// the list of keysets ceasing to exist are false of a partial selection, and a caller that
/// forgot must never reach this wording. It is the resolved selection covering the matrix that
/// triggers it, never the literal `--keys all`: spelling every key out destroys just as much.
/// `losing` is derived here from the same `m` and `usages` for the same reason, so the keysets
/// named can never have come from a different selection than the one being confirmed.
///
/// `out` is a caller-supplied stderr rather than stdout, for the reason
/// `confirm_whole_board_remove`'s own doc sets out at length: a redirected stdout would trap the
/// prompt in the file with nothing on screen, and no terminal check is needed once the prompt
/// goes to stderr, since stdin answers it either way.
#[allow(clippy::too_many_arguments)]
fn confirm_whole_board_create(
    out: &mut impl Write,
    kind: Kind,
    m: &Membership,
    usages: &[u8],
    new_index: u16,
    target: Target,
    plan: &keyset::WritePlan,
    input: &mut impl BufRead,
) -> Result<()> {
    if usages.len() != m.entries().len() {
        return Ok(());
    }
    let losing = losing_members(&keyset::group(m), usages);
    let keysets = if losing.is_empty() {
        format!("no {} keysets exist to lose", kind_name(kind))
    } else {
        let indices: Vec<String> = losing.iter().map(|(i, _)| i.to_string()).collect();
        format!(
            "{} keyset(s) {} will cease to exist, their members absorbed",
            kind_name(kind),
            indices.join(", ")
        )
    };
    // A count, not a per-key list, the same reason the two sibling prompts read one off `plan`:
    // the board this guards is 68 keys wide, and on a board with no keysets yet the keyset
    // clause reads as a no-op while every key's touch mode still moves permanently.
    let mode_clause = match kind {
        Kind::Ap => ap_mode_clause(plan),
        Kind::Rt => rt_on_mode_clause(plan),
    };
    let prompt = format!(
        "{}: this selects every key on the board: every key moves into the new keyset \
         {new_index} at {}\n    {keysets}{mode_clause}",
        kind_name(kind),
        target.display()
    );
    if !crate::confirm::confirm(out, &prompt, input)? {
        bail!(
            "{} keyset creation over the whole board was not confirmed",
            kind_name(kind)
        );
    }
    Ok(())
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
    // Resolved here, not in `reset_change`: `delete` refuses on both `Split` and
    // `NoneOutsideAKeyset` and offers a flag as the way out, which `remove` has no flag to
    // satisfy. Only the template built from the resolved value is shared.
    let target = match kind {
        Kind::Ap => {
            let v = match value {
                Some(v) => v,
                None => global_ap_or_bail(s, &m, "--value")?,
            };
            Target::Ap(v)
        }
        Kind::Rt => {
            let (p, r) = match rt {
                Some(v) => v,
                None => global_rt_or_bail(s, &m, "--press and --release")?,
            };
            Target::Rt(p, r)
        }
    };
    let change = reset_change(target);
    let cleared = keyset::KeysetIndex::clear(kind);
    let plan = keyset::plan(s, &ks.members, &change, Some(cleared))?;
    announce_delete(out, kind, index, target, &plan)?;
    Ok(plan)
}

/// The base actuation point when no free key remains to read one from, once every free key is
/// excluded from the read because it is itself being reset. A chosen default for that one
/// unanswerable case, not a measured factory setting: nothing has ever read an untouched profile.
/// Actuation point only: `2000` is the measured dominant `0x04` reading, and no equivalent exists
/// for rapid trigger, so `remove_base_rt` refuses in the same case rather than reusing it.
const NO_SIGNAL_BASE: Um = Um(2000);

/// Resolves `remove`'s target: the base actuation point read from the free keys `usages` leaves
/// behind, or `NO_SIGNAL_BASE` when none are left to read. A disagreement among those remaining
/// keys refuses rather than picking one, the same rule `global_ap_or_bail` applies, since
/// overriding it would write a value nobody chose. The second element is true exactly when the
/// value is `NO_SIGNAL_BASE`, so a caller can say the value was invented rather than read, the same
/// distinction the rt side makes by refusing outright instead of ever reaching this silently.
fn remove_base_ap<T: Transport>(
    s: &mut Session<T>,
    m: &Membership,
    usages: &[u8],
) -> Result<(Um, bool)> {
    match keyset::global_ap_excluding(s, m, usages)? {
        Global::Agreed(v) => Ok((v, false)),
        Global::Split(counts) => bail!("{}", remove_split_message("actuation point", &counts)),
        Global::NoneOutsideAKeyset => Ok((NO_SIGNAL_BASE, true)),
    }
}

/// The rapid trigger mirror of `remove_base_ap`, over the press/release pair, with one deliberate
/// difference: `NoneOutsideAKeyset` refuses rather than falling back to a constant. The corpus
/// shows the reset target always tracking the global sensitivity at write time, `100` in
/// `ks-delete-rt`, `200` in `ks-reset-keysets`, never a fixed number, and no `0x14`/`0x15` reading
/// has ever been `2000`. Inventing one here is exactly what this project measures against.
fn remove_base_rt<T: Transport>(
    s: &mut Session<T>,
    m: &Membership,
    usages: &[u8],
) -> Result<(Um, Um)> {
    match keyset::global_rt_excluding(s, m, usages)? {
        Global::Agreed(v) => Ok(v),
        Global::Split(counts) => {
            let shown: Vec<(String, usize)> = counts
                .iter()
                .map(|((p, r), n)| (format!("{:.2}/{:.2}mm", p.to_mm(), r.to_mm()), *n))
                .collect();
            bail!(
                "{}",
                remove_split_message_str("rapid trigger sensitivity", &shown)
            )
        }
        Global::NoneOutsideAKeyset => {
            // Same `Global` variant, two different board states: no key is free at all, or every
            // free key is also in this selection. `m.entries()` already has what is needed to
            // tell them apart, no extra read; conflating them would send an operator looking for
            // keysets that do not exist, on a board where the free keys causing this plainly do.
            if m.entries().iter().any(|&(_, membership)| membership == 0) {
                bail!(
                    "every key outside a rapid trigger keyset is also in this selection, so there \
                     is no global sensitivity left to reset these to, and no default is measured \
                     for one"
                )
            } else {
                bail!(
                    "no key is outside a rapid trigger keyset, so there is no global sensitivity \
                     to reset these to, and no default is measured for one"
                )
            }
        }
    }
}

fn remove_split_message(what: &str, counts: &[(Um, usize)]) -> String {
    let shown: Vec<(String, usize)> = counts
        .iter()
        .map(|(v, n)| (format!("{:.2}mm", v.to_mm()), *n))
        .collect();
    remove_split_message_str(what, &shown)
}

/// A contradictory reading from the board is not the same as no reading: overriding it would
/// invent a value nobody chose. Shared by both of `remove`'s kinds: `remove_base_ap` refuses this
/// and falls back to `NO_SIGNAL_BASE` only in the separate no-signal case; `remove_base_rt` refuses
/// both here and there, since it has no fallback to reach for. `rt_off_base` builds its own
/// wording rather than reusing this: it has `--press`/`--release` to offer and `remove` does not,
/// so telling its operator only to widen the selection would hide the shorter way out.
fn remove_split_message_str(what: &str, shown: &[(String, usize)]) -> String {
    format!(
        "the keys left outside every keyset disagree on the global {what} ({}); include them \
         in the selection so they are reset too",
        value_counts_list(shown)
    )
}

/// Resets named keys to the board's base value and to no keyset at all: the destination every
/// selected key reaches, whether it was a keyset member or already free. `usages` goes to `plan`
/// whole, so a key already at the base with nothing else to change gets no value record, `plan`'s
/// own skip rule; membership is still written for every selected key, the same unconditional
/// rewrite `plan` already applies for `create` and `delete`. `will_write` must agree with whether
/// the caller goes on to send `plan` to the device: pass `false` only when the caller's own next
/// step is printing the plan and stopping, never applying it, since the whole-matrix confirmation
/// below is skipped whenever it is `false`. `prompt_out` is separate from `out`, the announcement
/// writer: the whole-board prompt is a diagnostic and belongs on stderr, while the per-key
/// announcement is data someone may pipe, so a caller passes a locked stdout for `out` and a
/// locked stderr for `prompt_out`.
pub(crate) fn remove<T: Transport>(
    out: &mut impl Write,
    prompt_out: &mut impl Write,
    s: &mut Session<T>,
    kind: Kind,
    usages: &[u8],
    will_write: bool,
    input: &mut impl BufRead,
) -> Result<keyset::WritePlan> {
    let m = keyset::read_membership(s, kind)?;
    let sets = keyset::group(&m);
    let leaving = leaving_members(&sets, usages);

    // Resolved here, not in `reset_change`: `remove` has no `--value`/`--press`/`--release` to
    // fall back on, so `remove_base_ap` invents `NO_SIGNAL_BASE` where `remove_base_rt` refuses,
    // and neither behaves like `delete`. Only the template built from the resolved value is
    // shared.
    let (target, base_invented) = match kind {
        Kind::Ap => {
            let (v, invented) = remove_base_ap(s, &m, usages)?;
            (Target::Ap(v), invented)
        }
        Kind::Rt => {
            let (p, r) = remove_base_rt(s, &m, usages)?;
            (Target::Rt(p, r), false)
        }
    };
    let change = reset_change(target);

    // `plan` is built before the whole-matrix confirmation, not after: the prompt below describes
    // what `plan` actually contains, so the operator answers with that in front of them rather than
    // deciding first and reading what happened only afterward. `plan` only reads the device; no
    // write happens until the caller applies the returned plan, so reordering this costs nothing
    // but an earlier read.
    let cleared = keyset::KeysetIndex::clear(kind);
    let plan = keyset::plan(s, usages, &change, Some(cleared))?;

    if will_write && usages.len() == m.entries().len() {
        confirm_whole_board_remove(
            prompt_out,
            kind,
            &sets,
            target,
            &plan,
            input,
            &format!("{} keyset removal", kind_name(kind)),
        )?;
    }

    let source = if base_invented {
        TargetSource::InventedBase
    } else {
        TargetSource::BoardBase
    };
    announce_remove(out, kind, &leaving, usages, target, source, &sets, &plan)?;
    Ok(plan)
}

/// `wh set rt --off`'s plan: rapid trigger off on every selected key, its sensitivities reset, and
/// its `0xFE` membership cleared. Measured in `captures/rt-off-w.jsonl`: the vendor's per-key
/// rapid trigger off resets `W` from its own 500/500 to the board's global 100/100 and then writes
/// `0xFE = 0`, one record per frame, as the last thing it sends. That file's read sweep does not
/// cover `0xFE`, so whether `W` held a membership beforehand is unmeasured; what is measured is
/// that the clear goes out unconditionally, which is `plan`'s own rule for a `Some(index)`.
///
/// `rt` is the operator's `--press`/`--release`. `None` reads the board's base through
/// `rt_off_base`, which excludes the selection for the reason `remove_base_rt` does: the keys being
/// reset are usually the ones holding their own sensitivity, so including them would make the
/// commonest run of this command, turning rapid trigger off on the one key that has it, refuse as
/// a disagreement with itself.
///
/// Reuses `announce_remove`, and the wording is not a coincidence: this command reaches the same
/// destination `wh keyset remove rt` does, resetting each key to the base and to no keyset, so a
/// key leaving a keyset, a keyset emptied by that, and a key that only has its membership rewritten
/// all have to be said the same way in both. It reaches the same destruction too, which is why it
/// calls the same whole-board confirmation: this command can empty every rapid trigger keyset on
/// the board, which before it wrote membership at all it could not. `will_write` must agree with
/// whether the caller goes on to send the plan, since that guard is skipped whenever it is false;
/// `prompt_out` is the caller's stderr, `out` its stdout, the same split `remove` documents.
///
/// Reads only; the caller applies the plan.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rt_off<T: Transport>(
    out: &mut impl Write,
    prompt_out: &mut impl Write,
    s: &mut Session<T>,
    usages: &[u8],
    rt: Option<(Um, Um)>,
    will_write: bool,
    input: &mut impl BufRead,
) -> Result<keyset::WritePlan> {
    let m = keyset::read_membership(s, Kind::Rt)?;
    let sets = keyset::group(&m);
    let leaving = leaving_members(&sets, usages);
    let (target, source) = match rt {
        Some((p, r)) => (Target::Rt(p, r), TargetSource::Operator),
        None => {
            let (p, r) = rt_off_base(s, &m, usages)?;
            (Target::Rt(p, r), TargetSource::BoardBase)
        }
    };
    let change = reset_change(target);
    let cleared = keyset::KeysetIndex::clear(Kind::Rt);
    // Built before the confirmation, matching `remove`: the prompt's mode count is read off the
    // plan, so the operator answers with what is actually about to be sent in front of them.
    let plan = keyset::plan(s, usages, &change, Some(cleared))?;
    if will_write && usages.len() == m.entries().len() {
        confirm_whole_board_remove(
            prompt_out,
            Kind::Rt,
            &sets,
            target,
            &plan,
            input,
            "rapid trigger off",
        )?;
    }
    announce_remove(
        out,
        Kind::Rt,
        &leaving,
        usages,
        target,
        source,
        &sets,
        &plan,
    )?;
    Ok(plan)
}

/// The sensitivity `wh set rt --off` resets to when the operator named none: read from `0x14`/`0x15`
/// of every key that holds no rapid trigger membership and is not itself being reset. Excluding the
/// selection is the whole point, and it is the lesson `remove_base_rt` already learned: a key with
/// its own sensitivity is the ordinary reason to run this command, so counting it as a disagreement
/// would refuse the commonest run there is.
///
/// Both failures refuse rather than picking a winner, and both name `--press`/`--release`, which
/// `remove` cannot do because it has no such flags. `NoneOutsideAKeyset` carries two board states
/// here for the same reason it does in `remove_base_rt`, and they are told apart from `m.entries()`
/// rather than conflated: no free key exists at all, or free keys exist and every one of them is in
/// this selection, which is what a whole-board `--off` always looks like.
fn rt_off_base<T: Transport>(
    s: &mut Session<T>,
    m: &Membership,
    usages: &[u8],
) -> Result<(Um, Um)> {
    match keyset::global_rt_excluding(s, m, usages)? {
        Global::Agreed(v) => Ok(v),
        Global::Split(counts) => {
            let shown: Vec<(String, usize)> = counts
                .iter()
                .map(|((p, r), n)| (format!("{:.2}/{:.2}mm", p.to_mm(), r.to_mm()), *n))
                .collect();
            bail!(
                "the keys left outside this selection and outside every rapid trigger keyset \
                 disagree on the global sensitivity ({}), so there is no one value to reset to; \
                 pass --press and --release to say which, or include those keys in the selection \
                 so they are reset too",
                value_counts_list(&shown)
            )
        }
        Global::NoneOutsideAKeyset => {
            if m.entries().iter().any(|&(_, membership)| membership == 0) {
                bail!(
                    "every key outside a rapid trigger keyset is also in this selection, so there \
                     is no global sensitivity left to reset these to; pass --press and --release \
                     to say which value to use"
                )
            } else {
                bail!(
                    "no key is outside a rapid trigger keyset, so there is no global sensitivity \
                     to reset these to; pass --press and --release to say which value to use"
                )
            }
        }
    }
}

/// The typed confirmation guarding a remove that covers every key in the board's matrix: every
/// keyset of this kind ceases to exist once nothing is left to be a member of anything, and every
/// key moves to `target` regardless of what it held before, including a whole-board selection with
/// no live keysets at all, where `target` is the only thing about to change and so the only thing
/// worth naming, unless a touch mode also moves and says so too. `--dry-run` never reaches here,
/// since it writes nothing to confirm.
///
/// Called after `plan` is built, not before: the value and keyset clauses can both read as a
/// no-op, every key already at the target and no keyset to lose, on a board where every key's
/// touch mode still moves (measured: four free keys already at the base, no keysets, `remove`
/// promoting all of them off "follow global travel"). Answering the prompt before that fact exists
/// to describe would let an operator say `yes` to a sentence that is true and still miss the one
/// thing that changes.
///
/// No `picker::refuse_if_not_terminal`-style guard here, deliberately: `out` is a caller-supplied
/// stderr, not stdout, precisely so `wh keyset remove ap --keys all > log.txt` no longer traps the
/// prompt in the redirected file with nothing on screen, the hazard measured before this writer
/// was split from the per-key announcement's. `--pick` refuses a non-terminal stdout because its
/// live TUI cannot render to one at all; piping to it is not an escape hatch, it is meaningless.
/// This prompt is one line out and one line in, and `docs/tasks.md` rules that shape as the
/// sanctioned way to answer it, "no bypass flag, so tests pipe `yes` on stdin", for real scripted
/// use as well as for tests.
///
/// To be precise about which guard this argument rules out: a bare `refuse_if_not_terminal`-style
/// check on this writer, mirroring `--pick`'s, would refuse any run whose streams are piped, tests
/// included, since the harness pipes all three streams. That form really would break the
/// sanctioned path, not just guard against the hang. Sending the prompt to stderr instead needs no
/// terminal check at all: stdin, still open, answers it the same way regardless of what carries
/// the prompt itself.
#[allow(clippy::too_many_arguments)]
fn confirm_whole_board_remove(
    out: &mut impl Write,
    kind: Kind,
    sets: &[Keyset],
    target: Target,
    plan: &keyset::WritePlan,
    input: &mut impl BufRead,
    refusal: &str,
) -> Result<()> {
    let indices: Vec<String> = sets.iter().map(|k| k.index.to_string()).collect();
    let keysets = if indices.is_empty() {
        format!("no {} keysets exist to lose", kind_name(kind))
    } else {
        format!(
            "{} keyset(s) {} will cease to exist",
            kind_name(kind),
            indices.join(", ")
        )
    };
    // A count, not a per-key list: the board this guards is 68 keys wide, and the value and
    // keyset clauses above can both read as a no-op (every key already at the target, no keysets
    // to lose) while every key's touch mode still moves. Read from `plan` itself, built just
    // above, not inferred from `target` or `sets`: the mode transition is a property of what the
    // plan actually sends, the same reason `announce_remove` reads it from `plan` per key.
    let mode_clause = match kind {
        Kind::Ap => ap_mode_clause(plan),
        // Reached by `rt_off` and not by `remove`: a whole-board selection excludes every free
        // key from the base read, which then always hits `NoneOutsideAKeyset`, and `remove` has
        // no flag to answer that with while `wh set rt --keys all --off --press/--release` does.
        // So this branch guards the one route that gets this far, not a hypothetical one.
        Kind::Rt => {
            let moved_modes = moved_mode_count(plan);
            if moved_modes == 0 {
                String::new()
            } else {
                format!(", {moved_modes} key(s) have rapid trigger switched off")
            }
        }
    };
    let prompt = format!(
        "this selects every key on the board: every key moves to {}, and {keysets}{mode_clause}",
        target.display()
    );
    if !crate::confirm::confirm(out, &prompt, input)? {
        bail!("{refusal} over the whole board was not confirmed");
    }
    Ok(())
}

/// Announces a remove's effect on every selected key. A member leaving a keyset is always
/// "removing", regardless of what else moves. A free key with no value record at all, decided from
/// `plan.value_records()` rather than from comparing `prior` to `target`, is "membership rewritten,
/// value unchanged": `plan` can write a record that touches only MODE, not the owned value, so a
/// comparison against the owned value alone would miss that a frame was still sent. A free key
/// whose owned value actually moves is "returning". A free key whose owned value stays
/// but whose touch mode moves, most often rapid trigger switching off underneath an unchanged
/// sensitivity, or a key promoted off "follow global travel" at an unchanged actuation point, names
/// the mode transition instead of describing it as a value move: the same `mode_change` vocabulary
/// `describe_member` already renders as "mode Rt to Single".
///
/// The mode transition is not only that fourth case's whole line: `removing` and `returning` each
/// append it too, whenever `mode_change` reports one, since a key can leave a keyset or reach a new
/// value *and* have its touch mode move in the same write. A key with its own non-base rapid
/// trigger settings is the ordinary reason to run `wh keyset remove rt` at all, not the exception,
/// so naming the mode transition only in the case where the value happens to already sit at the
/// base would leave it silent in the cases the command is normally used for.
///
/// Two more things named wherever `target` is shown: `source` says where the value came from, so
/// a chosen default (`remove_base_ap`'s `NO_SIGNAL_BASE`) or a number the operator typed
/// (`wh set rt --off --press/--release`) never renders indistinguishably from a real reading of the
/// board's base, which is what "returning to" and "already at" otherwise imply; and a `removing`
/// line whose keyset has no member
/// left outside `leaving` says that keyset ceases to exist, the same fact the whole-board prompt
/// already names for every keyset at once, now named for a partial removal that empties just one.
/// Each key's current value comes from `plan.before()`, the same source `announce_delete` uses.
#[allow(clippy::too_many_arguments)]
fn announce_remove(
    out: &mut impl Write,
    kind: Kind,
    leaving: &[(u16, u8)],
    usages: &[u8],
    target: Target,
    source: TargetSource,
    sets: &[Keyset],
    plan: &keyset::WritePlan,
) -> std::io::Result<()> {
    // Said wherever `target` is shown, rather than letting a default nobody read, or a number the
    // operator typed, render exactly like a reading of the board's own base.
    let source_suffix = source.suffix();
    for &u in usages {
        let prior = plan
            .before()
            .iter()
            .find(|ks| ks.usage == u)
            .expect("every selected key was read into plan.before()");
        let prior_value = value_display(kind, prior);
        let mode_change_here = mode_change(plan, prior, u);
        let mode_suffix = mode_change_here
            .as_ref()
            .map(|change| format!(", {change}"))
            .unwrap_or_default();
        if let Some(&(index, _)) = leaving.iter().find(|&&(_, lu)| lu == u) {
            let disappear_suffix = if keyset_disappears(sets, leaving, index) {
                format!(", keyset {index} ceases to exist")
            } else {
                String::new()
            };
            writeln!(
                out,
                "{}: removing {} from keyset {index}, {prior_value} to {}{source_suffix}{mode_suffix}{disappear_suffix}",
                kind_name(kind),
                key_label(u),
                target.display()
            )?;
        } else if !plan.value_records().iter().any(|r| r.key == u) {
            // The membership record still goes out unconditionally (`plan`'s own rule), even
            // though it is idempotent here and destroys nothing: naming a real frame as no frame
            // at all is the exact shape CLAUDE.md now warns against, which is why this says
            // "membership rewritten, value unchanged" rather than treating the write as a no-op.
            writeln!(
                out,
                "{}: {} already at {}{source_suffix} in no {} keyset, membership rewritten, value unchanged",
                kind_name(kind),
                key_label(u),
                target.display(),
                kind_name(kind)
            )?;
        } else if value_moves(kind, plan, prior, u) {
            writeln!(
                out,
                "{}: returning {} to {}{source_suffix}{mode_suffix}, already in no {} keyset",
                kind_name(kind),
                key_label(u),
                target.display(),
                kind_name(kind)
            )?;
        } else if let Some(change) = mode_change_here {
            writeln!(
                out,
                "{}: {} keeps {prior_value}{source_suffix}, {change}, already in no {} keyset",
                kind_name(kind),
                key_label(u),
                kind_name(kind)
            )?;
        } else {
            // Unreachable given `Change::ap`/`Change::rt_off`'s fixed field sets: a value bundle
            // is emitted only when the mode value or the kind's own owned value differs, and
            // `Change::ap` leaves rt targets equal to `prior` while `Change::rt_off` leaves ap
            // equal, so "bundle emitted and the kind's own value unchanged" always means the mode
            // did change. Kept defensively, matching `describe_member`'s own fourth case.
            writeln!(
                out,
                "{}: {} keeps {prior_value}{source_suffix}, already in no {} keyset",
                kind_name(kind),
                key_label(u),
                kind_name(kind)
            )?;
        }
    }
    Ok(())
}

/// Whether removing every key in `leaving` empties `index` entirely, so `announce_remove` can name
/// a keyset that is about to cease to exist the same way the whole-board prompt already does for
/// every keyset at once. Compares `index`'s full member list, not just the ones in `leaving`: a
/// keyset with a member outside this selection survives, and only `sets`, read fresh at the top of
/// `remove`, knows what that full list is.
fn keyset_disappears(sets: &[Keyset], leaving: &[(u16, u8)], index: u16) -> bool {
    let Some(ks) = sets.iter().find(|k| k.index == index) else {
        return false;
    };
    ks.members
        .iter()
        .all(|member| leaving.iter().any(|&(li, lu)| li == index && lu == *member))
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

/// Each selected key that is currently in a keyset, as (its keyset's index, the key), in `usages`
/// order. What `announce_remove` needs to say a key is leaving a keyset rather than simply being
/// reset. Shared by `remove` and `rt_off`, which reach the same destination and so must agree on
/// which keys are leaving something; a second copy would be free to drift.
fn leaving_members(sets: &[Keyset], usages: &[u8]) -> Vec<(u16, u8)> {
    usages
        .iter()
        .filter_map(|&u| {
            sets.iter()
                .find(|k| k.members.contains(&u))
                .map(|k| (k.index, u))
        })
        .collect()
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

/// The typed confirmation guarding `wh set ap` when the resolved selection covers every key on
/// the board: every existing actuation point keyset loses all its members to one freshly
/// allocated keyset, so every one of them ceases to exist. The same hazard `remove` guards
/// through `confirm_whole_board_remove`, reached by a different route, so this reuses
/// `crate::confirm::confirm` rather than a second acceptance check.
///
/// Computes its own trigger from `m` and `usages`, the membership read and selection `run.rs`
/// already has, rather than trusting a caller's own pre-computed membership: a caller passing a
/// partial selection, or one where the whole board is already exactly one keyset
/// (`ApMembership::Keep`, nothing to lose and no new keyset to name), must never reach this
/// wording just because it forgot to check first. Returns without prompting in both cases.
///
/// Takes `plan`, built after it by the caller, matching `confirm_whole_board_remove`: the
/// keyset clause can read as a no-op (no ap keysets exist yet) on a board where `Change::ap`'s
/// own promotion still moves every free key off touch nibble 0 ("follow global travel") onto
/// its own pinned actuation point, and only `plan` knows how many.
pub(crate) fn confirm_whole_board_ap_set(
    out: &mut impl Write,
    m: &Membership,
    usages: &[u8],
    plan: &keyset::WritePlan,
    depth: Um,
    input: &mut impl BufRead,
) -> Result<()> {
    if usages.len() != m.entries().len() {
        return Ok(());
    }
    let (index, losing) = match ap_membership_for(m, usages)? {
        ApMembership::Keep => return Ok(()),
        ApMembership::Split { index, losing } => (index, losing),
    };
    let keysets = if losing.is_empty() {
        "no ap keysets exist to lose".to_string()
    } else {
        let indices: Vec<String> = losing.iter().map(|(i, _)| i.to_string()).collect();
        format!(
            "ap keyset(s) {} will cease to exist, their members absorbed",
            indices.join(", ")
        )
    };
    // A count, not a per-key list, the same reason `confirm_whole_board_remove` reads one off
    // `plan` rather than off `losing` or `depth`: a board with no losing keysets can still move
    // every free key permanently off touch nibble 0, and that is the one thing the keyset clause
    // above cannot say by itself.
    let mode_clause = ap_mode_clause(plan);
    let prompt = format!(
        "ap: this selection moves every key into one new keyset, keyset {}\n    {keysets}\
         {mode_clause}\n    to change the board's base instead, leaving keysets alone: wh set \
         ap --base {:.2}",
        index.value(),
        depth.to_mm()
    );
    if !crate::confirm::confirm(out, &prompt, input)? {
        bail!("ap set over the whole board was not confirmed");
    }
    Ok(())
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
/// key, or a delete's own write does to it. Four cases, since a value record can be present
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

/// How many keys in `plan` had their touch mode actually move, per `mode_change`. Behind
/// `ap_mode_clause`, `confirm_whole_board_remove`'s rapid trigger branch, and `wh set ap
/// --base`'s own announcement, so the count is always read off the plan being announced or
/// confirmed, never inferred separately from a guess at how many keys sit at touch nibble 0.
/// `rt_on_mode_clause` walks `plan.before()` itself instead, since it needs the same keys split
/// by which nibble each is leaving rather than one total.
pub(crate) fn moved_mode_count(plan: &keyset::WritePlan) -> usize {
    plan.before()
        .iter()
        .filter(|prior| mode_change(plan, prior, prior.usage).is_some())
        .count()
}

/// The `Kind::Ap` mode-transition clause every whole-board confirmation and `wh set ap --base`'s
/// own announcement appends: empty when no key moves off touch nibble 0 ("follow global
/// travel"), otherwise ", N key(s) move off global travel onto their own actuation point".
/// Holds `moved_mode_count`, the sentence, and the empty-on-zero rule in one place, since these
/// four call sites would otherwise be four separate copies of the same wording, only the count
/// itself shared, free to drift apart one edit at a time with the suite still green: each site
/// is pinned by its own test with its own literal string, so a wording change at one site alone
/// passes every gate. Neither rapid trigger clause is this: `confirm_whole_board_remove`'s
/// `Kind::Rt` branch says something different ("have rapid trigger switched off") and stays
/// inline there, and a create's own `Kind::Rt` clause is `rt_on_mode_clause`, which splits its
/// count by origin rather than reporting one.
pub(crate) fn ap_mode_clause(plan: &keyset::WritePlan) -> String {
    let moved_modes = moved_mode_count(plan);
    if moved_modes == 0 {
        String::new()
    } else {
        format!(", {moved_modes} key(s) move off global travel onto their own actuation point")
    }
}

/// The `Kind::Rt` mode-transition clause a create's whole-board confirmation appends, split by
/// what each moving key is coming from, since one sentence cannot honestly cover all of them.
/// `docs/protocol.md` records as measured that touch nibbles 0 and 1 are rapid trigger off, 2 is
/// rapid trigger on following the board's global sensitivity, and 3 and 4 are on with the key's
/// own. So a key leaving 0 or 1 really is having rapid trigger switched on, which changes how it
/// behaves under a keypress and has to be said; a key leaving 2 had it on already and is only
/// changing where its sensitivity comes from.
///
/// Nibble 5 and above is unmeasured, so it is counted with the second group, whose sentence
/// claims only the destination: nothing here asserts an unknown nibble was off. Both counts come
/// off `plan`, and each clause is omitted entirely at zero rather than printed as "0 key(s)".
fn rt_on_mode_clause(plan: &keyset::WritePlan) -> String {
    let mut switched_on = 0usize;
    let mut own_sensitivity = 0usize;
    for prior in plan.before() {
        if mode_change(plan, prior, prior.usage).is_none() {
            continue;
        }
        match prior.mode.touch {
            TouchMode::Global | TouchMode::Single => switched_on += 1,
            // RtGlobal, and any unmeasured nibble. `Rt` and `RtContinuous` never reach here at
            // all: `Change::rt_on` leaves both exactly where they are, so `mode_change` reports
            // nothing for them and the `continue` above has already skipped them.
            _ => own_sensitivity += 1,
        }
    }
    let mut clause = String::new();
    if switched_on > 0 {
        clause.push_str(&format!(
            ", {switched_on} key(s) have rapid trigger switched on"
        ));
    }
    if own_sensitivity > 0 {
        clause.push_str(&format!(
            ", {own_sensitivity} key(s) move onto their own rapid trigger sensitivity"
        ));
    }
    clause
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

/// How many keys in `plan` actually have a new actuation point value, per `value_moves`, not
/// merely a count of keys `plan` sent an AP record for: `plan` echoes a key's own value back
/// unchanged in the same bundle as an unrelated change, most often a touch mode promotion off
/// nibble 0, so a record existing is not the same claim as the value moving. `wh set ap --base`'s
/// own announcement uses this so it can never report movement a mixed board did not actually
/// make: reporting `free.len()` unconditionally was exactly that defect, caught on a board where
/// every free key already held the target value and the write sent nothing at all.
pub(crate) fn ap_value_moved_count(plan: &keyset::WritePlan) -> usize {
    plan.before()
        .iter()
        .filter(|prior| value_moves(Kind::Ap, plan, prior, prior.usage))
        .count()
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
/// cannot capture. `run.rs`'s `verify_rt`, the one write path left that does not go through
/// `verify_write_as`, builds its own mode-fault wording inline and so cannot be pinned this
/// precisely; this function is what makes that possible here.
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

    // -- confirm_whole_board_remove: the last guard before a whole-board write --

    /// The prompt names the value every key is about to move to, not only which keysets are lost:
    /// a board with live keysets still needs the value said, since that is what actually moves on
    /// every key, keyset members or not. Pins both halves so a rewrite that dropped the value
    /// clause, keeping only the keyset clause, fails here rather than passing every other test.
    #[test]
    fn confirm_whole_board_remove_names_the_value_and_the_keysets_lost() {
        use wh_device::replay::ReplayTransport;
        let sets = [ks(1, &[0x1A]), ks(3, &[0x04])];
        // A plan with no mode transition in it: this test is only about the value and keyset
        // clauses, which `keyset_remove_whole_board_prompt_names_a_mode_transition_a_no_op_value_would_hide`
        // (an end-to-end test) covers on its own.
        let lines = settings_script(0x1A, 2000, 0x18, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let plan = keyset::plan(&mut s, &[0x1A], &keyset::Change::ap(Um(2000)), None).unwrap();
        let mut out = Vec::new();
        confirm_whole_board_remove(
            &mut out,
            Kind::Ap,
            &sets,
            Target::Ap(Um(2000)),
            &plan,
            &mut "yes\n".as_bytes(),
            "ap keyset removal",
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("every key moves to 2.00mm"), "got: {text}");
        assert!(
            text.contains("ap keyset(s) 1, 3 will cease to exist"),
            "got: {text}"
        );
    }

    /// The branch a whole-board selection always reaches when no keysets exist for that kind:
    /// it still moves every key to an invented value, `NO_SIGNAL_BASE` on a board of free keys
    /// that disagree with it, so the value clause is the only warning the operator gets and must
    /// still fire. Also pins the "no keysets exist to lose" wording, which must not read as though
    /// nothing is about to happen.
    #[test]
    fn confirm_whole_board_remove_names_the_value_when_no_keysets_exist() {
        use wh_device::replay::ReplayTransport;
        let lines = settings_script(0x1A, 1800, 0x18, 100, 150, 0, 0);
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let plan = keyset::plan(&mut s, &[0x1A], &keyset::Change::ap(Um(1800)), None).unwrap();
        let mut out = Vec::new();
        confirm_whole_board_remove(
            &mut out,
            Kind::Ap,
            &[],
            Target::Ap(Um(1800)),
            &plan,
            &mut "yes\n".as_bytes(),
            "ap keyset removal",
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("every key moves to 1.80mm"), "got: {text}");
        assert!(text.contains("no ap keysets exist to lose"), "got: {text}");
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

    // -- confirm_whole_board_ap_set: guards `wh set ap --keys all` --

    /// Pins the exact wording the operator ruled on: the new index, every losing keyset in
    /// ascending order, the mode count `plan` alone knows (`w` starts at touch nibble 0, "follow
    /// global travel", and `Change::ap` promotes it), and the `--base` alternative. Does not say
    /// `--keys all`: the selection here is the raw usages, not that spelling, and the wording
    /// must not claim it was.
    #[test]
    fn confirm_whole_board_ap_set_names_the_new_index_the_losing_keysets_the_mode_count_and_the_base_alternative(
    ) {
        let mut lines = matrix_lines(&[0x1A, 0x04]);
        lines.extend(read_reply(0x1A, layout::KEYSET_AP, 2));
        lines.extend(read_reply(0x04, layout::KEYSET_AP, 7));
        lines.extend(settings_script(0x1A, 1200, 0x00, 100, 150, 2, 0)); // w: Global, promotes
        lines.extend(settings_script(0x04, 1300, 0x18, 100, 150, 7, 0)); // a: already pinned
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let m = keyset::read_membership(&mut s, Kind::Ap).unwrap();
        let usages = [0x1Au8, 0x04];
        let index = match ap_membership_for(&m, &usages).unwrap() {
            ApMembership::Split { index, .. } => index,
            ApMembership::Keep => panic!("two distinct keysets must split"),
        };
        let plan =
            keyset::plan(&mut s, &usages, &keyset::Change::ap(Um(1500)), Some(index)).unwrap();

        let mut out = Vec::new();
        confirm_whole_board_ap_set(
            &mut out,
            &m,
            &usages,
            &plan,
            Um(1500),
            &mut "yes\n".as_bytes(),
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("ap: this selection moves every key into one new keyset, keyset 8"),
            "got: {text}"
        );
        assert!(
            text.contains("ap keyset(s) 2, 7 will cease to exist, their members absorbed"),
            "got: {text}"
        );
        assert!(
            text.contains("1 key(s) move off global travel onto their own actuation point"),
            "got: {text}"
        );
        assert!(text.contains("wh set ap --base 1.50"), "got: {text}");
    }

    /// The branch a whole-board selection reaches when no ap keysets exist yet: nothing is lost,
    /// but every free key still moves into the new one, so the prompt must still fire and must
    /// not read as though nothing is about to happen.
    #[test]
    fn confirm_whole_board_ap_set_names_no_keysets_when_none_exist() {
        let mut lines = matrix_lines(&[0x1A]);
        lines.extend(read_reply(0x1A, layout::KEYSET_AP, 0));
        lines.extend(settings_script(0x1A, 1800, 0x18, 100, 150, 0, 0));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let m = keyset::read_membership(&mut s, Kind::Ap).unwrap();
        let usages = [0x1Au8];
        let index = match ap_membership_for(&m, &usages).unwrap() {
            ApMembership::Split { index, losing } => {
                assert!(
                    losing.is_empty(),
                    "a free-only board loses nothing: {losing:?}"
                );
                index
            }
            ApMembership::Keep => panic!("a free key must allocate, not keep"),
        };
        let plan =
            keyset::plan(&mut s, &usages, &keyset::Change::ap(Um(1800)), Some(index)).unwrap();
        let mut out = Vec::new();
        confirm_whole_board_ap_set(
            &mut out,
            &m,
            &usages,
            &plan,
            Um(1800),
            &mut "yes\n".as_bytes(),
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("no ap keysets exist to lose"), "got: {text}");
    }

    /// Declining refuses, through the same `crate::confirm::confirm` acceptance rule `remove`
    /// uses, not a second copy of it.
    #[test]
    fn confirm_whole_board_ap_set_refuses_on_no() {
        // Two distinct keysets, so `ap_membership_for` must `Split`, not `Keep`: a single key
        // that is already the whole of its own keyset would keep its index instead, and this
        // test needs a real hazard to decline.
        let mut lines = matrix_lines(&[0x1A, 0x04]);
        lines.extend(read_reply(0x1A, layout::KEYSET_AP, 1));
        lines.extend(read_reply(0x04, layout::KEYSET_AP, 2));
        lines.extend(settings_script(0x1A, 1200, 0x18, 100, 150, 1, 0));
        lines.extend(settings_script(0x04, 1300, 0x18, 100, 150, 2, 0));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let m = keyset::read_membership(&mut s, Kind::Ap).unwrap();
        let usages = [0x1Au8, 0x04];
        let index = match ap_membership_for(&m, &usages).unwrap() {
            ApMembership::Split { index, .. } => index,
            ApMembership::Keep => panic!("two distinct keysets must split"),
        };
        let plan =
            keyset::plan(&mut s, &usages, &keyset::Change::ap(Um(1500)), Some(index)).unwrap();
        let mut out = Vec::new();
        let err = confirm_whole_board_ap_set(
            &mut out,
            &m,
            &usages,
            &plan,
            Um(1500),
            &mut "no\n".as_bytes(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("was not confirmed"), "got: {err}");
    }

    /// `create`'s own sibling: a selection short of the whole matrix must never reach the prompt
    /// even when a caller hands it one directly, since both "this selects every key on the board"
    /// and the list of keysets ceasing to exist would be false of a partial selection. The trigger
    /// lives inside the function rather than at its one call site, so this pins it structurally
    /// rather than trusting the caller to keep checking first. Empty input: a prompt here would
    /// read it and hang or refuse; printing nothing and returning `Ok(())` unread is the point.
    #[test]
    fn confirm_whole_board_create_does_not_prompt_over_a_partial_selection() {
        let mut lines = matrix_lines(&[0x1A, 0x04]);
        lines.extend(read_reply(0x1A, layout::KEYSET_AP, 1));
        lines.extend(read_reply(0x04, layout::KEYSET_AP, 1));
        lines.extend(settings_script(0x1A, 2000, 0x18, 100, 150, 1, 0));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let m = keyset::read_membership(&mut s, Kind::Ap).unwrap();
        let usages = [0x1Au8]; // only one of the board's two keys
        let index = keyset::next_index(&m).unwrap();
        let plan =
            keyset::plan(&mut s, &usages, &keyset::Change::ap(Um(1200)), Some(index)).unwrap();

        let mut out = Vec::new();
        confirm_whole_board_create(
            &mut out,
            Kind::Ap,
            &m,
            &usages,
            index.value(),
            Target::Ap(Um(1200)),
            &plan,
            &mut "".as_bytes(),
        )
        .unwrap();
        assert!(
            out.is_empty(),
            "must print nothing for a partial selection: {out:?}"
        );
    }

    /// A selection short of the whole matrix must never reach the prompt: the function decides
    /// its own trigger from `m`/`usages` now, rather than trusting a caller's own pre-computed
    /// check, so this pins that decision directly rather than only through `run.rs`'s own
    /// end-to-end test. Empty input: a prompt here would read it and hang or refuse; printing
    /// and returning `Ok(())` unread is the whole point.
    #[test]
    fn confirm_whole_board_ap_set_does_not_prompt_over_a_partial_selection() {
        let mut lines = matrix_lines(&[0x1A, 0x04]);
        lines.extend(read_reply(0x1A, layout::KEYSET_AP, 0));
        lines.extend(read_reply(0x04, layout::KEYSET_AP, 0));
        lines.extend(settings_script(0x1A, 2000, 0x18, 100, 150, 0, 0));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let m = keyset::read_membership(&mut s, Kind::Ap).unwrap();
        let usages = [0x1Au8]; // only one of the board's two keys
        let plan = keyset::plan(&mut s, &usages, &keyset::Change::ap(Um(1200)), None).unwrap();

        let mut out = Vec::new();
        confirm_whole_board_ap_set(&mut out, &m, &usages, &plan, Um(1200), &mut "".as_bytes())
            .unwrap();
        assert!(
            out.is_empty(),
            "must print nothing for a partial selection: {out:?}"
        );
    }

    /// The `Keep` mirror of the test above: the whole board is already exactly one keyset, so
    /// nothing ceases to exist and no new keyset is named, and the prompt must stay silent.
    #[test]
    fn confirm_whole_board_ap_set_does_not_prompt_when_the_whole_board_already_keeps_its_index() {
        let mut lines = matrix_lines(&[0x1A, 0x04]);
        lines.extend(read_reply(0x1A, layout::KEYSET_AP, 1));
        lines.extend(read_reply(0x04, layout::KEYSET_AP, 1));
        lines.extend(settings_script(0x1A, 1200, 0x18, 100, 150, 1, 0));
        lines.extend(settings_script(0x04, 1200, 0x18, 100, 150, 1, 0));
        let mut s = Session::new(ReplayTransport::from_jsonl(&lines.join("\n")).unwrap());
        let m = keyset::read_membership(&mut s, Kind::Ap).unwrap();
        let usages = [0x1Au8, 0x04];
        let plan = keyset::plan(&mut s, &usages, &keyset::Change::ap(Um(1500)), None).unwrap();

        let mut out = Vec::new();
        confirm_whole_board_ap_set(&mut out, &m, &usages, &plan, Um(1500), &mut "".as_bytes())
            .unwrap();
        assert!(
            out.is_empty(),
            "must print nothing when Keep applies: {out:?}"
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
