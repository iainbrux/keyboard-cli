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

## The exclusive-access caveat

**The vendor's own web configurator, `terminal.wallhack.com`, holds the device exclusively while its
tab is open.** If a browser tab to that site is open (even in the background, even if you are not
looking at it), `wh` will fail to open the device. Close the tab first. This bites everyone once:
the failure looks like a missing or broken keyboard, not like a competing process, because nothing
about the error names the browser.

## Commands

Read the whole board configuration:

```
wh dump
wh dump --json
```

Read or write rapid trigger and actuation point for a key selection:

```
wh get rt --keys "w,a,s,d"
wh set rt --keys "w,a,s,d" --set 0.5
wh set rt --keys "w,a,s,d" --press 0.4 --release 0.6
wh set rt --keys "w,a,s,d" --off
wh get ap --keys wasd
wh set ap --keys wasd --set 1.2
```

Key selectors accept comma-separated names, contiguous runs typed as one word (`wasd`), ranges
(`f1-f12`), negation (`all,!space`), and user-defined groups (`wh keys group fps "w,a,s,d,space"`,
then `--keys fps`). `wh keys list` shows every known key name and stored group.

Pick keys interactively instead of naming them, on any `get`/`set` subcommand:

```
wh set rt --pick --set 0.5
```

Back up and restore a full snapshot:

```
wh backup --to my-profile.toml
wh restore my-profile.toml
wh restore --last
```

A self-test that exercises a real write/read round trip without changing anything on the board:

```
wh selftest
```

## A safety note before you write anything

`wh set`, `wh restore`, and `wh selftest` write to the physical keyboard. Every write-capable
subcommand accepts `--dry-run` (`wh set rt --keys w --set 0.5 --dry-run`), which prints the exact
64-byte reports a real run would send, and sends nothing. Use it to check a command before it
touches hardware, especially the first time you type a new key selector.

## What a backup does and does not contain, stated plainly

A snapshot recorded by `wh backup` (or the automatic backup every write command takes first)
contains: global travel and its press/release dead zones, actuation point and rapid trigger
press/release depth for every physical key, the raw per-key mode value, and, since Phase 1, the
profile the board was on when the snapshot was taken.

**It does not contain**, and `wh restore` cannot bring back:

- The base layer key mapping (which physical key produces which keystroke).
- The FN layer mapping.
- SOCD (simultaneous opposing cursor direction key pairing).
- Dynamic keystroke, mod tap, or any other advanced-key behaviour beyond the raw mode value.
- Gamepad configuration.
- RGB lighting.
- Polling rate.

**`wh restore` is not a factory-reset recovery path.** It restores exactly the settings listed above,
to exactly the profile they were recorded on (it refuses to restore onto a different profile unless
you pass `--force` and know what you are asserting), and nothing more. If you need to undo a change
to remapping, SOCD, lighting, or anything else in the list above, use the board's own **RESET
PROFILE** or **FACTORY RESET** under **Advanced > General** in the vendor web configurator; `wh`
does not implement either.

## Protocol

See `docs/protocol.md` for the wire protocol this tool speaks, and `docs/protocol-inventory.md` for
the underlying measured frame counts it is built from.
