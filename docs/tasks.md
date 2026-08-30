# Outstanding work

Live checklist for `wh`. Items are struck through and ticked as they complete. Anything needing the
keyboard physically present is marked **[hardware]**.

Evidence for every protocol claim below is in `docs/protocol.md`, `docs/protocol-inventory.md` and
`docs/keysets.md`, measured from 3696 frames of real device traffic across 27 capture files.

## Phase 1

Complete. See the Done section.

## Phase 2

Numbered 2.0 to 2.9 from `docs/superpowers/specs/2026-08-29-phase-2-design.md`, plus 2.10 to 2.13
added from what the hardware sessions measured. The objective
is close to 1:1 interoperability between the CLI and terminal.wallhack.com. Keysets come first,
because they are the one thing that makes our writes render as loose overrides in the vendor UI
rather than as settings it recognises.

- [x] ~~**2.0 JSON replaces TOML** for backups and snapshots. Older TOML backups are still read.~~
- [x] ~~**2.1 Read keyset membership.** Layouts `0xFF` (actuation point keyset index, inferred from
  read correlation) and `0xFE` (rapid trigger keyset membership, measured from write evidence),
  read per key and surfaced in `wh dump` (JSON), `wh dump --table`, `wh get`, and snapshots. Read
  only, no writes.~~
- [x] **[hardware verification outstanding]** ~~**2.2 Actuation point writes match the vendor.**
  `wh set ap` now also promotes MODE nibble 0 (Global) to nibble 1 (Single) on every actuation point
  change, the marker the vendor sets that our own writes previously omitted. `Single`, `Rt`,
  `RtContinuous`, and `Unknown` touch nibbles are left untouched. Covered by unit and end-to-end
  tests against replayed frames; not yet confirmed against real hardware, see `README.md`.~~
- [x] ~~**2.3 The keyset capture session.**~~ Done 2026-08-29. Seven scenarios captured against the
  real board. `0xFF` is host-written, allocation is max plus one and never reuses a freed index, the
  two layouts have separate counters, `0xFE` is an index and not a boolean, and a delete resets the
  value to the global before clearing membership. Full write-up in `docs/keysets.md`.
- [x] ~~**2.12 Model touch nibble 2 (global rapid trigger).**~~ Measured 2026-08-29 and confirmed
  by capture on 2026-08-30: switching GLOBAL RAPID TRIGGER on reads all 68 keys back at nibble `1`
  and writes nibble `2` to every one; switching it off writes nibble `1` back. `TouchMode` mapped
  `2` to `Unknown(2)` and `rt_enabled()` matched only `Rt`/`RtContinuous`, so `wh dump` and
  `wh get rt` reported rapid trigger **off** on a board where it was on for every key. A reporting
  bug, not a data-loss one: read-modify-write preserved the nibble it could not name.
  `TouchMode::RtGlobal` added, with `rt_enabled`, `rt_off_records` and `raw_mode_rt_on` fixed.
- [ ] **2.4 Write keyset membership. No longer blocked.** `docs/keysets.md` specifies it completely
  from fifteen capture scenarios: one write template shared by every operation, values always
  before membership, membership one record per frame and always last, non-owned layouts rewritten at
  each key's current value, the whole template written only when an owned value differs, max-plus-one
  allocation from live membership with no gap reuse, and a new keyset taking the global value rather
  than its members'. Creating a keyset over a key already in one steals it. **Scope grew:**
  `wh restore` must restore membership too. Measured on 2026-08-29, a restore put every value back
  and left four keysets in place, so the board no longer matched the snapshot it had just been
  restored from. CLI surface agreed: `wh keyset list|create|set|delete`, and `wh set ap` on a key
  already in a keyset splits it into a new keyset automatically, telling the user it did so.
- [ ] **2.13 `wh set rt --off` must clear rapid trigger keyset membership. Depends on 2.4.**
  Measured in `captures/rt-off-w.jsonl`, frame 70: the vendor's per-key rapid trigger off writes
  `0xFE = 0` after the value records, one record per frame, as the last thing it sends. `wh` writes
  the MODE record and stops, so a key turned off through `wh` keeps whatever membership it held and
  the configurator still lists it in a keyset. That file's read sweep does not cover `0xFE`, so
  whether `W` was in a keyset beforehand is unmeasured; what is measured is that the write is sent
  unconditionally. Implement by routing `ops::set_rt_off` through `keyset::plan` with
  `Change::rt_off(press, release)` and `Some(KeysetIndex::clear(Kind::Rt))` rather than by hand. The
  sensitivities
  come from `keyset::global_rt`, which reports whether the keys outside a keyset agree rather than
  trusting one of them, and refuses a `Membership` of the wrong kind.

  Related, from the same review: on a board with the global rapid trigger switch on, every key
  outside a keyset sits at nibble `2`, so `wh set rt --keys all --off` now writes all 68 keys where
  it previously wrote none. That is vendor-consistent, but it means this gap is reached far more
  often than it was.

  **Two things to settle before implementing, both found by review.** First, `global_rt` returns
  three variants and this task says only "the sensitivities come from `global_rt`". What a `Split`
  or a `NoneOutsideAKeyset` should do is undecided, and the obvious "unwrap or default" lands on
  `Um(0)`, which would write `0x14 = 0, 0x15 = 0`, a value the vendor has never been observed
  writing. Second, `keyset::plan`'s skip rule is all-four-records-or-none, so a key at touch nibble
  `0` whose sensitivities differ from the global gets a MODE record written at its unchanged
  nibble-0 value. `ops::rt_off_records` refuses to send nibble 0 at all, deliberately, because what
  it does to a key's actuation point is unmeasured. Routing this task through `plan` as written
  would start sending it. Neither is a defect in `plan`; both are decisions this task has to make.

  Measured in the same review, and settling an earlier doubt: the vendor **does** reset the
  sensitivities on a per-key rapid trigger off. `rt-off-w.jsonl` shows W going from 500/500 to the
  global 100/100. `ops::rt_off_records` writes MODE alone and leaves the private value in place, so
  it is the one that diverges; routing through `Change::rt_off` removes a divergence rather than
  creating one.
- [ ] **2.10 Rename `Snapshot::global.travel_mm`.** Measured: it is the configurator's `"MM" CUSTOM
  VALUE`, the step size for its steppers, not the global actuation point. The real global actuation
  point is not in that record; it is what every key in no keyset holds in layout `0x04`.
- [ ] **2.11 Stop writing zero dead zones on restore.** The vendor's `cmd 0x29` write always carries
  `press_dead=200` and `release_dead=200`, constants in its own SDK template. The board reports both
  as `0` on read, so `wh restore` writes `0, 0` where the vendor has only ever written `200, 200`.
  Send the vendor's constants instead of the zeros we read back.
- [x] ~~**2.5 `wh profile`, read and select.** `cmd 0x00` sub-order `0x70`, argument `0xFF` to read,
  a zero-based index to select.~~
- [x] ~~**2.6 `wh backups list`, and what `--last` means.** Manual and automatic backups are now
  distinguishable, and `wh backups list` names what took each snapshot.~~
- [x] ~~**2.7 Delete and rename a stored key group.** `wh keys ungroup` and `wh keys rename`.~~
- [x] ~~**2.8 A hex form in the selector**, so a key with no name, such as `0x01`, can be typed back
  into a selector after `wh keys list` shows it.~~
- [x] ~~**2.9 Documentation fixes.** Corrected the `0xFF` claim, the `ap_records` comment, the seven
  parked inaccuracies below, added the no-drift invariant, and documented the new CLI surface.~~
  - **Documentation inaccuracies, kept on record rather than deleted with the task.** Deleting the
    list when 2.9 was ticked removed the only trace of what had been claimed fixed.
    - `capture/README.md`, `remap-one-key`: marked fixed, still wrong. It described a re-read of
      layout `0x00` that the capture does not contain (four frames: a `0xbd` order and its ack, one
      `rw=0x01` write of key `0x0e` layout `0x00` value `0x003a`, and its ack). The re-read is in
      `remap-matrix-read`. Found at the final whole-branch review and corrected there.
    - The cause of the vendor UI greying our writes was asserted as fact in two places that
      contradicted each other, `ops.rs` naming the MODE nibble and `docs/backlog.md` naming `0xFF`.
      Both now read as hypotheses awaiting the hardware session. Same review.

## Backlog, not scheduled

### Hardware questions **[hardware]**

- [ ] **A device spy, so we can read the board directly.** Everything we know came from capturing the
  vendor website, so we can only see what it chooses to do. A spy would show what the board sends on
  its own. Start with `wh spy` over the vendor collection we already have access to, then Raw Input
  for key presses. **This unblocks the knob item below, and it settles whether key `0x01` is FN by
  observation, which we had parked as unmeasurable because confirming it means remapping FN away.**
- [ ] **Setting the colour of the LEDs beside the knob.** They do change colour, so they are RGB and
  usable as an output surface, not just a thing to decode. Command `0x18` is the candidate, on byte
  patterns alone. Capture a pure red, green and blue in sequence to fix the byte order.
- [ ] **Are the key backlights colour-programmable, or white only?** The LIGHT key (`0xFC`) is
  confirmed, so lighting is a first-class board function. Whether it is RGB, per-key addressable, or
  a single-colour backlight is open. Mostly answerable by looking at the board and the vendor UI
  before any capture.
- [ ] **How the knob is programmed.** Volume travels over the standard HID consumer-control
  collection, not the vendor collection we capture, so our existing method cannot see it at all.
  Blocked on the spy.

### Features

- [ ] **A TUI clone of the vendor configurator**, running inside the `wh` binary. The prerequisite is
  now met: the write path has been exercised against hardware.
- [ ] **A spinner on write commands.** Reads deliberately get none; their speed is a feature.

### Protocol gaps

- [ ] **Command `0x18`.** Suspected RGB or LED control, 8 frames. `0x2c` was resolved on
  2026-08-29: it is SOCD, measured, see `docs/keysets.md`.
- [ ] **Nine `cmd 0x00` sub-orders.** All request and reply balanced, none ever failing, none needed
  by anything in Phase 1.
- [ ] **Layouts `0x16`, `0x17` and `0x19`.** `0x16` and `0x17` were recorded as never once
  observed non-zero across 1858 records. That held only until a keyset was created: they hold `100`
  on every key a keyset touches. They are not the global rapid trigger sensitivity either, measured
  2026-08-29: they stayed at `100` through two global changes that moved `0x14`/`0x15` to `150` and
  then `200`. `0x19` is still only ever `0x0000` or `0x3e2c`.
- [ ] **Where the global rapid trigger sensitivity is stored.** No global command carries it. It
  appears only in `0x14`/`0x15` of keys outside a rapid trigger keyset, which would also be how the
  configurator reads it back. Plausible and testable, not measured. Needed to name the reset target
  of a rapid trigger keyset delete as something other than "what the vendor wrote".
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
