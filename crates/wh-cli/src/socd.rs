//! The `wh socd` command tree. Every handler reads the board's live pairings first: `wh` caches
//! no device state, and a key may sit in one pair only, so a stale view could pair a key that is
//! already paired.
//!
//! Nothing here ever prints or compares a raw priority byte. The board spells one pairing two
//! ways depending on which member was queried (`docs/protocol.md`, "SOCD"), so the byte is only
//! meaningful next to the record order it came with; `wh_proto::socd::Pairing` carries the
//! winner instead, and that is what reaches the operator.

use anyhow::{bail, Result};
use std::collections::HashMap;
use std::io::Write;
use wh_device::socd::{Board, RemovePlan};
use wh_proto::cmds::TouchMode;
use wh_proto::select::Selector;
use wh_proto::socd::{Pairing, Priority, LAST_INPUT};

use crate::run::{key_label, touch_mode_label};

/// One key argument, resolved through the shared selector grammar against the board's own live
/// matrix, then required to name exactly one key.
///
/// Going through `Selector` rather than a bare name lookup is what makes `wh socd` assert the key
/// exists on the board in front of it, the same assertion every other key-taking surface makes.
/// Without it, pairing a key this board does not have wrote a pairing that `list` could not show
/// and `unpair` could not undo, and pairing one real key with one absent key left every `wh socd`
/// subcommand refusing until the vendor UI cleared it. It also buys the hex form `wh socd list`
/// itself prints, stored and builtin group names, and the grammar's own "did you mean" hint.
///
/// Exactly one key, because `wh socd` names individual keys rather than a selection. That is also
/// what keeps a broad selector out of a write path: `all` resolves here and is then refused for
/// arity, so no whole-board form exists to need a typed-`yes` guard.
fn resolve_one(
    universe: &[u8],
    groups: &HashMap<String, Vec<u8>>,
    what: &str,
    arg: &str,
) -> Result<u8> {
    let usages = Selector::parse(arg)?.resolve(universe, groups)?;
    match usages.len() {
        1 => Ok(usages[0]),
        0 => bail!("{what} matches no keys on this board: '{arg}'"),
        n => {
            let names: Vec<String> = usages.iter().map(|&u| key_label(u)).collect();
            bail!(
                "{what} names exactly one key, but '{arg}' resolves to {n} keys ({})",
                names.join(", ")
            )
        }
    }
}

fn pair_or_pairs(n: usize) -> &'static str {
    if n == 1 {
        "pair"
    } else {
        "pairs"
    }
}

/// Lists every pairing on the board, one line each, plus a count line. The count is taken off
/// the same list that was printed, so it can never disagree with the lines above it.
pub(crate) fn list(out: &mut impl Write, board: &Board) -> Result<()> {
    let pairings = board.pairings();
    if pairings.is_empty() {
        writeln!(out, "socd pairs: none")?;
        return Ok(());
    }
    writeln!(out, "socd pairs:")?;
    for p in pairings {
        writeln!(out, "  {}", p.describe())?;
    }
    writeln!(out, "{} {}", pairings.len(), pair_or_pairs(pairings.len()))?;
    Ok(())
}

/// Resolves the two key arguments and `--priority` into a `Pairing`, every one of them against the
/// board's own matrix. `--priority` names one of the two keys or `last-input`; anything else is
/// refused rather than quietly falling back to the default, which would silently write a setting
/// nobody asked for.
pub(crate) fn resolve_pair(
    board: &Board,
    groups: &HashMap<String, Vec<u8>>,
    key_a: &str,
    key_b: &str,
    priority: &str,
) -> Result<Pairing> {
    let universe = board.matrix();
    let a = resolve_one(universe, groups, "wh socd pair's first key", key_a)?;
    let b = resolve_one(universe, groups, "wh socd pair's second key", key_b)?;
    if a == b {
        bail!(
            "wh socd pair needs two different keys, and '{}' names the same key twice",
            key_label(a)
        );
    }
    let prio = if priority.eq_ignore_ascii_case(LAST_INPUT) {
        Priority::LastInput
    } else {
        let w = resolve_one(universe, groups, "--priority", priority)?;
        if w != a && w != b {
            bail!(
                "--priority must name one of the two keys ({} or {}) or {}, and '{}' is neither",
                key_label(a),
                key_label(b),
                LAST_INPUT,
                key_label(w)
            );
        }
        Priority::Wins(w)
    };
    Ok(Pairing::new(a, b, prio)?)
}

/// Refuses when either key of `pair` already sits in a pairing on the board, naming that pairing
/// and the command that undoes it. The UI's model is disjoint pairs; whether the board would
/// accept an overlap is unmeasured, and refusing means `wh` never finds out by accident.
pub(crate) fn check_unpaired(board: &Board, pair: Pairing) -> Result<()> {
    let (a, b) = pair.keys();
    for u in [a, b] {
        if let Some(existing) = board.pairing_of(u) {
            bail!(
                "{} is already in the SOCD pair {}; a key may sit in one pair only, so run \
                 `wh socd unpair {}` first",
                key_label(u),
                existing.describe(),
                key_label(u)
            );
        }
    }
    Ok(())
}

/// Announces the write about to happen, from the pairing that is about to be written. Names the
/// board's own part explicitly: the mode flag is not in the frame `wh` sends, so an operator
/// reading `--dry-run`'s single frame would otherwise have no way to know the flag gets set.
pub(crate) fn announce_pair(out: &mut impl Write, pair: Pairing) -> Result<()> {
    writeln!(out, "socd: pairing {}", pair.describe())?;
    writeln!(
        out,
        "socd: the board sets the SOCD mode flag on both keys itself, so no mode record is sent"
    )?;
    Ok(())
}

pub(crate) fn report_pair(out: &mut impl Write, pair: Pairing) -> Result<()> {
    writeln!(
        out,
        "socd: {} verified, both keys report the SOCD mode flag",
        pair.describe()
    )?;
    Ok(())
}

/// The pairings the named keys belong to, deduplicated, in the order the arguments were given.
/// Each argument resolves against the board's matrix and must name exactly one key, so `unpair`
/// still has no whole-board form for a broad selector to reach. A key that is in no pairing is
/// refused, naming it, before anything is written.
pub(crate) fn resolve_unpair(
    board: &Board,
    groups: &HashMap<String, Vec<u8>>,
    args: &[String],
) -> Result<Vec<Pairing>> {
    let mut out: Vec<Pairing> = Vec::new();
    for arg in args {
        let u = resolve_one(board.matrix(), groups, "wh socd unpair's key", arg)?;
        match board.pairing_of(u) {
            Some(p) if !out.contains(&p) => out.push(p),
            Some(_) => {}
            None => bail!(
                "{} is not in any SOCD pair; run `wh socd list` to see the pairs on the board",
                key_label(u)
            ),
        }
    }
    Ok(out)
}

/// Announces one unpair from the plan that is about to be applied, not from the pairing alone:
/// the touch modes named are the ones the records actually carry forward.
pub(crate) fn announce_unpair(out: &mut impl Write, plan: &RemovePlan) -> Result<()> {
    let pair = plan.pair();
    writeln!(
        out,
        "socd: unpairing {} + {}, priority was {}",
        key_label(pair.keys().0),
        key_label(pair.keys().1),
        pair.priority().label()
    )?;
    let names: Vec<String> = plan
        .before()
        .iter()
        .map(|&(u, _)| key_label(u))
        .collect::<Vec<_>>();
    writeln!(
        out,
        "socd: clearing the SOCD mode flag on {}{}",
        names.join(" and "),
        touch_clause(plan)
    )?;
    Ok(())
}

/// What each key keeps when its SOCD flag is cleared. Silent about the individual modes when
/// every key is on `Global`, the nibble every captured vendor remove was on, and explicit about
/// each one otherwise, since preserving a non-zero touch nibble is `wh`'s own rule applied past
/// what the vendor was measured doing and the operator should see which key kept what.
fn touch_clause(plan: &RemovePlan) -> String {
    if plan
        .before()
        .iter()
        .all(|&(_, m)| m.touch == TouchMode::Global)
    {
        return ", each key keeps its own touch mode".to_string();
    }
    let parts: Vec<String> = plan
        .before()
        .iter()
        .map(|&(u, m)| format!("{} on {}", key_label(u), touch_mode_label(m.touch)))
        .collect();
    format!(", keeping {}", parts.join(" and "))
}

pub(crate) fn report_unpair(out: &mut impl Write, plan: &RemovePlan) -> Result<()> {
    let pair = plan.pair();
    writeln!(
        out,
        "socd: {} + {} unpaired, the SOCD mode flag is clear on both keys",
        key_label(pair.keys().0),
        key_label(pair.keys().1)
    )?;
    Ok(())
}
