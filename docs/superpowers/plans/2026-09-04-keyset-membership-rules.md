# Keyset Membership Rules Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make membership follow the operator's ruling: setting a key's actuation point puts it in a keyset, and leaving a keyset becomes its own command.

**Architecture:** Both changes reuse the existing `wh-device::keyset::plan` machinery rather than adding any. Task 1 deletes one early return in `ap_membership_for` so an all-free selection falls through to the `Split` arm that already exists. Task 2 adds a `wh keyset remove` subcommand whose handler is `keyset::delete`'s shape with a key selector in place of an index.

**Tech Stack:** Rust 2021, four-crate workspace, `clap` derive for the CLI surface, `ReplayTransport` for every test.

**Spec:** `docs/tasks.md` tasks 2.20 and 2.21 (the rulings), and `docs/keysets.md` (the measured evidence each ruling rests on). Read both.

## Global Constraints

- **No em dashes or en dashes anywhere**, in code, comments, docs or commit messages. Use a comma, parentheses, a colon, or a full stop.
- Commit messages are one line, `[type] - Message`, types `feat`/`fix`/`docs`/`test`/`refactor`/`chore`. **No trailers of any kind, including `Co-Authored-By`.**
- **Never loosen `ReplayTransport`'s byte-for-byte frame matching** to make a test pass. If a fixture stops matching, the code changed under it and the fixture is what should change.
- **Assert values, never coordinates.** An assertion that names a layout byte, key name or keyset index but only checks that *something* exists there is decorative. Every assertion must pin the value.
- **Establish that each test fails when the code is wrong.** Mutate the thing the test claims to check, watch it fail, restore it, and say so in your report. This is not optional and reports without it will be rejected.
- Crate layering: `wh-proto` does no I/O, `wh-device` does nothing user-facing, `wh-cli` never encodes frames by hand.
- Comments default to one or two lines, four is the ceiling. **Never cite a task number, review round or chunk number** in a comment: they point at gitignored files.
- Docs may state what was **measured** and may state inferences, but must say which is which.
- All three gates must pass before any commit: `cargo test --workspace --no-fail-fast`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`.

## File Structure

| File | Responsibility | Touched by |
|---|---|---|
| `crates/wh-cli/src/keyset.rs` | `ap_membership_for`'s rule, and the new `remove` operation | Tasks 1, 2 |
| `crates/wh-cli/src/cli.rs` | The `KeysetWhat::Remove` clap variant | Task 2 |
| `crates/wh-cli/src/run.rs` | The `KeysetWhat::Remove` dispatch arm | Task 2 |
| `crates/wh-cli/tests/dump.rs` | End-to-end `wh set ap` membership rows | Task 1 |
| `crates/wh-cli/tests/keyset.rs` | End-to-end `wh keyset` subcommand behaviour | Task 2 |
| `docs/tasks.md` | Closing 2.20 and 2.21 | Tasks 1, 2 |
| `README.md` | The command reference | Task 2 |

---

### Task 1: `wh set ap` on free keys creates a keyset

**Files:**
- Modify: `crates/wh-cli/src/keyset.rs` (`ap_membership_for` around line 378, and the `ApMembership::Keep` doc comment around line 361)
- Test: `crates/wh-cli/src/keyset.rs` unit tests, and `crates/wh-cli/tests/dump.rs` around line 1236

**Interfaces:**
- Consumes: `keyset::group(&Membership) -> Vec<Keyset>`, `keyset::next_index(&Membership) -> Result<KeysetIndex>`, `losing_members(&[Keyset], &[u8]) -> Vec<(u16, Vec<u8>)>`, all already in this file.
- Produces: `ap_membership_for(m: &Membership, usages: &[u8]) -> Result<ApMembership>`, same signature as today. Task 2 does not use it.

**The ruling this implements.** A key sits outside a keyset exactly when it holds the board's base value; any other value means it belongs to one. So `wh set ap --keys h --set 1.5` on a free key must allocate a keyset and put `h` in it. **This applies regardless of the value**, including when the value set happens to equal the base: the operator ruled that explicitly picking a key and giving it a value always means membership. Do not add a value-dependent branch.

The mirror case is ruled the other way and must not change: a selection that is exactly one whole keyset keeps its index, even when the new value equals the base. Leaving a keyset is `wh keyset remove`'s job, not `wh set ap`'s.

**One piece of counter-evidence, recorded so you are not surprised by it.** `docs/keysets.md` notes that `ks-value-ap` shows the vendor writing no `0xFF` record over a three-key value change, and that one reading of that capture is a mixed selection of free and member keys, which is exactly what `wh` will now split. The operator has ruled anyway. Do not soften the implementation because of this; it is recorded in the docs already.

- [ ] **Step 1: Write the failing unit test**

Add to the `ap_membership_for` test block in `crates/wh-cli/src/keyset.rs`, next to `ap_membership_for_splits_when_a_whole_keyset_rides_along_with_a_free_key`:

```rust
/// The 2.20 ruling: a selection where every key is free must still allocate a keyset, where
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
            assert_eq!(index.value(), 1, "first keyset on an empty board: {index:?}");
            assert!(losing.is_empty(), "no keyset loses anything: {losing:?}");
        }
        ApMembership::Keep => panic!("an all-free selection must now create a keyset"),
    }
}

/// The mirror case, unchanged by the 2.20 ruling: a selection that is exactly one keyset's
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
```

- [ ] **Step 2: Run the tests to verify the first fails and the second passes**

Run: `cargo test -p wh-cli --lib ap_membership_for`
Expected: `ap_membership_for_creates_a_keyset_when_every_selected_key_is_free` FAILS with "an all-free selection must now create a keyset". `ap_membership_for_keeps_the_index_when_the_selection_is_exactly_one_keyset` PASSES already.

- [ ] **Step 3: Delete the early return**

In `crates/wh-cli/src/keyset.rs`, remove these three lines from `ap_membership_for`:

```rust
    if losing.is_empty() {
        return Ok(ApMembership::Keep);
    }
```

That is the entire code change. An all-free selection now falls through to the existing `Split` arm with an empty `losing`, and `keyset::next_index(m)` allocates for it. Do not restructure anything else in the function.

- [ ] **Step 4: Correct the `ApMembership::Keep` doc comment**

The variant's comment currently says `Keep` covers two cases. Only one survives. Replace:

```rust
    /// Leave membership alone: either no selected key is in a keyset, or the selection is
    /// exactly one keyset's members and it keeps its index.
    Keep,
```

with:

```rust
    /// Leave membership alone: the selection is exactly one keyset's members, so it keeps its
    /// index. A selection of free keys does not come here; it allocates, since a key holding a
    /// value of its own belongs to a keyset.
    Keep,
```

- [ ] **Step 5: Run the unit tests again**

Run: `cargo test -p wh-cli --lib ap_membership_for`
Expected: all four `ap_membership_for` tests PASS.

- [ ] **Step 6: Invert the end-to-end free-keys test**

`crates/wh-cli/tests/dump.rs` has `set_ap_dry_run_over_free_keys_writes_no_membership_record` at roughly line 1236, which asserts the behaviour this task reverses. It must now assert the opposite. Rename it to `set_ap_dry_run_over_free_keys_creates_a_keyset`, rewrite its doc comment to describe the ruling rather than the old "row one" rule, and change the assertion:

- The script needs one more read than before, because the plan now allocates: check what the run actually demands and extend the fixture to match. **Do not loosen the replay matching to avoid adding a line.**
- The expected frame list is the existing eight value records, unchanged, followed by the membership records **one record per frame**, matching the vendor template that `plan` implements. Build them the same hand-built way the test already builds the value records, through `cmds::write_key_records`, not by copying bytes out of a failing run's output.
- Keep the assertion an exact `assert_eq!` on the full frame sequence. Do not weaken it to a `contains`.
- Add an assertion on stdout that the announcement names the allocated index and the enrolled keys with their prior values, for example `ap keyset 1: creating at 1.20mm` and `enrolling free key(s) w at 2.00mm,a at 2.00mm`. Take the exact wording from what `announce_steal` produces; the existing split tests around line 1560 show the shape.

- [ ] **Step 7: Confirm the neighbouring test still passes untouched**

Run: `cargo test -p wh-cli --test dump set_ap`
Expected: `set_ap_dry_run_over_a_whole_keyset_keeps_its_index` PASSES with no edit. If it fails, the change went too far and took the surviving `Keep` case with it.

- [ ] **Step 8: Prove the tests fail when the code is wrong**

Restore the deleted early return, run `cargo test -p wh-cli --no-fail-fast`, and record in your report exactly which tests fail and with what message. Then remove it again and confirm green. A report that does not name the failing tests from this step will be rejected.

- [ ] **Step 9: Run the three gates**

```bash
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```
Expected: all pass, 411 tests (409 plus the two new unit tests).

- [ ] **Step 10: Close 2.20 in the docs**

In `docs/tasks.md`, mark task 2.20 done by wrapping its heading in `~~` and prefixing `- [x]`, matching how 2.19 is struck through in that file. Add one sentence recording what shipped: an all-free selection now allocates, and the whole-keyset case still keeps its index. Do not restate the ruling; it is already written above the entry.

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "[feat] - Put free keys in a keyset when wh set ap gives them a value"
```

---

### Task 2: `wh keyset remove` for actuation point keysets

**Files:**
- Modify: `crates/wh-cli/src/cli.rs` (the `KeysetWhat` enum, after the `Delete` variant)
- Modify: `crates/wh-cli/src/keyset.rs` (a new `remove` function beside `delete` at line 283, and a new `announce_remove` beside `announce_delete` at line 318)
- Modify: `crates/wh-cli/src/run.rs` (a new `KeysetWhat::Remove` arm beside the `Delete` arm at line 935)
- Modify: `README.md` (the command reference)
- Test: `crates/wh-cli/tests/keyset.rs`

**Interfaces:**
- Consumes, all already present in `crates/wh-cli/src/keyset.rs`: `global_ap_or_bail<T: Transport>(s: &mut Session<T>, m: &Membership, flag: &str) -> Result<Um>`; `Target::Ap(Um)`; `describe_member(kind: Kind, plan: &keyset::WritePlan, usage: u8) -> String`; `kind_name(kind: Kind) -> &str`; `verify_write<T: Transport>(out, s, kind, op: &str, plan) -> Result<()>`; `kind_of(arg: KeysetKindArg) -> Kind`. From `wh-device`: `keyset::read_membership`, `keyset::group`, `keyset::Change::ap(Um)`, `keyset::KeysetIndex::clear(Kind)`, `keyset::plan(s, usages, change, membership) -> Result<WritePlan>`, `keyset::apply`. From `run.rs`: `resolve_keys(s, &keys, store) -> Result<Vec<u8>>`, `auto_backup(s, store, command)`, `print_frames(out, frames)`, `mm(f64) -> Result<Um>`.
- Produces: `keyset::remove<T: Transport>(out: &mut impl Write, s: &mut Session<T>, kind: Kind, usages: &[u8], value: Option<Um>) -> Result<keyset::WritePlan>`.

**Scope.** This task builds the actuation point half only. `wh keyset remove rt` must be **rejected with a clear message saying the rapid trigger case is not yet measured**, not silently accepted and not quietly treated as `remove ap`. The rapid trigger half is Task 3 and is deliberately absent from this plan until a capture exists for it, because what the removed key's MODE nibble should become is otherwise a guess.

**What the vendor does, measured.** `docs/keysets.md`, section "Removing one key from a keyset", records `ks-remove-one-key` and `ks-remove-to-empty`. Removing `J` from a three-key keyset sends the ordinary five-step template for `J` alone, ending in one `0xFF = 0` record, and writes **nothing at all** for the members that stay. The MODE record stays at touch nibble 1: the removed key is **not** dropped to nibble 0. Removing the last member is the same five frames with no teardown, so there is no empty-keyset case to handle. `keyset::plan` with `Change::ap(base)` and `KeysetIndex::clear(Kind::Ap)` already produces exactly this.

- [ ] **Step 1: Add the clap variant**

In `crates/wh-cli/src/cli.rs`, add to `enum KeysetWhat` immediately after `Delete`:

```rust
    /// Take keys out of their keyset, returning them to the global value: wh keyset remove ap --keys j
    Remove {
        kind: KeysetKindArg,
        #[command(flatten)]
        keys: KeysArg,
        /// Value in mm to return the keys to: the actuation point they will hold once they are
        /// in no keyset. Defaults to the board's global, and is required when the keys outside
        /// every keyset disagree on it.
        #[arg(long)]
        value: Option<f64>,
        /// Print the exact reports without sending
        #[arg(long)]
        dry_run: bool,
    },
```

There is deliberately no `--press` or `--release` here, unlike `Create`, `Set` and `Delete`: this task does not implement the rapid trigger case, and offering the flags would imply it does.

- [ ] **Step 2: Write the failing end-to-end test**

Add to `crates/wh-cli/tests/keyset.rs`. That file's `matrix_lines()` helper reports only four usages, `w` `0x1A`, `a` `0x04`, `s` `0x16`, `d` `0x07`, so the fixture uses those and not `j,k,l`.

The board: `w`, `a`, `s` are actuation point keyset 3 at `1.20mm`; `d` is free at `2.00mm`, which is what makes `2.00mm` the global. The command removes `w` alone.

```rust
/// The measured vendor shape for taking one key out of a keyset: the removed key gets the whole
/// per-key template ending in `0xFF = 0`, and the members that stay get no records at all
/// (`ks-remove-one-key`, `docs/keysets.md`). Exact frame equality is what pins the second half:
/// `a` and `s` must appear nowhere in the plan, which a rewrite that rewrote every member of the
/// keyset would break while still clearing `w`'s membership correctly.
#[test]
fn keyset_remove_ap_writes_only_the_removed_key_and_clears_its_membership() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 3u16), (0x04, 3), (0x16, 3), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    // global_ap reads the actuation point of the keys outside every keyset. `d` is the only one.
    lines.extend(layout_read_lines(0x07, layout::AP, 2000));
    // plan's own per-key read of w, in plan's read order.
    lines.extend(key_settings_lines(0x1A, 1200, 0x18, 100, 150, 3, 0));

    let script = write_script("keyset-remove-ap", &lines);
    let config_home = scratch_config_dir("keyset-remove-ap");
    let out = run_wh(
        &["keyset", "remove", "ap", "--keys", "w", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    // The keyset it leaves, its prior value and the value it returns to are all pinned, and all
    // three differ, so a mutation that printed any one of them in another's place fails here.
    assert!(
        stdout.contains("ap: removing w from keyset 3, 1.20mm to 2.00mm"),
        "got: {stdout}"
    );

    let value_records = [
        KeyRecord { key: 0x1A, layout: layout::MODE, value: 0x18 },
        KeyRecord { key: 0x1A, layout: layout::AP, value: 2000 },
        KeyRecord { key: 0x1A, layout: layout::RT_PRESS, value: 100 },
        KeyRecord { key: 0x1A, layout: layout::RT_RELEASE, value: 150 },
    ];
    let mut expected: Vec<String> = cmds::write_key_records(&value_records)
        .iter()
        .map(|f| hex(f))
        .collect();
    expected.extend(
        cmds::write_key_records_singly(&[KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_AP,
            value: 0,
        }])
        .iter()
        .map(|f| hex(f)),
    );
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "only w is written, membership last and alone: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}
```

The fixture's read order must match what the run actually demands. If the replay reports an unexpected frame, the fixture is what changes, never the matcher. If `plan`'s skip rule turns out to drop the MODE, press or release records here, correct `value_records` to what the code emits and say so in your report, but keep the assertion an exact full-sequence `assert_eq!`.

- [ ] **Step 3: Run it to verify it fails**

Run: `cargo test -p wh-cli --test keyset keyset_remove_ap`
Expected: FAIL, because `wh keyset remove` is not a subcommand yet, so the process exits with a clap usage error.

- [ ] **Step 4: Implement `remove` and `announce_remove`**

In `crates/wh-cli/src/keyset.rs`, beside `delete`:

```rust
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
) -> Result<keyset::WritePlan> {
    if kind != Kind::Ap {
        bail!(
            "wh keyset remove supports actuation point keysets only: what a removed key's mode \
             nibble should become for rapid trigger is not measured, and guessing it could turn \
             rapid trigger off on a key that should keep it. Use `wh keyset delete rt <index>` \
             to remove the whole keyset"
        );
    }
    let m = keyset::read_membership(s, kind)?;
    let sets = keyset::group(&m);
    let leaving: Vec<(u16, u8)> = usages
        .iter()
        .filter_map(|&u| sets.iter().find(|k| k.members.contains(&u)).map(|k| (k.index, u)))
        .collect();
    if leaving.is_empty() {
        bail!(
            "none of those keys is in an {} keyset; nothing to remove",
            kind_name(kind)
        );
    }
    let v = match value {
        Some(v) => v,
        None => global_ap_or_bail(s, &m, "--value")?,
    };
    let moving: Vec<u8> = leaving.iter().map(|&(_, u)| u).collect();
    let cleared = keyset::KeysetIndex::clear(kind);
    let plan = keyset::plan(s, &moving, &keyset::Change::ap(v), Some(cleared))?;
    announce_remove(out, kind, &leaving, Target::Ap(v), &plan)?;
    Ok(plan)
}
```

Decide deliberately what happens when the selection names a mix of keyset members and free keys. The code above **silently drops the free keys**, since they are already outside every keyset and have nothing to leave. That is the intended behaviour, but it must be visible: print one line naming any selected key that was already free, so the operator is never left thinking a key was moved when it was not. `announce_steal`'s "enrolling free key(s)" line exists for exactly this reason on the create path and was added after a review found the silent case. Do not repeat that mistake here.

Then `announce_remove`, beside `announce_delete`. One line per removed key, naming the keyset it leaves, its current value and the value it returns to. Keys may leave different keysets in one command, so the keyset is named per key and not in a header:

```
ap: removing w from keyset 3, 1.20mm to 2.00mm
ap: removing s from keyset 5, 0.80mm to 2.00mm
```

and, when the selection also named keys that were already outside every keyset, one further line so that case is never silent:

```
ap: a,d were already in no keyset, left alone
```

Take each key's current value from `plan.before()`, the same source `announce_delete` uses through `describe_member`, not from a second read.

- [ ] **Step 5: Add the dispatch arm**

In `crates/wh-cli/src/run.rs`, beside the `KeysetWhat::Delete` arm, following its exact shape:

```rust
        KeysetWhat::Remove { kind, keys, value, dry_run } => {
            let kind = crate::keyset::kind_of(kind);
            let value = value.map(mm).transpose()?;
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            with_session(|s| {
                let usages = resolve_keys(s, &keys, store)?;
                let plan = crate::keyset::remove(&mut out, s, kind, &usages, value)?;
                if dry_run {
                    return print_frames(&mut out, &plan.frames());
                }
                auto_backup(s, store, "keyset remove")?;
                wh_device::keyset::apply(s, &plan)?;
                crate::keyset::verify_write(&mut out, s, kind, "remove", &plan)
            })
        }
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -p wh-cli --test keyset keyset_remove_ap`
Expected: PASS.

- [ ] **Step 7: Add the three tests that pin the edges**

Each must assert a value or an exact message, never merely that an error occurred:

1. `keyset_remove_rt_is_refused_as_unmeasured`: `wh keyset remove rt --keys n` exits non-zero and stderr contains `not measured`. Without this, the rapid trigger guard could be deleted and nothing would notice.
2. `keyset_remove_refuses_keys_that_are_in_no_keyset`: every named key free, exits non-zero, stderr contains `nothing to remove`.
3. `keyset_remove_leaves_the_keyset_alive_when_others_remain`: after removing one of three, `wh keyset list ap` over a board scripted to reflect the result still reports the keyset with its two remaining members and its value. This is the assertion that a removal did not collapse a keyset it should not have.

- [ ] **Step 8: Prove each test fails when the code is wrong**

For each of the four tests, mutate the specific behaviour it claims to check, run it, record the failure message, restore. Suggested mutations: make `remove` pass `usages` rather than `moving` to `plan`; drop the `KeysetIndex::clear` to `None`; delete the `Kind::Ap` guard; make `leaving.is_empty()` return `Ok` instead of bailing. **Name each mutation and its resulting failure in your report.**

- [ ] **Step 9: Run the three gates**

```bash
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

- [ ] **Step 10: Update the docs**

- `README.md`: add `wh keyset remove` to the command reference beside `wh keyset delete`, saying it takes named keys out and returns them to the global value, and that it is actuation point only for now because the rapid trigger case is unmeasured.
- `docs/tasks.md`: mark 2.21 partially done rather than closed. Record that the actuation point half shipped and matches the measured vendor template, and that the rapid trigger half waits on a capture of the vendor taking one key out of a rapid trigger keyset.

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "[feat] - Add wh keyset remove for actuation point keysets"
```

---

## Not yet planned: the rapid trigger half of 2.21

Deliberately absent. `wh keyset remove rt` needs to know what the removed key's MODE touch nibble becomes, and the corpus does not contain the vendor taking a single key out of a rapid trigger keyset. Two readings are both plausible and they differ in behaviour: nibble 1, rapid trigger off, matching `ks-delete-rt`'s whole-keyset delete; or nibble 2, rapid trigger following the board's global settings, matching the rule that a key outside a keyset follows the base.

A capture of that operation settles it in one step. When it exists, this plan gains Task 3 and Task 2's guard comes out. Until then the guard is the honest behaviour, and this section is the reason it is there.
