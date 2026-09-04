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

This is the first of the three real divergences this document tracks between what the upstream
Sparklink source suggests and what the K-001 actually does on the wire. It is not that upstream
claims the bit is unset: `research/proto/package/src/controller/info.ts`'s `getCmd`/`getCmdSync`
decode whatever payload arrives without comparing or masking a reply bit against the request's `cmd`
at all, because the browser-side WebHID event model already separates outbound writes from inbound
input reports, so the JS source never needed to check. Nothing in the ported source predicted this
bit either way; the K-001 setting it had to be found from captures, and a port that carries over
upstream's "just decode whatever arrived" assumption into a request/reply-matched `Session` (as this
one's did, initially) reproduces the exact non-functional state described above.

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

`POLLING` (`0x50`) is a different case worth calling out explicitly, since it is not silent: upstream
names sub-order `0x50` as the polling-rate order, and `0x50` is exactly the sub-order the table above
lists as exercised, 3 pairs, reply always `005000`. That reply payload carries no polling-rate value
or anything else recognisable, and nothing in the corpus writes a rate through it, so this document
does not promote the upstream name to a fact: `0x50` is left in the unidentified list above rather
than labelled "polling rate", even though the name and the traffic line up, because line-up alone is
not the same standard of evidence this document uses everywhere else.

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
| `0x16` | 1858 | 1 | Recorded as always `0` from a corpus with no keysets in it. Read `100` throughout the 2026-08-29 sittings, including through two global sensitivity changes, so it is not the global sensitivity; read `0` again on every key in every 2026-09-04 keyset capture. Purpose unknown, and what moves it is unknown |
| `0x17` | 1858 | 1 | Same as `0x16`, and always written with it. Purpose unknown |
| `0x19` | 700 | 2 | **Unidentified.** Only ever `0x0000` or `0x3e2c` |
| `0xfe` | 424 | 2 | Rapid trigger keyset membership, an index and not a boolean: this sample writes only `1` and `0`, and the wider 27-capture corpus measures it reaching `2` (`docs/keysets.md`). Written host-side, one record per frame. Read and used by `wh` (Phase 2) |
| `0xff` | 420 | 3 | Actuation point keyset index, host-written, measured to `9`. Recorded here as read-but-never-written from a corpus that had never created a keyset. Read and used by `wh` (Phase 2) |


The record counts above are what these ten captured scenarios happened to exercise, not each field's
full possible range. Within the corpus: layout `0x04` (actuation point) only ever took `0, 300, 850,
1200, 1700, 2000, 3000` (um); layout `0x08` (mode) only ever took `0, 16, 24, 56, 72`; layout `0x14`
(RT press) only ever took `0, 100, 500`; layout `0x15` (RT release) only ever took `0, 100, 300,
500`.

`wh` models four of these: `0x04`, `0x08`, `0x14`, `0x15`. `0x00` and `0x01` are now identified (see
below) but `wh` does not read or write the key mapping through them; remapping keys is out of Phase 1
scope. `0x16`, `0x17`, and `0x19` remain unused by `wh`. `0xff` and `0xfe` are read by every `wh`
command that needs keyset membership, and written by `wh keyset create`, `wh keyset delete`, `wh
restore`, and by `wh set ap` when a selection splits a keyset. `wh keyset set` changes only a
keyset's value, never its membership: it always passes no index to `keyset::plan`, so no membership
record is ever sent.
How the vendor writes membership is fully measured in `docs/keysets.md`. Board keyset membership is
not the same thing as a `wh keys group`, which is a purely host-side name for a set of keys stored
in `wh`'s own `config.json` and never sent to the board at all. The two happen to share the word
"group"/"keyset"; they are not the same mechanism.

### Mode: nibble values, and a divergence from upstream

Layout `0x08` (`Layout_Mode` upstream) packs two 4-bit fields into the low byte of a 16-bit value:
touch mode in the high nibble, an advanced-key mode in the low nibble (`Mode`/`TouchMode` in
`crates/wh-proto/src/cmds.rs`). The touch mode nibble is the one a reimplementation needs to get
right to write `set rt` or `set ap` at all, and it is the second of this document's three tracked
divergences.

Upstream (`research/proto/package/src/constants/param.ts`'s `KeyTouchMode`, and
`byte.ts`'s near-identical copy) declares three values: `global = 0x00`, `single = 0x01`,
`rt = 0x02`. An earlier draft of this document recorded, from 1224 captured frames, that the K-001
never uses `0x02` for anything. **That was a statement about the sample, and the sample was blind:**
no capture in it had ever touched the GLOBAL RAPID TRIGGER switch. Measured on 2026-08-29:
switching it on reads all 68 keys back at nibble `1` and writes nibble `2` to every one of them,
and switching it off writes nibble `1` back. Upstream was right. See `docs/keysets.md`.

The five nibbles now measured:

| Nibble | Meaning |
|---|---|
| `0` | follows the global travel setting, no rapid trigger |
| `1` | per-key actuation point, no rapid trigger |
| `2` | rapid trigger, following the global settings |
| `3` | rapid trigger, own settings, continuous mode off |
| `4` | the same with continuous mode on |

Nibble `2` and nibble `3` are both rapid trigger on. They differ in where the sensitivity comes
from: `2` follows the global RT SENSITIVITY, `3` carries the key's own, which is what a rapid
trigger keyset holds. Any code deciding whether rapid trigger is enabled must treat `2`, `3` and `4`
alike; treating `2` as unknown reports rapid trigger off on a board where it is on for every key.
See `docs/keysets.md` for the full evidence.

**A mode write is not required for an actuation point change to take effect.** This is measured, not
assumed: `docs/tasks.md` records a hardware check where a key's actuation point was changed with no
accompanying mode write, and the key physically actuated at the new depth, checked using the board's
own actuation LEDs against another key at a known depth. The vendor's own web app sends a mode write
alongside every actuation-point change anyway, and, per `docs/keysets.md`, `wh set ap` now matches
it: `keyset::plan` sends `[MODE, AP, RT_PRESS, RT_RELEASE]` together whenever any of them changes,
the mode record echoing the touch nibble back unchanged except when it promotes `Global` to
`Single`. `ops::ap_records`, which writes `0x04` alone (with a MODE record only on that promotion),
still exists but is no longer on the `wh set ap` path.

Rapid trigger is the nibble this document's earlier draft got backwards, worth stating plainly
since the wrong answer looks plausible and fails silently. `wh` writes `3` to turn rapid trigger on
(the CLI never turns on the `4`, continuous, variant, though it preserves one on a read-modify-write
if it finds the board already in it, see `ops::rt_records`) and `1` to turn rapid trigger off, via
`ops::rt_off_records`. **Nibble `1` is rapid-trigger-off, not "write an actuation point"**: it is
what `wh` writes on a key that already has its own actuation point recorded in layout `0x04`, to
turn rapid trigger off while leaving that actuation point in place, which is why `rt_off_records`
writes `Single` (`1`) rather than `Global` (`0`). Following the wrong nibble by treating `1` as an
actuation-point-write instruction, on a key that currently has rapid trigger on, silently turns
rapid trigger off as a side effect: nothing errors, and both `dump` and the vendor UI report the key
as rapid-trigger-off afterward, with no indication that anything other than the actuation point was
touched.

**What writing nibble `0` does to a key's layout `0x04` value is unmeasured, and an earlier draft of
this document stated the opposite as fact.** Nibble `0` means "follow the global travel setting",
and the reasoning that seemed obvious was that this would discard whatever per-key actuation point
layout `0x04` recorded for that key. The one hardware test on record points the other way:
`docs/tasks.md` records a key sitting at nibble `0` with a custom actuation point of `0.30mm` in
layout `0x04`, and that key physically actuated at `0.30mm`, checked against another key at the
default `2.00mm` using the board's own actuation LEDs. A key at nibble `0` honoured its per-key
`0x04` value in that one test, which is the opposite of what the "discards the actuation point"
belief predicted.

`wh` still declines to write nibble `0` when turning rapid trigger off, via `ops::rt_off_records`
writing `Single` (`1`) instead of `Global` (`0`), but the honest reason is caution, not a known
destructive effect: the vendor's own web app was observed writing nibble `1`, not `0`, when its UI
turns rapid trigger off (removing a keyset), so matching that observed behaviour costs nothing and
avoids writing a value nobody has watched the vendor write in that situation, whatever it actually
does. A reimplementation that wants to match `wh` and the vendor should write `1` when turning rapid
trigger off; writing `0` there is unexplored territory, not a known-safe reset and not a confirmed
trap either.

The high byte of the 16-bit value (bits 8..16) carries information this document does not identify
and `wh` does not interpret. It must be preserved verbatim on every write: read the current 16-bit
value first, keep its high byte, and only replace the low byte's nibbles. Writing `0x0000` in the
high byte unconditionally has not been tested and may clear something this corpus never exercised
changing.

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
- Sending a zero-based index selects that profile; the reply echoes the same index back, the same
  shape as a read. The corpus contains exactly two selects, to index `1` and index `0`
  (`captures/profile-switch.jsonl`), not one for each of the board's four profiles. That `0x00` to
  `0x03` is the full valid range is an extrapolation, not a measurement of all four: it rests on
  `wh-proto`'s own `MAX_WIRE_INDEX = 3` bound and the vendor UI showing four profile slots, not on
  having selected profiles 2 and 3 on the wire and watched them succeed.
- Select measurably takes roughly 120 times as long as a read (task 19b group B measurement, from
  the same two selects above); a caller doing both in sequence should not assume they cost the same.

Phase 1 implemented read only. `wh profile <1-4>` (Phase 2) added select, over exactly this wire
behaviour.

## Open items

Honestly, what this corpus does not resolve:

- The `0x29` global record's field we call "travel": the vendor's own upstream naming calls it
  travel, but the meaning is not measured. The vendor never writes this record in the ten captures
  this section counts, though it does write it in the wider corpus, carrying `press_dead=200` and
  `release_dead=200` (`docs/keysets.md`). The measured value (`0x0064`, decimal 100, i.e. 0.1mm if it is a
  `Um`) is not a plausible switch travel for a board whose printed actuation scale runs to 3.5mm. It
  may be something else entirely.
- Layouts `0x16`, `0x17`, and `0x19`. `0x16`/`0x17` were recorded here as never non-zero. They
  read `0` in every Phase 1 capture, `100` in every 2026-08-29 capture including through two global
  sensitivity changes, and `0` again in every 2026-09-04 keyset capture. What moves them is not
  measured in either direction. `wh` never writes them, so neither transition is ours; RESET KEYSETS
  and a profile change from 1 to 2 both fall inside the window for the second one, and the captures
  do not separate them. Tying either transition to a keyset existing rather than to the sitting, the
  profile or the firmware remains an inference.
  `0x19` is still only ever `0x0000` or `0x3e2c`. `0xff` and `0xfe` are no longer open: both are
  host-written keyset indices, allocated max plus one, measured in `docs/keysets.md`.
- Commands `0x18` and `0x2c`: unidentified at the command level, discussed above.
- The nine unidentified `cmd 0x00` sub-orders: `0x22`, `0x50`, `0xa1`, `0xb9`, `0xba`, `0xbb`,
  `0xbc`, `0xbd`, `0xc0`. Two now have a context beyond the connect sequence: `0x22` is read three
  times at the head of every global rapid trigger capture, always replying `0`, and `0xbd` appears
  in 13 files, including before the write in `remap-one-key`. The write-barrier reading has
  direct counterexamples. `ks-global-rt-sens-150` and `ks-global-rt-sens-200` write exactly the same
  462 records each, and only the second carries a `0xbd`. It is also absent from both global switch
  captures, while appearing four times in `ks-create-rt-2`. It could still be sent conditionally,
  but it is not sent before every write.
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

## Keysets

Layouts `0xFF` (actuation point) and `0xFE` (rapid trigger) hold keyset membership. Both are
host-written, both are indices, and they are independent groupings with separate counters.
Allocation is max plus one and never reuses a freed index.

Measured on 2026-08-29 across seven capture scenarios. See `docs/keysets.md` for the write
sequences, the delete behaviour, and what it specifies for writing them.
