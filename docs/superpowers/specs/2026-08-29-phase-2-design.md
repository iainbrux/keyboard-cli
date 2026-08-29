# Phase 2 design: keysets, JSON, and the rest of the CLI surface

Date: 2026-08-29
Status: approved, ready for an implementation plan

## Objective

Close to 1:1 interoperability between `wh` and terminal.wallhack.com. A setting changed from the CLI
should render in the vendor configurator exactly as if the configurator had made it, and everything
the configurator shows should be readable from the CLI.

Phase 1 delivered the codec, the transport, snapshots, groups and the read and write paths, all
verified against a real board. Phase 2 closes the gap between "our writes work" and "our writes look
native".

## Evidence base, and two corrections

Every claim below was measured from the 1224 captured frames in `captures/`. Two entries in
`docs/backlog.md` did not survive that check and are corrected here.

### What the vendor writes on an actuation point change

Measured from `captures/ap-wasd-1.2.jsonl`, three slider drags on w, a, s, d. Each drag is a
five-frame sequence, and MODE is written three times per drag. The vendor is redundant; what matters
is the resulting per-key state:

| Layout | Value written | Meaning |
|---|---|---|
| `0x04` | the new depth | the actuation point |
| `0x08` | `0x18` | MODE, touch nibble 1 (Single), advanced nibble preserved |
| `0x14`, `0x15` | unchanged | rapid trigger press and release, written back as-is |
| `0x16`, `0x17` | `0` | never observed non-zero anywhere |

The touch nibble is the marker. Nibble 0 (Global) means the key follows the global travel setting.
Nibble 1 (Single) means the key has its own actuation point. The vendor writes nibble 1 on every
actuation point change.

`wh set ap` writes layout `0x04` alone, so it leaves a key on nibble 0 holding a private value. That
is a state the vendor never produces, and it is why the configurator renders our changes greyed. It
is a real inconsistency in the record, not a cosmetic one.

### Correction 1: `0xFF` as the actuation point keyset index is unproven

`docs/backlog.md` states that layout `0xFF` holds the actuation point keyset index. Across all 1224
frames:

```
write records by layout:  0x00:515  0x04:19  0x08:38  0x14:19  0x15:19  0x16:19  0x17:19  0xfe:2
read  records by layout:  ... 0xfe:195  0xff:195
```

`0xFF` is read 195 times and written zero times. The claim rests entirely on read correlation: it
reads `1` for w, a, s, d and `2` for esc, matching the two keysets the configurator displayed.
`captures/ap-wasd-1.2.jsonl` only changes the value of an existing keyset, so it writes no `0xFF`.

We have never captured the creation or deletion of an actuation point keyset. We therefore do not
know what writes `0xFF`, or whether anything host-side does. This is the same shape of error as the
earlier "nibble 0 discards the actuation point" claim: a correlation promoted to a mechanism.

`0xFE` is different and its conclusion stands. It has direct write evidence: `1` when rapid trigger
was switched on for w (`captures/rt-on-w-0.5.jsonl`), `0` when switched off
(`captures/rt-off-w.jsonl`).

### Correction 2: whether the vendor forces nibble 1 is unmeasured

In `captures/ap-wasd-1.2.jsonl` all four keys read back MODE `24` (nibble 1) **before** the first
write. The capture cannot distinguish "force nibble 1" from "write back the current nibble". We have
no capture of an actuation point change on a key that had rapid trigger on.

This matters because forcing nibble 1 would silently disable rapid trigger on such a key.

### Profile select

Measured from `captures/profile-switch.jsonl`:

```
out cmd=0x00 len=4 payload=7001      select profile 2
in  cmd=0x80 len=4 payload=0070      ack
out cmd=0x00 len=4 payload=7000      select profile 1
in  cmd=0x80 len=4 payload=0070      ack
```

The select payload is exactly `[0x70, wire_index]`, two bytes. `cmds::cmd_order` pads to
`[0x70, arg, 0xFF, 0xFF]`, so reusing it would send a frame the vendor never sends. The select needs
its own encoder. The ack carries no index, so a select must be confirmed by re-reading.

## The no-drift invariant

**`wh` caches no device state. Every command reads live over HID.**

This is the structural reason the CLI cannot show a stale value where the web configurator can. It
holds for `dump`, `get`, `keys`, `backup` and the read half of every write command. Settings changed
on the board itself, by the AP and RT keys or an FN combination, are picked up on the next command
with no synchronisation step.

Two exceptions, recorded honestly:

1. **The read-modify-write window.** `set rt`, `set ap` and `selftest` read the current MODE, compute
   from it, then write. If the board is changed by hand between the read and the write, we write from
   a stale read. The window is milliseconds. It is the only place `wh` can drift.
2. **A snapshot is a point-in-time copy by definition.** `restore` writing it back is intended
   behaviour, not drift. The existing profile guard blocks the dangerous case.

This invariant is a constraint on later phases as much as a description of this one. The TUI in the
backlog is the first component that will hold state across time, and it must re-read rather than
cache.

## Tasks

### 2.0 JSON replaces TOML

Snapshots and the group store serialise as JSON. `serde_json` is already a dependency, so this is a
serialisation swap rather than new machinery.

- `wh dump` outputs JSON by default. The human-readable table moves behind `--table`.
- `wh backup` writes `.json`.
- The group store writes `.json`.
- **Migration: read both, write only JSON.** `wh restore` and `wh backups list` select the parser from
  the file extension. The group store does the same. Every new file written is JSON.
- No file is auto-converted on read. A command that quietly rewrites existing backups is worse than
  one that just reads them.

The migration path is not optional politeness: the hardware session's backups are TOML and are the
evidence that the write path works against a real board.

### 2.1 Read keyset membership

No hardware needed. Comes straight after 2.0, so `Snapshot` changes shape once rather than twice,
and lands before the capture session so 2.3 can be verified with `wh dump` instead of raw hex.

- Add `layout::KEYSET_AP = 0xFF` and `layout::KEYSET_RT = 0xFE` to `crates/wh-proto/src/cmds.rs`.
- `read_key_settings` reads both. `KeySettings` carries them as raw `u16` with no interpretation,
  because whether `0xFE` is a boolean or an index is unmeasured.
- Surface in `wh dump` (both JSON and `--table`), `wh get ap`, `wh get rt`, and snapshots.
- New snapshot fields are `#[serde(default)]`. `restore` ignores them entirely; writing them is 2.4.

Cost: two extra reads per key, so a full dump goes from 272 roundtrips to 408. Measure the real
elapsed time on hardware before accepting it.

### 2.2 Actuation point writes match the vendor

`ap_records` becomes a read-modify-write, the shape `rt_records` already has. Per key it writes:

- `0x04` = the new actuation point.
- `0x08` = MODE with the touch nibble set to **1 (Single)** if the key is currently **0 (Global)**,
  and **left unchanged** if it is 3 (Rt) or 4 (RtContinuous). The advanced nibble and the high byte
  are preserved, as they are today.

Leaving nibbles 3 and 4 alone is the deliberate answer to correction 2. An RT key still carries its
own actuation point (`captures/rt-on-w-0.5.jsonl` writes MODE nibble 3 and keeps `w:0x04=300`), so an
actuation point change is not a request to disable rapid trigger. This is the same protection
`rt_records` already applies to `RtContinuous`.

Records that would not change anything are skipped, the rule `rt_off_records` already follows. That
is why `0x14`, `0x15`, `0x16` and `0x17` are not written: the vendor writes them at their existing
values.

Consequence: `ap_records` needs a `Session` where it is currently pure, so its existing unit test
moves to the replay harness.

### 2.3 The keyset capture session

Requires the board and the vendor configurator. Seven steps, each its own capture file, all on keys
never touched in Phase 1.

1. A page load, for a clean baseline.
2. Create an actuation point keyset on `u,i,o,p`. **Does anything write `0xFF`, and what index?**
3. Create a second actuation point keyset on `j,k`. Allocation rule: `3`, or next after existing?
4. Create a rapid trigger keyset on `u,i`, deliberately overlapping step 2. Confirms the two
   groupings are independent.
5. Create a second rapid trigger keyset on `m`. **Does `0xFE` reach 2, or is it a boolean?**
6. Delete the actuation point keyset from step 2. What does a delete write?
7. Create a third actuation point keyset. Does it reuse the gap, or take the maximum plus one?

Steps 2 and 5 carry the most information. If step 2 shows nothing writing `0xFF`, the field is
firmware-derived, keyset writing is not available to any host tool, and 2.4 leaves the plan.

### 2.4 Write keyset membership

Deliberately unspecified. Blocked on 2.3. Designing it now would mean guessing the allocation rule,
which is exactly how the `0xFF` claim in correction 1 came about.

### 2.5 `wh profile`

- `wh profile` prints the active profile.
- `wh profile <1-4>` selects it.

The select encoder emits exactly `[0x70, wire_index]`. Verify the ack is `[0x00, 0x70]`, rejecting a
reply whose sub-order byte is anything else, then re-read the profile to confirm the switch landed
rather than trusting the ack.

No auto-backup: this is a mode switch, not a settings write. The command should say plainly that
snapshots are per-profile, since switching profiles is what makes `restore` refuse.

### 2.6 `wh backups list`, and what `--last` means

**Decision: `--last` keeps meaning "the most recent snapshot, whatever took it".** Silently changing
an existing flag's meaning is worse than the current surprise. During the hardware session the tool
was correct and only the visibility was wrong, so fix the visibility.

- `Snapshot` gains `origin: Option<String>`, serde-defaulted, holding `manual` or the command that
  triggered the automatic backup, for example `auto: set rt`.
- `wh backups list` prints timestamp, origin, profile and filename.
- `wh restore --last` prints which snapshot it picked, and its origin, before restoring.

No `--last-manual`. Once backups can be listed, a specific one can be named.

### 2.7 Delete and rename a stored key group

`wh keys ungroup <name>` and `wh keys rename <old> <new>`. Both take the name in a position that is
unambiguously a group rather than a selector, so the ambiguity guard that creates the problem cannot
also block the recovery from it. `rename` refuses a new name colliding with a key name or a builtin
group, the same guard as create.

### 2.8 Hex form in the selector

`Selector::parse` accepts `0x01`, so a key with no name can be typed back after `wh keys list` prints
it. Creating a group whose name parses as a hex form is refused, the same class as the existing
key-name collision.

### 2.9 Documentation fixes

The seven parked inaccuracies listed in `docs/tasks.md`, plus:

- Correct the `ap_records` doc comment added on 2026-08-29. It says hardware proved the MODE write is
  not needed for the value to take effect. That is true but reads as a justification for omitting it,
  which 2.2 supersedes.
- Correct the `0xFF` claim in `docs/backlog.md` and `docs/protocol-inventory.md` per correction 1.

## Testing

- Every write change gets a replay fixture built from real capture bytes, never hand-written.
- 2.2 specifically needs a test asserting that a key at MODE nibble 3 comes back at nibble 3. That is
  the regression the whole rule exists to prevent, and a hand-written fixture would pass it while
  being wrong.
- 2.0 needs a test that a TOML snapshot from Phase 1 still restores, and that a JSON snapshot
  round-trips.
- 2.1 needs a test that a snapshot written before the keyset fields existed still deserialises.
- `ReplayTransport` matches byte for byte. Loosening that to make a test pass defeats the harness.

## Order and gates

| Step | Task | Hardware |
|---|---|---|
| 1 | 2.0 JSON migration | no |
| 2 | 2.1 read keysets | no |
| 3 | 2.9 documentation fixes | no |
| 4 | 2.2 actuation point write | to verify |
| 5 | 2.3 capture session | **yes** |
| 6 | 2.5, 2.6, 2.7, 2.8 | to verify 2.5 |
| 7 | 2.4 keyset write | blocked on step 5 |

## Out of scope

Key remapping (layouts `0x00` and `0x01`), SOCD (command `0x2c`), RGB and the LEDs (command `0x18`),
polling rate, the TUI, the Windows installer, and trimming `research/`. All remain in
`docs/backlog.md`.
