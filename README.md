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
wh get ap --keys wasd
wh set ap --keys wasd --set 1.2
```

`wh get rt`/`wh get ap` also print the key's raw keyset value as a suffix, `keyset N` or
`keyset none`.

Key selectors accept comma-separated names, contiguous runs typed as one word (`wasd`), ranges
(`a-f`), negation (`all,!space`), user-defined groups (`wh keys group fps "w,a,s,d,space"`, then
`--keys fps`), and a hex usage for a key with no name (`0x01`, as `wh keys list` prints it, typed
back into any selector). `wh keys list` shows every known key name and stored group. A range is a
range over `wh-proto`'s own key table, not the physical layout, so `f1-f12` parses but resolves to
nothing on this board: the K-001 is 68 keys with no F row, and `wh keys list` is the source of truth
for what actually exists to select.

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

A self-test that exercises a real write/read round trip without changing anything on the board:

```
wh selftest
```

## A safety note before you write anything

`wh set`, `wh restore`, and `wh selftest` write to the physical keyboard. `wh set rt` and `wh set ap`
accept `--dry-run` (`wh set rt --keys w --set 0.5 --dry-run`), which prints the exact 64-byte reports
a real run would send, and sends nothing. Use it to check a command before it touches hardware,
especially the first time you type a new key selector. `wh restore` and `wh selftest` have no
`--dry-run`; `wh restore` takes its own auto-backup before writing (see below), and `wh selftest`
only ever rewrites a setting to the value it already read.

Every `wh` command that touches the device (`dump`, `get`, `set`, `backup`, `restore`, `selftest`,
`keyset list|create|set|delete`, `profile`) names which transport it opened, on stderr, one line,
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
contains: global travel and its press/release dead zones, actuation point and rapid trigger
press/release depth for every physical key, the raw per-key mode value, each key's raw actuation
point and rapid trigger keyset value, and, since Phase 1, the profile the board was on when the
snapshot was taken. Snapshots are written as JSON; older TOML backups are still read.

Each key's `rt` field in the snapshot file is informational only, a human-readable summary of the
raw mode value at the moment the snapshot was taken. `wh restore` never reads it; it writes the raw
mode value back verbatim. Hand-editing `"rt": false` in a snapshot file before restoring it does not
turn rapid trigger off, and `wh restore` will report success and a verified readback while doing
exactly that: writing the mode value the file actually carries, unaffected by `rt`. If you want to
change what a restore writes, change the settings on the board and take a fresh backup, not the
`rt` field in an old one. The keyset fields are read into the snapshot and `wh restore` writes them
back too, one record per key per layout, last, matching the vendor's own write template
(`docs/keysets.md`): a restore puts both the values and the keyset membership back to what the
snapshot recorded.

**It does not contain**, and `wh restore` cannot bring back:

- The base layer key mapping (which physical key produces which keystroke).
- The FN layer mapping.
- SOCD (simultaneous opposing cursor direction key pairing).
- Dynamic keystroke, mod tap, or any other advanced-key behaviour beyond the raw mode value.
- Gamepad configuration.
- RGB lighting.
- Polling rate.

**`wh restore` is not a factory-reset recovery path.** It restores exactly the settings listed above,
and nothing more, and it guards the profile they were recorded on with two separate refusals that do
not share an override:

- If the snapshot recorded a profile and the board is currently on a different one, `wh restore`
  refuses unconditionally. There is no `--force` for this case: restoring would silently overwrite
  the wrong profile's settings, which `wh` will not do even if asked. Switch the board to the
  recorded profile first, or restore only when you actually mean to overwrite the profile you are
  currently on.
- If the snapshot has no recorded profile at all (it predates profile recording, or the board it
  came from reported a profile index this build does not recognise), `wh restore` also refuses by
  default, since it cannot verify which profile the settings belong to. `--force` rescues only this
  case, asserting the settings belong to the board's current profile; it does nothing for the
  mismatch case above.

If you need to undo a change to remapping, SOCD, lighting, or anything else in the list above, use
the board's own **RESET PROFILE** or **FACTORY RESET** under **Advanced > General** in the vendor web
configurator; `wh` does not implement either.

## No drift: `wh` caches no device state

Every `wh` command reads live over HID. There is no local cache of the board's settings, which is
why `wh` cannot show a stale value the way the web configurator sometimes can: there is nothing
cached to go stale.

Two things look like exceptions and are not:

- `set rt`, `set ap`, and `selftest` each read a key's current settings, then write back a change
  built from that read. Between the read and the write, the board could in principle be changed by
  hand (or by another tool); that is a real read-modify-write window, not `wh` caching anything.
- A snapshot is a point-in-time copy by definition. `wh restore` writing it back is the snapshot
  doing its job, not drift.

## Hardware verification still outstanding

These are built and tested against replay scripts, not yet confirmed on the real board:

- `wh set ap` on an untouched key should no longer render greyed in the vendor UI. That the MODE
  touch nibble is what causes the greying is a hypothesis, not an established cause: see
  `docs/backlog.md`.
- `wh set ap` on a key with rapid trigger on should leave rapid trigger on. `keyset::plan` resends
  the key's own touch nibble unchanged, since it only ever promotes nibble 0 (`Global`), and `wh`
  checks the readback against what it actually sent, failing the run and naming the key and both
  values (with rapid trigger state on each) if the board reports something else.
- If `wh set ap` fails part way through its write batch, expect a partial result. `keyset::plan`
  packs each key's own value records (MODE/AP/RT_PRESS/RT_RELEASE) into one frame each, so a
  failure among them can only land between keys, never inside one key's own group; a split's
  membership records follow, one key per frame, so the same is true there too. But across the two
  halves, a failure can now leave a key's values changed with its membership untouched, or move
  some of a split's keys into the new keyset while leaving others behind in the old one.
  **`wh restore --last` does fix this now.** It restores AP, MODE, RT_PRESS, RT_RELEASE, and both
  keyset memberships from the auto-backup taken before the write, values first and membership one
  record per key per layout, last: the vendor's own per-operation shape, measured
  (`docs/keysets.md`). Applying that shape to a whole-board restore, including writing every key's
  actuation point membership before any key's rapid trigger membership, is not itself measured; no
  capture contains a `wh restore` at all.
- `wh profile 2` then `wh profile` should confirm the switch landed.
- A full `wh dump` should be timed: it now issues six reads per key rather than four.

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
