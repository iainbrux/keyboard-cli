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

### A terminal UI mirroring the vendor configurator

**The idea.** A close clone of `terminal.wallhack.com` running inside the `wh` binary, so someone who
wants neither a command line nor a browser tab gets the same experience in their terminal, navigable
with mouse and arrow keys.

**Why it fits.** The vendor UI is already terminal-styled, so a faithful TUI is a smaller leap than
it would be for most configurators. `ratatui` and `crossterm` are already dependencies, and the
Task 17 picker established the pattern of a pure state core with a thin terminal shell around it,
which is what makes this testable rather than a blob of rendering.

**What it needs, roughly.** The vendor's structure is known from screenshots: a profile selector, a
keyboard render, tabs for Actuation Point, Rapid Trigger, Mapping, Switches and Advanced, and the
keyset concept where a set of keys shares a group of settings. The keyset model is worth thinking
about carefully: ours is per-key settings addressed by key, theirs is settings held by a named group
that keys belong to. Same wire format, different abstraction, and the TUI would have to pick one.

**Prerequisite.** Not until the write path has been exercised against hardware. A UI that makes it
easy to change many settings quickly is the worst place to discover a write bug.

### Writing keyset membership, layout `0xFE`

**What we know, measured on hardware.** Settings written by `wh` do apply and do work. `set ap` on F
was confirmed by feel against a 2.00mm key, and `set rt` on W was confirmed in the vendor UI. But the
vendor configurator renders those values greyed and outside any named keyset, because we never write
layout `0xFE`. The vendor writes `1` to it when a keyset is created and `0` when one is deleted, and
leaves it alone for edits within an existing set.

**Why it is worth doing.** Cosmetic for an alpha, and explicitly accepted as such. It is the
difference between our changes looking native in the vendor's own UI and looking like loose overrides
sitting outside its model. It also matters for the TUI item below, because the vendor's abstraction is
a named group that keys belong to, while ours is per-key settings addressed by key.

**How to find out the rest.** We know when `0xFE` is written but not how a keyset's name or membership
list is stored, and a keyset clearly has both. Capture while creating a named keyset with two keys in
it, then while renaming it, then while adding a third key.

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
- `0x19`, 700 records, only ever `0x0000` or `0x3e2c`. Purpose unknown.
- `0xFF`, 420 records, only ever `0`, `1` or `2`. Purpose unknown.
- `0xFE`, written as 1 when a keyset is created and 0 when it is deleted, and untouched by edits
  within an existing keyset. Reads as a membership flag.

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
