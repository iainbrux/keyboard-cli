# Outstanding work

Live checklist for `wh`. Items are struck through and ticked as they complete. Anything needing the
keyboard physically present is marked **[hardware]**.

Evidence for every protocol claim below is in `docs/protocol.md` and `docs/protocol-inventory.md`,
measured from 1224 frames of real device traffic.

## Phase 1

Complete. See the Done section.

## Backlog, not scheduled

### Hardware questions **[hardware]**

- [ ] **The knob.** What it is bound to by default, and whether it can be rebound at all. It is not
  one of the 68 keys, which is a useful negative we already have.
- [ ] **The numbered LEDs beside the knob.** What drives them, and whether they track the actuation
  point or the rapid trigger sensitivity. Command `0x18` is the candidate, on byte patterns alone.

### Features

- [ ] **Write keyset membership, layout `0xFE`.** Settings we write do apply and do work, confirmed
  on hardware, but the vendor configurator shows them greyed and outside any named keyset, because we
  never write that flag. The vendor writes `1` on keyset create and `0` on delete. Cosmetic for an
  alpha, but it is the difference between our changes looking native in their UI and looking like
  loose overrides.
- [ ] **List backups, and reconsider what `--last` means.** There is no `wh backups list`, and manual
  and automatic backups are indistinguishable. Every write takes an auto-backup first, so `--last`
  means "undo the last command", not "return to where I started". That is defensible but surprising,
  and during the hardware session it briefly looked like a restore bug when the tool was correct.
- [ ] **A TUI clone of the vendor configurator**, running inside the `wh` binary. The prerequisite is
  now met: the write path has been exercised against hardware.
- [ ] **A spinner on write commands.** Reads deliberately get none; their speed is a feature.
- [ ] **Delete or rename a stored key group.** There is no CLI route today, which matters because a
  group whose name collides with a key name is now correctly refused, and recreating it under a new
  name is the only recovery.
- [ ] **A hex form in the selector**, so a key with no name, such as `0x01`, can be typed back into a
  selector after `wh keys list` shows it.
- [ ] **`wh profile`, to read and select the active profile.** The protocol is already measured:
  `cmd 0x00` sub-order `0x70`, argument `0xFF` to read, a zero-based index to select. Reading is
  already implemented; only the command surface and the select encoder are missing.

### Seven residual documentation inaccuracies

Parked at the final whole-branch review with rulings rather than reopening a fix wave on the last
gate. Every underlying protocol claim is true; these are wrong pointers and over-broad scopes.

- [ ] `crates/wh-device/src/ops.rs` says nibble `0` is "something `wh` does not write". `restore`
  does write it, 58 times on this board, because it writes `mode_raw` verbatim.
  `docs/protocol.md` is correctly scoped to turning rapid trigger off; the paraphrase dropped the
  scope.
- [ ] `capture/README.md` credits a re-read to `remap-one-key`. That capture has four frames and no
  re-read; the re-read is in `remap-matrix-read`.
- [ ] Three stale references in the tracked plan sit outside the superseded banner's stated scope:
  `rt-w-0.6` and `ap-w-1.2` in the embedded capture README block, and three `write_and_save` mentions
  relying on a blanket note far above them.
- [ ] The plan's new "AP is layout `0x04` alone" annotation is right as an answer to that step's
  question, but reads as a claim about what an AP change writes, which the vendor capture contradicts.
- [ ] `crates/wh-config/src/snapshot.rs` says hand-editing a snapshot's `rt` field changes what
  dump-style tooling prints. `wh dump` reads the live device, never a snapshot file, so it changes
  nothing any command prints.
- [ ] The Task 20 report justifies its hidpkg conclusion by saying that directory holds only a
  tarball and a `package.json`. It holds a tracked `dist/{cjs,esm}` tree of compiled JavaScript. The
  conclusion is correct and was verified another way, but **do not reuse that reasoning in a
  licensing decision.**

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
- [x] ~~Task 20: the protocol document, the README, the licence and third-party notices, two
  refactors, and the em dash sweep. Four fix rounds.~~
- [x] ~~Final whole-branch review of all 41 commits, one fix wave, one scoped re-review. Approved.~~
- [x] ~~Merge `phase-1` into `main`.~~
- [x] ~~Read path verified against the real board: serial, all 68 keys, `get`, `backup`, `selftest`,
  and a dry-run frame whose records match the vendor's byte for byte.~~
- [x] ~~Identify layouts `0x00` and `0x01` as the base and FN mapping layers.~~
- [x] ~~**Write path verified against the real board.** `set rt --keys w --set 0.5` landed and the
  vendor UI confirmed it.~~
- [x] ~~**Restore drill.** `restore --last` restored exactly the snapshot it named, 68 keys verified,
  confirmed against the backup files on disk.~~
- [x] ~~**Does the board accept our short write frames?** Yes. We send `len=13`; the vendor pads to
  `len=57`. The padding is not required.~~
- [x] ~~**Does `set ap` on a key at touch nibble 0 write a register the board ignores?** No. F was set
  to 0.30mm and physically actuates at 0.30mm, checked against E at 2.00mm. The MODE write the vendor
  sends before every actuation point change is not needed for the value to take effect. This had been
  predicted as a probable bug from a correlation across 63 keys; the prediction was wrong, and
  measuring it was what settled it.~~
