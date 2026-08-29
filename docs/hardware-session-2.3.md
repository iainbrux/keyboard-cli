# Hardware session runsheet: task 2.3 and the Phase 2 verification checks

One sitting, roughly 45 to 60 minutes. Do the captures first and the `wh` checks second: capturing
the vendor first means the greying result later is interpretable rather than ambiguous.

## Before you start

1. `phase-2` merged to `main`, final re-review clean.
2. `wh.exe` rebuilt from the merged code:
   `cargo build -p wh-cli --release --target x86_64-pc-windows-gnu`
   A stale binary has already fooled a test once. Rebuild even if you think it is current.
3. Capture harness ready in the browser, vendor site loaded, board connected.
4. **First action of the session, before anything else touches the board:**
   ```
   wh backup
   wh backups list
   ```
   Note the filename. That is your restore point for the whole session.
5. Confirm every `wh` command prints `transport: hardware` on stderr. If it says `replay`, `WH_REPLAY`
   is set and nothing you do is real.

## Part one, task 2.3: how the vendor allocates a keyset

All of this is the vendor UI. `wh` does not write during part one. Use keys never touched in Phase 1:
`u, i, o, p, j, k, m`.

Save each capture to its own file and record what you did alongside it.

| Step | Action in the vendor UI | Capture as | The question |
|---|---|---|---|
| 1 | Load the page, nothing else | `ks-baseline` | A clean starting state for the diffs |
| 2 | Create an actuation point keyset on `u,i,o,p`, any value | `ks-create-ap-1` | **Does anything write `0xFF`? What index?** |
| 3 | Create a second actuation point keyset on `j,k` | `ks-create-ap-2` | Max plus one, or reuse? |
| 4 | Create a rapid trigger keyset on `u,i` | `ks-create-rt-1` | Are the two groupings independent? |
| 5 | Create a second rapid trigger keyset on `m` | `ks-create-rt-2` | **Does `0xFE` reach 2, or is it a boolean?** |
| 6 | Delete the actuation point keyset from step 2 | `ks-delete-ap-1` | What does a delete write? |
| 7 | Create a third actuation point keyset on `o,p` | `ks-create-ap-3` | Gap reuse, or max plus one? |

After each step run `wh dump --json` and keep the output. `wh` reads `0xFF` and `0xFE` per key now, so
diffing consecutive dumps shows exactly which keys changed membership without reading any hex.

**Steps 2 and 5 are the ones that matter.**

- If step 2 shows nothing writing `0xFF`, that field is firmware-derived, no host tool can write
  keysets, and task 2.4 leaves the plan. That is a real and useful outcome, not a failed session.
- If step 5 shows `0xFE` reaching `2`, it is an index like `0xFF` and not a boolean, which changes how
  2.4 has to model it.

## Part two: the five verification checks

These are `wh` writing to the board. Pick keys the vendor keysets do not already cover.

1. **Actuation point on an untouched key.**
   ```
   wh set ap --keys f --set 0.30 --dry-run
   wh set ap --keys f --set 0.30
   ```
   Then look at `f` in the vendor UI. Is it still greyed?
   That MODE nibble 0 causes the greying is a **hypothesis**. Part one tells you whether `0xFF` is
   the real cause, so this result is now readable either way.

2. **Rapid trigger must survive an actuation point change.** Pick a key with rapid trigger on, or turn
   it on first:
   ```
   wh set rt --keys w --set 0.5
   wh get rt --keys w
   wh set ap --keys w --set 1.20
   wh get rt --keys w
   ```
   `wh` now checks this itself: it reads every key's MODE before the write and fails the run, naming
   the key and both values, if a key it deliberately left alone reads back changed. **A silent
   success here is now meaningful.** Before this branch it was not.

3. **Profile select.**
   ```
   wh profile
   wh profile 2
   wh profile
   wh profile 1
   ```
   The middle read must report 2. Note that switching profiles is what makes `wh restore` refuse a
   snapshot taken on another profile, so return to the profile your backup came from before restoring.

4. **Time a full dump.** It is six reads per key now, 408 roundtrips over 68 keys.
   ```
   time wh dump > /dev/null
   ```
   Record the number. If it feels slow, that is a real finding and the read count is the cause.

5. **Partial-failure awareness, no action needed.** If a `set ap` fails part way through its batch, a
   key's MODE and AP records can land in different frames. Measured over a whole-board write: 126
   records in 9 frames, 4 keys straddling a boundary. Those keys end up detached from global travel
   holding their old actuation point. `wh restore --last` is the recovery. You do not need to trigger
   this; just recognise it if it happens.

## Finishing

```
wh restore --last          # or name the file from step 4 of the prep
wh dump --table            # eyeball the board back at baseline
```

`wh restore` refuses a snapshot whose recorded profile differs from the board's current one, with no
override, so make sure you are back on the profile you started from.

## What to bring back

- The seven capture files, and the `wh dump --json` output taken after each step.
- Whether `0xFF` was written at all, and by what.
- Whether `0xFE` ever read `2`.
- Whether `f` was still greyed after check 1.
- The dump timing.

That is enough to write 2.4, or to delete it.
