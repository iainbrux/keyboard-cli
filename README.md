# wh

A command-line tool for reading and writing rapid-trigger and actuation-point settings on a
Wallhack K-001 hall-effect keyboard, over raw HID. No other board is supported or tested.

## Why WSL and a Windows binary

`wh` talks to the keyboard through `hidapi`, which needs the Windows HID stack to see the device;
WSL has no direct access to it. The workflow this repository is built around is: develop and run
`cargo` from WSL, but cross-compile to a Windows binary and run that binary against the real
hardware, from WSL, through a small shim. You do not need a Windows shell open to use `wh`, only the
cross-compiled binary and the shim below.

## Install and build

You need the `x86_64-pc-windows-gnu` Rust target and a mingw-w64 linker:

```
rustup target add x86_64-pc-windows-gnu
```

On a Debian/Ubuntu-based WSL distribution:

```
sudo apt install mingw-w64
```

Then build the release binary:

```
cargo build --release --workspace --target x86_64-pc-windows-gnu
```

`bin/wh` is a shim script that execs the built `wh.exe` from WSL, so once the build above succeeds
you can run `./bin/wh <command>` directly from your WSL shell exactly as you would a native binary,
and it will find and control the keyboard through the Windows HID stack underneath.

### Building a release archive

A bare `wh.exe` download carries none of this project's licence, attribution, or third-party
notices with it, and Apache-2.0 requires all three to reach whoever receives the binary. Run
`scripts/package-release.sh` to build the actual release artefact, not just the binary:

```
scripts/package-release.sh
```

It builds the release binary, then writes `dist/wh-<version>-x86_64-pc-windows-gnu.zip` containing
`wh.exe`, `LICENSE`, `NOTICE`, `THIRD_PARTY_LICENSES.md`, `THIRD_PARTY_NOTICES.md`, and a short
`README.txt` pointer back to this repository, and prints the archive's contents so a run is its own
verification. Requires `cargo` and `python3` (used to build the archive itself; no `zip` binary is
assumed to be installed). Given the same source tree, two runs produce a byte-identical archive.
This is what should actually be attached to a GitHub release, not the bare `.exe`.

## The exclusive-access caveat

**The vendor's own web configurator, `terminal.wallhack.com`, holds the device exclusively while its
tab is open.** If a browser tab to that site is open (even in the background, even if you are not
looking at it), `wh` will fail to open the device. Close the tab first. This bites everyone once:
the failure looks like a missing or broken keyboard, not like a competing process, because nothing
about the error names the browser.

## Commands

Read the whole board configuration. The default is JSON; `--table` prints a human-readable table,
now with two extra columns, `apks` and `rtks`, for each key's raw actuation point and rapid trigger
keyset value (`-` for the value read outside any keyset):

```
wh dump
wh dump --table
```

Read or write rapid trigger and actuation point for a key selection:

```
wh get rt --keys "w,a,s,d"
wh set rt --keys "w,a,s,d" --set 0.5
wh set rt --keys "w,a,s,d" --set 0.5 --press 0.4 --release 0.6
wh set rt --keys "w,a,s,d" --off
wh set rt --keys "w,a,s,d" --off --press 0.1 --release 0.1
wh get ap --keys wasd
wh set ap --keys wasd --set 1.2
```

`wh get rt`/`wh get ap` also print the key's raw keyset value as a suffix, `keyset N` or
`keyset none`.

**`wh set rt --off` does the whole of what the vendor's own per-key rapid trigger off does**, not
just the mode nibble: it turns rapid trigger off, resets both sensitivities to the board's global,
and clears the key's rapid trigger keyset membership, which is the order and the content measured in
`captures/rt-off-w.jsonl`. So a key turned off this way stops being listed in an rt keyset by
`wh keyset list` and by the vendor configurator, and a keyset whose last member is turned off ceases
to exist, which the announcement says. The base it resets to is read from the keys outside every
rt keyset that the selection itself leaves behind, the same rule `wh keyset remove` follows and for
the same reason: the key you are turning rapid trigger off on is usually the one holding its own
sensitivity, so counting it would make it its own disagreement. If the keys left behind disagree, or
if the selection covers every key outside a keyset (which `--keys all` always does), `--off` refuses
and names each value with how many keys hold it rather than picking a winner. `--press`/`--release`
are the way past that (`wh set rt --keys w --off --press 0.1 --release 0.1`), and the announcement
says when a value came from those flags rather than from the board. Pass both or neither: `--off`
resets both sensitivities together. A key already off and already at the base still has its
membership rewritten, and the announcement says so rather than claiming nothing was written, since
a frame really is sent.

**A whole-board `wh set rt --keys all --off` needs a typed `yes`**, like `wh keyset remove --keys
all` and the two other whole-board routes: it clears `0xFE` on every key, so every rapid trigger
keyset on the board ceases to exist at once. The prompt names the value, the keysets being lost and
how many keys have rapid trigger switched off, and goes to stderr so redirecting stdout cannot trap
it. There is no bypass flag; scripts pipe `yes` on stdin. `--dry-run` never prompts, since it writes
nothing.

Key selectors accept comma-separated names, contiguous runs typed as one word (`wasd`), ranges
(`a-f`), negation (`all,!space`), user-defined groups (`wh keys group fps "w,a,s,d,space"`, then
`--keys fps`), and a hex usage for a key with no name (`0x01`, as `wh keys list` prints it, typed
back into any selector). `wh keys list` shows every known key name and stored group. A range is a
range over `wh-proto`'s own key table, not the physical layout, so `f1-f12` parses but resolves to
nothing on this board: the K-001 is 68 keys with no F row, and `wh keys list` is the source of truth
for what actually exists to select.

Manage keysets, the vendor's per-key groupings for actuation point (`ap`) and rapid trigger (`rt`)
settings (`0xFF` and `0xFE` on the wire, independent of each other):

```
wh keyset list
wh keyset list ap
wh keyset create ap --keys u,i,o,p --value 1.5
wh keyset create rt --keys j,k,l --press 0.3 --release 0.5
wh keyset set ap 3 --value 1.2
wh keyset delete ap 3
wh keyset remove ap --keys w
```

`wh keyset list` with no kind lists both `ap` and `rt`; naming one lists that kind. Each keyset is
shown with its index, value, and member keys: one shared value when the members agree
(`1 1.50mm  u,i,o,p`), or each distinct value with the keys holding it when they do not
(`1 disagree: u at 1.50mm, i at 1.20mm`). `wh set rt --set` does not write `0xFE`; running it on
part of an rt keyset's members can leave that keyset's members holding different values, which
`wh keyset list` then shows as this kind of disagreement. `wh set rt --off` does write it, clearing
membership, so it never leaves a keyset in that state. `wh keyset set` changes an existing keyset's
value in place. `create`, `delete`, `remove`, `wh set ap` and `wh set rt --off` above, and
`wh restore` (for keys whose snapshot recorded it, see below) each write keyset membership.
`create` and `delete` take `--value` (or, for `rt`, `--press`/`--release`); it defaults to the
board's current global value, and passing it is required when the keys outside every keyset
disagree, or when none are left outside one.

**`wh keyset remove` resets named keys to the board's base value and to no keyset at all**, whether
or not they were in one: it takes no value flags, since its job is a destination, not a choice. For
`ap`, that base is the actuation point, and a key still on touch nibble 0 ("follow global travel")
is promoted to nibble 1, a pinned per-key actuation point, the same promotion `create`/`set`/`delete`
already apply; for `rt`, both sensitivities, with rapid trigger turned off. The base comes from the
free keys the selection leaves behind, the same keys the removed ones would otherwise agree with. If
those remaining free keys disagree on it, `remove` refuses and names each value and how many keys
hold it, saying to include them in the selection so they are reset too, rather than inventing a
winner. If none are left outside the selection at all, `ap` falls back to `2.00mm`, a chosen default
for that one unanswerable case (the measured dominant `0x04` reading, not a measured factory
setting), and says so in the announcement rather than letting the invented value read as one read
from the board; `rt` has no equivalent default and refuses instead, since no capture has ever shown
a rapid trigger sensitivity reset to a fixed constant rather than to whatever the global held at the
time. A key already at the base still gets its membership rewritten unconditionally (`plan`'s own
rule, harmless since it is idempotent), so the announcement says "membership rewritten, value
unchanged" rather than "nothing to do", since a frame really is sent. Taking a keyset's last member
is named too, "keyset N ceases to exist", whether it happens on its own or as part of a larger
selection.

`wh keyset remove ap --keys w`, with `w` one of three members of a keyset, moves only `w` and leaves
the other two in place. Where two commands used to be needed to clear a stray key (`wh set ap` to
put it in a keyset, then `wh keyset remove` to take it back out), one now does it directly: `wh
keyset remove ap --keys w` works whether `w` is in a keyset or already free. Selecting every key in
the board's matrix (however it is spelled) destroys every `ap` keyset and moves every key to the
base in one write; `rt` cannot do this, since that selection always leaves no free key outside it to
read a sensitivity from, and `rt` refuses that case rather than inventing one. Neither, for the same
reason, can `wh keyset remove rt` reach *any* selection on a board where every key already sits in
an rt keyset; `wh keyset delete rt <index>`, which still takes `--press`/`--release`, is the route
there. Where an `ap` whole-board removal does go through, its outcome resembles `RESET KEYSETS` in
the configurator (every keyset destroyed) but is not identical: `RESET KEYSETS` is measured writing
over each keyset's existing members only, leaving an already-free key at whatever stray value it
held, while `wh` writes every selected key including free ones, rewriting that stray value to the
base too. The traffic differs the same way: no `0xFF`/`0xFE` record for a key that was already free
from `RESET KEYSETS`, an unconditional one from `wh`, matching `plan`'s existing rule for `create`
and `delete`. `remove` asks for a typed `yes` before a whole-board write, built after computing
what the write actually contains rather than before, so it names the value every key is about to
move to, every keyset that will cease to exist, and, if any key's touch mode is about to move too
(rapid trigger switching off, or a key coming off "follow global travel"), a count of how many:
otherwise a board where every key already holds the target value and no keyset exists to lose reads
as a complete no-op right up until the write that pins every key's actuation point. `--dry-run`
never prompts, since it writes nothing. The prompt itself goes to stderr, not stdout: redirecting
`wh keyset remove ap --keys all > log.txt` still shows it on screen and still reads the typed
answer from stdin, rather than trapping it in the file with nothing left to answer.

**Creating a keyset writes the same value to every member.** It writes the board's current global
value by default, or an explicit `--value`/`--press`/`--release` if given: `wh keyset create ap
--keys u,i,o,p --value 1.5` sets all four to 1.50mm.

**A create whose selection covers every key in the board's matrix asks for a typed `yes` first**,
for both kinds: every key moves into the one new index, so every existing keyset of that kind
loses all its members and ceases to exist, the same destruction `wh keyset remove --keys all` and
`wh set ap --keys all` already guard by a different route. Like those two, the prompt is built
after computing what the write actually contains, so it names the new keyset's index and value,
every keyset that will cease to exist (or says there are none to lose), and, if any key's touch
mode is about to move too, a count of how many. For `ap` that is keys coming off "follow global
travel" onto their own actuation point. For `rt` it is two separate counts, because one sentence
cannot honestly cover both cases: keys that had rapid trigger **off** are told it is being
switched on, which changes how they behave under a keypress, while keys that already had it on
following the board's global sensitivity are only moving onto their own, and are counted and
described separately. A touch mode `wh` has never measured is counted with the second group,
whose wording claims only where the key ends up, so nothing ever asserts an unmeasured mode was
off. Without these counts a board with no keysets of that kind reads as though nothing much is
about to happen, right up until the write that pins every key permanently. It fires on the
resolved selection covering the matrix, however it is spelled, not on the literal word `all`.
`--dry-run` never prompts, since it writes nothing, and there is no bypass flag, so a script that
used to run `wh keyset create ap --keys all` unattended now needs a `yes` on its stdin. The prompt
goes to stderr, not stdout, so redirecting the command's output still shows it on screen and still
reads the typed answer from stdin.

**`wh set ap` over a selection that is not exactly one existing keyset's members moves the whole
selection into one new keyset, and says so.** Four shapes: a selection that is part of a keyset
moves the selected members into a new index, leaving the rest of that keyset in place (`wh set ap
--keys w,s --set 1.5`, where `w` and `s` sit inside a keyset with `a` and `d`, moves `w` and `s`
into a new index and leaves `a` and `d` in the original one). A selection that is a whole keyset
plus a free key moves the keyset and the free key together into a new index. A selection spanning
two existing keysets moves the selected members from each into one new index, leaving each
keyset's unselected members in place. And a selection where every key is free also allocates a new
keyset for them: giving a free key its own value is what puts it in one.

A newly allocated index is one more than the current maximum live index, or `1` if none exists.
Selecting the whole board with `--keys all` follows the same rule as any other selection: it
leaves every keyset's membership as it is only when one keyset already holds every key on the
board (the value written still changes); on any board where that is not so, including a board
where every key is already free, it creates one new keyset holding every key, and every keyset
that existed before ends with no members. See `docs/keysets.md`, "Changing a value over a selection
that is not exactly one keyset", for what evidence supports each shape.

**Selecting the whole board now asks for a typed `yes` first**, the one case above that is not the
"leaves membership as it is" one: every existing `ap` keyset loses all its members, so every one of
them ceases to exist. The same guard as `wh keyset remove`'s own whole-board prompt, built after
computing what the write actually contains, so it names the new keyset's index, every keyset that
will cease to exist, and, if any key's touch mode is about to move too (a key coming off "follow
global travel"), a count of how many: otherwise a board with no `ap` keysets at all reads as though
nothing is about to happen right up until the write that pins every key's actuation point. It fires
on the resolved selection covering the board's matrix, however it is spelled, not on the literal
word `all`, and not when the whole board already is exactly one keyset, since nothing would be lost
in that case. `--dry-run` never prompts, since it writes nothing, and there is no bypass flag. A
script that used to run `wh set ap --keys all --set <mm>` unattended now needs a `yes` on its stdin,
the same way a script driving `wh keyset remove ap --keys all` already does. The prompt itself goes
to stderr, not stdout, so redirecting the command's output still shows it on screen and still reads
the typed answer from stdin.

**`wh set ap --base <mm>` changes the board's base actuation point instead, and is not the same
thing as `--keys all --set <mm>` above.** The base is not a stored setting: it is what every key
outside a keyset already holds in layout `0x04`. `--base` writes that value to every free key and
touches no keyset at all, so every existing keyset keeps its own value untouched, while `--keys all
--set <mm>` enrols every key, keyset members included, into one brand new keyset holding `<mm>`,
destroying every keyset that existed before. `--base` takes no `--keys`, since it names the board
rather than a selection, and refuses alongside `--set`, which names a selection's value instead of
the board's. It refuses outright, rather than writing nothing and reporting success, when every key
on the board is already in a keyset. Measured 2026-09-04 against the vendor's own GLOBAL ACTUATION
POINT field, which writes exactly this shape; see `docs/keysets.md` for the frame counts.

```
wh set ap --base 1.95
```

**`wh set mm --value <mm>` is a different setting again, and together with `--base` above it is the
pair operators most often confuse.** It writes the configurator's `"MM" CUSTOM VALUE`, the step size
for its `< >` stepper controls, held in the `cmd 0x29` global record alongside the dead zones; it is
not an actuation point at all, base or otherwise, and takes no `--keys` or `--pick` since the board
holds one value for the whole board. It reads the current value first so the announcement names
both the old and new value, or that the board already holds it and nothing was written; when it does
write, it sends the new value with the vendor's own dead-zone constants (`200`/`200`), the same ones
`wh restore` sends.

```
wh set mm --value 1.5
```

Manage SOCD pairs (simultaneous opposing cursor direction): two keys whose opposing inputs, held
together, resolve to a single one instead of cancelling or both registering.

```
wh socd list
wh socd pair a d
wh socd pair a d --priority d
wh socd unpair a
```

`wh socd list` prints one line per pair with the winner named, `a + d, priority: d` or
`q + e, priority: last-input`, never the wire's own priority byte: the board answers a query on one
member with the two records reordered and the byte re-based, so the same setting has two spellings
and only the winner is meaningful on its own. `wh` normalises both spellings to the same pair, which
is also why each pair is listed once although both members are queried.

`--priority` names one of the two keys, or `last-input` (the default), matching the configurator's
own PRIORITY selector. It takes exactly two different keys, and the order they are given in is the
order that reaches the wire, which changes the priority byte but not the setting.

**Every key argument goes through the same selector grammar as `--keys`, resolved against the
board's live matrix, and must name exactly one key.** So a key name, a stored or builtin group that
holds exactly one key, or the hex form `wh socd list` itself prints (`0xA0`) all work, typos get the
usual "did you mean" hint, and a key this board does not have is refused rather than paired: the
board accepts arbitrary usages on the wire, but a pairing on a key that is not there is one
`wh socd list` cannot show you and `wh socd unpair` cannot undo. A selector matching several keys is
refused for arity, which is also why there is no whole-board form here and so no typed-`yes` guard
to worry about.

**A key may sit in one pair only.** `wh socd pair` refuses if either key is already paired, names
the pair that holds it, and points at `wh socd unpair`. That is the vendor UI's model; whether the
board would accept a key in two pairs is unmeasured, and refusing means `wh` never finds out by
accident. `wh socd unpair` takes any member of a pair and removes the whole pair, both keys;
naming both members of one pair removes it once. A key that is in no pair is refused by name before
anything is written.

Participation is a flag in each key's mode value, and **the board sets that flag itself** on a pair
write, so `wh socd pair` sends one frame and no mode record, and says so. `wh socd unpair` is the
mirror: it clears the flag on both keys and sends no pair frame at all, which is what the vendor
does too. It preserves each key's own touch mode while clearing the flag, so unpairing a key that
holds its own actuation point does not quietly return it to the global one; every vendor removal
that was captured happened to be on a key with no touch mode set, so that preservation is `wh`'s
own rule rather than a measured one, and the announcement names the modes it is keeping.

A snapshot does not carry pairs, only the mode value that flags them; see the backup section below.

Manage stored groups:

```
wh keys group fps "w,a,s,d,space"
wh keys ungroup fps
wh keys rename fps arrows
```

Pick keys interactively instead of naming them, on any `get`/`set` subcommand:

```
wh set rt --pick --set 0.5
```

Back up and restore a full snapshot. Backups are written as JSON now; older TOML backups are still
read, by file extension:

```
wh backup --to my-profile.json
wh restore my-profile.json
wh restore --last
```

List stored backups, oldest first, each with its timestamp and what took it (`manual`, `set rt`,
`restore`, and so on):

```
wh backups list
```

Read or select the active profile, 1 to 4:

```
wh profile
wh profile 2
```

A self-test that exercises a real write/read round trip, rewriting the board's global record with
the values it just read (see the read-modify-write note below for the one part of that which is not
provably a no-op):

```
wh selftest
```

## A safety note before you write anything

`wh set`, `wh keyset create|set|delete|remove`, `wh restore`, and `wh selftest` write to the physical
keyboard. `wh set rt`, `wh set ap`, `wh set mm`, and every `wh keyset create|set|delete|remove`
accept `--dry-run` (`wh set rt --keys w --set 0.5 --dry-run`), which prints the exact 64-byte reports
a real run would send, and sends nothing. Use it to check a command before it touches hardware,
especially the first time you type a new key selector. `wh restore` and `wh selftest` have no
`--dry-run`; `wh restore` takes its own auto-backup before writing (see below), and `wh selftest`
only ever rewrites a setting to the value it already read.

Every `wh` command that touches the device (`dump`, `get`, `set`, `backup`, `restore`, `selftest`,
`keyset list|create|set|delete|remove`, `profile`) names which transport it opened, on stderr, one line,
before doing anything else: `transport: hardware (real keyboard)` or `transport: replay (<path>)`.
Check that line before trusting that a run did what you expected, especially when driving `wh` from
a script or another tool where the rest of the output might scroll past. `wh keys list` and
`wh keys group` never open a transport at all (they only ever touch the local key store), so they
print no such line; that absence is expected for those two, not a sign the announcement failed.

### Running against a script instead of hardware (`WH_REPLAY`)

Set `WH_REPLAY=<path-to-a-captured-jsonl-script>` and every `wh` command reads a scripted device
conversation instead of opening the keyboard at all; this is how the test suite drives the whole CLI
with no hardware attached, and it is the only way to safely try a command against something other
than your own board.

**On Linux, this just works**, since `wh` there is a native binary reading its own process
environment directly. **Through `bin/wh`, it needs one more thing to be true.** `bin/wh` execs a
Windows binary from a WSL shell, and WSL only carries an environment variable across that
WSL-to-Windows boundary when it is named in `WSLENV`; `bin/wh` sets this for you (`WH_REPLAY/p`, the
`/p` translating the WSL path into one the Windows binary can open), so `WH_REPLAY=<script> ./bin/wh
dump` works exactly as expected. If `bin/wh` cannot confirm the variable will actually reach the
Windows binary (for example, running somewhere `wslpath` is not on `PATH`), it refuses to start
rather than silently falling back to your real keyboard: **a `wh restore` or `wh set` you believe is
a replay must never turn out to have been a real write**, and the transport line above is the second
line of defence for exactly that if the first one is ever wrong.

## What a backup does and does not contain, stated plainly

A snapshot recorded by `wh backup` (or the automatic backup every write command takes first)
contains: the board's global record (`custom_value_mm` and its press/release dead zones), actuation
point and rapid trigger press/release depth for every physical key, the raw per-key mode value, each
key's raw actuation point and rapid trigger keyset value, and, since Phase 1, the profile the board
was on when the snapshot was taken. Snapshots are written as JSON; older TOML backups are still
read, including those written before `custom_value_mm` was called that.

`custom_value_mm` is **not** the global actuation point, whatever an older backup's `travel_mm`
spelling suggests. It is the vendor configurator's `"MM" CUSTOM VALUE`, the step size for its `< >`
controls. The global actuation point is not in that record at all: it is simply what every key
outside a keyset holds, which is what `wh set ap --base` reads and writes.

The two dead zone fields are informational only, a record of what the board reported when the
snapshot was taken, which is `0` for both on every read measured. `wh restore` does not send them:
it writes 200um for each, the value every measured vendor write carries, so hand-editing either
field changes nothing that reaches the board. Whether that 200 is a fixed constant or a user setting
sitting at its default is **not** established, and if it is the latter then a restore overwrites
your choice with no way to tell, since the board reports zero for both however they were set.
`docs/backlog.md` records the open question and what would settle it.

Each key's `rt` field in the snapshot file is informational only, a human-readable summary of the
raw mode value at the moment the snapshot was taken. `wh restore` never reads it; it writes the raw
mode value back verbatim. Hand-editing `"rt": false` in a snapshot file before restoring it does not
turn rapid trigger off, and `wh restore` will report success and a verified readback while doing
exactly that: writing the mode value the file actually carries, unaffected by `rt`. If you want to
change what a restore writes, change the settings on the board and take a fresh backup, not the
`rt` field in an old one. The keyset fields are read into the snapshot and `wh restore` writes them
back too, when the snapshot recorded them: values first, batched, then membership one record per
key per layout, last, the vendor's own per-operation shape (measured, `docs/keysets.md`); applying
that shape to a whole-board restore, including writing every key's actuation point membership
before any key's rapid trigger membership, is not itself measured, since no capture contains a
`wh restore` at all. A snapshot taken before these fields existed has no membership to write back:
`wh restore` leaves those keys' membership on the board exactly as it found it and says so on
stderr, rather than asserting the `0` the missing fields would otherwise default to.

**It does not contain**, and `wh restore` cannot bring back:

- The base layer key mapping (which physical key produces which keystroke).
- The FN layer mapping.
- SOCD pairings and their priorities, which `wh socd` reads and writes but a snapshot does not
  carry. This one can diverge rather than simply be missing: the raw mode value a snapshot does
  store carries the SOCD participation flag in its advanced nibble, so restoring an old snapshot
  can set that flag on a key whose pairing is gone. `wh socd list` refuses on such a board, naming
  what it found, rather than showing half a pair. Take a fresh snapshot after changing pairs.
- Dynamic keystroke, mod tap, or any other advanced-key behaviour beyond the raw mode value.
- Gamepad configuration.
- RGB lighting.
- Polling rate.

**`wh restore` is not a factory-reset recovery path.** It restores exactly the settings listed above,
and nothing more, and it refuses outright, before writing anything, in three cases. None of them has
an override:

- If the snapshot recorded a profile and the board is currently on a different one, `wh restore`
  refuses: restoring would silently overwrite the wrong profile's settings, which `wh` will not do
  even if asked. Switch the board to the recorded profile first, or restore only when you actually
  mean to overwrite the profile you are currently on.
- If the snapshot has no recorded profile at all, `wh restore` refuses, since nothing can establish
  which profile the settings belong to. `wh` never writes such a snapshot: the board reports its
  profile as a zero-based wire index, and one outside `0..=3` (the four profiles the board has)
  fails the read outright rather than being recorded as an unknown profile.
- If the snapshot carries a key the board in front of you does not have, `wh restore` refuses and
  names the keys. A snapshot taken on a different key matrix is unrestorable rather than partly
  restorable, deliberately: restoring it would write to keys this board does not have and then
  report them verified, because the readback re-reads exactly the keys it wrote to.

In all three cases, take a fresh snapshot on the board you are restoring to.

If you need to undo a change to remapping, lighting, or anything else in the list above, use the
board's own **RESET PROFILE** or **FACTORY RESET** under **Advanced > General** in the vendor web
configurator; `wh` does not implement either. SOCD is the exception: a snapshot cannot bring a pair
back, but `wh socd unpair` undoes a pairing directly, and `wh socd pair` recreates one.

## No drift: `wh` caches no device state

Every `wh` command reads live over HID. There is no local cache of the board's settings, which is
why `wh` cannot show a stale value the way the web configurator sometimes can: there is nothing
cached to go stale.

Two things look like exceptions and are not:

- `set rt`, `set ap`, and `keyset create`/`set`/`delete`/`remove` each read a key's current
  settings, then write back a change built from that read (all but `set rt --set` through the same
  `keyset::plan`).
  `selftest` does the same at the board level, not a key's: it reads the global custom value and
  both dead zones and writes back exactly what it read, to prove the write path works. That is a
  no-op for the custom value. For the dead zones it is only a no-op if the board really does hold
  the zeros it reports for them, which is unestablished (`docs/backlog.md`): `selftest` is the one
  place `wh` still writes a zero dead zone. Between a read and its write, the board could in
  principle be changed by hand (or by another tool); that is a real read-modify-write window, not
  `wh` caching anything. `set mm` reads the same global record too, both to make its announcement
  honest about the value it is about to replace and to tell whether it needs to replace anything at
  all: when the board already holds the target value, it announces that and skips the write
  outright, unlike `selftest`, which always writes back what it read. When it does write, it sends
  the vendor's own dead-zone constants rather than whatever it just read.
- A snapshot is a point-in-time copy by definition. `wh restore` writing it back is the snapshot
  doing its job, not drift.

## Hardware verification

### Confirmed on the real board, 2026-09-04

Every result below was measured on **profile 2**. Per-key state is per profile.
`layout-16-by-profile` measures profile 1 only, since it selects profile 2 as its last frame and
stops; that the two profiles held different actuation points, sensitivities and keysets that day is
measured on the profile 1 side and corroborated on the other by the operator's own note. Nothing
here has been checked on profile 1.

- **`wh set ap` on a free key allocates a keyset.** `H` sat at the global 2.00mm in no keyset;
  `wh set ap --keys h --set 1.5` created keyset 10 over it alone and read back `h: ap 1.50mm
  keyset 10`, leaving the four existing keysets untouched. Index 10 is max plus one over the live
  `{2,7,8,9}`, allocation measured on hardware rather than from frames.

- **`wh keyset remove ap` returns a key to the global and collapses an emptied keyset.**
  `wh keyset remove ap --keys h --value 2.0` read back `h: ap 2.00mm keyset none` and keyset 10
  ceased to exist, since `H` was its only member. The other four keysets were untouched. `--value`
  no longer exists on `remove`; the command shown is what actually ran that day, not today's syntax
  for the same result (`wh keyset remove ap --keys h`, no flag needed).

- **`wh` refuses to guess an ambiguous global, but excluding the key being reset from that reading
  removes the motivating case for it.** The first attempt at that removal, with no `--value`, was
  declined: "the keys outside every keyset disagree on the global actuation point (57 key(s) at
  2.00mm, 1 key(s) at 1.10mm)", `H` itself being the one key at 1.10mm, still outside a keyset and
  therefore counted in its own reading. Today `remove` excludes every key it is about to reset from
  that reading, so this exact scenario would succeed with no flag at all: the remaining 57 keys
  agree at 2.00mm, and `H`'s own stray 1.10mm is no longer in the count to disagree with them. The
  refusal itself is still real, on any board where the keys left *outside* the selection disagree
  with each other.

- **`wh set ap` on a rapid trigger keyset member leaves rapid trigger alone and adds actuation
  point membership.** `M` was in rt keyset 1 at 0.30/0.40mm. `wh set ap --keys m --set 1.3` sent
  MODE `0x0030`, the key's own touch nibble 3 resent unchanged, with press and release echoed at
  300 and 400. Afterwards `M` held **both** memberships at once, ap keyset 10 and rt keyset 1,
  which is the dual membership the corpus measures, now created by `wh`.

- **`wh keyset remove rt` turns rapid trigger off and preserves the key's own actuation point.**
  This is the fact most easily destroyed by a wrong implementation, so it was set up to be visible:
  `M` held 1.30mm against a global of 2.00mm. The write sent MODE `0x0010`, press and release back
  to the global 100, `0xFE = 0`, **AP `0x0514` (1300) rewritten unchanged, and no `0xFF` record at
  all**. Readback: `m: rt off press 0.10mm release 0.10mm keyset none` and `m: ap 1.30mm keyset 10`.
  Rapid trigger keyset 1 ceased to exist, `M` having been its last member.

- **`wh keyset create`, `set`, `delete`, and `wh set ap`'s split all work against the keyboard.**
  Each verified its own readback and took an auto-backup first. `create` over three free keys
  already at the global emitted membership records only, the skip rule; `set` emitted values and no
  membership; `delete` returned its members to the global and cleared membership last, one record
  per frame; and `wh set ap` over two of a four-member keyset moved exactly those two into a fresh
  index and left the other two where they were.
- **`wh set ap --keys all` and `wh restore` both work, and together they are a full round trip.**
  The whole-board set collapsed four keysets into one index over all 68 keys, 91 frames of which 68
  were membership, one record per key. `wh restore --last` then put all four back **including their
  indices, 2, 7, 8 and 9, gaps and all**. Allocation is max plus one, so no `create` could reproduce
  that set; restoring a snapshot's indices verbatim is what `KeysetIndex::restoring` exists for and
  this is its first confirmation on hardware.
- **Timings, measured, and they retire a concern rather than confirming one.** Whole-board
  `wh set ap`: 0.85 s, against 0.52 s for its dry run. `wh restore`: 0.70 s. Full `wh dump`: 0.47 s,
  despite now issuing six reads per key rather than four. Roughly 1300 HID roundtrips complete
  inside a second; there is no performance problem to design around.
- **`wh profile` round trips.** Read `2`, switch to `1`, read `1`, switch back to `2`, read `2`,
  with the per-profile snapshot warning printed on each switch.
- **The vendor cannot tell `wh`'s keysets from its own.** Operator observation of the interface, not
  a frame measurement: with two keysets made by the vendor and two produced by `wh`, the
  configurator's actuation point tab listed all four in its own pane, in ascending index order, with
  their values rendered normally. Every key `wh` had written showed its value undimmed.
- **`wh set ap` on a rapid trigger keyset member leaves rapid trigger alone.** `N` and `M` were put
  in rapid trigger keyset 1 at 0.30/0.40mm, then `wh set ap --keys n --set 1.1` moved `N`'s actuation
  point. Afterwards `N` still read rapid trigger on at 0.30/0.40mm in keyset 1, and `M` was
  untouched. The MODE record sent was `0x0030`, the key's own nibble 3 resent unchanged, which is
  `keyset::plan` promoting nibble 0 and nothing else. Creating that keyset also confirmed the
  **separate counters**: the rapid trigger index allocated as `1` while the actuation point counter
  stood at `10`.
- **The configurator greys on keyset membership, and on nothing else.** Settled by two controlled
  experiments, each changing one variable (`docs/backlog.md`). A key moved to MODE touch nibble 0,
  the only one on the board holding it, rendered identically to its nibble-1 neighbours outside any
  keyset at the same value, so the nibble is irrelevant. A keyset then created at exactly the global
  value rendered highlighted while a non-member holding that same value stayed grey, so the value is
  irrelevant. Layout `0xFF` is the whole of it.

### Still outstanding

These are built and tested against replay scripts, not yet confirmed on the real board:

- If `wh set ap` fails part way through its write batch, expect a partial result. `keyset::plan`
  packs each key's own value records (MODE/AP/RT_PRESS/RT_RELEASE) into one frame, so a
  failure among them can only land between keys, never inside one key's own group; a split's
  membership records follow, one key per frame, so the same is true there too. But across the two
  halves, a failure can now leave a key's values changed with its membership untouched, or move
  some of a split's keys into the new keyset while leaving others behind in the old one.
  **`wh restore --last` does fix this now.** It restores AP, MODE, RT_PRESS, RT_RELEASE, and both
  keyset memberships from the auto-backup taken before the write, values first and membership one
  record per key per layout, last: the vendor's own per-operation shape, measured
  (`docs/keysets.md`). Applying that shape to a whole-board restore, including writing every key's
  actuation point membership before any key's rapid trigger membership, is not itself measured; no
  capture contains a `wh restore` at all. The restore path itself now works on hardware, see above;
  what is untested is a restore run against a board left half-written by a failure.


## Protocol

See `docs/protocol.md` for the wire protocol this tool speaks, and `docs/protocol-inventory.md` for
the underlying measured frame counts it is built from.

## Licence, warranty, and liability

Read this before you run anything in this repository against a keyboard you care about.

### Licence

**`wh` is licensed under the Apache License 2.0.** See `LICENSE` for the full terms and `NOTICE` for
the attribution that goes with them.

You may use, modify, redistribute and fork this work, including commercially. Apache-2.0 asks a few
things in return, and section 4 has the detail:

- Keep the licence and the copyright notices with anything you redistribute.
- **Carry the `NOTICE` file's contents in your own distribution.** That is what keeps the credit and
  the link back to this repository attached to forks, and it is a requirement of the licence rather
  than a request.
- State that you changed the files, if you changed them.

### Credit

Originally developed by **brux**: <https://brux.gg/>, `@brux` on Discord, repository owner
`iainbrux`. Source: <https://github.com/iainbrux/keyboard-cli>.

If you fork this, please keep pointing back here. It costs you nothing and it is how anyone finds
where the work came from.

### The keyboard is Wallhack's

The Wallhack K-001, its firmware, its hardware design, its communication protocol, the Wallhack name
and logo, and the web configurator at terminal.wallhack.com all belong solely to Wallhack. This
project claims none of it.

This is an independent, unofficial project. It is **not affiliated with, endorsed by, sponsored by,
or supported by Wallhack.** `wh` is an independently written client that talks to the keyboard over
the USB HID interface the device already exposes. The notes in `docs/protocol.md` describe observed
device behaviour, recorded from traffic between a keyboard and its own vendor software on hardware
owned by the author. They describe an interface; they are not a copy of anyone's software.

Apache-2.0 grants no trademark rights, and none are claimed here.

### Other people's code, which stays theirs

**Parts of `crates/wh-proto` are a port of MIT licensed Sparklink Playjoy source**, and that notice
travels with the port. `THIRD_PARTY_NOTICES.md` names the files.

**Reference material under `research/`** is third-party work under its own MIT and ISC licences,
which this repository's licence does not override. Same file.

**The dependencies compiled into a released binary** are listed with their full licence texts in
`THIRD_PARTY_LICENSES.md`: 90 crates, generated from the real dependency graph, **plus a separate
section covering the Rust standard library's own runtime and the mingw-w64 C runtime**, neither of
which is a crates.io dependency and so neither shows up in a dependency-graph walk on its own; that
file explains why and what is in each. **If you distribute a binary of this project, those
obligations pass to you.** Three of the crate entries need more than a notice, and that file explains
each: HIDAPI is triple-licensed and this project elects the BSD-style option rather than the GPL,
`option-ext` is MPL-2.0 so its source must stay obtainable by recipients, and `unicode-ident` carries
a Unicode term on top of its permissive choice.

### No warranty

`wh` is provided **as is**, with no warranty of any kind, express or implied, as Apache-2.0 section 7
sets out.

It writes settings to keyboard hardware over a protocol worked out by observing traffic, not from a
specification anyone published. It has been tested against exactly one board, on one firmware
version.

**Using this tool may void your keyboard's manufacturer warranty.** Neither brux nor Wallhack is
obliged to support, update, or repair any device it has been used with.

### No liability

Neither brux nor Wallhack accepts any liability for anything that happens as a result of using this
tool, as far as the law allows and as Apache-2.0 section 8 sets out. That includes damage to or
malfunction of a keyboard or any other hardware, loss of settings, a voided warranty, and any direct,
indirect, incidental, special, or consequential damage.

**You use it entirely at your own risk.** If that is not acceptable to you, use the vendor's own web
configurator instead.

Nothing here excludes liability where the law does not allow it to be excluded.
