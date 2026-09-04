# Backlog

Things worth doing that are not in the Phase 1 plan. Each entry says what we know, what we do not,
and how we would find out, because the difference between those three is what makes an item
actionable later.

Evidence referenced here lives in `.superpowers/sdd/2026-08-28-wh-phase1/progress.md` and in the
capture files from the hardware session (local only, gitignored, backed up outside the repo).

## Hardware questions

### The LEDs beside the knob, and setting their colour

**What we know.** The top plate carries a printed scale reading 0.1, 0.2, 0.3, 0.4, 0.5, 1.0, 1.5,
2.0, 2.5, 3.0, 3.5 with a red mark at the end, and a row of LEDs above it. It reads as a travel
indicator. **The operator reports these LEDs do change colour, so they are RGB rather than
single-colour indicators.** That makes them a target for features, not just a thing to decode.

**What we do not know.** What drives them, whether the colour is host-controlled or firmware-driven,
and whether the row also tracks the actuation point, the rapid trigger sensitivity, or live key
depth. Colour and function may be independent: a travel indicator that happens to be RGB is a
different thing from a strip we can drive freely.

**Candidate.** Command `0x18` is unmodelled, appeared 6 times in the captures, and carries a payload
with a `7f7f` and `ff00ff00` shape. `ff00ff00` reads like a colour triple, and `7f7f` like a pair of
half-scale values. That is a guess from byte patterns and nothing stronger, and the same guess has
been wrong before on this project.

**How to find out.** Two captures, in this order.

1. Change the LED colour in the vendor configurator and capture. If `0x18` carries the colour, the
   payload should track the picker in an obvious way, and a pure red, pure green and pure blue in
   sequence would make the byte order unmistakable.
2. Change an actuation point, then a rapid trigger sensitivity, watching the row each time. If it
   tracks one and not the other, that is the function question answered separately from the colour
   one.

**What it unlocks.** If the colour is host-writable per LED, the row becomes a usable output surface:
a live actuation-point readout, a profile indicator, or anything else `wh` wants to show. That is the
reason this is worth more than curiosity.

### Are the key backlights colour-programmable, or white only?

**What we know.** The board has a LIGHT key, usage `0xFC`, confirmed by measurement (remapped in the
vendor UI and read back from the matrix). So lighting is a first-class board function with its own
key. The board lights up.

**What we do not know.** Whether the key backlights are RGB, per-key addressable, or a single-colour
backlight with brightness and effect control only. This is genuinely open: plenty of boards in this
class ship white-only backlighting with an RGB accent strip, which would match the knob LEDs being
colour-capable while the keys are not.

**How to find out.** Cheapest first, and most of this is looking rather than capturing.

1. Look at the board with the lighting on. A white-only backlight is obvious on sight.
2. Look at the vendor configurator's lighting section. A colour picker means RGB; brightness and
   effect sliders alone mean it is not.
3. Only then capture, changing colour or effect and watching for `0x18` or another unmodelled
   command.

**Why it is worth knowing before the TUI.** The planned TUI mirrors the vendor configurator one to
one. If lighting is a tab there, we need to know what it can express before designing the tab.

### How the knob is programmed

**What we know.** It adjusts system volume by default. Holding FN and turning it selects between
profiles 1 to 4. It is **not** one of the 68 keys: the full key enumeration was reconstructed and
every entry identified, so the knob is not addressed as a key in the matrix. The vendor
configurator's mapping view renders the 68 keys and does not appear to show the knob as a bindable
element.

**What we do not know.** What the knob sends, and whether its binding can be changed at all. The
board has four profiles and we already know how to read and select them on the wire (command `0x00`,
sub-order `0x70`, argument `0xFF` to read, a zero-based index to select), so if the FN combination is
host-mediated we know exactly what to look for.

**Why our existing captures cannot answer it, and this is the important part.** `wh` and the vendor
configurator both talk to the vendor-defined HID collection, usage page `0xFFA0` usage `0x01` on VID
`0x3879` PID `0x0806`. Volume control does not travel over that collection. It is a standard HID
consumer-control report on a different top-level collection, which our WebHID captures never see and
which Windows claims exclusively. So an empty capture while turning the knob is the expected result
and tells us nothing.

**Two separate questions, worth not conflating.**

1. **What does the knob send?** This is device input. Answering it needs the spy described below, not
   a configurator capture.
2. **Can the binding be changed?** This is configuration, and it would show up in vendor traffic if
   the configurator can do it at all. The FN combination is the promising half: if FN plus turn
   selects a profile, and that is host-mediated rather than handled in firmware, we should see the
   `0x70` sub-order we already know. If we see nothing, the profile switch is firmware-internal,
   which is itself a useful answer.

**How to find out.** Build the spy first. This item is blocked on it, which is why the spy is the
higher priority of the two.

### A device spy, so we can read the board directly

**The idea.** A development tool that reads what the keyboard actually sends, without going through
the vendor website and without hand-building a shim each time. Packaged as a `wh` dev feature so a
future collaborator gets the same visibility we had to improvise.

**Why it is worth building rather than improvising again.** Every protocol fact this project holds
came from capturing the vendor configurator. That means we can only ever observe what the
configurator chooses to do. Anything the board does on its own, key presses, the knob, board-side
setting changes via the AP and RT keys, is invisible to us today.

**It also closes a hazard we parked as unmeasurable.** `docs/tasks.md` records key `0x01` as probably
FN, deliberately unmeasured, because confirming it means remapping FN away and FN is how you reach
the layer that would let you undo that. A spy that reads what the board reports when FN is pressed
answers it by observation, with nothing written and nothing to undo.

**The constraint that shapes the design.** A USB HID device presents several top-level collections,
and Windows treats them very differently:

| Collection | What it carries | Can we read it? |
|---|---|---|
| Vendor-defined, `0xFFA0`/`0x01` | settings traffic | **Yes, already.** This is how `wh` works, and the OS does not claim it |
| Keyboard | key presses | No, not directly. Windows opens it exclusively |
| Consumer control | the knob's volume | No, same exclusivity |

So "read the vendor traffic" and "read key presses" are two different problems, and a design that
assumes one tool does both will fail on the second.

**Three routes, roughly in order of how much they cost.**

1. **Raw Input API** (`GetRawInputData` with `RIDEV_INPUTSINK`), through the `windows` crate. Gives
   system-wide keyboard and consumer-control input, can be filtered to one device by handle, needs no
   driver install and no administrator rights. Reports what the OS decoded rather than the raw bytes
   on the wire, which is enough to answer "which key did the board report" and therefore enough for
   the FN question. Recommended starting point.
2. **USBPcap plus Wireshark.** Captures at the USB bus level, so it sees every collection including
   the ones Windows claims, and shows the actual bytes. Needs a driver install and a reboot, and is a
   separate tool rather than something we can package. Right answer when route 1 is not specific
   enough, for example for the knob's exact report shape.
3. **libusb or `rusb`.** Rejected. It requires replacing the device driver with WinUSB, which stops
   the keyboard working as a keyboard while attached. Not acceptable for a tool an operator runs on
   their daily board.

**Scope for a first version.** A `wh spy` command that opens the vendor collection we already have
access to and prints every inbound report decoded through the existing codec, with a raw hex line
alongside. That alone is useful, needs no new dependency, and reuses `wh-proto` and `ReplayTransport`
directly. Key presses via Raw Input are a second, larger step and should be judged separately once
the first exists.

**A note on scope creep.** This is a development tool, not part of the product surface. It should be
behind a feature flag or a clearly marked dev subcommand, so it never becomes something an end user
runs by accident against their board.

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

**Measured in full on 2026-08-29. See `docs/keysets.md`**, which supersedes this entry and specifies
task 2.4. In short: `0xFF` and `0xFE` are both host-written and both indices, they are independent
groupings with separate counters, allocation is max plus one over live membership, so a freed
index returns to the pool and only gaps below the maximum are skipped, and a delete resets the
value to the global before clearing membership.

Two things this entry previously got wrong. `0xFF` was described as inferred from read correlation
with no evidence anything writes it; it is written, one record per frame. And the greying cause was
first asserted as `0xFF` being left at zero, then demoted behind a MODE nibble hypothesis.

**Settled on 2026-09-04, and the `0xFF` reading was the right one.** Two controlled experiments on
the real board, each changing one thing:

- `F` was put at MODE touch nibble 0 by a hand-edited `wh restore`, the only key on a board of 68 to
  hold it, while staying outside any keyset at the global 2.00mm. It rendered identically to `H`,
  `J`, `K` and `L` at nibble 1, also outside any keyset at 2.00mm. **The nibble makes no
  difference.**
- `J,K,L` were then put in a keyset at exactly the global 2.00mm, the same value `H` holds outside
  one. `J,K,L` rendered highlighted, `H` grey. **Membership makes the difference, and the value does
  not, since all four held the same one.**

So the configurator distinguishes on layout `0xFF` alone. The board accepting a nibble-0 write and
reporting it back is measured from frames; the rendering is an operator observation of the
interface, and is what it is: two screenshots taken minutes apart with one variable changed.

**A consequence worth carrying.** `wh set ap` over keys that are all free writes values and no
membership, which is measured vendor behaviour for that shape, so those keys stay grey. In the
configurator a per-key actuation point cannot be set without creating a keyset. That divergence was
invisible until greying was understood and is recorded in `docs/tasks.md`.

### Listing backups, and what `--last` should mean

**Resolved, shipped as `docs/tasks.md` 2.6.** `wh backups list` exists now and names what took each
snapshot. Kept for the history below.

**The problem, as it stood before 2.6.** `wh backup` was indistinguishable from the automatic backup
every write takes before it writes, and there was no way to list either. So `wh restore --last` meant
"undo the last command", not "return to where I started".

**How it showed up.** During the hardware session the sequence was: manual backup, `set rt`, `set ap`,
`restore --last`. That restored the auto-backup taken immediately before `set ap`, which already had
the rapid trigger change in it. The tool named the snapshot it used and restored it exactly, and 68
keys verified, but it briefly read as a restore bug.

**What shipped.** `wh backups list` shows each snapshot's timestamp and origin, naming the exact
command that took it (`manual`, `set rt`, `restore`, and so on). `--last` still means "the most
recent snapshot, whatever took it": a deliberate decision, not an oversight, so it stays predictable
across commands.

### Deleting or renaming a stored key group

**Resolved, shipped as `docs/tasks.md` 2.7.** `wh keys ungroup` and `wh keys rename` exist now. Kept
for the history below.

**The problem, as it stood before 2.7.** `wh keys group <name> <selector>` creates a group, and
nothing removed one. That became visible when four board-function key names (`ap`, `rt`, `play`,
`light`) were added to the key table: a group created under one of those names before the change was
refused by the selector, correctly, because a bare name that is both a key and a stored group is
ambiguous and writing to the wrong key on hardware is unacceptable. But the operator's only recovery
was to read the group's members off `wh keys list` and retype them under a new name.

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

## Post 1.0, not necessary

Parked deliberately. Neither of these is needed for the tool to do its job, and both should wait
until after a 1.0 release. Recorded because the constraints behind them are measured facts that are
annoying to rediscover.

### `wh serve`, a daemon that owns the device

**The problem it solves, and it exists today.** The vendor HID collection takes one process at a
time. `crates/wh-device/src/hid.rs` already returns `DeviceError::Busy` for it, with the comment
"most likely held exclusively by the web configurator". So having the vendor website open in a
browser tab stops `wh` working right now.

**Why it matters more later than now.** Three planned features all want the device at once: the TUI,
the spy, and anything else long-running. Today they would fight each other. A daemon that owns the
HID handle and multiplexes over a local socket is the only design where they coexist.

**The second thing it unlocks.** The hardware path is Windows-only: `crates/wh-device/src/lib.rs`
gates the `hid` module behind `#[cfg(windows)]` and `Cargo.toml` gates `hidapi` the same way. A
daemon splits that cleanly, since the device stays on Windows while a client can live anywhere.

**Deliberately not now.** Nothing today needs it, and a daemon is a large surface: lifecycle,
socket permissions, protocol versioning, and a new way to leave a process holding the board.

### A PostgreSQL foreign data wrapper

**The idea.** Expose the board as SQL tables, so `SELECT name, ap_mm FROM wh.keys WHERE rt` works.
Read-only. Writing to hardware from a SQL prompt, with no dry run and no backup, is not something to
build casually.

**Blocked on `wh serve`, for two reasons.** `pgrx`, the Rust framework for Postgres extensions,
targets Linux and macOS rather than Windows, while our HID path is Windows-only. The extension
cannot run where the device is, and the device code cannot compile where the extension runs. Talking
to a daemon instead of the hardware resolves both. It also resolves the exclusivity problem above,
since a Postgres backend holding the board open would lock out every other tool.

**A constraint worth respecting if it is ever built.** The spec's no-drift invariant says `wh` caches
no device state. An FDW is a caching-shaped thing, so the honest design is no cache at all: every
scan re-reads, at roughly 400 HID roundtrips per full table scan. That is fine for 68 rows and keeps
the invariant intact.

**Honest assessment.** A good demo, weak on necessity. `wh dump | jq` already covers most of what
anyone would actually query, and needs neither a daemon nor an extension.

## Protocol gaps

These are known unknowns from the hardware session, listed so nobody re-derives them.

### Unidentified commands

- `0x18`, now 8 frames. Suspected RGB or LED control. See the LED item above. A fresh sample was
  captured on 2026-08-29 in `captures/custom-value-nudge-after-restore.jsonl`.
- ~~`0x2C`~~ **Identified 2026-08-29: SOCD.** Query is `[rw, key, 0xFF]`, reply is
  `[status, keyA, keyB, 0, keyB, keyA]`, the pair given both ways round. Measured `w` with `s` and
  `a` with `d`, and the ADVANCED tab carries a SOCD control holding exactly those pairs. The name is
  no longer an inference from byte shapes. See `docs/keysets.md`.

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
- `0x16` and `0x17`, 1858 records each in this ten-capture session, zero in every one of them.
  Overturned since: both read and are written `100` throughout the keyset sitting, 580 write records
  across fourteen files. What moved them is unmeasured. See `docs/keysets.md`. Purpose unknown.
- `0x19`, 700 records, only ever `0x0000` or `0x3e2c`, and non-zero on 68 of the 69 enumerated keys.
  Purpose unknown.
- `0xFE` is the **rapid trigger keyset membership**, measured from write evidence in this very
  session: 424 records in total, of which the write evidence is two request and reply pairs, `1` on
  a keyset create and `0` on a delete. `0xFF` is the **actuation point keyset index**, read `210` times and written
  `0` in this ten-capture session; host-written and measured directly only in the wider 36-capture
  corpus. Both are indices, not booleans. See the keyset entry above and `docs/keysets.md`.

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
