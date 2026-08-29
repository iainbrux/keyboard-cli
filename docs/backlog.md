# Backlog

Things worth doing that are not in the Phase 1 plan. Each entry says what we know, what we do not,
and how we would find out, because the difference between those three is what makes an item
actionable later.

Evidence referenced here lives in `.superpowers/sdd/2026-08-28-wh-phase1/progress.md` and in the
capture files from the hardware session (local only, gitignored, backed up outside the repo).

## Hardware questions

### The knob

**What we know.** It adjusts system volume by default. Holding FN and turning it selects between
profiles 1 to 4. The board has four profiles and we know how to read and select them on the wire
(command `0x00`, sub-order `0x70`, argument `0xFF` to read and a zero-based index to select).

**What we do not know.** Whether the knob can be rebound at all, and if so how that binding is
expressed on the wire. The vendor configurator's mapping view renders the 68 keys and does not
appear to show the knob as a bindable element.

**A useful negative we already have.** The knob is *not* one of the 68 keys. The full key
enumeration was reconstructed and every entry identified, so the knob is not addressed as a key in
the matrix. It is either a separate control with its own command, or not configurable.

**How to find out.** Capture while turning the knob, both plain and with FN held. Volume changes
almost certainly leave over the standard HID consumer-control interface rather than the vendor
collection we capture, so an empty capture would itself be informative. The FN combination is more
promising: if it drives profile selection, we should see the `0x70` sub-order we already know.

### The numbered LEDs beside the knob

**What we know.** The top plate carries a printed scale reading 0.1, 0.2, 0.3, 0.4, 0.5, 1.0, 1.5,
2.0, 2.5, 3.0, 3.5 with a red mark at the end, and a row of LEDs above it. It reads as a travel
indicator.

**What we do not know.** What drives them, whether they are host-controlled or firmware-driven, and
whether they reflect the actuation point, the rapid trigger sensitivity, or live key depth.

**Candidate.** Command `0x18` is unmodelled, appeared 6 times in the captures, and carries a payload
with a `7f7f` and `ff00ff00` shape that reads like colour or level data. That is a guess from byte
patterns, nothing stronger.

**How to find out.** Capture while changing an actuation point and watching the LEDs, then while
changing a rapid trigger sensitivity. If the LED row tracks one and not the other, and `0x18`
traffic accompanies it, that is most of the answer.

## Features

### A TUI that mirrors terminal.wallhack.com, one to one

**The idea.** Rebuild the vendor configurator as a terminal UI inside the `wh` binary, matching its
layout closely enough that someone who knows the website can drive it without relearning anything.
The vendor UI is already terminal-styled, so this is a far smaller leap than it would be for a
typical configurator.

**The header is a collaboration mark, not a copy.** The vendor's ASCII logo at the top is replaced by
a joint one: the `wh` logo in ASCII art alongside `brux` in ASCII art. Do not reproduce Wallhack's
logo. The Wallhack name and logo are theirs, this project is independent and unendorsed, and the
header is the most visible place that distinction gets made.

**The structure to match**, read off the vendor UI:

- ASCII header, then a version line, then a hint line about navigation.
- A device line showing model and firmware, and a `PROFILE < 1 >` stepper.
- A tab bar: ACTUATION POINT, RAPID TRIGGER, MAPPING, SWITCHES, ADVANCED.
- A left pane of settings rows, each a label padded with dots and a `< value >` stepper, with
  keyset rows carrying a checkbox and their shared value.
- A right pane rendering the 68-key board in its real physical layout, each key showing its current
  value, with keys selectable to form a keyset.
- A status line.

**What we can already drive.** Actuation point, rapid trigger, profile reading, and now keysets,
since layout `0xFF` turned out to hold the actuation-point keyset index and `0xFE` the rapid trigger
equivalent. So the ACTUATION POINT and RAPID TRIGGER tabs are buildable today, keysets included.

**What is blocked on protocol work.** MAPPING needs layouts `0x00` and `0x01`, which are measured but
unmodelled. SWITCHES and ADVANCED are unmeasured entirely. Profile *select* is measured but not
implemented. So a genuinely one to one TUI depends on the remapping work and a capture session
covering switches and the advanced tab. Build the two tabs we can drive first rather than waiting.

**What it needs, technically.** `ratatui` 0.29 and `crossterm` 0.28 are already dependencies, used by
the `--pick` picker, and Task 17 established the pattern of a pure state core with a thin terminal
shell around it, which is what makes this testable rather than a blob of rendering. The vendor UI
supports mouse as well as arrows; `crossterm` can capture mouse events, so that is achievable.

**Prerequisite, now met.** The write path has been verified against hardware. A UI that makes it easy
to change many settings quickly was the worst place to discover a write bug, and that risk is now
retired.

### A Windows installer with a licence acceptance step

**The idea.** Ship a proper graphical installer, the familiar window with Next, Next, Finish, rather
than asking people to download a bare executable or build from source. It should present the licence
for acceptance, install the binary, and register it on `PATH` so `wh` just works from a shell.

**Why it is worth more than convenience.** The installer is the natural vehicle for obligations this
project already carries. Distributing a binary means the recipient must get the Apache-2.0 licence
text and the `NOTICE` contents, plus the third-party notices for everything statically linked. An
installer that lays `LICENSE`, `NOTICE`, `THIRD_PARTY_LICENSES.md` and `THIRD_PARTY_NOTICES.md` down
beside the binary satisfies that cleanly and visibly.

**On the word EULA.** Apache-2.0 is not an end user licence agreement and does not require anyone to
click accept before using the software. An installer can still present it, and doing so is common and
harmless, but it should present the actual Apache-2.0 terms rather than invent an agreement on top of
them. If a genuine additional agreement is ever wanted, that is a separate decision with real
consequences for a permissively licensed project.

**Tooling.** `cargo-wix` produces an MSI from the crate metadata and is the closest fit for a Rust
project. Inno Setup and NSIS are the alternatives and give more control over the wizard's appearance.
An MSI is the better citizen for managed environments; an Inno installer is easier to make look the
way you want.

**The awkward part, and it needs deciding before any of this is built.** `wh` is currently a Windows
binary driven from WSL through the `bin/wh` shim, because the keyboard is attached to the Windows
host. An installer serves someone running `wh.exe` directly from PowerShell, which is a different
workflow with a different `PATH`, a different config location, and no shim. Both can be supported,
but the installer implies the native Windows path is a first-class way to use the tool, and the
README currently does not describe it that way.

### Decide what stays in `research/`

**Where this came from.** GitHub reported the repository as 41% Rust, 34% Vue, 24% TypeScript,
because linguist counts every tracked file and `research/` holds 195 vendored third-party files
against 26 of our own. That symptom is fixed: `.gitattributes` marks `research/**` as
`linguist-vendored`, so the files stay, their licences stay, and they stop being counted as ours.

**The real question it exposed** is whether all four vendored packages have earned their place, and
they have not equally:

| Directory | Files | Size | Cited by our source? |
|---|---|---|---|
| `research/proto/` | 51 | 484K | **Yes.** Both `frame.rs` and `cmds.rs` name it as the origin of the port |
| `research/kbdocs/` | 42 | 680K | No |
| `research/hidpkg/` | 20 | 144K | No, and `THIRD_PARTY_NOTICES.md` says explicitly that nothing ports from it |
| `research/aure/` | 82 | **4.3M** | No |

For comparison, `crates/` is 26 files and 472K.

**`research/proto/` should stay regardless.** It is load-bearing, not decorative:
`crates/wh-proto/src/frame.rs` and `cmds.rs` carry `//! Port of research/proto/package/src/...` in
their module docs, and those citations are how anyone verifies which files the Sparklink MIT notice
attaches to. Removing it leaves the attribution chain pointing at nothing.

**`research/aure/` is the one to think about.** It is a separate Vue driver project by another
author, larger than the rest of the repository combined, cited nowhere in our code, and useful only
as reference during reverse engineering. Keeping it is entirely legal with its MIT notice intact, and
it is a genuinely useful frozen snapshot. It is also the bulk of a clone.

**The two options.**

1. **Keep all four as a pinned reference snapshot.** Costs clone size and nothing else. Defensible:
   these are the exact versions the protocol work was done against, and a future capture session may
   want to diff behaviour against them.
2. **Trim to `research/proto/` and reference the rest by URL and exact version or commit.** Smaller
   repository, but a URL can rot and a snapshot cannot, so record enough to re-fetch the identical
   artefact rather than just a project name.

**What a trim would touch**, so it is not done casually: `THIRD_PARTY_NOTICES.md` and
`THIRD_PARTY_LICENSES.md` both enumerate all four packages with their licences, `research/README.md`
tabulates them, and `.gitattributes` references the directory. The notices must keep covering
whatever remains, and anything removed must stop being claimed.

**Deliberately not urgent.** Nothing is wrong today. The licences are correct, the notices are
complete, and the language bar is fixed. This is a housekeeping decision to take once a release is
out, not something to rush while one is in flight.

### Writing keyset membership, so our changes render as keysets

**Measured, and this corrects an earlier wrong entry.** Keysets are stored on the board, not in the
browser. Two per-key layouts hold them:

| Layout | Meaning | Evidence |
|---|---|---|
| `0xFF` | Actuation point keyset index. `0` means the key is in none | reads `1` for `w,a,s,d` and `2` for `esc`, matching both entries the vendor UI displayed |
| `0xFE` | Rapid trigger keyset membership | written `1` when an RT keyset was created on `w`, `0` when deleted |

Confirmed from the other side too: the operator inspected the vendor site's browser storage and found
five keys, none keyset-related, so there is nowhere else for this state to live.

**Why our writes render greyed.** `wh set ap --keys f` writes F's actuation point and leaves
`f.0xFF = 0`, so the board holds a per-key value belonging to no keyset and the UI shows it as an
orphan. Writing the index alongside the value fixes it.

**A keyset has no name.** The UI's labels, `W,A,S,D` and `ESC`, are just the member list. Nothing on
the board carries a name and nothing in browser storage does either, so a keyset is exactly "the keys
sharing an index" and needs no name modelling.

**What is still unknown**, and one capture settles all of it: how the UI allocates the next index,
whether it reuses a gap left by a deleted keyset or takes the maximum plus one, and whether `0xFE` is
a boolean or an index we have only ever seen `0` and `1` of. Create two fresh actuation point keysets
over untouched keys, delete the first, create a third, and watch `0xFF`.

**An earlier version of this entry said keysets were probably browser state and the work likely
unreachable from a CLI.** That was wrong. It came from checking layout `0xFE`, finding every key read
zero, and generalising, when `0xFF` was sitting in the same inventory with a value distribution that
matched the keyset count exactly.

### Listing backups, and what `--last` should mean

**The problem.** There is no `wh backups list`, and a manual `wh backup` is indistinguishable from the
automatic one every write takes before it writes. So `wh restore --last` means "undo the last
command", not "return to where I started".

**How it showed up.** During the hardware session the sequence was: manual backup, `set rt`, `set ap`,
`restore --last`. That restored the auto-backup taken immediately before `set ap`, which already had
the rapid trigger change in it. The tool named the snapshot it used and restored it exactly, and 68
keys verified, but it briefly read as a restore bug.

**What it needs.** At minimum a way to list backups with their timestamps and a marker for manual
versus automatic. Possibly `--last` should prefer the last manual backup, or there should be a
separate flag for each meaning. Worth deciding deliberately rather than by accident.

### Deleting or renaming a stored key group

**The problem.** `wh keys group <name> <selector>` creates a group, and nothing removes one. That
became visible when four board-function key names (`ap`, `rt`, `play`, `light`) were added to the key
table: a group created under one of those names before the change is now refused by the selector,
correctly, because a bare name that is both a key and a stored group is ambiguous and writing to the
wrong key on hardware is unacceptable. But the operator's only recovery is to read the group's
members off `wh keys list` and retype them under a new name, because `wh keys group` cannot delete
and cannot rename.

**What it needs.** A delete, and probably a rename. The awkward part is that a group whose name
collides is exactly the one you most want to remove, and any command that takes the group's name as a
selector will hit the same ambiguity guard, so the delete has to address the group by name in a
position that is unambiguously a group, not a selector.

**Deliberately deferred.** It is a new CLI surface, and it was found during a task of protocol
corrections where adding one would have been unreviewed scope creep.

### A loading spinner on CLI commands

**The idea.** When a command runs, show a brief spinner cycling `|`, `/`, `-`, `\` for something in
the region of 100 to 300ms, so a write feels like it went somewhere rather than returning instantly.
The board really is that fast; this is presentation.

**Worth getting right rather than sprinkling in.**

- The delay must not gate the actual work. Run the real operation, then hold the frame briefly, or
  spin while it runs and stop when it finishes. A cosmetic pause that delays a write to hardware is
  the wrong way round.
- It must never imply success. If the command fails, the spinner should stop on the failure rather
  than completing its cycle first and then printing an error.
- Suppress it when stdout is not a terminal. We already have that check: Task 17 added
  `refuse_if_not_terminal` and the picker uses `std::io::stdout().is_terminal()`, so the machinery
  exists. A spinner in a redirected file or a pipeline is noise, and every frame would land in
  someone's captured output.
- **Decided: reads get no spinner.** `wh get`, `wh dump` and `wh keys` return instantly, and that
  speed is a feature rather than something to hide. The spinner is for the write path only, where a
  moment's pause reads as the board thinking about it, and where a user has just changed something
  on hardware and wants to feel that it landed.
- A `--no-spinner` or a respect for `NO_COLOR`-style conventions is probably worth having for anyone
  scripting against it.

**In the TUI.** Open question, and it may not belong there at all. A TUI redraws continuously, so a
modal spinner would be a different thing from a one-shot CLI flourish. Decide when the TUI exists
rather than now.

## Protocol gaps

These are known unknowns from the hardware session, listed so nobody re-derives them.

### Unidentified commands

- `0x18`, 6 frames. Suspected RGB or LED control. See the LED item above.
- `0x2C`, 8 frames. Almost certainly SOCD: it queries by key and replies with symmetric pairs,
  measured as W with S and A with D, matching the linked pairs visible in the vendor UI. The
  behaviour is measured; the name is inference.

### Unidentified sub-orders of command `0x00`

All request and reply balanced, none ever failing, all confined to the connect sequence except
`0xBD` which recurs as a poll. None is needed for anything Phase 1 does.

| Sub-order | Pairs | Reply payload |
|---|---|---|
| `0x22` | 5 | `002200` |
| `0x50` | 3 | `005000` |
| `0xA1` | 6 | `00a100` |
| `0xB9` | 3 | `00b900` |
| `0xBA` | 3 | `00ba0000` |
| `0xBB` | 3 | `00bb0000` |
| `0xBC` | 3 | `00bc6400` |
| `0xBD` | 9 | `00bd01ff` |
| `0xC0` | 3 | `00c001` |

### Layouts, identified and not

Two former unknowns are now measured. See `docs/protocol-inventory.md` for the full table and counts.

- `0x00` is the **base layer key mapping** and `0x01` is the **FN layer**. Measured from
  `initial-load` by reading each key's `0x00` against its `0x01`: esc maps to grave, and 1 through 0
  map to F1 through F10, holding across 69 distinct values in two independent series. That is
  exactly how the board behaves under FN.
- `0x16` and `0x17`, 1858 records each across the corpus, written as zero alongside every rapid
  trigger change and **never once observed non-zero**. Purpose unknown.
- `0x19`, 700 records, only ever `0x0000` or `0x3e2c`, and non-zero on 68 of the 69 enumerated keys.
  Purpose unknown.
- `0xFF` is the **actuation point keyset index** and `0xFE` the **rapid trigger keyset membership**.
  Both measured; see the keyset entry above.

### One key identity still inferred

`0x01` is probably the FN key, from its position in the enumeration. Deliberately not measured,
because confirming it means remapping FN away and FN is how you reach the FN layer. The other four
non-standard keys were confirmed by measurement: `0xFA` is AP, `0xFB` is RT, `0xD6` is PLAY, and
`0xFC` is LIGHT.

One honest limit on those four, raised in review. The captures prove that the four usage codes exist,
are configurable, and were remapped to F2, F3, F4 and F5 respectively. They do not by themselves
prove that `0xFA` is the key *legended* AP rather than the one legended RT. That binding rests on the
operator's report of which key they remapped, corroborated by the four codes sitting in the same
column across four consecutive matrix rows. Good enough to name them; not the same standard as the
byte-level facts elsewhere in this document.

### Settings a snapshot does not capture

`wh backup` stores global travel plus four layouts per key, and, as of Phase 1, the profile the
board was on when the snapshot was taken: `Snapshot::profile` records it. `wh restore` checks that
recorded profile against the board's current one, and the two refusals are not the same and do not
share an override. When the snapshot's recorded profile differs from the board's, `wh restore`
refuses unconditionally; there is no `--force` for that case, since restoring would silently
overwrite the wrong profile's settings. When the snapshot has no recorded profile at all (an older
snapshot from before this field existed, or one whose board reported a profile index this build does
not recognise), `wh restore` refuses by default but accepts `--force`, asserting the settings belong
to the board's current profile. That gap is closed. It still does not capture the base layer key
mapping (layout `0x00`), the FN layer (layout `0x01`), SOCD, dynamic keystroke, mod tap, gamepad
configuration, RGB, or polling rate. Those are Phase 2 scope questions, and the README says plainly
what a snapshot does and does not contain either way.
