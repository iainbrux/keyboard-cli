# Keysets, measured

How the vendor configurator creates, values and deletes a keyset, and what the global rapid trigger
switch does, measured on 2026-08-29 against a real K-001 running firmware `App_V1.1.046000`.
Fifteen keyset capture scenarios across two sittings, each changing one thing.

This document is the evidence base for task 2.4. It is reliable about frame shapes and less reliable
about board state: only four of the 31 captures read layouts `0xFF` or `0xFE` at all, and only one
of those, `custom-value-nudge-after-restore` at 22:25, falls inside the keyset sitting. Almost every
statement below about which keyset a key was in rests on that single read. Each claim says whether
it is measured or inferred, and a verification pass on 2026-09-03 rewrote the ones that had it
wrong.

## What a keyset is

Two per-key layouts hold membership, and they are **independent groupings over the same 68 keys**:

| Layout | Grouping | Values seen |
|---|---|---|
| `0xFF` | actuation point keyset | `0` to `9`, every value in that range |
| `0xFE` | rapid trigger keyset | `0`, `1`, `2` |

`0` means the key is in no keyset of that kind. Measured over the 31 files: written values are
`0, 3, 5, 6, 7, 8, 9` and read values are `0, 1, 2, 4, 5`. An earlier version of this table said `7`
and `8` were never written; the 2026-09-04 captures write both.

**A key can sit in one of each at the same time. Inferred, not read.** No capture reads a key with
both `0xFF != 0` and `0xFE != 0`. What the 22:25 read measures is the two layouts differing over
the same keys: `u` and `i` at `0xFF=0`, `0xFE=1`, while `o` and `p` sat at `0xFF=5`, `0xFE=0`. Dual
membership is inferable for `w` from a window rather than a read: `rt-on-w-0.5` wrote `w 0xFE=1` at
22:46 and `rt-off-w` cleared it at 22:54, and the reads either side of that window both show
`w 0xFF=1` with no `0xFF` write in between.

Independence was confirmed three ways, one fewer than an earlier draft claimed:

- Creating a rapid trigger keyset over a key in no actuation point keyset left `0xFF` at `0`.
- Deleting an actuation point keyset left `0xFE` untouched. Well measured: `ks-delete-ap-1` deletes
  the keyset over `u,i,o,p` at 22:09 and the 22:25 read still shows `u 0xFE=1, i 0xFE=1`.
- Deleting a rapid trigger keyset over a key holding its own actuation point of `0.30mm` rewrote
  that `0.30mm` back unchanged rather than resetting it.

The fourth, creating a rapid trigger keyset over two members of an existing actuation point keyset,
is not in the corpus. The three captured rapid trigger creates are over `m`, over `n` and `,`, and
over `w`, and only `w` held an `0xFF` at the time. The third bullet above is also weaker than it
looks: `m` read `0xFF=0`, so its `0.30mm` was its own actuation point and there was no keyset value
to protect.

**Membership is exclusive within a layout.** Creating a keyset over a key that already belongs to
one takes it out of the old keyset, which survives with its remaining members and its own value.
Measured: an `A,D,S,W` keyset at `0.30mm` became `D,S,W` at `0.30mm` when `A` was pulled into a new
keyset alongside `G`. The steal is a plain `0xFF` rewrite, from the old index straight to the new
one, with no intervening write of `0`.

A keyset holds exactly one value across its members. The configurator's left pane lists keysets in
ascending index order with deleted indices simply absent.

## Allocation

**Max plus one, never reusing a gap within a layout's live membership.** Measured four times.

| Step | Existing indices | Allocated | Status |
|---|---|---|---|
| Create on `u,i,o,p` | 1, 2 | **3** | allocation measured, `ks-create-ap-1`; pre-state from the day-earlier reads, since that file has no reads |
| Delete 3, then create on `o,p` | 1, 2, 4 | **5**, not 3 | measured, `ks-create-ap-3` |
| Create on `A` (stolen), `G` | up to 5 | **6** | measured, `ks-steal-ap` |
| Create on `N`, `,` in `0xFE` | 1 | **2** | allocation measured, `ks-steal-rt`; pre-state derived, that file never reads `0xFE` |
| Create on `j,k` | 1, 2, 3 | **4** | **post-state only**, never captured |
| Create on `H`, `J` (stolen) | unobserved | **9** | **unexplained**, see below |

**The `9` row is not evidence for the rule and an earlier draft used it as though it were.** Every
`0xFF` write across the 31 files, by file: `ks-create-ap-1` `u,i,o,p` to `3`; `ks-create-ap-3`
`o,p` to `5`; `ks-delete-ap-1` `u,i,o,p` to `0`; `ks-steal-ap` `a,g` to `6`; `ks-steal-equal-value`
`h,j` to `9`; and from 2026-09-04, `ks-value-over-all` the whole board to `3`,
`ks-value-five-members` `w,a,s,d,g` to `3`, `ks-consume-whole` `w,a,s,d,g` to `7`, `ks-span-two`
`w,u,i,a` to `8`. An earlier version of this paragraph said `7` and `8` were never written; the
2026-09-04 captures write both, allocated normally as max plus one. The last captured allocation
before `h,j` took `9` was `6`, and the three captures in between write no `0xFF` record at all. From
the frames alone the corpus shows a maximum of `6` followed by an allocation of `9`, which
contradicts max plus one. The earlier draft rescued it by asserting a pre-state of "up to 8" that no
frame shows, which is circular: `9` is the only reason to believe `7` and `8` ever existed. It also
sits badly with the six keysets still live at the end of the sitting, `{1, 2, 4, 5, 6, 9}`, rather
than the eight that a maximum of `8` with `3` deleted would leave. Settling it needs one capture
that reads `0xFF` between 22:50 and 23:00, and none exists.

The `j,k` row is a post-state observation, not an allocation. No capture writes `0xFF=4`; the 22:25
read simply shows `j` and `k` holding it. The pre-state, the allocation and the ordering are all
unobserved.

**Indices are reused after a delete.** `0xFE=2` was allocated to `m` at 22:08, cleared twice by
22:59, and allocated again to `n` and `,` at 23:02. Allocation is max plus one over *live*
membership, so a freed index returns to the pool. An earlier draft called this monotonic, which the
same table falsifies.

The two layouts have **separate counters.** Measured in `ks-steal-rt` at 23:02: the rapid trigger
create took `0xFE=2` when the highest `0xFF` in play was `9`. An earlier draft cited a different
event for this, the first rapid trigger keyset taking `0xFE=1` with `0xFF` already at 4, and that
event is not in the corpus: the only captured allocation of `0xFE=1` is `rt-on-w-0.5` at 22:46, when
the highest `0xFF` read anywhere was `2`.

The maximum is derived from live membership, not from a stored counter: after deleting keyset 3, the
next allocation was 5 because 4 was the highest index any key still held.

The upper bound is unmeasured. The field is 16 bits and only single-digit values have been observed.

## The write template

Every value-writing operation the configurator performs uses the same five-step shape. Only the
target values differ.

```
1. MODE  (0x08)                  at most two records per frame
2. AP    (0x04)                  batched across keys
3. MODE, RT press, RT release    (0x08, 0x14, 0x15) batched across keys
4. 0x16, 0x17                    batched across keys
5. membership (0xFF or 0xFE)     ONE RECORD PER FRAME, always last
```

**Step 1 is a two-record cap, not one frame per distinct value.** Of the 162 MODE-only write frames
in the corpus, 147 carry exactly two records and 15 carry one. None carries more. The vendor splits
one value across two frames (`ks-create-ap-1` frames 8 and 10, both `0x10`) and puts two different
values in one frame (`ks-global-rt-on` frame 108, `0x20` and `0x28`), so the grouping is not by
value. An earlier draft read this backwards.

**Only membership is written one record per frame**, and it always comes last. That asymmetry is the
vendor's, and worth matching rather than optimising.

Steps 2 to 4 batch across keys, but not always across every key of one operation. `ks-steal-rt`
writes `n`'s whole template at frames 60 to 66, re-reads the board, writes `,`'s whole template at
frames 128 to 134, and only then sends the two membership frames. Task 2.4 should not assume two
members of one create arrive in the same frame.

The largest write frame anywhere in the corpus carries **12** records. Fourteen record slots appear
only in read requests, so the 14-record batching limit is an inference from the frame size rather
than a measurement of the vendor's behaviour.

**Layouts the operation does not own are rewritten at each key's current value.** This is the single
most important rule in this document, and the global rapid trigger captures measure it 66 ways: a
sensitivity change rewrote layout `0x04` for every key with that key's own value, `2000` for sixty
of them, `2050` for `S`, `W` and `X`, `300` for `D` and `M`, `3000` for `ESC`. Those 66 records are
not one frame set. The vendor works the board in blocks of about eight keys, repeating the five
steps per block, so they are spread over nine blocks (eight keys each, then two) and 90 write
frames carrying 462 write records in total.

| Operation | Owns | Rewritten unchanged |
|---|---|---|
| Create an actuation point keyset | `0x04` to the global actuation point | MODE, `0x14`, `0x15` |
| Change an actuation point value | `0x04` to the new value, and MODE promoted `Global` to `Single`, three of three opportunities in the corpus | `0x14`, `0x15`, and MODE where it is already `Single` |
| Delete an actuation point keyset | `0x04` to the global actuation point | MODE, `0x14`, `0x15` |
| Create a rapid trigger keyset | MODE touch nibble to `3`, `0x14`/`0x15` to the global sensitivity | `0x04` |
| Delete a rapid trigger keyset | MODE touch nibble to `1`, `0x14`/`0x15` to the global sensitivity | `0x04` |
| Global rapid trigger on | MODE touch nibble to `2`, `0x14`/`0x15` to the global sensitivity | `0x04` |
| Global rapid trigger off | MODE touch nibble to `1` | `0x04`, `0x14`, `0x15` |

**`0x16` and `0x17` are rewritten at the key's current value like any other non-owned layout.** They
are not a constant. Every write of them in the keyset-era captures is `100`, 580 records across
fourteen files, and every write of them in the five Phase 1 captures is `0`, 38 records. Where a
file both reads and writes them, the written value equals the value it read for that key. Of the 27
captures, 18 both read and write them, four read without writing, four do neither, and
`ks-create-ap-1` writes eight records of `100` while containing no read frames at all. Hard-coding
`100` would write `100` over `0` on a board that has never had a keyset. An earlier draft said
they are written `100` in every template, which is false for three of the seven rows above.

### The MODE promotion, and what is actually measured about it

Searching all 31 files for a MODE record written non-zero over a usage whose most recent read in the
same file was `0`, there are exactly three:

```
ks-create-rt-2   frame 60    m      0x00 -> 0x30
ks-steal-rt      frame 128   comma  0x00 -> 0x30
ks-value-ap      frame 62    x      0x00 -> 0x10
```

The first two are unambiguously rapid trigger keyset creates. The third is ambiguous, and two
earlier drafts resolved the ambiguity in their own favour in opposite directions.

`ks-value-ap` frames 60 to 68 read and write `w 0x04: 2000 -> 2050` at MODE `0x18 -> 0x18`, the same
for `s`, and `x 0x04: 2000 -> 2050` at MODE `0x00 -> 0x10`. That much is exact. What the capture
does not contain is any read of `0xFF`, so whether `x` was in an actuation point keyset at 22:51 is
unknown. The 22:25 read has `x` free and `w,s` at `0xFF=1` alongside `a` and `d`, but that state
cannot have survived unchanged, because `w` and `s` read `0x04=300` at 22:25 and `2000` at 22:51.
Two readings fit: an operation over three keys of which one was free, in which case the promotion
happened outside a keyset, or a create over `{w,s,x}` at the global `2000` followed by a value
change to `2050`, in which case `x` was a member and the skip rule explains why its MODE was still
`0x00`.

So: the promotion from `Global` to `Single` on an actuation point change is measured. Whether it is
specific to keyset members, specific to non-members, or common to both is **not** measured, and no
capture in the corpus settles it. `ops::ap_records` and `keyset::plan` both promote unconditionally,
which is the shipped behaviour task 2.2 still lists for hardware verification.

What is measured, and is the strongest statement the corpus supports, is the negative: sweeping all
31 files for keys given a `0x04` write and checking the MODE each had read, **every key that read
MODE `0x00` and received an actuation point write was written a non-zero MODE.** Three of three
opportunities, no counterexample. No capture leaves a nibble-`0` key at nibble `0` after an
actuation point write.

One blind spot in that search: `ks-create-ap-1` has no read frames at all, yet writes MODE `0x10` to
`u,i,o,p`. Those four prior values are unobserved, so the honest count is three observed promotions
and four MODE writes whose prior value is unknown.

**The template does not vary with keyset membership.** `ap-wasd-1.2` is an actuation point change on
four keys with no keyset traffic anywhere in the file, and it emits the same five steps: MODE
(frames 60 and 62), `0x04` batched (64), MODE with `0x14` and `0x15` batched (66), then
`0x16`/`0x17` (68). Three separate changes in that one capture, to `850`, `1200` and `300`, all
identical in shape. Whether those keys were in a keyset is unmeasured, but that is the point: the
frames are the same either way, so `wh set ap` does not need to know before choosing what to send.

### The skip rule

**A key gets the whole template if any owned value differs, and nothing at all if none does.** It is
not per-layout diffing.

Measured four ways:

- `A` was stolen into a new keyset while holding `0.30mm`, target `2.00mm`: full template.
- `G` was free and already at `2.00mm`, target `2.00mm`: no writes at all, membership only.
- `J` was stolen out of keyset 4, whose value was already `2.00mm`: **no writes at all.** This kills
  the reading that the trigger is "the key was stolen"; the trigger is only the value. `H`, which
  the same operation captured, was free. `ks-steal-equal-value` contains no reads, so `J`'s `2.00mm`
  comes from the 22:25 read rather than from the capture itself.
- `N` and `,` were put into a new rapid trigger keyset, `N` needing only a sensitivity change and
  `,` only a MODE change. Both got the full template. `N` was at rapid trigger with its own settings
  beforehand, so it was probably stolen from another keyset, but which index it held is never read.

### Changing a value over a selection that is not exactly one keyset

**Measured on 2026-09-04, three captures, all three shapes.** This section previously recorded an
operator observation of the vendor's interface and said in terms that two of the three shapes had
no support of any kind. They now have frames.

| Capture | Board before | Selection | Result |
|---|---|---|---|
| `ks-consume-whole` | keyset 7 `w,a,s,d` at 0.50, `g` free at the global | `w,a,s,d,g` | `0xFF = 8` to all five. Keyset 7 ceased to exist |
| `ks-span-two` | keyset 7 `w,a,s,d,g` at 0.50, a second keyset `u,i,o,p` at 1.20 | `w,u,i,a` | `0xFF = 8` to those four only. `s,d,g` and `o,p` kept their original indices |
| `ks-value-over-all` | two keysets live | every key on the board | `0xFF = 3` to all 68 |

So the rule, measured on those three boards: **a selection that is not exactly one keyset's members
takes a fresh index, and every selected key goes into it.** A keyset entirely inside the selection
ceases to exist. A keyset only partly selected survives with its remaining members, and gets no
membership record at all.

`ks-span-two` is the important one. It is the only capture of two keysets in a single selection, and
it settles that the vendor merges the **selection** rather than the keysets: exactly four membership
records were written, and the five keys left behind across the two originals got none.

**What the captures do not cover.** Every selection above was made in the configurator by clicking
keys, so the vendor allocates a fresh index at the moment of selection; the value change follows.
The corpus still contains no case of a selection spanning three or more keysets, and none where the
allocated index was anything other than max plus one.

Two rules from elsewhere in this document turn up again in both `ks-consume-whole` and
`ks-span-two`, which is worth recording because they were measured on different operations before:
the new keyset's members are written the **global** value before membership (`0x04 = 2000` in both),
and a member already holding the global gets no value record at all, which is the skip rule.

### A new keyset starts at the global value

Creating a keyset does not carry its members' existing values in. An actuation point keyset starts
at the global actuation point; a rapid trigger keyset starts at the global sensitivity. Measured
both ways: `A` went from `0.30mm` to `2.00mm` on being pulled into a new actuation point keyset, and
`N` went from `0.50mm` to the global `0.10mm` on being pulled into a new rapid trigger keyset.

**Creating a keyset is therefore destructive to the values of the keys it captures.** The vendor
does this by writing the global value, not by leaving the old one in place.

### Removing one key from a keyset

Measured 2026-09-04 in `ks-remove-one-key` and `ks-remove-to-empty`, over an actuation point keyset
holding `J`, `K`, `L` at `1200` against a global of `2000`.

The configurator can take a single key out of a keyset without deleting it, and does so with the
ordinary five-step template applied to that key alone:

```
j/0x08 = 16                        MODE, touch nibble 1
j/0x04 = 2000                      the global actuation point
j/0x08 = 16, j/0x14 = 100, j/0x15 = 100
j/0x16 = 0,  j/0x17 = 0
j/0xFF = 0                         one record, last
```

**The keys that stay carry no records at all.** Removing `J` produced no write naming `K` or `L`,
and the read sweep before the next removal showed both still at `1200`. The keyset survives losing
a member.

**The MODE record stays at touch nibble 1.** The vendor did not drop the removed key to nibble `0`
even though it is returning to the global value and to no keyset. The operator confirmed at the
screen that `J` rendered grey and read `2.00mm` afterwards, so a key at nibble `1`, at the global
value, outside every keyset, greys. That is the third independent measurement that greying tracks
`0xFF` and not the nibble.

**Removing the last member is the same five frames and nothing more.** `ks-remove-to-empty` took
`K` and then `L` out of the keyset, `L` being the last one. The write for `L` is identical in shape
to the write for `J`: no teardown record, no `0xFF` write to any other key, nothing after the
membership clear. Had the firmware kept a keyset table apart from the per-key `0xFF` values, the
configurator would have had to write it. A keyset therefore stops existing when no key carries its
index, and code deleting members needs no special case for emptying one.

Confirmed off the board rather than off the configurator's cache. With the browser closed,
`wh keyset list ap` read `0xFF` live and returned four keysets, none of them 10, and
`wh get ap --keys j,k,l` returned `2.00mm keyset none` for all three.

## Touch nibble 2 is global rapid trigger

Nibble `2` is rapid trigger following the board's global settings, and both transitions are
measured.

**On.** `ks-global-rt-on` reads every one of the 68 keys back at nibble `1` (`0x10` on 64 of them,
`0x18` on four) *before* its first write, then writes nibble `2` to all 68 (`0x20` and `0x28`),
advanced nibble preserved, with the global sensitivity `200` into `0x14`/`0x15` alongside. No key
sat at nibble `0` when it was captured, so the transition measured is `1` to `2` and nothing else.

**Off.** `ks-global-rt-off` writes nibble `1` back to 66 of the 68, again preserving the advanced
nibble.

**While on**, the nibble `2` keys carry the global sensitivity in `0x14`/`0x15`, which moved with
it from `100` to `150` to `200` across the three captures, while rapid trigger keyset members sit
at nibble `3` and keep their own.

That ordering matters, because an earlier draft of this document claimed the on-transition from a
capture that did not contain it. Both `ks-global-rt-sens-150` and `ks-global-rt-sens-200` read
nibble `2` back before writing and then write the same nibble again: they are sensitivity changes
on a board already in that state. The claim was an inference from the operator's description of
what they clicked, written down as a measurement, which is the same error this project has made
several times. `ks-global-rt-on` was then captured specifically to settle it.

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

**Two keys are skipped entirely** by the off write and by both sensitivity changes: 68 read, 66
written, three times over, the two skipped being `n` and `,`. That is the mechanism by which a
keyset's own sensitivity survives a global change, and it is the same shape as an actuation point
keyset shielding its members from global travel. The on write is untested on this point: no rapid
trigger keyset existed when it was captured, so all 68 keys were written and there was nothing to
skip.

**What the skip is keyed on is not measured.** None of the four global captures reads `0xFE`. `n`
and `,` are both the members of `0xFE=2` and the only two keys read at MODE nibble `3`, so
membership and nibble coincide and the frames cannot separate them. This matters for task 2.4: a
skip implemented on membership will diverge from the vendor for any key at nibble `3` with
`0xFE=0`, and `rt-on-w-0.5` with `rt-off-w` show the board can be driven into that state. There is
also a counterexample in the other direction. At 22:25 `u` and `i` held `0xFE=1`, and in
`ks-global-rt-off` at 23:09 they were written rather than skipped, reading MODE `0x20`. Either their
membership was cleared uncaptured, or `0xFE != 0` does not by itself cause a skip. The corpus cannot
resolve it.

**Nothing observed writes nibble `0`.** Across every capture, in every scenario, the configurator
takes keys out of nibble `0` and never puts one back. Turning the global switch on takes `1` to `2`;
turning it off takes `2` to `1`; deleting a keyset leaves `1` or `3`; disabling rapid trigger on a
single key writes `1`. A board that has had the global switch toggled once holds nibble `1` on
nearly every key.

That last point matters for the greying question. Nibble `1` cannot be the marker separating "the
user set this key" from "this key follows global", because a single click of the global switch
stamps nibble `1` across nearly the whole board without the user touching any key. **Inference, not
measurement:** whatever causes the configurator to grey a value, our MODE promotion is unlikely to
be the whole of it.

## RESET KEYSETS is tab-scoped

The RESET KEYSETS control deletes every keyset of the **current tab's kind only**. Pressed on the
RAPID TRIGGER tab it cleared the one rapid trigger keyset that was live in the capture, index 2 with
members `n` and `,`, and wrote no `0xFF` record at all. It uses the ordinary delete template,
batched across every member at once, with membership still one record per frame.

Two limits on that. `ks-reset-keysets` does not read `0xFF`, so "left the actuation point keysets
untouched" rests on the absence of writes rather than on a read. And `u` and `i` held `0xFE=1` at
22:25 with nothing in any capture clearing it, so whether a second rapid trigger keyset still
existed when RESET was pressed is unobserved; the MODE read inside the capture shows both at `0x10`,
which is consistent with it having been deleted off camera.

This is also how rapid trigger keysets created in an earlier sitting disappeared without any
recorded delete.

## What this specifies for task 2.4

- Two independent counters, one per layout, derived from live membership.
- Allocate max plus one over live membership, so a freed index returns to the pool. Do not scan for
  a free gap below the maximum; the vendor does not.
- Write membership one record per frame, always last.
- Write the value template for a key only if one of the operation's owned values differs.
- Rewrite every non-owned layout at the key's current value, read first. That includes `0x16` and
  `0x17`: read them, do not send a constant.
- A new keyset's value is the global, not the member's previous value.
- Creating a keyset over a key already in one steals it, with a plain membership rewrite.
- Deleting must not touch the other layout's membership or its values.

**`wh restore` must also restore membership.** Task 2.1 had restore ignore `0xFF` and `0xFE`, which
was correct while nothing was known to write them. That reason is gone. No capture contains a
`wh restore`, so the consequence is an operator observation rather than a measurement: a restore put
every value back to its snapshot and left the keysets in place, so the board no longer matched the
snapshot it had just been restored from. What the frames do show is the 22:25 read, four actuation
point keysets and two rapid trigger ones still present after that restore.

## Corrections to earlier claims

**The actuation point create order was recorded backwards.** This document previously said an
actuation point keyset is created membership first, values second, and that rapid trigger keysets
reverse it. Both were readings of a single capture in which the members were already at the global
value, so the create wrote membership alone and the value change was a separate user action seconds
later. Measured now over nine captures: **values always precede membership**, for both layouts, in
every operation. `ks-create-ap-1` and `ks-create-ap-3` do open with membership at frame 0, with
their value writes 6.8 and 4.4 seconds later, which the timestamps support as separate user actions.

**A rapid trigger keyset delete is no longer uncaptured.** It was previously listed as never
observed, and the plan was to compose it from two measured behaviours. That composition would have
been wrong: it would have left the keyset's sensitivity in place, where the vendor resets
`0x14`/`0x15` to the global. Measured three times over two different globals: `100` in
`ks-delete-rt` and `ks-rt-delete-over-ap` when the global read 0.10mm, `200` in `ks-reset-keysets`
when it read 0.20mm. The reset target tracks the global rather than being a constant.

**Layouts `0x16` and `0x17` are not the global rapid trigger sensitivity**, and they are not always
zero either. They were recorded as "never once observed non-zero" across 1858 records. Every value
seen for them in a Phase 1 capture is `0` and every value seen in a keyset-sitting capture is `100`,
and they stayed at `100` through two global sensitivity changes that moved `0x14`/`0x15` to `150`
and then `200`. What changed the value is **not** measured. The only capture of a `0` to `100`
transition is a bare write in `ks-create-ap-1`, a file containing no reads at all, so attributing it
to a keyset existing rather than to the sitting, the firmware or the configurator version is an
inference. Purpose still unknown.

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

## Command `0x2c` is SOCD

Measured from the configurator's connect sequence, and reproducing in `initial-load`,
`remap-matrix-read` and `custom-value-nudge-after-restore`:

```
query w (0x1a)  ->  1a 16 00 16 1a      w and s
query a (0x04)  ->  04 07 00 07 04      a and d
query s (0x16)  ->  16 1a 00 1a 16      s and w
query d (0x07)  ->  07 04 00 04 07      d and a
```

Query is `[rw, key, 0xFF, ...]`. The reply is `[status, keyA, keyB, 0, keyB, keyA, ...]`, the pair
given both ways round. The frames measure the pairs and the reply shape and nothing more. That the
ADVANCED tab carries a SOCD control holding exactly these pairs is an observation of the UI, so the
name is corroborated by the interface rather than measured from the wire.

## The configurator never re-reads membership

Measured across all three 2026-09-04 removal captures. The configurator re-reads six layouts for
all 68 keys before every single-key change (`0x04`, `0x14`, `0x15`, `0x16`, `0x17`, `0x08`, thirty
frames a sweep), which is why two removals cost 142 frames. **It never reads `0xFF` or `0xFE` in
any of them.** Its picture of which keys are in which keyset comes from page load and lives in the
browser for the rest of the session.

This is the opposite of `wh`, which reads membership live on every command, and it is why the
configurator can show a keyset list that no longer matches the board while `wh` cannot.

## SEPARATE PRESS AND RELEASE is not a stored bit

Measured in `ks-set-10-to-1.2`, whose read sweep covers all six layouts for all 68 keys, taken while
the configurator was displaying `SEPARATE PRESS AND RELEASE ... ON` for a rapid trigger keyset
holding `N` and `M`.

Those two keys read `0x14 = 300`, `0x15 = 400`, `0x08 = 0x30`, and **`0x16 = 0` and `0x17 = 0`, the
same as the other 66 keys**. Nothing in the six layouts the configurator reads distinguishes them
from a separate-off key except that press and release differ. The global block agrees: separate off,
and a single `RT SENSITIVITY` over keys whose `0x14` and `0x15` are both `100`.

The keyset had been created by `wh`, which wrote MODE, `0x04`, `0x14`, `0x15` and `0xFE` and no
separate flag of any kind. So within the layouts the vendor reads there is no candidate bit, and the
toggle is displayed from `0x14 != 0x15`. This is a statement about those six layouts; a bit in a
layout neither `wh` nor the configurator reads would not show up here.

## Still open

- **Where the global rapid trigger sensitivity is stored.** No global command carries it. It appears
  only in `0x14`/`0x15` of the keys outside any rapid trigger keyset, which would also be how the
  configurator reads it back. Plausible and testable, not measured.
- **What `0x16` and `0x17` are for**, and what moved them from `0` to `100`.
- **`cmd 0x00` sub-order `0x22`**, read three times at the head of every global rapid trigger
  capture. Not the switch's state: it replies `0` with the switch off (`ks-global-rt-on`) and `0`
  with it on (both sensitivity captures).
- **Whether the on write skips rapid trigger keyset members**, as the off write and both
  sensitivity changes do, and whether the skip is keyed on membership or on the MODE nibble. Needs
  one capture with a keyset present before the switch is thrown, and one with a key at nibble `3`
  holding `0xFE=0`.
- **How `0xFF` reached `9`.** Needs one capture that reads `0xFF` between 22:50 and 23:00.
- **`cmd 0x00` sub-order `0xbd`.** An earlier draft called it a possible write barrier sent once
  before the sensitivity change. It appears in 13 files, and the controlled comparison refutes that
  reading: `ks-global-rt-sens-150` and `ks-global-rt-sens-200` write exactly the same 462 records
  each, and only the second carries a `0xbd`. It is absent from both global switch captures too,
  while appearing four times in `ks-create-rt-2`. It could still be sent conditionally, but it is
  not sent before every write.

## Corpus

Thirty-four capture files, 5630 frames, all decoding with correct framing and checksums and no hard
failures. Up from ten files and 1224 frames after Phase 1, twenty-seven after the 2026-08-29
sittings, and thirty-one after the first 2026-09-04 sitting.

Read requests and writes are separable by the `cmd 0x23` payload's lead byte, which matters when a
write carries a genuinely zero value. Lead `0x00` occurs 1035 times and is all-zero valued every
time; lead `0x01` occurs 488 times, 469 with non-zero values and 19 with zeros. Those 19 are exactly
the membership deletes and the Phase 1 `0x16`/`0x17` writes of `0`.
