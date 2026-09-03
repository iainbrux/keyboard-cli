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
/// silently. Tested directly below; still unreached outside a test build until `create`/`set`/
/// `delete` call it.
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
/// nobody typed over every member of the keyset being created. Unused until `create`/`set`/
/// `delete` land and call it.
#[allow(dead_code)]
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

/// Unused until `create`/`set`/`delete` land and call it; `#[allow(dead_code)]` until then.
#[allow(dead_code)]
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
/// keys hold it. `wh` never picks a winner here, the same refusal `global_ap`/`global_rt` apply
/// to the board's global value.
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
            Ok(agreement_line(&values, |v: Um| {
                format!("{:.2}mm", v.to_mm())
            }))
        }
        Kind::Rt => {
            let mut values = Vec::with_capacity(ks.members.len());
            for &u in &ks.members {
                let press = Um(ops::read_layout_value(s, u, layout::RT_PRESS)?);
                let release = Um(ops::read_layout_value(s, u, layout::RT_RELEASE)?);
                values.push((u, (press, release)));
            }
            Ok(agreement_line(&values, |(p, r): (Um, Um)| {
                format!("{:.2}/{:.2}mm", p.to_mm(), r.to_mm())
            }))
        }
    }
}

/// Renders one keyset's members and their values: every member's name if they agree, or, if they
/// do not, each distinct value with the names of the keys holding it. Never a single member's
/// value passed off as the whole keyset's, which is exactly the defect this replaces.
fn agreement_line<V: PartialEq + Copy>(members: &[(u8, V)], fmt: impl Fn(V) -> String) -> String {
    let first = members[0].1;
    if members.iter().all(|&(_, v)| v == first) {
        let names: Vec<String> = members.iter().map(|&(u, _)| key_label(u)).collect();
        return format!("{}  {}", fmt(first), names.join(","));
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
    format!("disagree: {}", parts.join(", "))
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
