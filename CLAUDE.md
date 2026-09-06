# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# wh

A Rust CLI that reads and writes actuation-point and rapid-trigger settings on a Wallhack K-001
hall-effect keyboard over raw HID. It speaks the Sparklink Playjoy vendor protocol, the same one
terminal.wallhack.com uses. The goal is close to 1:1 interoperability with that configurator.

## Architecture

Four crates, and the layering is the point. Keep changes in the right one.

| Crate | Owns | Never does |
|---|---|---|
| `wh-proto` | Frame codec, command encoders and parsers, key names, selectors | Any I/O |
| `wh-device` | `Transport`, `Session`, high-level operations, hidapi | Anything user-facing |
| `wh-config` | JSON snapshots, the key-group store | Talk to the device |
| `wh-cli` | The clap surface, output formatting | Encode frames by hand |

The hardware transport is `#[cfg(windows)]`. The binary cross-compiles to
`x86_64-pc-windows-gnu` and is driven from WSL through `bin/wh`.

**How a command travels.** `wh-cli` parses, resolves a key selector against the board's real key
matrix (read from the device, not assumed), and calls a `wh-device` operation. That operation builds
`KeyRecord`s via `wh-proto` encoders, sends them through a `Transport`, and reads back to verify.
`Transport` has two implementations: `HidTransport` for the real board, and `ReplayTransport`, which
matches every outgoing frame against a scripted JSONL capture byte for byte. Every test in the repo
runs against the second one, which is why the matching must never be loosened.

**Writes are read-modify-write.** A settings write reads the key's current MODE first, so that
changing one thing cannot silently clear another. `wh set ap`'s live path is `keyset::plan` with
`Change::ap`, whose `apply_touch` promotes touch nibble 0 to 1 and deliberately leaves 1, 2, 3, 4
and unknown nibbles alone; `ops::ap_records` still does the same but is no longer on that path.
`rt_records` preserves `RtContinuous`. `wh set rt --off` goes the same way, through `keyset::plan`
with `Change::rt_off` and a `0xFE` clear, so it resets the sensitivities and clears the keyset
membership the vendor clears; `ops::rt_off_records` and `ops::set_rt_off` are off that path too.
This is the single most important invariant in the codebase and it exists because clobbering a
nibble silently disables a feature the user set from the vendor UI.

**Only one process can hold the device.** The vendor HID collection (usage page `0xFFA0`) is
exclusive, so `wh` fails with `DeviceError::Busy` while terminal.wallhack.com has it open. This is
why there is no daemon and why long-running features are backlogged rather than built.

**`wh` caches no device state.** Every command reads live over HID, which is why it cannot show a
stale value where the web configurator can. The only exception is the read-modify-write window
above.

**The board can change under you, and it says so.** The keyboard's own AP and RT keys edit settings
without the host involved, and while that is happening the board stops being a keyboard at all: it
will not type until the key is pressed again. It announces both edges with an unsolicited `cmd 0x00`
sub-order `0xbe`, `be 00` entering and `be 01` leaving, and the vendor configurator ignores the
first and re-reads the whole board on the second. Measured, see `docs/protocol.md`. As of 3.4,
`Session` queues these edges instead of discarding them (`poll_event`/`pending_events`), and the
lock is measured input-only: reads and writes both work mid-lock, so hearing the edges is the only
detection there is.

**The board has four profiles and every per-key layout is per profile.** `cmd 0x00` payload
`70 0xFF` reads the active profile; `70 <index>` selects one. A snapshot belongs to the profile it
was taken on, which is why `wh restore` refuses a mismatch outright.

**Only seven of the 55 captures record which profile they were on**, so a value comparison between
two captures is invalid unless both sides are established, and for most pairs that cannot be done
from the frames at all. `layout-16-by-profile` measures only profile 1: it selects index `1` as its
last outbound frame and stops, so it contains no profile 2 read. That the two profiles held
different values **on 2026-09-04** is therefore measured on one side and corroborated on the other
by the operator's note, not measured on both. Do not flatten that to "measured". For 2026-08-28 both
sides are established from frames: `initial-load` reads index `0` and `profile-switch` selects index
`1`, and neither contains a key-record write, so the values each reads are the board's own.

`profile-switch` is the trap: it selects index `1` as its first frame, so despite sitting among the
2026-08-28 captures every read in it is profile 2. Seven of the ten Phase 1 files record no profile
at all, so "the others are profile 1" is itself the unestablished inference this paragraph warns
against.

## Commands

```bash
cargo test --workspace --no-fail-fast                     # the whole suite
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo build -p wh-cli --release --target x86_64-pc-windows-gnu   # the real binary
./bin/wh dump                                             # runs it, needs the board
```

All three gate commands must pass before any commit. Use `--no-fail-fast`: the plain form stops at
the first failing target and has masked a real result here.

The suite's size is deliberately not written down. It was stated as a number twice and rotted within
a day both times, and a stale count is worse than none: a reader who sees fewer tests than the file
claims cannot tell whether coverage was lost or the number was simply old.

`scripts/check-doc-repeats.py $(git ls-files '*.md' ':!research/*')` flags a phrase repeated back to
back in prose, which is what an edit appending text that was already there looks like. It found one
real defect that survived five rounds of review and has no false positives on the current tree. It
is deliberately narrow: line-length and short-line checks were measured here and produced 103 false
positives with no true ones.

Running one test, or one suite:

```bash
cargo test -p wh-device ap_records                     # by name substring
cargo test -p wh-proto --test golden -- --nocapture    # decodes captures/, prints its summary
cargo test -p wh-cli --test dump                       # end-to-end CLI over replay scripts
cargo test -p wh-cli --test keyset                     # the wh keyset tree, the largest suite
cargo test -p wh-cli --test socd                       # the wh socd tree
```

`--nocapture` matters for `golden`: without it cargo swallows the summary on a passing run and you
see only `ok`. There are four integration suites: `wh-proto/tests/golden.rs`, which decodes real
captured traffic; `wh-cli/tests/dump.rs`, which drives the real binary over scripted replays;
`wh-cli/tests/keyset.rs`, the largest and the only end-to-end cover of the `wh keyset` tree; and
`wh-cli/tests/socd.rs`, the same for `wh socd`.

**A `--test <name>` run is not evidence the crate compiles.** It does not build the bin target's
unit tests, so a change breaking one of those leaves `cargo test -p wh-cli --test dump` green while
`cargo test --workspace` fails with a compile error. Measured 2026-09-05, when a function signature
changed under two unit tests in `crates/wh-cli/src/keyset.rs` and the scoped run passed. Scoped runs
are for iterating; run the workspace before you believe a result.

## Safety rules, each one learned the hard way

**Never trust `WH_REPLAY` without checking it arrived.** `bin/wh` execs a Windows binary, and WSL
forwards nothing across that boundary unless `WSLENV` names it. A reviewer once performed a real
write to the operator's board believing it was a replay. Every command prints
`transport: replay|hardware` on stderr. Read that line.

**Never run `wh keys group` against the real user config** while checking examples. It has needed
manual cleanup twice. Redirect `XDG_CONFIG_HOME`, and note that does not cross the WSL boundary
either.

**Never loosen `ReplayTransport`'s byte-for-byte frame matching** to make a test pass. If a fixture
stops matching, the code changed under it and the fixture is what should change.

**Prove a destructive hardware test was undone, do not eyeball it.** `wh keyset list` showing the
right keysets is not evidence the board is back. Take `wh dump` before and after, and diff every key
on every field: actuation point, MODE, both sensitivities, both memberships. A 68-key restore that
looks right in the keyset list can still differ in a nibble nothing prints.

**Rebuild the cross-compiled binary before running the shim test.**
`bin_wh_shim_propagates_wh_replay_and_never_touches_hardware` executes the real `wh.exe`, so a stale
one makes that test pass or fail for reasons unrelated to the diff.

## Measure, never infer

This is the project's defining rule, and it exists because inference has been wrong repeatedly:

- The reply high bit, the MODE nibble meanings, and "nibble 0 discards the actuation point" were all
  overturned by hardware.
- Layout `0xFF` was recorded as read-but-never-written, from 1224 frames. It is host-written; the
  sample had simply never created a keyset.
- Layouts `0x16` and `0x17` were recorded as "never once observed non-zero". Same cause.
- An analysis script that sliced frame payloads as `bytes[4 : 4+len-2]` produced a confident, wrong
  protocol claim that reached the spec. The length byte counts the payload itself.

Practical consequences:

- Comments and docs may state what was **measured**. They may not state an inference as though it
  were measured. Say which it is.
- "Never observed" is a statement about a sample, not about the device.
- Pin a decoder against something known-good before trusting its output.
- If a brief contradicts what you measure, trust the measurement and say so.

## The review loop is lockstep, and this rule cannot be bypassed

Every task's diff gets an adversarial review, and **every fix round returns to the reviewer that
raised the findings, until that reviewer returns an explicit approve**. The loop ends when neither
side has anything further to raise, and nowhere earlier. Ordered by the operator on 2026-09-06 and
not subject to judgement calls:

- A fix round closed on the implementer's report alone is not closed.
- A controller spot-check (re-running one mutation, reading the diff) supplements the confirming
  pass and never substitutes for it. The reviewer that found a gap holds context about its subtler
  variant; a spot-check re-runs only the obvious one.
- This applies regardless of how small or mechanical the fix appears. The rule exists because the
  drift happened exactly there: coverage-gap fixes that looked too simple to send back, closed
  without the reviewer's confirmation, on the same branch where a reviewer's second look had been
  finding real defects in every round.
- Findings the controller fixes directly get the same confirming pass as findings an implementer
  fixes.

## Test discipline

A test that passes for the wrong reason is worse than no test. Establish that each test **fails when
the code is wrong**, not merely that it passes when the code is right. Mutate the thing the test
claims to check, watch it fail, restore, and say so in the report. Several tests in this repo were
found to be decorative exactly that way.

**Three shapes have bitten here, and they are not the same shape. Only the first reached `main`.**

*A string something else can also emit.* `contains("mismatch")` matches `ReplayTransport`'s own
"send mismatch" wording, so the test cannot tell a real readback mismatch from a broken fixture.
`contains("--press")` matches clap's "unexpected argument '--press' found", so it cannot tell the
code's refusal from the flag not existing. Both were caught by review and strengthened, and both
patterns are still live elsewhere in `crates/wh-cli/tests/`. Assert text only `wh`'s own code emits.

*A string true only of its own fixture.* A rapid trigger refusal named one cause for two different
board states, and the test pinning it was built on the one board where that cause was true. The
test could still fail on a wording change; what it could never do is catch the wrong-cause defect.
Ask which other boards reach the line, and whether the assertion still holds there.

*An assertion too narrow for the line it guards.* A confirmation prompt was asserted by one
substring, so half the prompt could be deleted with the suite green. Found by mutation, not shipped.

**And mutate one level up.** Unit tests proving a string is built correctly do not prove it reaches
the operator through the real command path. Those are different claims and need different tests.

**The commonest real MODE value is close to untested.** Counting the literal strings, the fixtures
write `0x18` 104 times against `0x10` once in `tests/keyset.rs`, and 38 against 8 in `tests/dump.rs`
(`0x0018` also appears and the grep misses it, so the real ratio is worse). Meanwhile a `wh dump` of
profile 2 on 2026-09-04 read `0x10` on all 68 keys, and `layout-16-by-profile` read it on 64 of 68
on profile 1 the same day. So a change touching MODE should add a case at `0x10`.

The fixtures are not fabricated, and an earlier draft of this paragraph said they were. In those
two files every `0x18` fixture sits on `w`, `a`, `s` or `d`, and those four keys really did hold
`0x18` on profile 1 on 2026-09-04. (`wh-device`'s own tests are not so confined.) They are a real
board shape, just not the common one.

## What the code does and what it says are separate claims

The frames can be right while the operator is told something false, and that defect survives review
because everything looks green. A `wh keyset remove` reported "nothing to do" while sending a frame
that turned rapid trigger off; the wire was correct and the sentence was not. It took three fix
rounds to kill, and the reason is worth knowing: each round's brief scoped a narrower case than the
defect covered. The first replaced the predicate, the second added the case where only the mode
moves, and the third finally reached the ordinary case where the value moves as well. The defect was
never reintroduced; it was three times under-scoped.

So when a write's announcement is built from anything other than the plan it is announcing, treat
that as a defect. `plan.value_records()` is the predicate for "did anything actually change for this
key", and comparing only the value a command owns will miss a MODE change the same write makes.

## Docs

| File | What it holds |
|---|---|
| `docs/tasks.md` | The live checklist. Start here |
| `docs/protocol.md` | The wire protocol |
| `docs/protocol-inventory.md` | Measured frame counts the protocol doc rests on |
| `docs/keysets.md` | Keyset semantics, measured 2026-08-29 and 2026-09-04 |
| `docs/backlog.md` | Unscheduled work, with what is known and unknown for each |
| `capture/README.md` | How to capture real device traffic |

Plan and spec files under `docs/superpowers/` are dated records of what was planned, not living
documents: a signature or example in one may no longer compile, and that is expected. Correct a
stale statement where code reads it (comments, README, `docs/*.md`); leave plan files as the record
of what was decided at the time. The one exception is a plan still being executed: its task briefs
are extracted from it, so a wrong byte in it flows into the next task, and it is corrected like any
live document while its branch is unmerged. The state is merge status and nothing else: a plan on
`main` is closed, a plan on an unmerged branch is live, no third case exists. The execution note
appended before merge ("Executed: <date>") records when execution finished and claims nothing
about the merge, which git states by itself; a plan on `main` missing one gets it as an ordinary
docs fix.

`captures/` holds real device traffic and is **gitignored**. It is the operator's own data. The
golden test (`cargo test -p wh-proto --test golden`) decodes every frame in it; a missing directory
is the normal state everywhere except the operator's machine.

## House style

- Commit messages: one line, `[type] - Message`. Types: `feat`, `fix`, `docs`, `test`, `refactor`,
  `chore`. No body. **No trailers of any kind, including `Co-Authored-By`.**
- **No em dashes or en dashes anywhere**, in code, comments, docs or commit messages. Use a comma,
  parentheses, a colon, or a full stop.
- Comments default to one or two lines; four is the ceiling and must earn it. Never cite a task
  number, review round or chunk number: they point at gitignored files and become dead pointers.
- When a change makes an existing statement false, fix it. Removed flags left in documentation and
  "there is no way to X" surviving the feature that added X have both happened repeatedly here, and
  are always found later by someone else.

## Licence

Apache-2.0, `Copyright 2026 Iain Brookes (brux)`. `research/` holds vendored third-party packages
under their own licences; `NOTICE` and `THIRD_PARTY_LICENSES.md` must keep covering whatever is
there. The keyboard and the Wallhack name are Wallhack's; this project is independent and unendorsed.
