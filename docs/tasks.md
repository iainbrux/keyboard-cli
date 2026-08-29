# Outstanding work

Live checklist for `wh`. Items are struck through and ticked as they complete. Anything needing the
keyboard physically present is marked **[hardware]**.

Evidence for every protocol claim below is in `docs/protocol-inventory.md`, measured from 1224 frames
of real device traffic.

## Phase 1

### In flight

- [ ] **Task 20.** `docs/protocol.md`, `README.md`, corrections to `docs/backlog.md`, rename
  `order::CONFIG` to `order::PROFILE`, move `ProfileNumber` into `wh-proto`, sweep the remaining em
  dashes from the plan and spec, then the gate.

### Then, no hardware needed

- [ ] **Final whole-branch review** of every commit on `phase-1`.
- [ ] **Merge `phase-1` into `main`.**

### Hardware session part two **[hardware]**

Needs the keyboard attached and the vendor configurator tab closed, since it holds the device
exclusively.

- [ ] **First real write.** `bin/wh set rt --keys w --set 0.5`, then confirm the web UI shows 0.5mm on
  W. The dry run frames have already been checked against the vendor's own captured frames and match
  byte for byte, so this is the last unverified step of the write path.
- [ ] **Restore drill.** `bin/wh restore --last`, and confirm `dump` matches the pre-write state.
- [ ] **Does `set ap` on a key at touch nibble 0 write a register the board ignores while reporting
  success?** This is an unproven mirror of a bug already found and fixed, where RT-off wrote nibble 0
  and silently discarded a per-key actuation point.
- [ ] **Does the board accept our short write frames?** We send `len=13`, the vendor pads to `len=57`
  with zero record slots. Reads prove the board honours the length field, since that is what makes
  `dump` work. Writes are not yet proven.

## Backlog, not scheduled

### Hardware questions **[hardware]**

- [ ] **The knob.** What it is bound to by default, and whether it can be rebound at all. It is not
  one of the 68 keys, which is a useful negative we already have.
- [ ] **The numbered LEDs beside the knob.** What drives them, and whether they track the actuation
  point or the rapid trigger sensitivity. Command `0x18` is the candidate, on byte patterns alone.

### Features

- [ ] **A TUI clone of the vendor configurator**, running inside the `wh` binary. Blocked until the
  write path has been exercised against hardware: a UI that makes it easy to change many settings
  quickly is the worst place to discover a write bug.
- [ ] **A spinner on write commands.** Reads deliberately get none; their speed is a feature.
- [ ] **Delete or rename a stored key group.** There is no CLI route today, which matters because a
  group whose name collides with a key name is now correctly refused, and recreating it under a new
  name is the only recovery.
- [ ] **A hex form in the selector**, so a key with no name, such as `0x01`, can be typed back into a
  selector after `wh keys list` shows it.
- [ ] **`wh profile`, to read and select the active profile.** The protocol is already measured:
  `cmd 0x00` sub-order `0x70`, argument `0xFF` to read, a zero-based index to select. Reading is
  already implemented; only the command surface and the select encoder are missing.

### Protocol gaps

- [ ] **Commands `0x18` and `0x2c`.** `0x2c` is almost certainly SOCD: it queries by key and replies
  with symmetric pairs, measured as W with S and A with D. The behaviour is measured, the name is
  inference.
- [ ] **Nine `cmd 0x00` sub-orders.** All request and reply balanced, none ever failing, none needed
  by anything in Phase 1.
- [ ] **Layouts `0x16`, `0x17`, `0x19` and `0xff`.** `0x16` and `0x17` carry 1858 records each and
  were never once observed non-zero.
- [ ] **Key `0x01`, probably FN. [hardware]** Deliberately unmeasured, because confirming it means
  remapping FN away and FN is how you reach the layer that would let you undo that.
- [ ] **Widen what a snapshot captures.** It currently records global travel, four layouts per key,
  and the profile. It does not record key mappings, the FN layer, SOCD, dynamic keystroke, mod tap,
  gamepad configuration, RGB, or polling rate.

## Done

- [x] ~~Tasks 1 to 18: the four-crate workspace, the codec, the transport, snapshots and groups, the
  CLI surface, and the capture harness.~~
- [x] ~~Task 19: the hardware capture session. Ten scenarios, 1224 frames, all passing framing and
  checksum in both directions with zero failures.~~
- [x] ~~Task 19b group A: stop sending a SAVE order the vendor never sends, read the firmware string
  from its length prefix, and name the four board-function keys.~~
- [x] ~~Task 19b group B: record the active profile in snapshots and refuse a profile-mismatched
  restore.~~
- [x] ~~Read path verified against the real board: serial, all 68 keys, `get`, `backup`, `selftest`,
  and a dry-run frame whose records match the vendor's byte for byte.~~
- [x] ~~Identify layouts `0x00` and `0x01` as the base and FN mapping layers.~~
