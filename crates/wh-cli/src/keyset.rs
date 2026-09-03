//! The `wh keyset` command tree. Every handler here reads the board's live membership first;
//! `wh` caches no device state, and allocation is max plus one over live membership, so a stale
//! view could hand out an index a key already holds.

use anyhow::{bail, Result};
use std::io::Write;
use wh_device::keyset::{self, Global, Keyset, Kind, Membership};
use wh_device::session::Session;
use wh_device::transport::Transport;
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
///
/// Unused until `create`/`set`/`delete` land and call it; `#[allow(dead_code)]` until then.
#[allow(dead_code)]
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
///
/// Unused until `create`/`set`/`delete` land and call it; `#[allow(dead_code)]` until then.
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

/// Lists one kind's keysets with their members and the value those members hold. The value comes
/// from the first member's live settings rather than being assumed: a keyset is supposed to hold
/// one value across its members, and printing what is actually there is how a caller sees when
/// that has stopped being true.
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
        let names: Vec<String> = ks.members.iter().map(|&u| key_label(u)).collect();
        let first = wh_device::ops::read_key_settings(s, ks.members[0])?;
        let value = match kind {
            Kind::Ap => format!("{:.2}mm", first.ap.to_mm()),
            Kind::Rt => format!(
                "{:.2}/{:.2}mm",
                first.rt_press.to_mm(),
                first.rt_release.to_mm()
            ),
        };
        writeln!(out, "  {} {}  {}", ks.index, value, names.join(","))?;
    }
    Ok(())
}
