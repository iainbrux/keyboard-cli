# Reset To Base Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `wh keyset remove` mean "reset these keys to the board's base and take them out of every keyset", with no value flags, and guard whole-board selections behind a typed confirmation.

**Architecture:** Three layers, each independently testable. `wh-device` gains a way to read the board's base while ignoring named keys, since the keys being reset are usually the ones causing a disagreement. `wh-cli` gains one shared confirmation helper, built here because task 2.23 reuses it. Then `keyset::remove` is rewritten to use both.

**Tech Stack:** Rust 2021, four-crate workspace, `clap` derive, `ReplayTransport` for every test.

**Spec:** `docs/tasks.md` task 2.22 (the ruling), and task 2.23 for what the confirmation helper must also serve. Read both.

## Global Constraints

- **No em dashes or en dashes anywhere**, in code, comments, docs or commit messages. Use a comma, parentheses, a colon, or a full stop.
- Commit messages are one line, `[type] - Message`, types `feat`/`fix`/`docs`/`test`/`refactor`/`chore`. **No trailers of any kind.**
- **Never loosen `ReplayTransport`'s byte-for-byte frame matching** to make a test pass. If a fixture stops matching, the fixture changes.
- **Assert values, never coordinates.** An assertion naming a layout byte, key or index that only checks something exists there is a defect on this project.
- **Every test must fail when the code is wrong.** Mutate what the test claims to check, watch it fail, restore, and record the failure message. Reports without this are rejected.
- **No string assertion may be satisfiable by an error from something other than the code under test.** This project has twice shipped a test that passed because `contains("mismatch")` also matched the replay harness's own wording, and once because `contains("--press")` also matched clap's "unexpected argument" message. Assert text only `wh`'s own code can emit.
- Crate layering: `wh-proto` does no I/O, `wh-device` does nothing user-facing, `wh-cli` never encodes frames by hand.
- Comments default to one or two lines, four is the ceiling. **Never cite a task number** in a comment.
- Docs may state what was **measured** and may state inferences, but must say which.
- All gates must pass before any commit: `cargo test --workspace --no-fail-fast`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, and `python3 scripts/check-doc-repeats.py $(git ls-files '*.md' ':!research/*')`.

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `crates/wh-device/src/keyset.rs` | Reading the base while ignoring named keys | 1 |
| `crates/wh-cli/src/confirm.rs` | The shared typed confirmation, new file | 2 |
| `crates/wh-cli/src/main.rs` | Declaring the new module | 2 |
| `crates/wh-cli/tests/keyset.rs` | `run_wh` stdin handling, and the new behaviour | 2, 3 |
| `crates/wh-cli/src/keyset.rs` | `remove`'s new semantics and announcement | 3 |
| `crates/wh-cli/src/cli.rs` | Dropping `--value`, `--press`, `--release` from `Remove` | 3 |
| `crates/wh-cli/src/run.rs` | The `Remove` dispatch arm | 3 |
| `docs/tasks.md`, `README.md` | Closing 2.22 | 3 |

---

### Task 1: Read the base while ignoring named keys

**Files:**
- Modify: `crates/wh-device/src/keyset.rs`, beside `global_ap` and `global_rt`
- Test: the same file's unit test block

**Interfaces:**
- Consumes: `Membership` with its `kind` and `entries: Vec<(u8, u16)>`, `summarize<T: PartialEq>(Vec<T>) -> Global<T>`, `ops::read_layout_value`, all already in this file.
- Produces:
  - `pub fn global_ap_excluding<T: Transport>(s: &mut Session<T>, m: &Membership, exclude: &[u8]) -> Result<Global<Um>, DeviceError>`
  - `pub fn global_rt_excluding<T: Transport>(s: &mut Session<T>, m: &Membership, exclude: &[u8]) -> Result<Global<(Um, Um)>, DeviceError>`

**Why this exists.** `wh keyset remove` resets keys to the board's base, and the base is what the keys outside every keyset hold. The keys being reset are very often exactly the ones that make that reading disagree: a stray key at 1.10mm against 57 keys at 2.00mm is both the odd one out and the thing you are trying to fix. Excluding the selection is what lets the command work with no flag.

- [ ] **Step 1: Write the failing tests**

Add to the unit test block in `crates/wh-device/src/keyset.rs`, alongside the existing `global_ap` tests. Follow those tests' fixture style exactly.

Three tests, each pinning a value rather than a shape:

1. `global_ap_excluding_ignores_the_named_keys`: a board where `w` holds 1100 and three other free keys hold 2000. Excluding `w` must return `Global::Agreed(Um(2000))`. Without the exclusion the same board returns `Split`, so assert the un-excluded call returns `Split` in the same test, which is what proves the parameter is doing the work rather than the fixture being agreed anyway.
2. `global_ap_excluding_still_splits_when_the_rest_disagree`: two free keys at 2000, one at 1500, one at 1100, excluding only the 1100. Must return `Global::Split` whose counts name **both** 2000 and 1500 with their key counts. Assert the values and counts, not just the variant.
3. `global_ap_excluding_reports_none_when_every_free_key_is_excluded`: all free keys named in `exclude`. Must return `Global::NoneOutsideAKeyset`.

Add the rapid trigger mirror of test 1 for `global_rt_excluding`, pinning `(Um, Um)` values.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p wh-device global_ap_excluding`
Expected: FAIL to compile, "cannot find function `global_ap_excluding`".

- [ ] **Step 3: Implement**

Add both functions beside their existing counterparts. Each is the existing body with one added skip:

```rust
/// The board's base actuation point, read from `0x04` of every key holding no actuation point
/// membership, ignoring any key in `exclude`. Callers resetting keys to the base pass those keys
/// here: they are frequently the reason the remaining keys disagree.
pub fn global_ap_excluding<T: Transport>(
    s: &mut Session<T>,
    m: &Membership,
    exclude: &[u8],
) -> Result<Global<Um>, DeviceError> {
    if m.kind != Kind::Ap {
        return Err(DeviceError::KeysetKindMismatch {
            expected: Kind::Ap,
            found: m.kind,
        });
    }
    let mut values = Vec::new();
    for &(usage, membership) in &m.entries {
        if membership != 0 || exclude.contains(&usage) {
            continue;
        }
        values.push(Um(ops::read_layout_value(s, usage, layout::AP)?));
    }
    Ok(summarize(values))
}
```

Then rewrite the existing `global_ap` as a one-line call to it with an empty slice, so the two cannot drift, and do the same for `global_rt`/`global_rt_excluding`. **Keep `global_ap` and `global_rt` public with their current signatures**: `create` and `delete` call them and this task does not touch those.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p wh-device excluding`
Expected: PASS, and every pre-existing `global_ap`/`global_rt` test still passes untouched.

- [ ] **Step 5: Prove the tests fail when the code is wrong**

Mutate `exclude.contains(&usage)` to `false` so the parameter is ignored, run the suite, and record which tests fail and with what message. Restore and confirm green. Then mutate it to `!exclude.contains(&usage)` and record that too, since an inverted filter is the likelier bug and test 1's un-excluded assertion is what should catch it.

- [ ] **Step 6: Run the gates and commit**

```bash
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add -A
git commit -m "[feat] - Read the board base while ignoring named keys"
```

---

### Task 2: The shared typed confirmation

**Files:**
- Create: `crates/wh-cli/src/confirm.rs`
- Modify: `crates/wh-cli/src/main.rs` to declare the module
- Modify: `crates/wh-cli/tests/keyset.rs`, the `run_wh` helper
- Test: `crates/wh-cli/src/confirm.rs` unit tests

**Interfaces:**
- Produces: `pub(crate) fn confirm(out: &mut impl Write, prompt: &str, input: &mut impl BufRead) -> Result<bool>`. Takes its reader so it is testable without a subprocess; the caller passes `std::io::stdin().lock()`.

**The rule, from 2.22 and 2.23.** Print the prompt, read one line, trim it, lowercase it, and return true only if it equals `yes`. `y`, `ye`, `yess` and everything else are false. EOF is false. This guards the two commands that destroy every keyset on the board, so it is the one piece of code whose whole job is to be hard to get past by accident: **build it once and share it**.

- [ ] **Step 1: Write the failing tests**

In `crates/wh-cli/src/confirm.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The exact set the operator ruled on: any capitalisation of the whole word passes, and no
    /// prefix or extension of it does. Table-driven so a rewrite that accepted a prefix, or that
    /// compared case-sensitively, fails on the specific input it got wrong rather than on a
    /// single representative case.
    #[test]
    fn confirm_accepts_only_the_whole_word_in_any_case() {
        for (input, want) in [
            ("yes\n", true),
            ("YES\n", true),
            ("Yes\n", true),
            ("yEs\n", true),
            ("  yes  \n", true),
            ("y\n", false),
            ("ye\n", false),
            ("yess\n", false),
            ("yes please\n", false),
            ("no\n", false),
            ("\n", false),
            ("", false),
        ] {
            let mut out = Vec::new();
            let got = confirm(&mut out, "destroy everything?", &mut input.as_bytes()).unwrap();
            assert_eq!(got, want, "input {input:?} should give {want}");
        }
    }

    /// The prompt reaches the operator, and reaches the writer the caller passed rather than
    /// being printed directly, so a caller can capture it.
    #[test]
    fn confirm_writes_the_prompt_it_was_given() {
        let mut out = Vec::new();
        confirm(&mut out, "keysets 2, 7 will cease to exist", &mut "no\n".as_bytes()).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("keysets 2, 7 will cease to exist"),
            "got: {text}"
        );
    }
}
```

The empty-string case is EOF and must return false, not error.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p wh-cli --bin wh confirm`
Expected: FAIL to compile, no such module.

- [ ] **Step 3: Implement**

```rust
//! The typed confirmation guarding the two commands that destroy every keyset on the board.
//! One implementation, shared: two copies would drift, and the laxer one would win by accident.

use anyhow::Result;
use std::io::{BufRead, Write};

/// Prints `prompt`, reads one line, and returns true only for the whole word `yes` in any case.
/// A prefix like `y`, an extension like `yess`, anything else, and EOF are all false.
pub(crate) fn confirm(out: &mut impl Write, prompt: &str, input: &mut impl BufRead) -> Result<bool> {
    writeln!(out, "{prompt}")?;
    write!(out, "type yes to continue: ")?;
    out.flush()?;
    let mut line = String::new();
    if input.read_line(&mut line)? == 0 {
        return Ok(false);
    }
    Ok(line.trim().to_ascii_lowercase() == "yes")
}
```

Declare `mod confirm;` in `crates/wh-cli/src/main.rs` beside the other modules.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p wh-cli --bin wh confirm`
Expected: PASS, both tests.

- [ ] **Step 5: Stop a prompt from ever hanging the test suite**

`run_wh` in `crates/wh-cli/tests/keyset.rs` currently calls `.output()` without configuring stdin, so a child that reads stdin inherits the test runner's. Once Task 3 adds a prompt, a test that reaches it could block forever.

Set stdin explicitly to null in `run_wh`, so any unexpected prompt gets EOF and is refused rather than hanging:

```rust
.stdin(std::process::Stdio::null())
```

Then add a sibling that feeds a line, for the tests that need to answer:

```rust
/// `run_wh` with a line on stdin, for the commands that ask for a typed confirmation.
fn run_wh_stdin(
    args: &[&str],
    replay: &std::path::Path,
    config_home: &std::path::Path,
    input: &str,
) -> std::process::Output {
    use std::io::Write;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_wh"))
        .env("WH_REPLAY", replay)
        .env("XDG_CONFIG_HOME", config_home)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
```

Apply the same `Stdio::null()` change to `run_wh` in `crates/wh-cli/tests/dump.rs` if that file has its own copy. Check before assuming; if it shares the helper, change it once.

- [ ] **Step 6: Prove the tests fail when the code is wrong**

Three mutations, each recorded with its failure message: drop the `to_ascii_lowercase`, which must fail on `YES`; change the comparison to `starts_with("yes")`, which must fail on `yess`; and return `Ok(true)` on the EOF branch, which must fail on the empty-string case. Restore after each.

- [ ] **Step 7: Run the gates and commit**

```bash
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add -A
git commit -m "[feat] - Add the shared typed confirmation for whole-board writes"
```

---

### Task 3: `wh keyset remove` resets to the base

**Files:**
- Modify: `crates/wh-cli/src/cli.rs`, the `KeysetWhat::Remove` variant
- Modify: `crates/wh-cli/src/keyset.rs`, `remove` and `announce_remove`
- Modify: `crates/wh-cli/src/run.rs`, the `Remove` dispatch arm
- Modify: `docs/tasks.md`, `README.md`
- Test: `crates/wh-cli/tests/keyset.rs`

**Interfaces:**
- Consumes: `global_ap_excluding` and `global_rt_excluding` from Task 1; `confirm` from Task 2; and, already present, `keyset::plan`, `keyset::KeysetIndex::clear`, `keyset::Change::ap`/`rt_off`, `describe_member`, `kind_name`, `verify_write`, `resolve_keys`, `auto_backup`, `print_frames`.
- Produces: `keyset::remove<T: Transport>(out: &mut impl Write, s: &mut Session<T>, kind: Kind, usages: &[u8]) -> Result<keyset::WritePlan>`. **The `value` and `rt` parameters are gone.**

**What changes, from 2.22.**

The command's job is a destination, not a transition. So the "none of those keys is in an ap keyset; nothing to remove" refusal is **deleted**, and every named key is passed to `plan`, not only the ones currently in a keyset. A key already at the base with no membership gets nothing written, because `plan`'s skip rule already suppresses a write where no owned value differs.

`--value`, `--press` and `--release` are removed from the `Remove` clap variant entirely, along with the `Kind::Ap` refusal of `--press`/`--release` in the dispatch arm and its test, which has nothing left to refuse.

**Where the base comes from, in this order.** Call `global_ap_excluding(s, &m, usages)` (or the rapid trigger pair for `Kind::Rt`). Then:

- `Global::Agreed(v)`: use `v`.
- `Global::Split(counts)`: **refuse**, naming each value and its key count, and say to include those keys in the selection so they are reset too. Do not fall back to the constant: a contradictory signal from the board is not the same as no signal, and overriding it would invent a value.
- `Global::NoneOutsideAKeyset`: use **`Um(2000)`**, 2.00mm. This is a chosen default for the one unanswerable case, not a measured factory setting. Name it as a constant with that written in a comment of no more than four lines.

**The announcement has three cases**, since the current "free key(s) d left alone, already in no ap keyset" line becomes false:

```
ap: removing w from keyset 3, 1.20mm to 2.00mm
ap: returning n to 2.00mm, already in no ap keyset
ap: h already at 2.00mm in no ap keyset, nothing to do
```

Take each key's current value from `plan.before()`, the same source `announce_delete` uses.

**The confirmation.** When the selection covers every key in the board's matrix, print the warning and require the typed `yes` before writing. The trigger is `usages.len() == m.entries.len()`, the resolved selection against the membership read the function already performs, **not** the literal string `all`: spelling out all 68 usages must reach it too. `--dry-run` does not prompt, since it writes nothing. There is no bypass flag. The warning must name every keyset that will cease to exist.

- [ ] **Step 1: Write the failing tests**

Add to `crates/wh-cli/tests/keyset.rs`. Six tests, each pinning values or exact messages `wh`'s own code emits:

1. `keyset_remove_returns_a_free_key_to_the_base`: `w` free at 1100, three other free keys at 2000, no keysets. `wh keyset remove ap --keys w --dry-run` writes `w` at 2000 with `0xFF = 0`, asserted by exact full-sequence frame equality, and prints `ap: returning w to 2.00mm, already in no ap keyset`. **This is the case the whole task exists for**: today it refuses.
2. `keyset_remove_writes_nothing_for_a_key_already_at_the_base`: `w` free at 2000 alongside other free keys at 2000. The frame list is empty and stdout contains `nothing to do`.
3. `keyset_remove_takes_the_base_from_the_keys_it_is_not_resetting`: `w` free at 1100 and every other free key at 2000, so an un-excluded reading would `Split`. The command must succeed and write 2000. Without Task 1 this is the test that fails.
4. `keyset_remove_refuses_when_the_remaining_free_keys_disagree`: `w` at 1100 being reset, but `a` at 1500 and others at 2000 left out. Exits non-zero, and stderr names both `1.50mm` and `2.00mm` with their counts.
5. `keyset_remove_uses_the_base_constant_when_no_free_key_is_left`: every key in a keyset and every key selected, so nothing is left to read. Writes 2000.
6. `keyset_remove_over_the_whole_board_requires_a_typed_yes`: two runs over the same board with `run_wh_stdin`. With `no\n` it exits without writing and stderr or stdout names the keysets that would cease to exist. With `yes\n` it proceeds. Assert on the warning text `wh` emits, never on a clap or replay-harness message.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p wh-cli --test keyset keyset_remove`
Expected: compile failure first, since `remove`'s signature changes; then the new tests fail.

- [ ] **Step 3: Change the clap variant**

Delete `value`, `press` and `release` from `KeysetWhat::Remove`, leaving `kind`, `keys` and `dry_run`. Update the doc comment to say the command returns the keys to the board's base value, so `--help` stops implying a choosable value.

- [ ] **Step 4: Rewrite `remove` and `announce_remove`**

Drop the `leaving.is_empty()` bail. Compute `leaving` as before, for the announcement only, and pass the **full** `usages` to `plan`. Resolve the base through `global_ap_excluding`/`global_rt_excluding` with the three-way match above. Add the whole-matrix confirmation before returning the plan.

`remove` takes `out`, so the confirmation prompt writes there; pass `std::io::stdin().lock()` from the dispatch arm as the reader.

- [ ] **Step 5: Update the dispatch arm**

In `crates/wh-cli/src/run.rs`, remove the `--press`/`--release` refusal and the `mm`/`resolve_rt_override` conversions, which have nothing left to convert, and call `remove` with its new signature.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p wh-cli --test keyset keyset_remove`
Expected: all pass, including the rewritten inverted test named in Step 8.

- [ ] **Step 7: Run the whole suite and fix what the change breaks**

Run: `cargo test --workspace --no-fail-fast`

`keyset_remove_ignores_a_free_key_selected_alongside_a_member` asserts that free keys are dropped from the plan. They are now included. **Rewrite it to assert they are written, do not delete it**: it is the only coverage of that path, and it was added because the mutation it catches was invisible to every other test. `keyset_remove_ap_refuses_press_and_release` has nothing left to refuse and should be deleted along with the flags.

- [ ] **Step 8: Prove the tests fail when the code is wrong**

Five mutations, each named in your report with the test that caught it and its message: pass `leaving`'s keys to `plan` instead of the full `usages`; make the `Split` arm fall back to the constant instead of refusing; change the constant to `Um(2500)`; make the confirmation trigger on the literal string `all` rather than the resolved length; and make `confirm`'s result be ignored so the write proceeds regardless.

- [ ] **Step 9: Run the gates**

```bash
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
python3 scripts/check-doc-repeats.py $(git ls-files '*.md' ':!research/*')
```

- [ ] **Step 10: Update the docs**

- `README.md`: `wh keyset remove` now resets to the board's base and takes no value flags. Say what happens when the remaining free keys disagree, and that a whole-board selection asks for confirmation. Add the two-command sequence it replaces, since that is the motivating case.
- `docs/tasks.md`: close 2.22, recording that the base constant is a chosen default for the no-signal case and not a measured factory setting.

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "[feat] - Make wh keyset remove reset keys to the board base"
```
