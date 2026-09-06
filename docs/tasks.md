# Outstanding work

Live checklist for `wh`. Items are struck through and ticked as they complete. Anything needing the
keyboard physically present is marked **[hardware]**.

Evidence for every protocol claim below is in `docs/protocol.md`, `docs/protocol-inventory.md` and
`docs/keysets.md`, measured from 5860 frames of real device traffic across 36 capture files.

## Phase 1

Complete. See the Done section.

## Phase 3

Ordered by the operator on 2026-09-05. The north star is a TUI replicating terminal.wallhack.com
one to one, mouse-clickable and arrow-navigable. The ordering principle: extend the data model and
finish the transport before the TUI is written, so it is written once against its final foundation.
Key remapping and the lighting build (3.6 and 3.7) can swap freely; 3.4 must land before 3.5.

- [x] ~~**3.1 `wh set --mm`, the configurator's "MM" CUSTOM VALUE.**~~ The stepper step size, not an
  actuation point. Already measured: three vendor writes in the corpus (`custom-value-change` at
  travel 400 and 650, `custom-value-nudge-after-restore` at 150), all through `cmd 0x29` with dead
  zones 200/200. Snapshot field renamed to `custom_value_mm` on 2026-09-05 and already restored
  faithfully. The flag name `--mm` is reserved for this by the operator's 2026-09-04 ruling. What is
  left is only the write command and its tests. No new captures needed.

  Closed 2026-09-05 as `wh set mm --value <mm>`, a subcommand rather than a literal `--mm` flag on
  `set` itself: a flag on the parent command would conflict with its own subcommands, an awkward
  clap shape for no gain, and the operator's ruling reserves the name `--mm` for this setting, not
  a specific flag position. `wh set mm` honours that reservation and takes no `--keys`/`--pick`,
  since the record is one value for the whole board, like `--base` but with no selection to make
  at all. It reads `ops::global_travel` first so its announcement names the value it is about to
  replace, writes `cmds::write_global_travel` with the new value and the same vendor dead-zone
  constants `wh restore` sends (`run::VENDOR_PRESS_DEAD`/`VENDOR_RELEASE_DEAD`), and verifies the
  travel field on readback only, since the dead zones read back as `0` regardless of what was
  written on every measured board. `BackupReason::SetMm` is tied to a real run from the start by
  `set_mm_end_to_end_records_its_own_command_as_the_backup_origin` in `tests/dump.rs`, unlike the
  six untied variants 2.30 still lists.

  Review ruling: when the pre-read shows the board already at the target, `wh set mm` skips the
  write entirely (no backup, no frame), announcing `already matches ... nothing written` in
  `--base`'s own vocabulary rather than reporting a no-op after taking one. `--base`'s own skip was
  the precedent; whether the vendor itself writes on a no-op set is unmeasured, so this is `wh`'s
  own choice, made to avoid an avoidable 200/200 dead-zone write while `docs/backlog.md`'s question
  stays open.

- [x] ~~**3.2 One capture session: SOCD, RGB/LED, dead zones.**~~ Run 2026-09-05, sixteen files,
  written up in `docs/protocol.md` and `docs/keysets.md`. Everything it was for is answered, and
  more: the SOCD write model is measured end to end, `cmd 0x18` is the lighting record with
  brightness and sleep timer decomposed, sub-order `0xc0` is Show Analog Output, layouts
  `0x16`/`0x17` are the safety-zone margins (closing a Phase 1 mystery), the configurator has no
  dead-zone control so the `cmd 0x29` 200/200 exposure is retired as not operator-reachable (why
  200 stays open,
  `docs/backlog.md` has the split), an untouched profile's
  actuation points read 2.00mm on all 68 keys (corroborating `NO_SIGNAL_BASE`), and the SHARE
  tab's export format was cracked and proven in both directions. `socd-reload-read` and
  `profile-select-3` establish profiles 4 and 3 from their own frames; the other fourteen files
  are attributed by sitting continuity, not frames.

- [x] ~~**3.3 SOCD, now fully unblocked.**~~ The wire model, measured 2026-09-05, is in
  `docs/protocol.md` under "SOCD": pair writes are one `cmd 0x2c` frame carrying both directions
  plus a priority enum (`0` last-input, `1` first key, `2` second key, and the board normalises
  replies per queried key, so reads must normalise before comparing), participation is MODE's
  advanced nibble equal to `8`, an enum value per the vendored docs and never a bit test, set by
  the board itself on a pair write, removal writes MODE with the nibble cleared and sends no
  `0x2c`, discovery is a MODE sweep then a `0x2c` query per flagged key, and arbitrary keys are
  accepted. CLI surface to design against the UI's own vocabulary (pairs with a
  PRIORITY of LAST-INPUT or one of the two keys): something like `wh socd list | pair | unpair`.
  Open questions carried from the captures: whether an orphaned pairing survives a remove on the
  board, and the two further priority modes the vendored docs name but the corpus never reached
  (`3` neutral, `4` depth-based).

  Closed 2026-09-05 as `wh socd list | pair | unpair`. A pairing is modelled as an unordered pair
  plus a winner (`wh_proto::socd::Pairing`), never as the wire's priority byte, which is what
  makes the board's per-key normalisation a non-problem: the two spellings of one pairing compare
  equal, and `list` shows each pairing once even though both members are queried. The codec
  reproduces every captured `cmd 0x2c` frame byte for byte, checksum included. Participation is
  `Mode::is_socd`, the one place the advanced nibble is compared, and it compares `== 8`; a
  fixture at nibble `9` (RS, from the vendored docs, never observed on this board) pins that a bit
  test would be wrong. `pair` sends one frame and no MODE record, since the board sets the flag
  itself, and verifies by re-reading both keys' MODE and re-querying both rows. `unpair` writes
  MODE with the advanced nibble cleared on both keys and no `cmd 0x2c`, matching the vendor, and
  preserves each key's touch nibble; every captured vendor remove was on touch nibble 0, so that
  last part is `wh`'s own read-modify-write rule applied past the measurement, and it says so.
  A key may sit in one pair only, which the UI's model implies and the operator confirmed;
  whether the board would accept an overlap stays unmeasured, because `pair` refuses rather than
  finding out by accident. Every key argument resolves through the shared `Selector` grammar
  against the board's own live matrix and must name exactly one key. The board accepting arbitrary
  keys is about the wire, not about what `wh` should send: pairing a key the board in front of you
  does not have wrote a pairing `list` could not show and `unpair` could not undo, and pairing one
  real key with one absent one left the whole command family refusing until the vendor UI cleared
  it, both measured in review. The arity rule is also what keeps a broad selector out of a write
  path here, so there is still no whole-board form needing a typed-`yes` guard.

  The two open questions above are unchanged by this work: nothing here can see an orphaned
  pairing, since discovery only queries flagged keys, and priority `3` and `4` are refused by
  their own decode error rather than silently read as last-input.

- [x] ~~**3.4 Teach the transport to receive the board's unsolicited `0xbe` frame.**~~ `Session`
  now queues an edge that arrives during any roundtrip rather than losing it: a roundtrip parses
  every `cmd 0x00` sub-order `0xbe` reply-shaped frame (`be_event`, wider than the strict
  `adjust_event`) and queues it, since sub-order `0xbe` appears in no request in the corpus, so
  every such frame is certainly unsolicited regardless of its third byte. The two measured edges
  queue as themselves; an unmeasured third byte queues as `Unknown` rather than falling through to
  the reply match and killing the command outright, the failure this widening closed. `any_event`
  (used by `poll_event`) is unchanged and still wraps anything at all, `be`-shaped or not.
  `poll_event`/`pending_events` surface the queue; `pending_events` hands back an owned `Vec`, not
  a lazy `Drain`, since a short-circuiting read (`.any(...)`, a natural TUI idiom) over a `Drain`
  silently discards whatever it never yielded.

  Every one-shot command drains the queue after its own work, success or failure, and prints one
  stderr note per kind seen (never `Unknown`, which is `poll_event`'s own concern for 3.5), worded
  exactly and never duplicated on stdout. The notes print in the order of each kind's own latest
  arrival, not a fixed entered-then-left order: on wire order `be 01` then `be 00`, printing
  entered before left would leave the final line claiming the board is still adjusting when it is
  not. A mid-roundtrip mismatch was found and fixed on the way here: an edge shares `cmd 0x80`
  with an ordinary reply, so the routing has to be checked before the awaited reply's own match,
  not after, or the edge is read back as the reply it happens to resemble.

  An edge arriving between two commands was expected to reach the next command via the OS input
  buffer. **Measured 2026-09-06: it does not.** Both edges were emitted and concluded with no
  command running, and the next command (`wh dump`, live hardware) printed no note. The likely
  mechanism, an inference and labelled as one: each command opens the device fresh, and HID input
  reports are delivered per open handle, so a frame sent while no handle exists is never delivered.
  Within one open session the buffering is real, which is what the mid-roundtrip fixtures and the
  listener probe exercise. An edge with no read after it inside a session is never seen either, the
  limit `README.md` states. `ReplayTransport`'s positional scripts represent that by placing the `In` entry after the next
  `Out`, which is exactly what this task's fixtures do; splicing it any earlier hits the script's loud
  mismatch rather than proving anything.

  The hardware check ran on 2026-09-06, all three items measured on the real board:
  - Both edges arrived through `poll_event` over a real HID read (`AdjustModeEntered` at 4.98s,
    `AdjustModeLeft` at 10.87s of a 60s listen, a throwaway probe binary since deleted).
  - The timeout path held: the other ~54s of that listen were quiet `recv` timeouts handled as
    `Ok(None)` with no error and no spurious frames, so `hid.rs`'s zero-byte mapping behaves on
    hardware as the build assumes.
  - Between-commands buffering: measured no, see above.
  **[hardware]**

- [ ] **3.5 The TUI.** Replicates terminal.wallhack.com one to one: mouse-clickable,
  arrow-navigable, values populated by reading the board on open (a full read costs ~40ms, so no
  cache), re-reading on `be 01`, and showing a locked-board banner between `be 00` and `be 01`.
  Split into two plans so it is written once against a settled foundation:
  `docs/superpowers/plans/2026-09-06-tui-foundation.md` (read-only) and an editing plan written
  after that one lands.

  The foundation plan is done. `wh tui` opens the board, reads it once, and renders the vendor
  frame: Wallhack's own ASCII logo (used with Wallhack's permission, held by the operator, stated
  2026-09-06, extracted verbatim from the vendor bundle), the banner
  (`WALLHACK TERMINAL BY "@BRUX" - V<version>`),
  the device and profile lines, five clickable and arrow-cyclable tabs, the 68-key matrix with
  two-line caps and hit testing, dotted-leader setting rows, keyset rows and a prompt line. It
  stubs MAPPING, SWITCHES and ADVANCED's GENERAL/GAMEPAD/SHARE honestly and shows ADVANCED/DEVICE
  live. It routes the board's adjust-mode edges: a locked banner on `be 00`, a full re-read on
  `be 01`, a status note on an unrecognised event, and a stale-values note if a re-read times out.
  Nothing writes to the device anywhere in this plan.

  The replay-scripting blocker this entry used to describe is resolved. `poll_event` against a
  replay script whose cursor sat on an `Out` entry used to be a loud `DeviceError::Replay` rather
  than a quiet `Ok(None)`, so a poll-then-read event loop could not be scripted. A `wait` entry in
  the replay script format (`{"dir":"wait"}`, `{"dir":"wait","count":N}`) now serves that many
  timeouts before the script's cursor advances, and the TUI's own event-loop tests run against it.

  What remains before 3.5 closes: the editing plan, covering the steppers that actually write,
  the keyset gesture, the SOCD editor, profile switching, typed-yes confirmation modals, and the
  one `auto: tui` backup per session.

- [ ] **3.6 Key remapping, base layer and FN layer.** Layouts `0x00` and `0x01`, both measured
  (`remap-one-key`, `initial-load`, and the FN-layer table in `docs/protocol-inventory.md`). `wh`
  already reads them for key identity. Independent CLI work; can land before or after 3.7.

- [ ] **3.7 Lighting, no longer an investigation.** `cmd 0x18` is decomposed (brightness in
  twelfths, sleep in minutes, mode and speed named from the export schema), so the build is
  `wh led brightness|sleep` against a measured record. The one remaining investigation is whether
  the constant colour-table block is writable, which stays in `docs/backlog.md` and should be
  probed through the eventual `wh` write path rather than a hand-built frame. Key backlights are
  measured white-only with no UI control: a documented refusal under the beta definition. For the
  builder: a vendor-matching outbound read request carries the colour triples and the constant
  tail `01 03 04 03 0f ff ff` (measured, all 12 read requests), so a replay fixture derived from
  vendor traffic will not match a read built any other way.

- [ ] **3.8 Profile export and import, the `WHKB1.` envelope.** Measured 2026-09-05, written up in
  `docs/protocol.md` under "The profile export envelope". The format is cracked (prefix plus
  base64url of raw-deflate JSON, schema `wallhack-keyboard-profile` v1, encoder read from the
  vendor bundle), a `wh`-authored envelope was accepted by the vendor UI, and the UI's import is
  diff-and-write: it wrote exactly the one key that differed, through the standard template. So
  `wh profile export` and `wh profile import` are both buildable, and import should mimic the
  vendor's diff-and-write. The envelope carries fields `wh` does not yet model (gamepad, DKS), so
  a first import implementation must decide loudly what it applies and what it refuses, never
  silently dropping a field. This is the most literal 1:1 interoperability feature the project can
  build: snapshots exchangeable with the vendor UI in both directions.

### Phase 3 exit criteria: what "beta" means

Agreed with the operator on 2026-09-05. Beta is not literal 1:1 with terminal.wallhack.com; it is
**every control the configurator exposes being either supported or explicitly refused, with nothing
unknown.**

- The TUI and CLI cover everything measured: the performance core, keysets, profiles, MM, SOCD,
  remapping, and lighting brightness and sleep (colour is measured as not supported).
- Whatever remains (dynamic keystroke, mod tap, gamepad mode, polling rate) is a documented "not
  supported" with a measured reason, in the README, the way the snapshot docs already state what a
  snapshot does not capture. A stated limit is fine; an unstated one is a defect.
- `0xbe` handling works: a long-running interface that goes stale when the operator touches the
  knob is below the bar.

The beta phase then carries stability commitments, not only bugfixes:

- **Snapshot format stability.** Old backups keep restoring; any format change carries a serde
  alias or a migration, as the `custom_value_mm` rename already does.
- **CLI surface stability.** No breaking flag changes without a deprecation cycle. Alpha allowed
  removing `--force` overnight; beta gives that up.
- **Invariants frozen.** New commands are fine. Changing what an existing command writes to the
  board needs the same measured justification a protocol claim needs.

The beta announcement must say plainly: verified against one board on one firmware
(`WALLHACK K-001` / `App_V1.1.046000`), hardware results measured on profile 2. A beta label
implies it works on the next K-001 that is not the operator's; the first external bug report is
the thing beta exists to collect.

## Phase 2

Numbered 2.0 to 2.9 from `docs/superpowers/specs/2026-08-29-phase-2-design.md`, plus 2.10 to 2.16
added from what the hardware sessions and the reviews measured. The objective
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
  `TouchMode::RtGlobal` added, with `rt_enabled` and `rt_off_records` fixed, and the same nibble-2
  gap closed everywhere else a mode value's rapid trigger state gets named, including `wh-cli`'s
  `keyset.rs` `mode_fault`, `raw_mode_rt_on`'s eventual successor once `wh set ap` moved onto
  `keyset::plan`.
- [x] ~~**2.4 Write keyset membership.**~~ `docs/keysets.md` specified it completely from the
  fifteen capture scenarios available at the time: one write template shared by every operation,
  values before membership (three exceptions measured later),
  membership one record per frame and always last, non-owned layouts rewritten at each key's current
  value, the whole template written only when an owned value differs, max-plus-one allocation from
  live membership with no gap reuse, and a new keyset taking the global value rather than its
  members'. Creating a keyset over a key already in one steals it. CLI surface shipped: `wh keyset
  list|create|set|delete`, `wh set ap` on a key already in a keyset splits it into a new keyset
  automatically and tells the user it did so, and `wh restore` writes membership back too (its own
  gap: `KeysetIndex::restoring` reproduces a snapshot's index, including one allocation would never
  produce, since `next_index`'s max-plus-one rule cannot). `ops::ap_records` and `ops::set_ap`
  remain in the tree, exercised only by their own unit tests: `wh set ap` moved onto
  `keyset::plan`/`Change::ap` (2.14), so `run.rs` no longer calls either. Documented in `README.md`.
- [x] ~~**Hardware verification of the keyset write path.**~~ Run 2026-09-04 against the real board
  on firmware `App_V1.1.046000`, profile 2. `wh keyset create`, `set`, `delete` and `wh set ap`'s
  split all wrote correctly and verified their own readback, each taking an auto-backup first. The
  split moved exactly the selected keys into a fresh index and left the remainder in place, which is
  the behaviour the same session measured the vendor performing in `ks-span-two`.

  Operator observation of the interface, weaker than the frames above and recorded as such: with two
  vendor-made keysets and two made by `wh` live at once, the configurator listed all four in its own
  pane with values rendered normally. It does not distinguish our writes from its own.

  Also confirmed in the same session: `wh set ap --keys all` collapsed four keysets into one across
  all 68 keys and `wh restore --last` put all four back **with their original indices, 2, 7, 8 and
  9, gaps included**, which allocation could never reproduce and which is the only reason
  `KeysetIndex::restoring` exists. `wh profile` round trips. Timings retire a concern rather than
  raising one: whole-board set 0.85s, restore 0.70s, full dump 0.47s, so roughly 1300 HID
  roundtrips complete inside a second.

  Also confirmed: `wh keyset create rt` allocated index `1` while the actuation point counter stood
  at `10`, the separate-counters rule on hardware; and `wh set ap` on a member of that rapid trigger
  keyset moved the actuation point while leaving rapid trigger, both sensitivities and `0xFE`
  membership untouched, resending the key's own nibble 3 rather than promoting it.

  The nibble-0 write was exercised too, by a hand-edited snapshot rather than a profile switch: the
  board accepts touch nibble 0 from `wh restore` and reports it back. What that does to the key's
  actual behaviour is unmeasured and would need someone to press it.

  Still not exercised: a restore over a board left half-written by a genuine failure. See
  `README.md`.

- [x] ~~**2.20 `wh set ap` on free keys must create a keyset.**~~ Ruled by the operator on 2026-09-04,
  after the greying result settled that the configurator distinguishes a recognised setting from a
  loose override on keyset membership (`0xFF`) alone, not on the MODE nibble and not on the value.

  The rule the operator stated: a key sits outside a keyset exactly when it holds the board's base
  value, and any other value means it belongs to one. Grey means "follows the base"; highlighted
  means "has its own value". So `wh set ap --keys h --set 1.5` on a free key must allocate a keyset
  and put `h` in it, where it previously wrote the value and no membership.

  This is a ruling about what `wh` should do, not a measured firmware invariant, and the difference
  matters because the board does not enforce it. `docs/keysets.md` records keyset 4 holding `2.00mm`
  in `ks-steal-equal-value` while the base was also `2.00mm`, and on 2026-09-04 `wh` created keyset
  10 at `2.00mm` and the configurator listed and highlighted it. Anything implementing this rule has
  to cope with boards that already sit outside it.

  The mirror case is ruled the other way, deliberately: `wh set ap --keys w --set 2.0` on a keyset
  member keeps `w` in its keyset even though the value returns to the base, because the operator
  picked that key and changed its value explicitly. Leaving a keyset is a membership operation and
  gets its own command, not an inference from the value. `wh keyset delete <kind> <index>` already
  covers the whole-keyset case; per-key removal is 2.21.

  This also retires task 2.2's stated rationale. The nibble 0 to 1 promotion stays, because the
  vendor demonstrably does it, but "so our writes stop rendering greyed" was the wrong reason and is
  now measured false.

  Shipped: an all-free selection now allocates a keyset, and a selection that is exactly one
  keyset's members still keeps its index.

- [x] ~~**2.21 `wh keyset remove` to take individual keys out of a keyset. Depends on 2.20.**~~
  Ruled with 2.20: because setting a value never removes a key from its keyset, membership needs its
  own command. `wh keyset delete <kind> <index>` already deletes a whole keyset and returns every
  member to the base value; what was missing was `wh keyset remove <kind> --keys j`, which clears
  `0xFF` (or `0xFE`) for the named keys only, writes them back to the base value, and leaves the rest
  of the keyset alone.

  Both open questions were measured on 2026-09-04 and the answers made this straightforward. The
  vendor sends the ordinary five-step template for the removed key alone, ending in one `0xFF = 0`
  record, and writes nothing at all for the members that stay (`ks-remove-one-key`). The MODE record
  stays at touch nibble `1`, so the removed key must not be dropped to nibble `0`. Removing the last
  member is the same five frames with no teardown of any kind (`ks-remove-to-empty`), confirmed
  afterwards by `wh keyset list ap` reading `0xFF` live with the keyset gone, so there is no
  empty-keyset case to handle. Built as `keyset::plan` with the base value and a membership clear.
  See `docs/keysets.md`.

  Shipped: both kinds. The rapid trigger side was unmeasured when this task was first written; it is
  now measured in `ks-remove-one-rt`, and confirms the removed key's own MODE goes to touch nibble
  `1` (rapid trigger off, not nibble `2` following the global) and that its actuation point is
  preserved rather than reset to the base, since a rapid trigger removal never touches `0x04`.

- [x] ~~**2.22 `wh keyset remove` resets a key to the board's base, and loses its value flags.**~~
  Ruled by the operator on 2026-09-04, after clearing a stray key needed two commands: `wh set ap`
  put it in a keyset purely so `wh keyset remove` could take it back out.

  The command's job is a destination, not a transition: make these keys follow the board's base and
  belong to no keyset. So it stops refusing when a named key is already outside every keyset, and it
  loses `--value`, `--press` and `--release` entirely. A key already at the base with no membership
  gets no value record, `plan`'s own skip rule. It still carries the membership-clear record, since
  `plan` writes that unconditionally for every key it is given, whether or not the value it already
  holds matches: this predates 2.22 (`create`/`delete` already relied on it) and is not something
  this task's own skip rule could suppress without a second read of the same key.

  **Where the base comes from, in order.** Read it from the keys outside every keyset that are *not*
  in the selection. That is what makes the motivating case work with no flag: 57 free keys agreed on
  `2000` and only the key being reset disagreed. This is what the vendor does, measured in
  `ks-reset-keysets`: it wrote `0x04 = 2000` while 64 other free keys held `2000`, and `0x14`/`0x15
  = 200` while 68 keys held `200`. It read no stored base, because there is none.

  If those remaining free keys disagree, refuse and name them, saying to include them in the
  selection so they are reset too. Do not fall back to the constant there: a contradictory signal
  from the board is not the same as no signal, and overriding it would invent a value.

  If there are no free keys outside the selection at all, **for `ap`** use **`2000` (2.00mm)**.
  Operator's ruling, reaffirmed on 2026-09-04 after a review argued against it, so the behaviour
  below is decided rather than merely shipped.

  **The case is not only `--keys all`, and the operator ruled with that in front of them.** A review
  measured a four-key board where `s` and `d` sat in a keyset and `w` and `a` were the only free
  keys, both reading `1500`. Selecting `w,a` excludes both, leaves nothing to read, and writes
  `2000` over a board whose free keys had stated `1500`. There is no confirmation, since two of four
  keys is not a whole-board selection. Scaled up, that is sixty keys in keysets and eight free
  strays normalised to `2000` with no prompt.

  Three alternatives were offered and declined: refusing as `rt` does; reading the selected keys'
  own agreed value and treating the command as a no-op; and keeping the constant while moving the
  confirmation trigger from "whole board" to "no signal". The announcement does name the value as a
  default whenever it is used, measured on hardware, so the operator is told. It is also the measured
  dominant value: across every layout `0x04` read in the corpus it accounts for 3453 of them,
  against sixteen other distinct values and no reading of `2500` ever.

  When this was ruled it was a **chosen default**, not a measurement of the board's factory
  setting: nothing had read an untouched profile. That read happened on 2026-09-05. Profile 3,
  whose actuation points the operator states were never changed, read `0x04 = 2000` on all 68 keys,
  136 records across two sweeps (`safety-zone-on`), so the constant now matches the one value
  ever read from an AP-untouched profile. Scope stays honest: one board, one firmware, one
  profile, and the "untouched"
  half rests on the operator's word (profile 3 did carry a rapid trigger keyset, so it is
  AP-untouched, not pristine).

  **`rt` has no such default and refuses in the same case, ruled during review rather than at this
  entry's first writing.** No `0x14`/`0x15` reading has ever been `2000`, and the corpus shows the
  reset target tracking the global sensitivity at write time (`100` in `ks-delete-rt`, `200` in
  `ks-reset-keysets`), never a fixed number, so there is no dominant value the way `2000` is for
  `ap`. A practical consequence, broader than only the whole-board case: `wh keyset remove rt`
  refuses whenever every key currently free of an rt keyset is also in the selection, since
  `global_rt_excluding` then reports `NoneOutsideAKeyset`. A single `--keys w` refuses the same way
  when `w` is the only free rt key on the board, not only `--keys all`. `wh keyset delete rt
  <index>`, which still takes `--press`/`--release`, is the route past this refusal.

  **Announcement needs four reachable cases**, not three, since "free key(s) d left alone,
  already in no ap keyset" becomes false: removed from keyset N with its old and new value;
  returned to the base from a stray value while already outside every keyset; membership rewritten
  with the value unchanged, for a free key already at the base (not "nothing to do": `plan` still
  sends the membership record unconditionally, so calling it nothing at all would be the same false
  no-op this whole entry exists to reject); and a fourth found only during review, a free key whose
  owned value already sits at the base but whose touch mode still moves (rapid trigger switching
  off, or a key promoted off "follow global travel"), which must name the mode transition rather
  than read as either of the other two. The first two also append the mode transition when it
  applies alongside them, the same fix.

  **One existing test inverts.** `keyset_remove_ignores_a_free_key_selected_alongside_a_member`
  asserted free keys are dropped from the plan. They are now included, so it became a test that they
  are written, rewritten and renamed to `keyset_remove_writes_a_free_key_selected_alongside_a_member`
  rather than deleted: it is the only test covering that path.

  **`--keys all` becomes a full board reset for `ap`**, every key to `2000` and every ap keyset
  destroyed, which is `RESET KEYSETS` in the configurator. It half-does this today for keyset
  members, so this widens an existing hazard rather than creating one. **`rt` cannot do this at
  all**: the ruling above means a whole-board `rt` selection always hits `NoneOutsideAKeyset` and
  always refuses, so there is no `--keys all` reset on that side.

  **It carries the same confirmation as `wh set ap --keys all`.** Ruled by the operator on
  2026-09-04: the two commands reach the same destruction by different routes, so guarding one and
  not the other would be arbitrary. Print the warning naming every keyset that will cease to exist,
  read one line from stdin, trim and lowercase it, and proceed only if it equals `yes`. EOF is a
  rejection. `--dry-run` does not prompt. There is no bypass flag, so tests pipe `yes` on stdin.
  The trigger is the resolved selection covering the whole matrix, not the literal `--keys all`.

  Share one implementation with 2.23 rather than writing it twice.

  Shipped: both kinds, the base read excluding the reset selection, and the `Split` refusal. The
  `NoneOutsideAKeyset` case ships differently per kind and stays that way on purpose: `ap` falls
  back to `2000`, `rt` refuses whenever every key currently free of an rt keyset is also in the
  selection, whole-board or not. Also shipped, all found during review rather than in the first
  draft of this entry: the `ap` fallback names itself as invented in the announcement, rather than
  rendering indistinguishably from a value read off the board; a partial removal that empties a
  keyset says so, "keyset N ceases to exist", the same fact the whole-board prompt already names for
  every keyset at once; that prompt is built after `keyset::plan`, not before, so it can count and
  name how many keys are about to move off touch nibble 0 rather than answering a question the
  operator cannot yet see the answer to; and the four-case announcement itself. `--value`, `--press`
  and `--release` are gone from the clap variant and from the `Kind::Ap` refusal, which had nothing
  left to refuse and was deleted with them.

- [x] ~~**2.23 `wh set ap --base <mm>` to set the board's base actuation point. Depends on 2.22.**~~
  Done 2026-09-04. The base is not a stored setting: it is what every key outside a keyset holds in
  layout `0x04`, which is also why 2.10 exists. Setting it writes the value to every free key and
  touches no membership.

  `--base` takes no `--keys` and refuses alongside `--set`: it names the board, not a selection.
  The flag is `--base` and not `--mm` by the operator's ruling, since `--mm` is reserved for 2.10's
  `"MM" CUSTOM VALUE`, which is a different setting the docs already record as easy to confuse with
  this one.

  **`wh set ap --keys all --set X` stays, and is not the same thing.** Since 2.20 it enrols all 68
  keys into one new keyset, which is measured vendor behaviour (`ks-value-over-all` writes
  `0xFF = 3` to all 68) and the configurator supports it, so `wh` should not diverge by removing it.
  But it is rarely what someone means, and it is destructive in a way that is not obvious: every key
  moves into the new index, so **every existing keyset loses all its members and ceases to exist**.

  So it must say so before it writes, naming what is lost, in the style `announce_steal` already
  uses. Something of this shape:

  ```
  ap: --keys all moves every key into one new keyset, keyset 11
      keysets 2, 7, 8, 9 will cease to exist, their members absorbed
      to change the board's base instead, leaving keysets alone: wh set ap --base 1.50
  ```

  **Prompt, and accept only the exact word.** Ruled by the operator on 2026-09-04, overriding an
  earlier recommendation in this entry to announce and proceed. After printing the warning, read one
  line from stdin and act only if it is exactly `yes`. `y`, `ye`, `yess` and everything else are
  rejected and nothing is written. EOF counts as a rejection, so a closed or empty stdin is safe.

  The objection that `wh` cannot prompt was wrong and is recorded here so it is not raised again:
  `bin/wh` ends in `exec`, so `wh.exe` inherits stdin straight from the WSL shell and a prompt
  reaches the operator normally.

  Two consequences to build for. `--dry-run` must **not** prompt, since it writes nothing. And there
  is deliberately no `--yes` flag: a bypass would defeat the ruling, so the tests cover the confirmed
  path by piping `yes` on stdin rather than by skipping the prompt.

  **The match is case-insensitive.** Trim the line, lowercase it, and require it to equal `yes`.
  So `YES`, `Yes` and `yEs` all pass, while `y`, `ye` and `yess` still do not. Operator's ruling.

  **Trigger on the resolved selection, not on the literal flag.** The prompt fires when the
  selection covers every key in the board's matrix, however it was written, so spelling out all 68
  usages reaches it too. `--keys all` is the usual spelling, not the condition.

  **The same prompt guards `wh keyset remove --keys all`, shipped in 2.22 as
  `crates/wh-cli/src/confirm.rs`.** Reuse it rather than building a second copy; two copies will
  drift, and this is the one piece of code whose whole job is to be hard to get past by accident.

  Measured 2026-09-04 in `captures/ks-set-global-ap.jsonl`: changing the configurator's GLOBAL
  ACTUATION POINT field sent 75 write frames carrying 413 records to 59 keys, `0x04` the new base
  and `0x14`/`0x15`/`0x16`/`0x17` echoed unchanged, no `0xFF` record anywhere. Separately measured:
  nine of the 68 keys were read and never written; that those nine are keyset members is an
  inference, not itself measured, since the capture has no `0xFF` request and no matrix read. See
  `docs/keysets.md`, "Setting the base actuation point", for the full breakdown, the nine usages,
  and `wh`'s own two documented divergences from that template.

- [x] ~~**2.24 Share what is still identical between `keyset::delete` and `keyset::remove`, and stop
  before the part that is not.**~~ Deferred during 2.21 because extracting it would have refactored
  `delete`, which is shipped and hardware-verified, inside a task that did not ask for it. 2.22
  changed the two branches enough that this is no longer the same task it was when first written:
  read what is actually shared before touching either.

  What is still shared: the `Kind::Ap` arm builds `Change::ap`, the `Kind::Rt` arm builds
  `Change::rt_off`, and this shape is the reason to share it at all. The vendor sends the same
  template for a single-key removal as for a whole-keyset delete (`ks-delete-rt` and
  `ks-remove-one-rt`), so a future correction to that template must land on both branches at once,
  and nothing today would catch a fix applied to only one.

  **What is no longer shared, and must not be merged into one function.** `delete` resolves its
  value through `global_ap_or_bail`/`global_rt_or_bail`, which refuse on both `Split` and
  `NoneOutsideAKeyset` and take `--value`/`--press`/`--release` as an escape hatch. `remove` has no
  such flags and resolves through its own `remove_base_ap`/`remove_base_rt`, which refuse on
  `Split` but diverge on `NoneOutsideAKeyset`: `remove_base_ap` falls back to the `2000` constant
  (2.22's own ruling, `NO_SIGNAL_BASE`), while `remove_base_rt` refuses, since there is no measured
  rapid trigger equivalent of that constant. Extracting a single shared "resolve the kind branch"
  helper risks the hazard running either way: `delete` could silently inherit `remove`'s constant
  fallback, writing a value the operator never passed `--value` for, or `remove` could silently
  inherit `delete`'s refuse-and-ask-for-a-flag behaviour, which `remove` has no flag to satisfy
  since `--value`/`--press`/`--release` do not exist there. A future implementer following this
  task literally, the way an earlier version of it read, would give one command the other's
  `NoneOutsideAKeyset` behaviour without meaning to, either direction. Share only the `Change`
  construction shape, parameterised over an already-resolved value each caller keeps producing its
  own way.

  **Done 2026-09-05.** `crates/wh-cli/src/keyset.rs` gained `reset_change`, a function
  taking an already-resolved `Target` and returning `Change::ap` or `Change::rt_off`. It takes no
  `Session` and no `Kind` of its own: the `Target` variant carries the kind, so a caller cannot
  hand it a value and a kind that disagree, and it can resolve nothing. `delete` still resolves
  through `global_ap_or_bail`/`global_rt_or_bail` and `remove` still through
  `remove_base_ap`/`remove_base_rt`, each building its own `Target` first. `create` is not a
  caller: its rapid trigger arm needs `Change::rt_on`.

  One test was added, `keyset_delete_ap_refuses_where_remove_would_invent_a_base`, the only test in
  the repo pinning `delete`'s `NoneOutsideAKeyset` refusal. On a board where every key sits in a
  keyset, `delete ap` must refuse and name `--value` rather than reach for `NO_SIGNAL_BASE`.
  Proved by wiring `delete` through `remove_base_ap`, which made a `--dry-run` delete announce
  "returning members to 2.00mm" and emit three write frames for a value nobody passed.

  `remove`'s two halves were already pinned, by
  `keyset_remove_ap_names_the_base_as_invented_when_every_key_is_already_in_a_keyset` and
  `keyset_remove_rt_refuses_when_no_free_key_is_left_to_read_a_sensitivity_from`; a first draft
  added a duplicate of each and they were dropped, since wiring `remove` through
  `global_ap_or_bail`/`global_rt_or_bail` is caught by twenty pre-existing tests. Both now carry a
  pointer saying they are also `remove`'s half of the divergence, so the three read as a set
  without a test that guards nothing.

- [x] ~~**2.27 `wh keyset create --keys all` is a third unguarded route to destroying every
  keyset.**~~ Found by a reviewer probing the guard added for `wh set ap --keys all`, and measured:
  `wh keyset create ap --keys all --value 1.50` on a board holding keysets 1 and 2 ran straight
  through the membership sweep into `plan`'s reads with stdin closed and no prompt on either stream.
  Every key moves into the new index, so every existing keyset loses all its members and ceases to
  exist, exactly as with the other two routes.

  Two commands are now guarded and this one is not, which is worse than none being guarded: an
  operator who has learned that `wh` asks before a whole-board write will not expect the third route
  to be silent.

  Reuse `crate::confirm::confirm` and the pattern the other two settled on: prompt on stderr with
  the announcement on stdout, trigger on the resolved selection covering the matrix rather than the
  literal `all`, no prompt on `--dry-run`, no bypass flag, and a test asserting the prompt does NOT
  reach stdout, which is the half that has twice been the one missing.

  `crates/wh-cli/src/confirm.rs`'s module doc has already been narrowed to say the routes it lists
  are "only the ones guarded so far", so it no longer implies the list is complete. This task need
  only add the third route to it.

  **Done 2026-09-05.** `keyset::create` now takes the same two extra parameters `remove` already
  carried, a `prompt_out` writer and a `will_write` flag, plus the input reader, and calls
  `confirm_whole_board_create` after `plan` exists and before `announce_steal`, so the prompt is
  built from the plan and a refusal announces nothing at all. `run.rs`'s `KeysetWhat::Create` arm
  passes a locked stderr, a locked stdin, and `!dry_run`, which is what keeps the guard off a
  preview without moving the call past that arm's own `dry_run` check. `confirm_whole_board_create`
  decides the whole-board trigger itself from the membership and the selection, matching
  `confirm_whole_board_ap_set`, so a caller that forgot to check cannot reach a prompt whose every
  sentence would be false of a partial selection.

  Both kinds prompt, and the `rt` mode clause (`rt_on_mode_clause`) splits its count by the nibble
  each key is leaving, since one sentence cannot honestly cover them all: a key leaving nibble 0
  or 1 is having rapid trigger switched on and is told so, a key leaving nibble 2 had it on
  already and is only moving onto its own sensitivity, and an unmeasured nibble is counted with
  the second group, whose wording claims only the destination. Reusing `ap_mode_clause`'s or
  `remove`'s single sentence would have been false on one board or the other.

  Eight end-to-end tests in `crates/wh-cli/tests/dump.rs`, covering `ap` decline and accept, the
  two `rt` origin splits, the prompt's absence from stdout, a partial selection, `--dry-run`, a
  whole-board selection spelled key by key, and a board with no keysets where the mode count is
  the only warning; plus one unit test pinning that a partial selection reaching
  `confirm_whole_board_create` directly prints nothing at all.

- [x] ~~**2.26 Two regression-guard gaps in `wh keyset remove`'s announcement, each one fixture.**~~
  Closed 2026-09-05, test-only. Found by a cold reviewer that built its own replay generator and
  drove the binary, after the committed behaviour had already been measured correct in both cases.
  **The shipped code was right; what was missing was a test that would notice if it stopped being.**
  Each was one fixture, `crates/wh-cli/tests/keyset.rs`.

  **The mode count can be over-claimed on a board the three current fixtures cannot distinguish.**
  The whole-board prompt counts keys whose touch nibble moves. Two wrong predicates survive the
  suite green, counting keys with any value record, and counting keys with a MODE record. Both agree
  with the correct answer on the shipped fixtures (4 of 4, 2 of 4, 0 of 4) and diverge only on a
  whole board where every key is already at nibble 1 and holds a stray value: every key gets value
  records, no nibble moves, and the mutant prints "4 key(s) move off global travel" when none do.
  Fixture added: `keyset_remove_whole_board_prompt_omits_the_mode_clause_when_only_the_value_moves`,
  that board, asserting the clause is absent. Both named wrong predicates were mutated in and each
  made only this fixture fail, then reverted.

  The under-reporting direction, which is the dangerous one, is already pinned: counting only keys
  whose owned value also moves, and counting only `Rt` transitions while missing the nibble-0
  promotion, are each killed by two tests. The prompt cannot silently omit a mode change.

  **`keyset_disappears` can be under-claimed.** Rewriting it as `leaving.len() == ks.members.len()`
  survives the suite green. Measured on two keysets, 1 holding `w,a` and 2 holding `s,d`, removing
  `w,a,s`: keyset 1 is emptied and the mutant omits "keyset 1 ceases to exist". Consequence is mild,
  since the operator still sees a `removing` line for every member, so the destruction stays
  inferable. Fixture added:
  `keyset_remove_ap_names_a_keyset_that_ceases_to_exist_from_a_partial_removal_of_two_keysets`, two
  keysets, remove all of one plus part of the other. The named mutant was mutated in and made only
  this fixture fail, then reverted.

  Test-only close: the shipped predicates were not touched. Seven fix rounds ran on the branch that
  found these gaps and three of the last four introduced a defect of the class they were fixing, so
  guarding the measured-correct behaviour rather than touching it again was the deliberate choice.

- [x] ~~**2.25 Move the whole-board confirmation prompt from stdout to stderr. Depends on 2.22,
  should land before or with 2.23.**~~ Measured: `wh keyset remove ap --keys all > log.txt` puts both
  prompt lines (the warning and "type yes to continue: ") in the redirected file and then blocks on
  stdin with nothing at all on the operator's screen, since `confirm` writes to whatever `Write` its
  caller hands it, and `keyset::remove`'s caller in `run.rs` hands it real stdout.

  Writing to stderr instead closes this for every redirection combination (`> log.txt`,
  `2>&1 > log.txt`, either stream piped alone), needs no `is_terminal()` check and so no
  platform-dependent behaviour on the Windows target, matches what this binary already does with
  its own `transport: replay|hardware` line, and leaves the piped-stdin confirmation mechanism
  (2.22's own "no bypass flag, so tests pipe `yes` on stdin") completely untouched: stdin is not
  stdout, so nothing about how the prompt is answered changes.

  **The cost.** One more writer threaded through `keyset::remove` from `run.rs`, alongside the
  `out` it already takes for the announcement, since the prompt and the announcement are meant for
  different streams now. Every end-to-end assertion that currently checks this prompt's text on
  stdout moves to the equivalent check on stderr instead: the two in
  `keyset_remove_over_the_whole_board_requires_a_typed_yes`, and each one in the mode-transition
  and invented-base tests added alongside this entry
  (`keyset_remove_whole_board_prompt_names_a_mode_transition_a_no_op_value_would_hide` and its two
  siblings covering the mixed and all-nibble-1 boards) that checks the prompt rather than the
  per-key announcement that follows it; count them again at the time this lands rather than
  trusting a number written here, since the count has already grown twice since this entry was
  first drafted. No other behaviour changes: the per-key announcement itself
  (`removing`/`returning`/ "membership rewritten, value unchanged") still goes to stdout, since
  that is what `--dry-run` prints and what `wh keyset remove ap --keys all > log.txt` is presumably
  being redirected to capture in the first place.

  **Land this before or with 2.23**, so `wh set ap --keys all`'s own confirmation, whenever it is
  built, calls the corrected version from the start rather than repeating the stdout choice and
  needing this fix a second time.

  **Done 2026-09-04.** The hazard was measured, not supposed: `keyset::remove` now takes a second
  writer, so `run.rs` hands it a locked stderr for the prompt and keeps the locked stdout it already
  passed for the per-key announcement. Every end-to-end assertion on the prompt's text moved from
  stdout to stderr, and a new test,
  `keyset_remove_prompt_goes_to_stderr_not_stdout`, pins the negative half directly: the prompt is
  in stderr *and* absent from stdout, so a future change routing it to both streams fails there even
  though every other assertion on the prompt's wording would still pass.

- [x] ~~**2.13 `wh set rt --off` must clear rapid trigger keyset membership. Depends on 2.4.**~~
  Measured in `captures/rt-off-w.jsonl`, frame 70: the vendor's per-key rapid trigger off writes
  `0xFE = 0` after the value records, one record per frame, as the last thing it sends. `wh` writes
  the MODE record and stops, so a key turned off through `wh` keeps whatever membership it held and
  the configurator still lists it in a keyset. That file's read sweep does not cover `0xFE`, so
  whether `W` was in a keyset beforehand is unmeasured; what is measured is that the write is sent
  unconditionally. Implement by routing `ops::set_rt_off` through `keyset::plan` with
  `Change::rt_off(press, release)` and `Some(KeysetIndex::clear(Kind::Rt))` rather than by hand.
  The sensitivities come from `keyset::global_rt`, which reports whether the keys outside a keyset
  agree rather than trusting one of them, and refuses a `Membership` of the wrong kind.

  Related, from the same review: on a board with the global rapid trigger switch on, every key
  outside a keyset sits at nibble `2`, so `wh set rt --keys all --off` now writes all 68 keys where
  it previously wrote none. That is vendor-consistent, but it means this gap is reached far more
  often than it was.

  **Two things to settle before implementing, both found by review, both now closed.** First,
  `global_rt` returns three variants and this task said only "the sensitivities come from
  `global_rt`". What a `Split` or a `NoneOutsideAKeyset` should do was undecided, and the obvious
  "unwrap or default" lands on `Um(0)`, which would write `0x14 = 0, 0x15 = 0`, a value the vendor
  has never been observed writing. Settled by 2.15: both refuse and both name `--press`/`--release`.
  Second: `keyset::plan` used to send a MODE record at an unchanged nibble-0 value, which
  `ops::rt_off_records` refuses to do. `plan` no longer emits one at all, measured against 1150
  MODE write records in the corpus of which none is at nibble 0, so routing this task through
  `plan` no longer introduces that write.

  Measured in the same review, and settling an earlier doubt: the vendor **does** reset the
  sensitivities on a per-key rapid trigger off. `rt-off-w.jsonl` shows W going from 500/500 to the
  global 100/100. `ops::rt_off_records` writes MODE alone and leaves the private value in place, so
  it is the one that diverges; routing through `Change::rt_off` removes a divergence rather than
  creating one.

  **Done 2026-09-05.** `wh set rt --off` routes through `crate::keyset::rt_off`, which builds
  `Change::rt_off` over `keyset::plan` with `Some(KeysetIndex::clear(Kind::Rt))`, and reuses
  `announce_remove` and `verify_write_as`: this command now reaches the same destination
  `wh keyset remove rt` does, so a key leaving a keyset, a keyset emptied by that, and a key that
  only has its membership rewritten are said the same way in both. `ops::rt_off_records` and
  `ops::set_rt_off` are left in place and are no longer on the path, exactly as 2.14 left
  `ops::ap_records`; `run.rs`'s `verify_rt_off`, which checked MODE alone and so could not see the
  sensitivities or the membership this write now sends, was deleted rather than left checking too
  little. `--press`/`--release` lost `conflicts_with = "off"`, per this entry's own ruling: a
  refusal naming a flag the operator cannot pass is the defect, not the fix. They must be passed
  together or not at all, since `--off` resets both sensitivities and a half-given override would
  have to read the other half from the very base reading whose disagreement was the reason to reach
  for them.

  **Two corrections from fix round 1, both measured, both changing what this entry prescribed.**
  This entry said the sensitivities come from `keyset::global_rt`. They come from
  `global_rt_excluding` over the selection instead, which is what 2.22 already settled for
  `remove`: reading without excluding makes `wh set rt --keys w --off`, on the ordinary board where
  `w` is the one key with its own sensitivity, refuse as a disagreement with itself. That is the
  commonest way the command is run and it worked before this task. `NoneOutsideAKeyset` therefore
  carries two board states here as it does in `remove_base_rt`, told apart from `m.entries()`; both
  refuse and both name `--press`/`--release`.

  And `wh set rt --keys all --off` became a fourth route to whole-board destruction the moment it
  started writing membership: measured on a board with two keys in rt keyset 1, it exited 0 with
  stdin closed, destroyed the keyset and cleared every key's `0xFE`, asking nothing. It now calls
  the same `confirm_whole_board_remove` the other three call, so `crates/wh-cli/src/confirm.rs`
  names four routes. A whole-board selection excludes every free key from the base read, so that
  guard is reachable only with `--press`/`--release`; without them the run refuses earlier.
- [x] ~~**2.14 Decide what `wh set ap` emits, before the CLI is written.**~~ Settled by
  measurement, 2026-09-03: **one shape, always.** `wh set ap` routes through `keyset::plan` with
  `Change::ap`, whether the key is in an actuation point keyset or not, and `ops::ap_records`
  becomes the divergent path rather than a second supported one.

  The same intent was expressible three ways with different frames. For a key at MODE `0x10` and AP
  1000 with a target of 2000, `ops::ap_records` emits `[AP]` alone, `keyset::plan` with
  `Change::ap` emits `[MODE, AP, RT_PRESS, RT_RELEASE]`, and `Change::ap_keeping_touch` on a key
  still following global travel emits `[AP, RT_PRESS, RT_RELEASE]`.

  Two measurements close it. `ks-value-ap` shows the vendor promoting a member from `Global` to
  `Single` during a keyset value change: `X` read MODE `0x0000` and was written `0x0010`, alongside
  `W` and `S` which read and were written `0x18`. That kills the third shape. `ap-wasd-1.2` shows
  the vendor emitting the identical five-step template on an actuation point change with no keyset
  traffic in the file at all, three times over, at `850`, `1200` and `300`. That kills the split
  between a keyset path and a non-keyset one: the frames do not vary with membership, so the CLI
  does not have to know before choosing what to send. Write-up in `docs/keysets.md`.

  **One thing inside this stays unmeasured and is not made measured by ticking the task.** The MODE
  promotion from `Global` to `Single` on an actuation point change is real and reproduces exactly,
  but whether it is specific to keyset members is not measured. `ks-value-ap` never reads `0xFF`,
  and the only in-era membership read has `X` free while showing `W` and `S` at a value they had
  moved off by the time of that capture, so both readings fit the frames. An earlier version of this
  entry said all three promotions in 3696 frames are keyset operations; that was resolving an
  ambiguity in the document's own favour, and the verification pass caught it. `Change::ap` promotes
  unconditionally either way, which is what `ops::ap_records` already ships under 2.2, and 2.2's
  hardware verification is still the thing that confirms it.

- [x] ~~**2.15 Decide what `global_ap` returning `Split` or `NoneOutsideAKeyset` should do.**~~
  Decided 2026-09-03, by the operator, and the same ruling covers `global_rt` and closes the last
  open question in 2.13: **both variants are an error, and both name `--value` as the way past it.**
  Neither picks a winner.

  - `Split`: refuse, and name the disagreement in the message, the distinct values with how many
    keys hold each, descending, which is the order `Global::Split` already carries. A majority vote
    would write a value the operator never typed over every member's actuation point.
  - `NoneOutsideAKeyset`: refuse, and say why, that no key sits outside a keyset so the board holds
    no global to read. Rejected alternative: the whole board's majority, which returns some
    keyset's value wearing the global's name. (The vendor's own read, in `ks-value-ap`, covers the
    whole board too, five 14-record frames of all 68 keys, not five keys singled out as an earlier
    draft of this bullet said; its disagreement behaviour is unmeasured, and one read key was
    itself in a keyset.)

  This gives `wh keyset create ap`, `wh keyset delete ap` and `wh set rt --off` a `--value` (or
  `--press`/`--release`) escape hatch that is optional on an agreeing board and required on a
  disagreeing one. Implemented in 2.4b and 2.13, not here.

- [x] ~~**2.30 `auto_backup`'s reason is a forgeable string that gets persisted.**~~ Nine call
  sites in `crates/wh-cli/src/run.rs` passed a literal like `"keyset create"`, which became
  `snap.origin = "auto: keyset create"`, was written into the backup file, and is shown by
  `wh backups list`. Nothing tied a real run to the label it wrote: the only tests touching the
  origin were `wh-config`'s round trip and one `dump.rs` test over a hand-built snapshot.

  Closed 2026-09-05. `BackupReason` in `run.rs`, eight variants: one per command family that takes
  an auto-backup, plus `Manual` for `wh backup`. `origin()` renders the exact strings the literals
  produced, `"auto: set ap"` through `"auto: restore"` and a bare `"manual"`, because those words
  are already in the operator's backup files on disk and are what `wh backups list` and `--last`
  print. This is a type change, not a wording change, and
  `every_backup_reason_renders_its_persisted_origin_string` pins all eight verbatim so a rename
  has to be deliberate. A ninth, `SetMm`, joined in 3.1, pinned the same way, and a tenth and
  eleventh, `SocdPair` and `SocdUnpair`, in 3.3.

  The missing end-to-end tie is now two tests that drive a real command and read the file it
  wrote: `set_ap_end_to_end_records_its_own_command_as_the_backup_origin` in `tests/dump.rs` and
  `keyset_create_ap_end_to_end_records_its_own_command_as_the_backup_origin` in `tests/keyset.rs`,
  one per command family, since one family's label reaching the file says nothing about another's.
  The task's claim is measured: swapping the `set ap` and `keyset create` literals before the
  change failed exactly those two new tests and nothing else in the workspace.

  Six of the eleven variants (`set rt`, `keyset set`, `keyset delete`, `keyset remove`, `restore`
  and the manual backup) still have no end-to-end test tying a run to its label, and `set ap
  --base`'s own call site is untied as well even though its variant is pinned through the plain
  `--set` path. A wrong variant at one of those sites would still persist quietly: the enum makes
  the label visible at the call site, it does not make the wrong one impossible. `SetMm` is not
  among the six: unlike every variant recorded here, it was born with its own end-to-end tie,
  `set_mm_end_to_end_records_its_own_command_as_the_backup_origin` in `tests/dump.rs`, from 3.1.
  `SocdPair` and `SocdUnpair` are not among them either, born tied the same way by
  `socd_pair_end_to_end_records_its_own_command_as_the_backup_origin` and its `unpair` twin in
  `tests/socd.rs`.

- [x] ~~**2.31 `confirm_whole_board_create` still takes a `kind` beside its `Target`.**~~ The last
  kind-beside-target in `crates/wh-cli/src/keyset.rs`, left deliberately when 2.18 closed: its
  kind genuinely selects between `ap_mode_clause` and `rt_on_mode_clause`, so it was pinned rather
  than safe by construction, and the argument for leaving it lived only in a gitignored report.

  Closed 2026-09-05 by applying the `Target::kind()` cure rather than writing the argument down.
  Selecting a clause needs a kind, not a parameter carrying one, and the `Target` already has one:
  the parameter is gone and the kind picking the clause builder, the keyset wording and the
  refusal subject is read off the value being confirmed.
  `#[allow(clippy::too_many_arguments)]` went with it, seven arguments being at the lint's
  threshold, and clippy passes without it.

  The seven tests the entry named are measured, and they are seven only as a union of both
  directions: forcing `Kind::Rt` inside the function fails four (the `ap` whole-board create
  tests), forcing `Kind::Ap` fails the other three (the `rt` ones), and nothing else in the
  workspace moves either way. All seven pass untouched after the cure, and the mismatch they
  guarded against, a wrong `kind` beside a right `Target`, is no longer representable, so there is
  nothing left at that call site to mutate.

- [ ] **2.32 `keyset::mode_change` still prints a `TouchMode` through `{:?}`.** Opened 2026-09-06
  by 3.3's review, which measured the case as live rather than theoretical: `wh keyset create rt`
  over a key sitting at an unmeasured touch nibble reaches `describe_member`, and the operator
  reads `mode Unknown(5) to Rt`, Rust tuple-variant syntax in a sentence meant for a person.

  The renderer to adopt already exists and is shipped: `crate::run::touch_mode_label`, written for
  3.3, which gives the five measured nibbles their measured meanings and anything else "an
  unmeasured mode (5)". So this is not a design question, only a change nobody has made yet.

  Why 3.3 did not do it: `mode_change` feeds `describe_member`, `moved_mode_count`,
  `ap_mode_clause` and `rt_on_mode_clause`, so changing its wording moves announcement text pinned
  across `crates/wh-cli/tests/keyset.rs`, the largest suite in the repo. That is a keyset-tree
  change with a keyset-tree blast radius, and folding it into a SOCD diff would have hidden it
  inside a feature it has nothing to do with. Until it is picked up, `mode_change`'s own comment
  says plainly that this site prints Debug names and that the gap is owned here.

  Second one-liner while someone is in the area, pre-existing and unrelated to 3.3:
  `wh keys group solo "q"` prints "1 keys". `run::key_or_keys` is the fix, already shared by three
  other call sites.

- [ ] **2.29 Two stale corpus counts in `ops::ap_records`'s doc.**
  `crates/wh-device/src/ops.rs:264-267` says "across all 27 keyset-era captures" and "469 measured
  echoes". The corpus has grown twice since (39 files when this was opened, 55 as of 2026-09-05).
  Left deliberately when the rest of 2.16 was corrected on
  2026-09-05: re-deriving them means simulating `ap_records`'s own per-key output against every
  capture to decide which MODE writes it would and would not have sent, which is a materially
  riskier measurement than counting records, and a wrong number here would be worse than a stale
  one. 2.16's header was narrowed instead so it no longer claims these were corrected.

  Both numbers support the same load-bearing claim, that the vendor rewrites MODE where
  `ap_records` sends nothing, and that claim was not in doubt. What is stale is the evidence
  offered for it.

  When it is picked up: write the simulation as a test rather than a script, so the number is
  re-derived by the suite instead of pasted into a comment that rots again.

- [x] ~~**2.28 Four whole-board refusal assertions match a string three commands emit.**~~ Four
  `contains("was not confirmed")` assertions on a declined whole-board run, where
  `wh keyset remove`, `wh keyset create` and `wh set rt --off` all end a refusal with that phrase,
  so none of them could tell its own command's refusal from another's.

  **Measured 2026-09-05, and this was not theoretical.** The same shape in
  `crates/wh-cli/tests/keyset.rs` let a mutation pass the whole workspace green while
  `wh keyset remove ap` told the operator "rapid trigger off over the whole board was not
  confirmed", naming a command they had not run.

  Closed 2026-09-05: all four now assert `ap set over the whole board was not confirmed` in full.
  Proved by mutating the refusal to `wh keyset create`'s noun: exactly five tests fail, these four
  and the unit test named below, and nothing else in the workspace. Two corrections to this entry,
  both measured while closing it. All four guard `confirm_whole_board_ap_set` alone, not
  `confirm_whole_board_create` as well: every create refusal already asserted its own subject. And
  a fifth site carried the same weak assertion, the unit test
  `confirm_whole_board_ap_set_refuses_on_no` in `crates/wh-cli/src/keyset.rs`, now tightened with
  the other four; it is the fifth failure under that mutation.

- [x] ~~**2.16 Comment cleanup in `wh-device`, from the final review of the keyset layer.**~~ All
  non-blocking, all in code files. The five-fixed-keys, nibble-0 and template-step-1 counts below
  are re-measured against the 39-file corpus current at this close; the already-closed `0x18`
  distribution and hypothesis bullets predate this round and still cite the 27-file corpus they
  were measured against, untouched here:
  - ~~`keyset.rs` `Change::ap` calls the vendor's promotion unmeasured.~~ Closed: the comment now
    says plainly that the promotion itself is measured (`ks-value-ap`, key `x`: MODE `0x0000` to
    `0x0010`) and that only its dependence on keyset membership is not.
  - ~~`keyset.rs` `frames()` claims a per-key group is at most 4 records.~~ Closed: the comment now
    attributes the cap to `plan`'s deduplicated `usages`, not to `frames()` itself, and says a
    repeated usage would split here, unreachable through the CLI since `Selector::resolve` and
    `read_matrix` both dedupe.
  - ~~`keyset.rs` `value_records()` says the slice is "packed per key below the 14-record
    limit".~~ Closed: it now says flat and unpacked, packing happens in `frames()`, and a batch of
    exactly 14 is reachable.
  - ~~`keyset.rs` `plan`'s divergence list omitted that the vendor writes MODE twice per key per
    operation.~~ Closed: added to the list.
  - ~~`keyset.rs` said the vendor reads `0x04` from five fixed keys "at the head of every
    capture".~~ Closed: now says "where it reads `0x04` at all", and states 5 of the 39 captures
    contain no `0x04` read request at all (re-measured; the five is unchanged, the denominator is).
  - ~~`ops.rs` said the vendor writes MODE `0x18` on every actuation point change.~~ Closed: the
    comment now says the measured distribution across all 27 captures, `{0x10: 154, 0x18: 40,
    0x20: 376, 0x28: 24, 0x30: 12, 0x38: 10, 0x48: 2}`, and keeps the load-bearing half, that the
    vendor rewrites MODE where `ap_records` sends nothing, which holds over 469 measured echoes.
  - ~~`ops.rs` still said a hypothesis "stays a hypothesis until the hardware session tests it".~~
    Closed: the session ran on 2026-08-29, and the comment now points at `docs/keysets.md`'s own
    ranking, that the MODE marker is unlikely to be the whole of the greying story.
  - ~~`keyset.rs`'s nibble-0 justification was rewritten to give the semantic reason and dropped
    the measurement.~~ Closed: the measurement is back, re-counted at 1150 MODE write records
    across the 39-file corpus, none at nibble 0 (the 618 the bullet named was stale).
  - ~~`wh set rt --set` is a third pair of routes to one intent with different frames.~~ Still true
    after `wh set rt --off` moved onto `plan`: `--set` still reaches the board through
    `ops::rt_records`, which writes MODE unconditionally where `plan` applies the skip and
    nibble-0 rules. Recorded on `Change::rt_on` itself.

- [x] ~~**2.17 What the `docs/keysets.md` verification pass found that touches code.**~~ Of the
  three code-touching findings:
  - ~~`0x16`/`0x17` are not a constant.~~ Advice to a task that never added them: confirmed
    `keyset::plan` still never writes those layouts. Nothing to change.
  - ~~Template step 1 is a two-record cap, not one frame per distinct value.~~ Closed: re-measured
    at 300 MODE-only write frames across the 39-file corpus (275 carry two records, 25 carry one,
    none more), added to `frames()`'s own comment as a divergence that is measured, not a match.
  - ~~The global rapid trigger skip is not measured as a membership test.~~ Nothing in the CLI
    implements a whole-board rapid-trigger toggle at all; `global_ap_excluding`/`global_rt_excluding`
    skip on membership by design, to compute a baseline, not as a guess at the vendor's own
    undocumented rule. No comment to correct.

  The remaining 26 findings are in the document, not repeated here.

- [x] ~~**2.18 Parked findings from task 2.4b's `wh keyset create` reviews.**~~ Five review rounds
  closed everything that changes behaviour. The bullets below are what survived, all judged not to
  block the rest of the CLI, all measured rather than suspected. Two were added after this entry was
  written. All are now closed, the last four on 2026-09-05.

  - ~~`verify_write_as`'s (named `verify_create` when this was written) `rt_keyset` fallback to the
    pre-write value was unpinned.~~ Closed: mutating it to compare the readback against itself fails
    `keyset_set_rt_end_to_end_catches_a_membership_drift_on_the_second_member`.
  - ~~`value_moves`'s rapid trigger arm is pinned as a unit but neither of its two comparisons
    individually, because the fixture moves both press and release.~~ Closed:
    `keyset_create_announces_a_rapid_trigger_steal_when_only_one_half_of_the_pair_moves` moves only
    `w`'s release and only `a`'s press, in one create, so each comparison is pinned on its own.
    Mutating the arm to compare press alone, then release alone, fails it each time and leaves the
    original both-halves fixture green. The press-only run printed "loses w (keeps 0.10/0.50mm)"
    beside frames writing 0.30mm, which is the consequence this bullet named.
  - ~~`describe_member` (renamed from `describe_loss` once it started covering a freshly-enrolled
    free key too, which loses nothing) documents a fourth outcome that appears to be
    unreachable.~~ Closed: the branch and its doc bullet are deleted. Established from `Change`'s
    own closed set of constructors, whose fields are private: each carries at most one kind's
    value, so a bundle whose described kind's value did not move can only have come from a moved
    MODE. `plan_writes_no_bundle_when_nothing_the_change_carries_moves` pins that for the three
    constructors that reach `describe_member`, `ap`, `rt_on` and `rt_off`, each over a board
    holding none of the tree's own constants, since an injected second value escapes only by
    coinciding with what is already there. `ap_keeping_touch` (no production caller today) and
    `membership_only` (carries no value) are outside it, and `ReplayTransport`'s byte-for-byte
    matching is their guard. Measured, not reasoned: the one thing that did reach the branch was a
    forged `Target`, and that is now what picks the kind.
  - ~~`mode_change`'s comment justified printing a `TouchMode` through `{:?}` by a precedent in
    `dump` that does not exist.~~ Closed: the comment now says `dump` prints `on`/`off` and a raw
    `mode_raw` instead, that this announcement is the only place in `wh` that names a touch mode to
    the operator, and that an unknown nibble prints rough Rust tuple-variant syntax matching
    `ops::rt_records`, meaning the behaviour, since that function builds records and prints
    nothing. The behaviour itself was already right and is unchanged.

    Addendum 2026-09-06: the "only place in `wh`" half of that comment stopped being true when 3.3
    landed `wh socd unpair`, which names touch modes through `run::touch_mode_label`. The comment
    has been corrected, and it no longer offers exclusivity as a reason to keep `{:?}` here. What
    remains is a real gap rather than a settled choice, so it is now owned by its own entry below
    rather than by this closure note.
  - ~~`announce_steal`'s `kind` still selects what is compared, unlike `verify_create`'s.~~ Closed
    by removing the parameter: `Target::kind()` derives it from the value the announcement is
    already printing, so what is compared and what is named can no longer disagree.
    `announce_delete` and `announce_remove` had the identical shape and lost theirs the same way,
    so "the last surviving instance" was three. The residual forgery, a `Target` whose variant
    disagrees with the `Change` beside it, is measured as caught: building an rt create's target as
    `Target::Ap` fails five fixtures, on a header and member lines naming actuation points while
    the frames carry rapid trigger values. `confirm_whole_board_create` kept a `kind`
    alongside its `Target` at this close, since it picks a different clause builder from it, and
    both of its refusals are pinned in full; 2.31 took that one too.
  - ~~`verify_create`'s `op` is a `&str` with three intended values, so a delete can label itself a
    create.~~ Closed: `KeysetOp`, four variants, since `set` had joined the three since this was
    written. The kind is not folded in, unlike `WholeBoardOp::KeysetRemove`'s, because all four run
    over either kind. `remove`'s label turned out to be the one of the four with no end-to-end
    cover, so a remove could still have reported itself as a create with the workspace green;
    `keyset_remove_leaves_the_keyset_alive_when_others_remain` now asserts it, and fails alone
    under that swap.
  - ~~`verify_restore` was pinned as a whole and not per comparison.~~ Closed: every comparison,
    `ap`, `rt_press`, `rt_release`, `mode`, and both keyset memberships (`0xFF` and `0xFE`), is now
    its own fixture-backed fault, confirmed by disabling each one at a time against the full
    workspace and finding exactly one failing test per row.
  - ~~`wh restore` never checks the snapshot's key usages against the board's live matrix, so a
    snapshot from a different matrix writes values and membership to usages the board may not have.
    Worse than cosmetic, because `verify_restore` reads back the snapshot's usages rather than the
    board's, so a phantom usage the firmware echoes is reported as verified rather than refused.~~
    Closed: the operator ruled refuse, on 2026-09-05. `check_restore_matrix` compares the
    snapshot's usages against a live `ops::read_matrix` inside the session, after the profile
    check and before `auto_backup`, and refuses naming the absent keys. The cost is accepted and
    stated in the refusal: a snapshot from a different matrix is unrestorable, not partly
    restorable. Three roundtrips added to every restore.
  - ~~The cross-layout membership check is a new hardware assumption stated nowhere: an actuation
    point create now asserts that `0xFE` is untouched, and the converse.~~ Closed: one line added
    in `verify_write_as` naming the assumption and the separate-counters finding that makes it right.

- [x] ~~**Decision: `wh set ap --keys all` keeps its current behaviour.**~~ Ruled by the operator on
  2026-09-03, after the whole-branch review raised it. On a board holding keysets, that command
  collapses every one of them into a single new index and the old indices cease to exist. It follows
  from the split rule but extends it to two shapes `docs/keysets.md` says nothing supports, a keyset
  consumed whole and a selection spanning two. Rejected alternatives: refusing those two shapes,
  which would make an ordinary bulk command fail on any board with a keyset, and gating them behind
  `--force`, which buys safety with a divergence from the vendor that is itself unmeasured. What
  makes the current behaviour acceptable is measured: the announcement names every keyset losing
  members before anything is written, a backup is taken first, and the review drove the full
  `wh restore --last` round trip and confirmed membership and every value returned exactly.

- [x] ~~**2.19 Pin the two-keyset merge in `wh set ap`.**~~ At the time, `ap_membership_for` returned
  `Keep` in two cases: no selected key was in a keyset, and exactly one keyset lost members with the
  selection being exactly that keyset. Everything else produced one new keyset containing the whole
  selection. So a selection spanning two keysets merges them, and where a keyset is wholly consumed
  its index ceases to exist; a keyset only partly selected survives with its remaining members. (The
  first case was closed by 2.20 above: an all-free selection now allocates too.)

  It wanted a test rather than a comment because the wrong implementations are plausible and all
  passed the suite that existed at the time. Closed by
  `set_ap_over_a_selection_spanning_two_keysets_merges_them_into_a_new_index` in
  `crates/wh-cli/tests/dump.rs`, over a board where `w,a` wholly consume keyset 1 and `s` wholly
  consumes keyset 2, with two selections in the one test: `w,a,s,d` (a free key `d` riding along)
  and `w,a,s` alone (exactly the union of the two losing keysets, nothing free). Both pin the same
  freshly allocated index, `3`, never a reuse of `1` or `2`, and the losing lines for both keysets.

  Three rewrites, and which selection catches which matters, because an earlier version of this
  note paired one rewrite's description with another's coverage and a reader trimming the suite
  would have deleted live coverage on its word.

  1. "If every losing keyset is wholly consumed, keep the lowest index", with no further condition.
     Already caught by other fixtures before this test existed. Redundant here.
  2. The same, confined to the multi-keyset case, with no `total == usages.len()` guard. Reuses
     index 1 for `w,a,s,d`, so the **free-key selection** catches it.
  3. The same, confined to the multi-keyset case, **with** the `total == usages.len()` guard. On
     `w,a,s,d` the guard is false, since three keys are taken from four selected, so allocation is
     unaffected and that selection passes. Only the **`w,a,s` selection**, where the guard is true,
     catches it. That is the rewrite the whole suite survived before this round, and the second
     `run_wh` call is the only thing that catches it.

- [x] ~~**2.10 Rename `Snapshot::global.travel_mm`.** Measured: it is the configurator's `"MM" CUSTOM
  VALUE`, the step size for its steppers, not the global actuation point.~~ Now `custom_value_mm`,
  carrying `#[serde(alias = "travel_mm")]` so backups already on disk keep restoring. The real
  global actuation point is still not in that record; it is what every key in no keyset holds in
  layout `0x04`. `wh dump --table` names it "custom value" too.

  **`--mm` is reserved for this, and must not be spent elsewhere.** Ruled by the operator on
  2026-09-04 while naming 2.23's flag. `"MM" CUSTOM VALUE` is the one term the configurator uses for
  this setting, and it is the exact term this task exists to stop being confused with the actuation
  point. Any flag `wh` grows for it should be `--mm`; 2.23 uses `--base` for the actuation point so
  the two cannot collide. No such flag existed at the time: the field was only recorded and
  restored, never set. 3.1 later spent the reservation as `wh set mm --value`, a subcommand rather
  than a literal `--mm` flag; see that entry for why.
- [x] ~~**2.11 Stop writing zero dead zones on restore.**~~ `wh restore` now sends the 200 and 200
  every measured vendor write carries. Measured 2026-09-05 across every `cmd 0x29` frame in
  `captures/`: 14 read requests in 7 files, every reply reporting both dead zones as `0`, and 3
  vendor writes at three different travel values, all carrying `200` and `200`. The snapshot still
  records what the board reported, informational only. Whether 200 is a fixed constant or a user
  setting at its default is not established, and is open below.
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
- [ ] **Setting the colour of the LEDs beside the knob.** Narrowed 2026-09-05: `cmd 0x18` is the
  lighting record and the strip's observed colours are firmware-driven; the one remaining probe is
  whether the colour-table block in `0x18` writes is writable to other values. See
  `docs/backlog.md` and `docs/protocol.md`.
- [x] ~~**Are the key backlights colour-programmable, or white only?**~~ Answered 2026-09-05 by
  looking, per this entry's own suggestion: white only, no UI control anywhere, one firmware pulse
  behaviour on FN. Brightness and sleep timer are host-controllable through `cmd 0x18`.
- [ ] **How the knob is programmed.** Volume travels over the standard HID consumer-control
  collection, not the vendor collection we capture, so our existing method cannot see it at all.
  Blocked on the spy.

### Features

- [ ] **A TUI clone of the vendor configurator**, running inside the `wh` binary. The prerequisite is
  now met: the write path has been exercised against hardware.
- [ ] **A spinner on write commands.** Reads deliberately get none; their speed is a feature.

### Protocol gaps

- [x] ~~**Command `0x18`.**~~ Identified 2026-09-05: PRGB, the lighting record, see
  `docs/protocol.md`. `0x2c` was resolved on 2026-08-29 as SOCD's read side and confirmed by
  writes on 2026-09-05.
- [ ] **Eight `cmd 0x00` sub-orders.** All request and reply balanced, none ever failing, none
  needed by anything shipped. `0xc0` left the list on 2026-09-05, identified as Show Analog
  Output.
- [ ] **Layouts `0x16`, `0x17` and `0x19`.** `0x16` and `0x17` were recorded as never once
  observed non-zero across 1806 records. That held for Phase 1 only: they read `100` on every key of
  profile 1 from 2026-08-29 onward, measured in `layout-16-by-profile`, and `0` on every key of
  profile 2, measured in `profile-switch`, which establishes its own profile from its frames. An
  earlier revision cited `layout-16-by-profile` for both halves; it contains no profile 2 read. They are not the global rapid trigger sensitivity, measured 2026-08-29:
  they stayed at `100` through two global changes that moved `0x14`/`0x15` to `150` and then `200`.
  Identified 2026-09-05 as
  the per-key safety-zone margins (`docs/protocol.md`); what flipped them on profile 1 between the
  2026-08-28 and 2026-08-29 sittings is still unmeasured. `0x19`
  is still only ever `0x0000` or `0x3e2c` and stays open.
- [ ] **Where the global rapid trigger sensitivity is stored.** No global command carries it. It
  appears only in `0x14`/`0x15` of keys outside a rapid trigger keyset, which would also be how the
  configurator reads it back. Plausible and testable, not measured. Needed to name the reset target
  of a rapid trigger keyset delete as something other than "what the vendor wrote".
- [ ] **Key `0x01`, probably FN. [hardware]** Deliberately unmeasured, because confirming it means
  remapping FN away and FN is how you reach the layer that would let you undo that.
- [ ] **Are the `cmd 0x29` dead zones fixed constants or a user setting at its default?** Every
  measured vendor write carries 200 for both, which `wh restore` now writes too, but the vendored
  `pack.ts` defaults them to `0` while a sibling app exposes them as sliders initialised at `0.2`mm.
  Unsettleable by readback: the board reports `0` for both whatever was written. If they are a user
  setting, `wh restore` overwrites the operator's choice invisibly, and `wh selftest`, the one place
  `wh` still writes a zero dead zone, zeroes it. `wh set mm` writes the same 200/200 on every value
  change too, and is the routine, operator-initiated case rather than restore's occasional one. See
  `docs/backlog.md`.
- [ ] **Widen what a snapshot captures.** It currently records the `cmd 0x29` global record, four
  layouts per key, and the profile. It does not record key mappings, the FN layer, SOCD, dynamic
  keystroke, mod tap, gamepad configuration, RGB, or polling rate. SOCD is the one of those that
  can diverge rather than merely be absent, now that 3.3 can change pairs: the mode value a
  snapshot does store carries the SOCD participation flag, so a restore can set that flag on a key
  whose pairing is gone. `docs/backlog.md` has what closing it would take.

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
