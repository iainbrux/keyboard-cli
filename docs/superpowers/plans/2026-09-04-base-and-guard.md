# Guard `wh set ap --keys all`, then add `--base` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop `wh set ap --keys all` destroying every keyset without warning, then give `wh` a way to set the board's base actuation point at all.

**Architecture:** Three tasks in a forced order. The prompt moves to stderr first, while it has one caller, so the second caller inherits the corrected stream rather than repeating the choice. Then the guard goes on `wh set ap --keys all`, which ships today unprotected. Then `--base`, which is `keyset::plan` over the free keys with no membership, a shape now measured from the vendor.

**Tech Stack:** Rust 2021, four-crate workspace, `clap` derive, `ReplayTransport` for every test.

**Spec:** `docs/tasks.md` tasks 2.25 (the stream), 2.23 (the guard and `--base`). Read both.

## Global Constraints

- **No em dashes or en dashes anywhere**, in code, comments, docs or commit messages.
- Commit messages are one line, `[type] - Message`, types `feat`/`fix`/`docs`/`test`/`refactor`/`chore`. **No trailers of any kind.**
- **Never loosen `ReplayTransport`'s byte-for-byte frame matching** to make a test pass. If a fixture stops matching, the fixture changes.
- **Assert values, never coordinates.**
- **Every test must fail when the code is wrong.** Mutate what the test claims to check, watch it fail, restore, and record the message the mutation ACTUALLY produced, not the one you predicted. Two reports on this project have repeated a predicted message that did not match the panic.
- **No string assertion may be satisfiable by something other than the code under test.** Three shapes have bitten here and `CLAUDE.md` lists them: a string another component also emits, a string true only of its own fixture, and an assertion too narrow for the line it guards. Read that section before writing a test.
- **Do not run any automated line-rewrap over a Markdown file.** It has broken this repo's docs three times, twice by collapsing checklist items whose continuation lines lost their indent. Fix a long line by hand or leave it.
- Crate layering: `wh-proto` does no I/O, `wh-device` does nothing user-facing, `wh-cli` never encodes frames by hand.
- Comments default to one or two lines, four is the ceiling, and never cite a task number, review round or chunk.
- Docs may state what was **measured** and may state inferences, but must say which.
- Gates before any commit: `cargo test --workspace --no-fail-fast`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, and `python3 scripts/check-doc-repeats.py $(git ls-files '*.md' ':!research/*')`.

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `crates/wh-cli/src/keyset.rs` | `confirm_whole_board_remove`'s writer | 1 |
| `crates/wh-cli/src/run.rs` | Which stream the prompt gets; the `SetWhat::Ap` guard; the `--base` arm | 1, 2, 3 |
| `crates/wh-cli/src/confirm.rs` | Unchanged in signature, reused by both callers | 2 |
| `crates/wh-cli/src/cli.rs` | The `--base` flag on `SetWhat::Ap` | 3 |
| `crates/wh-cli/tests/keyset.rs`, `tests/dump.rs` | The new behaviour | 1, 2, 3 |
| `docs/tasks.md`, `README.md` | Closing 2.25 and 2.23 | 1, 3 |

---

### Task 1: Move the confirmation prompt to stderr

**Files:**
- Modify: `crates/wh-cli/src/keyset.rs` (`remove` and `confirm_whole_board_remove`)
- Modify: `crates/wh-cli/src/run.rs` (the `KeysetWhat::Remove` arm)
- Test: `crates/wh-cli/tests/keyset.rs`

**Interfaces:**
- Consumes: `crate::confirm::confirm(out: &mut impl Write, prompt: &str, input: &mut impl BufRead) -> Result<bool>`, unchanged.
- Produces: `keyset::remove` takes one further writer for the prompt, so the announcement and the prompt can go to different streams. Task 2 uses the same pattern.

**Why now, and why first.** Measured: `wh keyset remove ap --keys all > log.txt` puts both prompt lines in the file and then blocks on stdin with nothing on screen. Writing the prompt to stderr fixes that for every redirection, needs no `is_terminal` call and so no platform-dependent behaviour on the Windows target, and leaves the piped-stdin mechanism untouched. `wh` already sends its `transport:` line to stderr and the project's own safety rule tells operators to read it. Doing this before Task 2 means the second caller inherits the right stream rather than repeating the decision.

The announcement's per-key lines and `--dry-run`'s frame hex stay on **stdout**: they are data. The prompt is a diagnostic.

- [ ] **Step 1: Write the failing test**

Change `keyset_remove_over_the_whole_board_requires_a_typed_yes` in `crates/wh-cli/tests/keyset.rs` so the two prompt assertions read stderr instead of stdout. Both are currently on `decline_stdout`: the keyset line and the value line. Leave every announcement assertion on stdout. Update the comment there that says the prompt is on stdout.

Add one new test, `keyset_remove_prompt_goes_to_stderr_not_stdout`, which asserts the prompt text is in stderr **and** `!decline_stdout.contains("type yes to continue")`. The negative half is what stops a future change sending it to both.

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p wh-cli --test keyset keyset_remove_over_the_whole_board keyset_remove_prompt_goes_to_stderr`
Expected: both FAIL, the prompt text still being on stdout.

- [ ] **Step 3: Thread a second writer**

Give `keyset::remove` a further parameter for the prompt's writer, and pass it to `confirm_whole_board_remove`. In `run.rs`'s `KeysetWhat::Remove` arm, pass the locked stdout for the announcement as today and a locked stderr for the prompt.

Do not reach for `eprintln!` inside `confirm_whole_board_remove`: the writer stays a parameter, which is what makes its unit tests possible without a subprocess. The two unit tests in `crates/wh-cli/src/keyset.rs` inject their own `Vec<u8>` and must keep working untouched.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p wh-cli --test keyset`
Expected: all pass, including the two `confirm_whole_board_remove` unit tests unchanged.

- [ ] **Step 5: Prove the tests fail when the code is wrong**

Three mutations, each recorded with the message it actually produced: send the prompt to the stdout writer again; send it to both; and drop the `!contains` half of the new test's assertion and confirm the remaining half no longer catches the both-streams case. Restore after each.

- [ ] **Step 6: Run the gates and update the docs**

Close 2.25 in `docs/tasks.md`, recording that the hazard was measured rather than supposed. Note in `README.md` where the prompt appears, since a user redirecting output needs to know.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "[fix] - Send the whole-board confirmation to stderr so redirection cannot hide it"
```

---

### Task 2: Guard `wh set ap --keys all`

**Files:**
- Modify: `crates/wh-cli/src/run.rs` (the `SetWhat::Ap` arm, around line 747)
- Test: `crates/wh-cli/tests/dump.rs`

**Interfaces:**
- Consumes: `crate::confirm::confirm` and the stderr writer pattern from Task 1; `ap_membership_for`, `ApMembership::Split`, `keyset::plan`, all already in the arm.

**The hazard, and it ships today.** `wh set ap --keys all --set X` moves all 68 keys into one new keyset. Every existing keyset loses all its members, so **every keyset on the board ceases to exist**. Nothing warns. That is measured vendor behaviour (`ks-value-over-all` writes `0xFF = 3` to all 68) and the configurator supports it, so the command stays; what it must not do is stay silent.

**The rules, all operator rulings, and identical to `remove`'s guard:** print the warning, read one line, trim it, lowercase it, and proceed only on `yes`. `y`, `ye`, `yess` and everything else refuse. EOF refuses. `--dry-run` does not prompt. There is no bypass flag. **Trigger on the resolved selection covering the board's matrix, not on the literal string `all`.**

**Reuse `crate::confirm::confirm`.** Do not write a second acceptance check. Two copies will drift and the laxer one wins by accident.

The warning names what is lost and points at the alternative:

```
ap: --keys all moves every key into one new keyset, keyset 11
    keysets 2, 7, 8, 9 will cease to exist, their members absorbed
    to change the board's base instead, leaving keysets alone: wh set ap --base 1.50
```

The third line names a flag Task 3 adds. Write it anyway: this task and Task 3 land together, and a warning that suggests nothing is a worse warning.

- [ ] **Step 1: Write the failing tests**

In `crates/wh-cli/tests/dump.rs`, using the `run_wh_stdin` pattern from `tests/keyset.rs` (add an equivalent helper here if `dump.rs` lacks one; check first, it has its own `run_wh`):

1. `set_ap_over_the_whole_board_requires_a_typed_yes`: a board with two keysets, `--keys all --set 1.5`. With `no` on stdin it exits without writing and the warning names both keysets and the `--base` alternative. With `yes` it proceeds. Assert on text only `wh` emits.
2. `set_ap_over_the_whole_board_names_every_keyset_that_will_cease_to_exist`: three keysets, all three named.
3. `set_ap_over_the_whole_board_does_not_prompt_on_dry_run`: `--dry-run` with empty stdin still prints frames and exits zero.
4. `set_ap_over_a_partial_selection_does_not_prompt`: a selection short of the whole matrix writes with no prompt.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p wh-cli --test dump set_ap_over_the_whole_board set_ap_over_a_partial`
Expected: the first three FAIL, no prompt existing yet. The fourth passes already, which is the point: it is the regression guard for not over-triggering.

- [ ] **Step 3: Implement**

In the `SetWhat::Ap` arm, after `ap_membership_for` and before `auto_backup`, when the selection covers the whole matrix and this is not a dry run, build the warning and call `confirm` with a locked stderr. Refuse with a message naming the command, in the shape `remove`'s refusal uses.

Compare `usages.len()` against the membership read the arm already performs, the same trigger `remove` uses. Do not re-read the matrix and do not inspect the `--keys` string.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p wh-cli --test dump`

- [ ] **Step 5: Prove the tests fail when the code is wrong**

Four mutations, each with its actual message: disable the guard entirely; trigger on the literal string `all` instead of the resolved length; ignore `confirm`'s result and write regardless; and prompt on `--dry-run` too. Restore after each.

- [ ] **Step 6: Run the gates and commit**

```bash
git add -A
git commit -m "[feat] - Ask before wh set ap --keys all destroys every keyset"
```

---

### Task 3: `wh set ap --base <mm>`

**Files:**
- Modify: `crates/wh-cli/src/cli.rs` (`SetWhat::Ap`)
- Modify: `crates/wh-cli/src/run.rs` (the `SetWhat::Ap` arm)
- Modify: `docs/tasks.md`, `docs/keysets.md`, `README.md`
- Test: `crates/wh-cli/tests/dump.rs`

**Interfaces:**
- Consumes: `wh_device::keyset::read_membership`, `Change::ap(Um)`, `keyset::plan(s, usages, change, None)`, `keyset::apply`, `verify_write_as`, `auto_backup`, `print_frames`, `mm`.

**What the vendor does, measured 2026-09-04 in `captures/ks-set-global-ap.jsonl`.** Changing the configurator's GLOBAL ACTUATION POINT field sent 75 write frames carrying 413 records to **59 keys**, which is every key outside a keyset on a 68-key board holding 9 members. Per key: `0x08 = 16` twice (its template puts MODE in steps 1 and 3), `0x04 = 1950` the new base, `0x14 = 100` and `0x15 = 100` echoed unchanged, `0x16 = 0` and `0x17 = 0` echoed unchanged. **No `0xFF` record anywhere**, and **not one of the nine keyset members was written**.

So setting the base is: the ordinary per-key template over the free keys, with membership untouched. `keyset::plan(s, free_keys, &Change::ap(v), None)` produces exactly that, minus `wh`'s two documented divergences, which are that it never writes `0x16`/`0x17` and emits MODE once rather than twice.

**The flag.** `--base <mm>` on `SetWhat::Ap`. It takes no `--keys` and refuses alongside `--set`: it names the board, not a selection. Refuse both combinations up front with a message saying which flag to use. `--base` is deliberately not `--mm`: that name is reserved for the configurator's `"MM" CUSTOM VALUE`, a different setting `docs/tasks.md` 2.10 exists to stop being confused with this one.

**What it selects.** Every key holding membership `0` in the actuation point layout, read from `read_membership`. If every key is in a keyset there is nothing to write: refuse, saying so, rather than writing nothing and reporting success.

- [ ] **Step 1: Write the failing tests**

In `crates/wh-cli/tests/dump.rs`:

1. `set_ap_base_writes_every_free_key_and_no_membership`: a board with a two-key keyset and four free keys. `wh set ap --base 1.95 --dry-run` writes `0x04 = 1950` to the four free keys and **no `0xFF` record at all**, asserted by exact full-sequence frame equality. The keyset's two members must appear nowhere in the frames, which is the half a wrong selection would break.
2. `set_ap_base_refuses_alongside_keys`: `--base 1.5 --keys w` exits non-zero, stderr names both flags.
3. `set_ap_base_refuses_alongside_set`: `--base 1.5 --set 1.2` exits non-zero.
4. `set_ap_base_refuses_when_every_key_is_in_a_keyset`: exits non-zero with a message saying there is no key outside a keyset to write.
5. `set_ap_base_does_not_prompt`: `--base` is not a whole-board keyset write, so it must not reach Task 2's confirmation even though it touches most keys.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p wh-cli --test dump set_ap_base`
Expected: FAIL, `--base` not existing yet.

- [ ] **Step 3: Add the flag and the arm**

Add `base: Option<f64>` to `SetWhat::Ap`, with `--set` becoming optional so the two can be checked against each other. Refuse `--base` with `--keys` or `--set`, and refuse neither being given. In the arm, when `--base` is present: read membership, collect every usage with membership `0`, refuse if empty, then `plan(s, &free, &Change::ap(v), None)`, announce, back up, apply and verify.

The announcement should say what it is doing and how many keys it covers, for example `ap base: 59 keys outside every keyset move to 1.95mm, keysets untouched`.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p wh-cli --test dump set_ap_base`

- [ ] **Step 5: Prove the tests fail when the code is wrong**

Four mutations with their actual messages: pass `Some(index)` for membership instead of `None`; select every key rather than only the free ones; drop the `--set` refusal; drop the empty-selection refusal. Restore after each.

- [ ] **Step 6: Update the docs**

- `README.md`: `wh set ap --base` and how it differs from `--keys all`. This is the pair a user will confuse, so put them next to each other.
- `docs/keysets.md`: a section recording `ks-set-global-ap`, with the measured figures above. Say plainly that it is measured, and that `wh` sends fewer records by its two documented divergences.
- `docs/tasks.md`: close 2.23.
- `capture/README.md` and `docs/keysets.md`'s corpus line: 37 files, 6076 frames.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "[feat] - Add wh set ap --base to set the board's base actuation point"
```
