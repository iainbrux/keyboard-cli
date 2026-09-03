# Measured protocol inventory

Generated from all ten capture files of the 2026-08-28 hardware session. Everything here is counted
from real bytes. Where a meaning is inferred rather than measured, it says so.

**Every count here is of that ten-file sample and no other.** The corpus is now 27 files and 3696
frames, and several values were first seen after this session. Where a row and `docs/protocol.md` or
`docs/keysets.md` disagree, those two are current and this is a record of what the first session
measured. Any absolute in a row below, "always", "never", "only ever", is a statement about these
ten files.

**1224 frames, 0 framing failures and 0 checksum failures.** The checksum formula
`(0x35 + 0x5C + len + cmd + payload.last()) & 0xFF` holds on every single captured frame in both
directions. Replies set bit 7 of the command byte.

## Commands

Every command is perfectly request/reply balanced across the corpus.

| cmd | requests | replies | Meaning |
|---|---|---|---|
| `0x00` | 42 | 42 | orders, sub-order in `payload[0]` |
| `0x01` | 4 | 4 | SYNC, device identity |
| `0x18` | 6 | 6 | **unidentified.** Suspected LED or RGB; payload has a `7f7f` and `ff00ff00` shape |
| `0x23` | 540 | 540 | KEY, per-key layout records |
| `0x29` | 6 | 6 | DB, the global record |
| `0x2b` | 6 | 6 | DEFKEY, physical key matrix |
| `0x2c` | 8 | 8 | **unidentified.** Almost certainly SOCD: queries by key, replies with symmetric pairs, measured as W with S and A with D. Behaviour measured, name inferred |

## cmd 0x00 sub-orders

| sub | pairs | Meaning |
|---|---|---|
| `0x22` | 5 | unidentified, reply `002200` |
| `0x50` | 3 | unidentified, reply `005000` |
| `0x70` | 4 | **profile read and select.** Arg `0xFF` reads, a zero-based index selects |
| `0xa1` | 6 | unidentified, reply `00a100` |
| `0xb9` | 3 | unidentified, reply `00b900` |
| `0xba` | 3 | unidentified, reply `00ba0000` |
| `0xbb` | 3 | unidentified, reply `00bb0000` |
| `0xbc` | 3 | unidentified, reply `00bc6400` |
| `0xbd` | 9 | unidentified, reply `00bd01ff`. Recurs as a poll rather than sitting in the connect sequence |
| `0xc0` | 3 | unidentified, reply `00c001` |

Order `0x02` (SAVE in the upstream Sparklink spec) appears **nowhere** in the corpus, including
across five complete write sequences.

## Layout ids, counted from cmd `0x23` records only

DEFKEY (`0x2b`) uses a different record shape and is excluded; parsing it as layout records produces
nonsense.

| layout | records | Distinct values | Meaning |
|---|---|---|---|
| `0x00` | 418 | 74 | **base layer key mapping.** Measured: values are HID usages matching each key |
| `0x01` | 420 | 69 | **FN layer key mapping.** Measured, see below |
| `0x04` | 1858 | 7 | actuation point, micrometres. Modelled |
| `0x08` | 2252 | 5 | mode. Modelled |
| `0x14` | 1858 | 3 | RT press, micrometres. Modelled |
| `0x15` | 1858 | 4 | RT release, micrometres. Modelled |
| `0x16` | 1858 | 1 | `0` in every one of these frames. Later sessions read and write `100`, see `docs/keysets.md` |
| `0x17` | 1858 | 1 | Same as `0x16`, and always written with it |
| `0x19` | 700 | 2 | unidentified. Only ever `0x0000` or `0x3e2c` |
| `0xfe` | 424 | 2 | rapid trigger keyset membership, an index and not a boolean: only `0` and `1` in this ten-capture session, reaching `2` only in the wider 27-capture corpus, see `docs/keysets.md`, untouched by edits within a set |
| `0xff` | 420 | 3 | read 210 times, written 0 in this ten-capture session; **host-written and measured since**, see `docs/keysets.md`, which reaches values up to `9` across the wider 27-capture corpus |

The counts above are what these ten captured scenarios happened to exercise, not the fields'
possible ranges: layout `0x04` (actuation point) only ever took `0, 300, 850, 1200, 1700, 2000,
3000`; layout `0x08` (mode) only ever took `0, 16, 24, 56, 72`; layout `0x14` (RT press) only ever
took `0, 100, 500`; layout `0x15` (RT release) only ever took `0, 100, 300, 500`.

### Layout `0x01` is the FN layer, and this is measured

From `initial-load`, reading each key's layout `0x00` against its layout `0x01`:

| key | layout `0x00` | layout `0x01` |
|---|---|---|
| esc | `0x29` esc | `0x35` grave |
| 1 | `0x1e` 1 | `0x3a` F1 |
| 2 | `0x1f` 2 | `0x3b` F2 |
| ... | ... | ... |
| 0 | `0x27` 0 | `0x43` F10 |

FN+Esc gives the backtick and FN+number gives the function keys, which is exactly how the operator
described the board behaving. Two independent series agreeing across 69 distinct values is
measurement, not a guess.

## Device identity, cmd `0x01` reply

Payload is length-prefixed, 60 bytes:

| Offset | Value | Meaning |
|---|---|---|
| `p[8]` | `0x10` | serial length, 16 |
| `p[9..25]` | `3483141393E03502` | serial |
| `p[25]` | `0x10` | firmware length, 16 |
| `p[26..42]` | `App_V1.1.046000\0` | firmware |
| `p[43..54]` | `Aug 20 2026` | build date, NUL-terminated |
| `p[55..60]` | `ff ff ff ff ff` | padding |

Note the vendor UI displays firmware as `V0.046` while the device string is `App_V1.1.046000`.

## The global record, cmd `0x29`

Reply payload is `000000640000000000000000000000` in all three captures that read it. `p[3..5]` is
`0x0064`. We call the field "travel" from the upstream spec, but **the meaning is not measured**: the
vendor only ever reads this record and never writes it, and 0.1mm is not a plausible switch travel
for a board whose printed scale runs to 3.5mm.
