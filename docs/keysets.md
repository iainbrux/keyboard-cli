# Keysets, measured

How the vendor configurator creates, values and deletes a keyset, and what the global rapid trigger
switch does, measured on 2026-08-29 against a real K-001 running firmware `App_V1.1.046000`.
Fifteen keyset capture scenarios across two sittings, each changing one thing.

This document is the evidence base for task 2.4. Everything here is measured. Where something is
inferred, it says so.

## What a keyset is

Two per-key layouts hold membership, and they are **independent groupings over the same 68 keys**:

| Layout | Grouping | Values seen |
|---|---|---|
| `0xFF` | actuation point keyset | `0` to `9` |
| `0xFE` | rapid trigger keyset | `0`, `1`, `2` |

`0` means the key is in no keyset of that kind. A key can sit in one of each at the same time, with
different membership. Measured: `u` and `i` held `0xFF=3` and `0xFE=1` simultaneously while `o` and
`p` held `0xFF=3` and `0xFE=0`.

Independence was confirmed four ways: creating a rapid trigger keyset over two members of an
existing actuation point keyset left `0xFF` untouched; creating one over a key in no actuation point
keyset left it at `0`; deleting an actuation point keyset left `0xFE` untouched; and deleting a
rapid trigger keyset over a key holding an actuation point keyset value of `0.30mm` rewrote that
`0.30mm` back unchanged rather than resetting it.

**Membership is exclusive within a layout.** Creating a keyset over a key that already belongs to
one takes it out of the old keyset, which survives with its remaining members and its own value.
Measured: a `W,A,S,D` keyset at `0.30mm` became `A,D` at `0.30mm` when `W` and `S` were pulled into
a new keyset. The steal is a plain `0xFF` rewrite, from the old index straight to the new one, with
no intervening write of `0`.

A keyset holds exactly one value across its members. The configurator's left pane lists keysets in
ascending index order with deleted indices simply absent.

## Allocation

**Max plus one, monotonic, never reusing a gap.** Measured five times, over both layouts.

| Step | Existing indices | Allocated |
|---|---|---|
| Create on `u,i,o,p` | 1, 2 | **3** |
| Create on `j,k` | 1, 2, 3 | **4** |
| Delete 3, then create on `o,p` | 1, 2, 4 | **5**, not 3 |
| Create on `A` (stolen), `G` | up to 5 | **6** |
| Create on `H` (stolen), `J` | up to 8 | **9** |
| Create on `N` (stolen), `,` in `0xFE` | 1 | **2** |

The two layouts have **separate counters**. With `0xFF` already at 4, the first rapid trigger keyset
took `0xFE=1`, not 5. Stealing a key from an existing keyset does not change how allocation works.

The maximum is derived from live membership, not from a stored counter: after deleting keyset 3, the
next allocation was 5 because 4 was the highest index any key still held. A keyset that loses every
member to a steal therefore frees its index for reuse, which is consistent behaviour rather than a
special case.

The upper bound is unmeasured. The field is 16 bits and only single-digit values have been observed.

## The write template

Every value-writing operation the configurator performs uses the same five-step shape. Only the
target values differ.

```
1. MODE  (0x08)                  one frame per distinct value
2. AP    (0x04)                  batched across keys
3. MODE, RT press, RT release    (0x08, 0x14, 0x15) batched across keys
4. 0x16, 0x17                    batched across keys
5. membership (0xFF or 0xFE)     ONE RECORD PER FRAME, always last
```

Values batch normally, up to the protocol's 14 records per report. **Only membership is written one
record per frame**, and it always comes last. That asymmetry is the vendor's, and worth matching
rather than optimising.

**Layouts the operation does not own are rewritten at each key's current value.** This is the single
most important rule in this document, and the global rapid trigger captures measure it 66 ways in
one frame set: a sensitivity change rewrote layout `0x04` for every key with that key's own value,
`2000` for sixty of them, `2050` for `W`, `S` and `X`, `300` for `D` and `M`, `3000` for `ESC`.

| Operation | Owns | Rewritten unchanged |
|---|---|---|
| Create an actuation point keyset | `0x04` to the global actuation point | MODE, `0x14`, `0x15` |
| Change a keyset's value | `0x04` to the new value | MODE, `0x14`, `0x15` |
| Delete an actuation point keyset | `0x04` to the global actuation point | MODE, `0x14`, `0x15` |
| Create a rapid trigger keyset | MODE touch nibble to `3`, `0x14`/`0x15` to the global sensitivity | `0x04` |
| Delete a rapid trigger keyset | MODE touch nibble to `1`, `0x14`/`0x15` to the global sensitivity | `0x04` |
| Global rapid trigger on | MODE touch nibble to `2`, `0x14`/`0x15` to the global sensitivity | `0x04` |
| Global rapid trigger off | MODE touch nibble to `1` | `0x04`, `0x14`, `0x15` |

`0x16` and `0x17` are written `100` in every one of these, on every key, and have never been
observed holding anything else since keysets appeared. See the corrections below.

### The skip rule

**A key gets the whole template if any owned value differs, and nothing at all if none does.** It is
not per-layout diffing.

Measured four ways:

- `A` was stolen into a new keyset while holding `0.30mm`, target `2.00mm`: full template.
- `G` was free and already at `2.00mm`, target `2.00mm`: no writes at all, membership only.
- `H` was stolen out of a keyset whose value was already `2.00mm`: **no writes at all.** This kills
  the reading that the trigger is "the key was stolen"; the trigger is only the value.
- `N` was stolen into a new rapid trigger keyset needing only a sensitivity change, and `,` needed
  only a MODE change. Both got the full template.

### A new keyset starts at the global value

Creating a keyset does not carry its members' existing values in. An actuation point keyset starts
at the global actuation point; a rapid trigger keyset starts at the global sensitivity. Measured
both ways: `W` and `S` went from `0.30mm` to `2.00mm` on being pulled into a new actuation point
keyset, and `N` went from `0.50mm` to the global `0.10mm` on being pulled into a new rapid trigger
keyset.

**Creating a keyset is therefore destructive to the values of the keys it captures.** The vendor
does this by writing the global value, not by leaving the old one in place.

## Touch nibble 2 is global rapid trigger

Nibble `2` is rapid trigger following the board's global settings, and both transitions are
measured.

**On.** `ks-global-rt-on` reads every one of the 68 keys back at nibble `1` (`0x10` on 64 of them,
`0x18` on four) *before* its first write, then writes nibble `2` to all 68 (`0x20` and `0x28`),
advanced nibble preserved, with the global sensitivity `200` into `0x14`/`0x15` alongside.

**Off.** `ks-global-rt-off` writes nibble `1` back, again preserving the advanced nibble.

**While on**, the nibble `2` keys carry the global sensitivity in `0x14`/`0x15`, which moved with
it from `100` to `150` to `200` across the three captures, while rapid trigger keyset members sit
at nibble `3` and keep their own.

That ordering matters, because an earlier draft of this document claimed the on-transition from a
capture that did not contain it. Both `ks-global-rt-sens-150` and `ks-global-rt-sens-200` read
nibble `2` back before writing and then write the same nibble again: they are sensitivity changes
on a board already in that state. The claim was an inference from the operator's description of
what they clicked, written down as a measurement, which is the same error this project has made
three times before. `ks-global-rt-on` was then captured specifically to settle it.

This falsifies `docs/protocol.md`'s claim that the firmware never uses `0x02`, which was
recorded from a corpus in which the global switch had never been touched. Upstream's
`KeyTouchMode.rt = 0x02` was right; our sample was simply blind to it. The full nibble set now
measured:

| Nibble | Meaning |
|---|---|
| `0` | follows the global actuation point, no rapid trigger |
| `1` | own actuation point, no rapid trigger |
| `2` | rapid trigger, following the global settings |
| `3` | rapid trigger, own settings (a rapid trigger keyset) |
| `4` | rapid trigger continuous, own settings |

**Keys in a rapid trigger keyset are skipped entirely** by the off write and by both sensitivity
changes. The on write is untested on this point: no rapid trigger keyset existed when it was
captured, so all 68 keys were written and there was nothing to skip. That is the mechanism by which a keyset's own sensitivity survives a global
change, and it is the same shape as an actuation point keyset shielding its members from global
travel. Measured three times: 66 of 68 keys written, the two skipped being exactly the two members
of the live rapid trigger keyset.

**Nothing observed writes nibble `0`.** Across every capture, in every scenario, the configurator
takes keys out of nibble `0` and never puts one back. Turning the global switch on takes `0` or `1`
to `2`; turning it off takes `2` to `1`; deleting a keyset leaves `1` or `3`; disabling rapid
trigger on a single key writes `1`. A board that has had the global switch toggled once holds nibble
`1` on nearly every key.

That last point matters for the greying question. Nibble `1` cannot be the marker separating "the
user set this key" from "this key follows global", because a single click of the global switch
stamps nibble `1` across the whole board without the user touching any key. **Inference, not
measurement:** whatever causes the configurator to grey a value, our MODE promotion is unlikely to
be the whole of it.

## RESET KEYSETS is tab-scoped

The RESET KEYSETS control deletes every keyset of the **current tab's kind only**. Pressed on the
RAPID TRIGGER tab it cleared both rapid trigger keysets and left all six actuation point keysets
untouched. It uses the ordinary delete template, batched across every member at once, with
membership still one record per frame.

This is also how rapid trigger keysets created in an earlier sitting disappeared without any
recorded delete.

## What this specifies for task 2.4

- Two independent counters, one per layout, derived from live membership.
- Allocate max plus one. Do not scan for a free gap; the vendor does not.
- Write membership one record per frame, always last.
- Write the value template for a key only if one of the operation's owned values differs.
- Rewrite every non-owned layout at the key's current value, read first.
- A new keyset's value is the global, not the member's previous value.
- Creating a keyset over a key already in one steals it, with a plain membership rewrite.
- Deleting must not touch the other layout's membership or its values.

**`wh restore` must also restore membership.** Task 2.1 had restore ignore `0xFF` and `0xFE`, which
was correct while nothing was known to write them. That reason is gone. Measured consequence: a
restore put every value back to its snapshot and left four keysets in place, so the board no longer
matched the snapshot it had just been restored from.

## Corrections to earlier claims

**The actuation point create order was recorded backwards.** This document previously said an
actuation point keyset is created membership first, values second, and that rapid trigger keysets
reverse it. Both were readings of a single capture in which the members were already at the global
value, so the create wrote membership alone and the value change was a separate user action seconds
later. Measured now over six captures: **values always precede membership**, for both layouts, in
every operation.

**A rapid trigger keyset delete is no longer uncaptured.** It was previously listed as never
observed, and the plan was to compose it from two measured behaviours. That composition would have
been wrong: it would have left the keyset's sensitivity in place, where the vendor resets
`0x14`/`0x15` to the global. Measured twice with two different globals (`100` when the global read
0.10mm, `200` when it read 0.20mm), so the reset target tracks the global rather than being a
constant.

**Layouts `0x16` and `0x17` are not the global rapid trigger sensitivity**, and they are not always
zero either. They were recorded as "never once observed non-zero" across 1858 records, which held
only until a keyset existed. They have read `100` on every key touched since, and they stayed at
`100` through two global sensitivity changes that moved `0x14`/`0x15` to `150` and then `200`. They
are written in every template and have never been observed changing. Purpose still unknown.

**`Snapshot::global.travel_mm` is not the global actuation point.** It is the configurator's
`"MM" CUSTOM VALUE`, the step size for its `< >` controls. Measured: changing that control from 0.10
to 0.15 moved the field from `0.1` to `0.15`, and the write carried `travel=150um`. The real global
actuation point, 2.00mm, is not in that record at all; it is simply what every key in no keyset
holds in layout `0x04`. The field should be renamed.

## An open item on the global record

The vendor's `cmd 0x29` write always carries `press_dead=200` and `release_dead=200`. Those are
constants in the vendor's own SDK template, not user settings. **The board reports both as `0` on
read**, so the values cannot be preserved across a read-modify-write.

`ops::restore_all` passes the snapshot's values into those fields, which means `wh restore` writes
`0, 0` where the vendor has only ever written `200, 200`. Whether the board cares is unmeasured, and
unobservable through the read path. The safe fix is to send the vendor's constants rather than the
zeros we read back.

## Command `0x2c` is SOCD, no longer inferred

Measured from the configurator's connect sequence:

```
query w (0x1a)  ->  1a 16 00 16 1a      w and s
query a (0x04)  ->  04 07 00 07 04      a and d
query s (0x16)  ->  16 1a 00 1a 16      s and w
query d (0x07)  ->  07 04 00 04 07      d and a
```

Query is `[rw, key, 0xFF, ...]`. The reply is `[status, keyA, keyB, 0, keyB, keyA, ...]`, the pair
given both ways round. The ADVANCED tab carries a SOCD control, and these are the pairs it holds, so
the name is now measured rather than inferred from byte shapes.

## Still open

- **Where the global rapid trigger sensitivity is stored.** No global command carries it. It appears
  only in `0x14`/`0x15` of the keys outside any rapid trigger keyset, which would also be how the
  configurator reads it back. Plausible and testable, not measured.
- **What `0x16` and `0x17` are for.** Written `100` in every template, never observed changing.
- **`cmd 0x00` sub-order `0x22`**, read three times at the head of every global rapid trigger
  capture. Not the switch's state: it replies `0` with the switch off (`ks-global-rt-on`) and `0`
  with it on (both sensitivity captures).
- **Whether the on write skips rapid trigger keyset members**, as the off write and both
  sensitivity changes do. Needs one capture with a keyset present before the switch is thrown.
- **`cmd 0x00` sub-order `0xbd`**, sent once before the sensitivity change, and also before the
  write in `remap-one-key`. Possibly a write barrier.

## Corpus

Twenty-seven capture files, 3696 frames, all decoding with correct framing and checksums and no
hard failures. Up from ten files and 1224 frames after Phase 1.
