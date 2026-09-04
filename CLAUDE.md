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
`Change::ap`, whose `apply_touch` promotes touch nibble 0 to 1 and deliberately leaves 1, 3, 4 and
unknown nibbles alone; `ops::ap_records` still does the same but is no longer on that path.
`rt_records` preserves `RtContinuous`. This is the single most important invariant in the codebase
and it exists because clobbering a nibble silently disables a feature the user set from the vendor
UI.

**Only one process can hold the device.** The vendor HID collection (usage page `0xFFA0`) is
exclusive, so `wh` fails with `DeviceError::Busy` while terminal.wallhack.com has it open. This is
why there is no daemon and why long-running features are backlogged rather than built.

**`wh` caches no device state.** Every command reads live over HID, which is why it cannot show a
stale value where the web configurator can. The only exception is the read-modify-write window
above.

**The board has four profiles and every per-key layout is per profile.** Two profiles were measured
carrying different actuation points, different sensitivities and different keysets at the same time.
Two consequences bite constantly. A snapshot belongs to the profile it was taken on, which is why
`wh restore` refuses a mismatch outright. And **only five of the 36 captures record which profile
they were on**, so comparing values between two captures is invalid unless both sides are
established, and for most pairs that cannot be done from the frames at all. `cmd 0x00` payload
`70 0xFF` reads the active profile; `70 <index>` selects one. One Phase 1 capture, `profile-switch`,
is profile 2 despite everything around it being profile 1.

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
```

`--nocapture` matters for `golden`: without it cargo swallows the summary on a passing run and you
see only `ok`. The two integration suites are `wh-proto/tests/golden.rs`, which decodes real captured
traffic, and `wh-cli/tests/dump.rs`, which drives the real binary over scripted replays.

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

## Test discipline

A test that passes for the wrong reason is worse than no test. Establish that each test **fails when
the code is wrong**, not merely that it passes when the code is right. Mutate the thing the test
claims to check, watch it fail, restore, and say so in the report. Several tests in this repo were
found to be decorative exactly that way.

**The recurring mechanism is a string assertion something else can satisfy.** Four have shipped:
`contains("mismatch")` also matched `ReplayTransport`'s own "send mismatch" wording, so the test
passed on a broken fixture; `contains("--press")` also matched clap's "unexpected argument '--press'
found", so it passed when the refusal it checked was unreachable; an assertion on a confirmation
prompt kept passing after the prompt lost half its content; and a refusal message was pinned by a
test whose fixture was the only board that message was true of. Assert text only `wh`'s own code can
emit, and prefer a phrase no other component would produce.

**Also mutate the thing one level up.** Unit tests proving a string is built correctly do not prove
it reaches the operator. One gap here survived because the only end-to-end assertion was on a
different substring of the same line.

## What the code does and what it says are separate claims

The frames can be right while the operator is told something false, and that defect survives review
because everything looks green. A `wh keyset remove` reported "nothing to do" while sending a frame
that turned rapid trigger off; the wire was correct and the sentence was not. It took three fix
rounds to kill, because each round addressed a narrower case than the one before: first the wording,
then the mode-only case, then the far commoner case where the value moves too.

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
