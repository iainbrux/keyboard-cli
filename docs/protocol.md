# The K-001 wire protocol

This describes the HID protocol `wh` speaks to a Wallhack K-001 hall-effect keyboard. It is written
so that someone with no access to this codebase, only a HID sniffer and this document, could
reimplement the read and write paths described here.

Everything in this document is either measured against a real device (the 2026-08-28 hardware
session, ten capture files, 1224 frames total, recorded in `docs/protocol-inventory.md`) or ported
from the vendor's own TypeScript source under `research/proto/package/src/`. Where the two disagree,
this document says so explicitly and follows the measurement, since the vendor source describes a
family of Sparklink-based boards and the K-001 does not follow all of it.

## Framing

Every report is 64 bytes, report ID 0. That report ID held on all 1224 captured frames in both
directions; a HID library that defaults to prefixing a different report ID will silently break
every write.

The header is four bytes, followed by the payload, zero-padded out to 64 bytes total:

```
[0x5C, len, cmd, crc, payload[0], payload[1], ..., payload[len-1], 0, 0, ..., 0]
```

- `0x5C` is a fixed magic byte, always present.
- `len` is the payload length in bytes (not the frame length; the frame is always 64).
- `cmd` identifies the command. See "Commands" below.
- `crc` is the checksum, see below.
- Ported from `research/proto/package/src/utils/index.ts` (`createProtocol`).

## Checksum

```
crc = (0x35 + 0x5C + len + cmd + payload.last()) & 0xFF
```

`payload.last()` is the final byte of the payload as sent, i.e. after any zero-padding within the
declared `len` but before the frame's own trailing zero-padding out to 64 bytes; a zero-length
payload contributes nothing (there is no last byte to add). This formula, ported from
`research/proto/package/src/utils/index.ts` (`computeCRC`), was checked against all 1224 frames of
the hardware session, requests and replies both, and held with zero failures. That is the strongest
claim in this document: every single byte the device and the CLI exchanged in that session satisfies
it.

## Replies

Bit 7 of the command byte is set on every reply: a request of `cmd 0x23` gets back `cmd 0xA3`, a
request of `cmd 0x29` gets back `cmd 0xA9`, and so on. This was measured across 90 request/reply
pairs in `captures/initial-load.jsonl`: `reply cmd == request cmd` held 0 times out of 90, `reply
cmd == request cmd | 0x80` held 90 out of 90.

This matters more than a passing framing detail. The CLI was entirely non-functional before the
hardware session precisely because of this bit: earlier code compared a reply's command byte
directly against the request's, so every reply was silently rejected as unrecognised and every read
appeared to hang or fail. Any reimplementation that skips this bit will reproduce that exact failure.

## Value encoding

Travel-like values (actuation point, rapid trigger press and release depth, global travel) are
encoded in micrometres (mm times 1000), as a little-endian `u16`. `wh_proto::value::Um` is the
in-repo type for this; `Um::to_mm` and `Um::from_mm` are the only conversion points between the
wire's integer micrometres and the millimetre values a human types or reads.

## Command table

Every command in the corpus is perfectly request/reply balanced: for every request seen, exactly one
reply was seen, and vice versa.

| cmd | requests | replies | Meaning |
|---|---|---|---|
| `0x00` | 42 | 42 | CMD, orders. Sub-order lives in `payload[0]`; see "cmd 0x00 sub-orders" below |
| `0x01` | 4 | 4 | SYNC, device identity; see "Device identity" below |
| `0x18` | 6 | 6 | **Unidentified.** Suspected LED or RGB control: the payload has a `7f7f` and `ff00ff00` shape that reads like colour or level data, but this is a guess from byte patterns, not a measurement |
| `0x23` | 540 | 540 | KEY, per-key layout records; see "Key records" below |
| `0x29` | 6 | 6 | DB, the global record; see "The global record" below |
| `0x2b` | 6 | 6 | DEFKEY, the physical key matrix; see "The DEFKEY matrix" below |
| `0x2c` | 8 | 8 | **Unidentified.** Almost certainly SOCD: it queries by key and replies with symmetric pairs, measured as W paired with S and A paired with D, matching the linked-key pairs visible in the vendor web UI. The behaviour is measured; the name (SOCD) is inference |

## `cmd 0x00` sub-orders

`cmd 0x00` (CMD) carries a sub-order in `payload[0]`, with the rest of the payload and reply shape
depending on which sub-order it is. Every sub-order below is request/reply balanced and none was
ever seen to fail. These ten rows account for all 42 `cmd 0x00` request/reply pairs in the corpus
(the counts sum to exactly 42), so this table is a complete accounting, not a sample.

| sub-order | pairs | Meaning |
|---|---|---|
| `0x22` | 5 | **Unidentified.** Reply payload always `002200` |
| `0x50` | 3 | **Unidentified.** Reply payload always `005000` |
| `0x70` | 4 | **Profile read and select.** Argument `0xFF` reads the active profile; a zero-based index selects one. See "Profiles" below |
| `0xa1` | 6 | **Unidentified.** Reply payload always `00a100` |
| `0xb9` | 3 | **Unidentified.** Reply payload always `00b900` |
| `0xba` | 3 | **Unidentified.** Reply payload always `00ba0000` |
| `0xbb` | 3 | **Unidentified.** Reply payload always `00bb0000` |
| `0xbc` | 3 | **Unidentified.** Reply payload always `00bc6400` |
| `0xbd` | 9 | **Unidentified.** Reply payload always `00bd01ff`. Recurs as a poll rather than sitting only in the connect sequence, so it is not purely a handshake step |
| `0xc0` | 3 | **Unidentified.** Reply payload always `00c001` |

Order `0x02` (`ORDER_TYPE_SAVING_PARAMETER` upstream, SAVE) deserves its own sentence: the upstream
Sparklink source names it as the save order, but across five complete write sequences (write
settings, read them back, repeat) in the captured session, it was never sent once, and it does not
appear in the table above either, since the table is a complete accounting of the corpus. Either the
K-001 persists every write immediately with no separate save step, or persistence happens through a
mechanism outside this corpus. `wh` does not send it and nothing in Phase 1 depends on it existing.

Since the table above accounts for all 42 pairs, several other order constants `wh-proto` carries
purely as ported vocabulary from upstream, `PROTOCOL_VERSION` (`0x01`), `FACTORY_RESET` (`0x11`),
`PRECISION` (`0x25`), and `KEYBOARD_NAME` (`0x26`), also never appeared in this corpus at all, the
same as SAVE. They are not exposed by the CLI and this document makes no claim about their real
behaviour beyond their upstream names.

## Key records

A key record is four bytes: `[key, layout, value_lo, value_hi]`, where `key` is the USB HID usage
code (or one of the four board-specific function codes, see "Key identity" below), `layout` selects
which per-key field this record addresses (see "Layout ids" below), and `value_lo`/`value_hi` are the
little-endian value for that field.

Records are batched into a single `cmd 0x23` frame, up to 14 records per frame
(`MAX_RECORDS_PER_REPORT`, from upstream's `MaxPack`), with `payload[0]` a read/write flag (`0x00`
read, `0x01` write) preceding the records. Records in a batch are grouped by key: every record for one
key is written together before moving to the next, rather than interleaving fields across keys.
Ported from `research/proto/package/src/utils/pack.ts` and `recdata.ts`
(`getSingleTravelRecdata`/`KeyDataPack`).

## Layout ids

Counted from `cmd 0x23` records only (`0x2b` DEFKEY replies use a different record shape entirely;
parsing them as layout records produces nonsense).

| layout | records | Distinct values | Meaning |
|---|---|---|---|
| `0x00` | 418 | 74 | **Base layer key mapping**, measured. Values are HID usages matching each physical key |
| `0x01` | 420 | 69 | **FN layer key mapping**, measured. See "The FN layer is measured" below |
| `0x04` | 1858 | 7 | Actuation point, micrometres. Modelled and used by `wh` |
| `0x08` | 2252 | 5 | Mode (touch mode nibble plus advanced-key nibble). Modelled and used by `wh` |
| `0x14` | 1858 | 3 | RT press depth, micrometres. Modelled and used by `wh` |
| `0x15` | 1858 | 4 | RT release depth, micrometres. Modelled and used by `wh` |
| `0x16` | 1858 | 1 | **Always `0`.** Never once observed non-zero across the whole corpus, written alongside every rapid trigger change. Purpose unknown |
| `0x17` | 1858 | 1 | **Always `0`,** same as `0x16`. Purpose unknown |
| `0x19` | 700 | 2 | **Unidentified.** Only ever `0x0000` or `0x3e2c` |
| `0xfe` | 424 | 2 | Keyset membership. `1` on keyset create, `0` on delete, untouched by edits within a set |
| `0xff` | 420 | 3 | **Unidentified.** Only ever `0`, `1`, or `2` |

The record counts above are what these ten captured scenarios happened to exercise, not each field's
full possible range. Within the corpus: layout `0x04` (actuation point) only ever took `0, 300, 850,
1200, 1700, 2000, 3000` (um); layout `0x08` (mode) only ever took `0, 16, 24, 56, 72`; layout `0x14`
(RT press) only ever took `0, 100, 500`; layout `0x15` (RT release) only ever took `0, 100, 300,
500`.

`wh` models four of these: `0x04`, `0x08`, `0x14`, `0x15`. `0x00` and `0x01` are now identified (see
below) but `wh` does not read or write the key mapping through them; remapping keys is out of Phase 1
scope. `0x16`, `0x17`, `0x19`, `0xfe`, and `0xff` remain either unused by `wh` or, in `0xfe`'s case,
managed entirely by the store rather than the wire.

### The FN layer is measured, not inferred

Layout `0x01` is the FN layer mapping, and this rests on more than one plausible-looking byte
pattern. From `captures/initial-load.jsonl`, reading each key's layout `0x00` value against its
layout `0x01` value gives, for example:

| key | layout `0x00` | layout `0x01` |
|---|---|---|
| esc | `0x29` (esc) | `0x35` (grave) |
| 1 | `0x1e` (1) | `0x3a` (F1) |
| 2 | `0x1f` (2) | `0x3b` (F2) |
| ... | ... | ... |
| 0 | `0x27` (0) | `0x43` (F10) |

FN+Esc producing the backtick and FN+number producing the function keys is exactly how the operator
described the board behaving under FN. Two independent series (the esc-to-grave mapping and the
ten number-to-function-key mappings) agreeing across 69 distinct values is a measurement, not a
guess from one coincidental byte.

## Device identity: `cmd 0x01` reply

The SYNC reply payload is a fixed 60 bytes, with the serial and firmware strings length-prefixed
rather than fixed-width:

| Offset | Value (measured) | Meaning |
|---|---|---|
| `p[8]` | `0x10` | Serial string length, 16 |
| `p[9..25]` | `3483141393E03502` | Serial |
| `p[25]` | `0x10` | Firmware string length, 16 |
| `p[26..42]` | `App_V1.1.046000\0` | Firmware string, NUL-terminated |
| `p[43..54]` | `Aug 20 2026` | Build date, NUL-terminated |
| `p[55..60]` | `ff ff ff ff ff` | Padding |

The firmware length and serial length happened to both be `0x10` on the measured device; nothing
guarantees that on a different unit, which is why the parser reads the length byte rather than
assuming a fixed 16-byte field, and why the firmware string starts wherever the serial's own declared
length says it ends, not at a fixed offset. Ported from
`research/proto/package/src/utils/recdata.ts` (`getCmdSyncRecdata`), corrected against the real
device: the vendor's own fixed offsets truncated the firmware string, which the wire declares as 16
bytes, not the 10 the old fixed offset assumed.

One divergence worth calling out on its own: the vendor web UI displays this firmware as `V0.046`,
not the `App_V1.1.046000` the device actually reports on the wire. The two are clearly related (both
contain `046`) but are not the same string, and this document describes only what is on the wire.

## The DEFKEY matrix

`cmd 0x2b` reads the physical key matrix: `[rw, rowA, 21 usages, rowB, 21 usages]`, six rows of 21
columns (`MATRIX_ROWS = 6`, `MATRIX_COLS = 21`), read two rows per request. A cell holding `0` means
no physical key exists there; a non-zero cell holds a HID usage code.

The values DEFKEY returns are physical key identifiers, not the board's current key mappings. This
was proven by remapping a key live (through the vendor UI) and re-reading DEFKEY afterward: DEFKEY
returned the same value it always had for that physical position, while the key's actual behaviour
under a keypress, and its `cmd 0x23` layout `0x00` record, had changed. This is why `wh` can address
keys by their DEFKEY-reported usage even after an operator has remapped them: the matrix identifies
where a key physically is, not what it currently does.

## Profiles

Sub-order `0x70` of `cmd 0x00` reads or selects the board's active profile:

- Sending argument `0xFF` reads the current profile. The reply echoes the sub-order and returns a
  zero-based index in `payload[2]`.
- Sending a zero-based index (`0x00`-`0x03` for the board's four profiles) selects that profile. The
  reply echoes the same index back, the same shape as a read.
- Select measurably takes roughly 120 times as long as a read (task 19b group B measurement); a
  caller doing both in sequence should not assume they cost the same.

`wh` implements read only. Select is documented here because the wire behaviour is known, but
nothing in Phase 1 needs to change the board's active profile.

## Open items

Honestly, what this corpus does not resolve:

- The `0x29` global record's field we call "travel": the vendor's own upstream naming calls it
  travel, but the meaning is not measured. The vendor only ever reads this record and never writes
  it across the whole corpus, and the measured value (`0x0064`, decimal 100, i.e. 0.1mm if it is a
  `Um`) is not a plausible switch travel for a board whose printed actuation scale runs to 3.5mm. It
  may be something else entirely.
- Layouts `0x16`, `0x17`, `0x19`, and `0xff`: present, measured, and never once informative in this
  corpus (`0x16`/`0x17` never non-zero; `0x19` only ever two values; `0xff` only ever three).
- Commands `0x18` and `0x2c`: unidentified at the command level, discussed above.
- The nine unidentified `cmd 0x00` sub-orders: `0x22`, `0x50`, `0xa1`, `0xb9`, `0xba`, `0xbb`,
  `0xbc`, `0xbd`, `0xc0`. All confined to the connect sequence except `0xbd`, which recurs as a
  poll.
- Key `0x01`'s identity: probably FN, from its position in the key enumeration, but this was
  deliberately never measured directly, because confirming it means remapping FN away, and FN is
  how the board reaches the FN layer used to identify every other remapped key in this document.
  The other four non-standard keys (`0xfa` AP, `0xfb` RT, `0xd6` PLAY, `0xfc` LIGHT) were confirmed
  by measurement, remapped in turn to F2 through F5 and observed to change on the wire accordingly.

## Firmware tested against

Measured directly off the real device used for the hardware session:

- Serial: `3483141393E03502`
- Firmware string (as reported on the wire): `App_V1.1.046000`
- Build date: `Aug 20 2026`

The vendor's own web configurator displays this same device's firmware as `V0.046`, not the string
above; both are noted here because a future implementer comparing their own capture against this
document should expect the wire string, not the UI's shorter one.
