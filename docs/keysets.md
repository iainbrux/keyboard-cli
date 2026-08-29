# Keysets, measured

How the vendor configurator creates, values and deletes a keyset, measured on 2026-08-29 against a
real K-001 running firmware `App_V1.1.046000`. Seven capture scenarios plus a `wh dump` after each
step, diffed against a baseline taken before anything was touched.

This document is the evidence base for task 2.4. Everything here is measured. Where something is
inferred, it says so.

## What a keyset is

Two per-key layouts hold membership, and they are **independent groupings over the same 68 keys**:

| Layout | Grouping | Values seen |
|---|---|---|
| `0xFF` | actuation point keyset | `0`, `1`, `2`, `3`, `4`, `5` |
| `0xFE` | rapid trigger keyset | `0`, `1`, `2` |

`0` means the key is in no keyset of that kind. A key can sit in one of each at the same time, with
different membership. Measured: `u` and `i` held `0xFF=3` and `0xFE=1` simultaneously while `o` and
`p` held `0xFF=3` and `0xFE=0`.

Independence was confirmed three ways: creating a rapid trigger keyset over two members of an
existing actuation point keyset left `0xFF` untouched; creating one over a key in no actuation point
keyset left it at `0`; and deleting an actuation point keyset left `0xFE` untouched.

A keyset holds exactly one value across its members. After four keysets existed, every index mapped
to a single distinct actuation point, and the configurator's left pane listed them in ascending index
order with deleted indices simply absent.

## Allocation

**Max plus one, monotonic, never reusing a gap.** Measured three times.

| Step | Existing indices | Allocated |
|---|---|---|
| Create on `u,i,o,p` | 1, 2 | **3** |
| Create on `j,k` | 1, 2, 3 | **4** |
| Delete 3, then create on `o,p` | 1, 2, 4 | **5**, not 3 |

The two layouts have **separate counters**. With `0xFF` already at 4, the first rapid trigger keyset
took `0xFE=1`, not 5.

The upper bound is unmeasured. The field is 16 bits and only single-digit values have been observed.

## The write sequences

Membership is always written **one record per frame**, never batched, even though the protocol allows
14 records per report. Values are batched normally. That asymmetry is the vendor's, and worth
matching rather than optimising.

### Creating an actuation point keyset

Two separate user actions, seconds apart. Membership first, then the value.

```
u:0xFF=3          one key per frame
i:0xFF=3
o:0xFF=3
p:0xFF=3
        ... separate action ...
u:MODE=0x10  i:MODE=0x10          touch nibble 1 (Single)
o:MODE=0x10  p:MODE=0x10
u,i,o,p: AP=1500
u:MODE, u:RT press=100, u:RT release=100   (and the same for i, o, p)
u:0x16=100, u:0x17=100                     (and the same for i, o, p)
```

### Creating a rapid trigger keyset

One action, and the order is **reversed**: values first, membership last.

```
u:MODE=0x30  i:MODE=0x30          touch nibble 3 (Rt)
u:AP=1500    i:AP=1500            rewritten unchanged
u:MODE, u:RT press=100, u:RT release=100   (and i)
u:0x16=100, u:0x17=100                     (and i)
u:0xFE=1          one key per frame
i:0xFE=1
```

The actuation point is rewritten unchanged, and MODE moves from nibble 1 to nibble 3 while `0x04`
keeps its value. **A rapid trigger key keeps its own actuation point.** This is the second
independent measurement of that fact, and it is what the `ap_records` rule in `wh-device` depends on.

### Deleting a keyset

Values reset **first**, membership cleared last.

```
u:MODE=0x30  i:MODE=0x30          RT members keep nibble 3
o:MODE=0x10  p:MODE=0x10          non-RT members keep nibble 1
u,i,o,p: AP=2000                  reset to the global actuation point
...RT press/release and 0x16/0x17 rewritten...
u:0xFF=0
i:0xFF=0          one key per frame
o:0xFF=0
p:0xFF=0
```

**A delete does not return the key to touch nibble 0 (Global).** It leaves nibble 1, or nibble 3 for
a rapid trigger key, and resets the actuation point to the global value instead.

That last point matters for the greying question. The vendor had the option of leaving those keys
holding 1.50mm with no keyset, and does not take it. It writes the global value back first. On this
evidence the vendor never leaves a key with a private actuation point that belongs to no keyset,
which is exactly the state `wh set ap` produces today. **Inference, not measurement:** our MODE fix
alone may therefore not be enough to stop the configurator greying our writes, and task 2.4 may be
required for it. The check is one line in `docs/hardware-session-2.3.md`.

## What this specifies for task 2.4

- Two independent counters, one per layout.
- Allocate max plus one. Do not scan for a free gap; the vendor does not.
- Write membership one record per frame.
- Creating an actuation point keyset: membership, then values. Creating a rapid trigger keyset:
  values, then membership. Deleting: reset values to the global, then clear membership.
- Deleting must not touch the other layout's membership.

**`wh restore` must also restore membership.** Task 2.1 had restore ignore `0xFF` and `0xFE`, which
was correct while nothing was known to write them. That reason is gone. Measured consequence: a
restore taken tonight put every value back to its snapshot and left four keysets in place, so the
board no longer matched the snapshot it had just been restored from.

## Two corrections to earlier claims

**Layouts `0x16` and `0x17` are not always zero.** Every capture before tonight showed them zero
across 1858 records, and this was recorded as "never once observed non-zero". They hold `100` on
every key touched tonight. They are also not a copy of the rapid trigger press and release values:
when a keyset's sensitivity moved to `550`, `0x14` and `0x15` followed and `0x16` and `0x17` stayed
at `100`. The configurator's global `RT SENSITIVITY` reads 0.10mm, so they may track that.
Unconfirmed.

**`Snapshot::global.travel_mm` is not the global actuation point.** It is the configurator's
`"MM" CUSTOM VALUE`, the step size for its `< >` controls. Measured: changing that control from 0.10
to 0.15 moved the field from `0.1` to `0.15`, and the write carried `travel=150um`. The real global
actuation point, 2.00mm, is not in that record at all; it is simply what every key in no keyset holds
in layout `0x04`. The field should be renamed.

## An open item on the global record

The vendor's `cmd 0x29` write always carries `press_dead=200` and `release_dead=200`. Those are
constants in the vendor's own SDK template, not user settings. **The board reports both as `0` on
read**, so the values cannot be preserved across a read-modify-write.

`ops::restore_all` passes the snapshot's values into those fields, which means `wh restore` writes
`0, 0` where the vendor has only ever written `200, 200`. Whether the board cares is unmeasured, and
unobservable through the read path. The safe fix is to send the vendor's constants rather than the
zeros we read back.

## Command `0x2c` is SOCD, no longer inferred

Measured in the same session, from the configurator's connect sequence:

```
query w (0x1a)  ->  1a 16 00 16 1a      w and s
query a (0x04)  ->  04 07 00 07 04      a and d
query s (0x16)  ->  16 1a 00 1a 16      s and w
query d (0x07)  ->  07 04 00 04 07      d and a
```

Query is `[rw, key, 0xFF, ...]`. The reply is `[status, keyA, keyB, 0, keyB, keyA, ...]`, the pair
given both ways round. The ADVANCED tab carries a SOCD control, and these are the pairs it holds, so
the name is now measured rather than inferred from byte shapes.

## Corpus

Sixteen capture files, 2210 frames, all decoding with correct framing and checksums and no hard
failures. Up from ten files and 1224 frames after Phase 1.
