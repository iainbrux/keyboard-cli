# Outstanding work

Live checklist for `wh`. Items are struck through and ticked as they complete. Anything needing the
keyboard physically present is marked **[hardware]**.

Evidence for every protocol claim below is in `docs/protocol.md`, `docs/protocol-inventory.md` and
`docs/keysets.md`, measured from 5860 frames of real device traffic across 36 capture files.

## Phase 1

Complete. See the Done section.

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

  This is a **chosen default**, not a measurement of the board's factory setting. Nothing has read an untouched profile. Profiles 3 and 4 are believed never used and one
  read of either would replace this constant with a measured number.

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

- [ ] **2.23 `wh set ap --base <mm>` to set the board's base actuation point. Depends on 2.22.**
  There is currently no way to do this, and 2.22 makes the gap visible. The base is not a stored
  setting: it is what every key outside a keyset holds in layout `0x04`, which is also why 2.10
  exists. So setting it means writing the value to every free key and touching no membership.

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

  Unmeasured, and worth a capture before building: what the configurator sends when its GLOBAL
  ACTUATION POINT field is changed. That the base is what free keys hold is established, so writing
  `0x04` to every non-member key is the only way to set it; what is not known is whether the vendor
  sends anything else alongside, a MODE record for instance. `begin("ks-set-global-ap")`, change the
  field, copy.

- [ ] **2.24 Share what is still identical between `keyset::delete` and `keyset::remove`, and stop
  before the part that is not.** Deferred during 2.21 because extracting it would have refactored
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

- [ ] **2.27 `wh keyset create --keys all` is a third unguarded route to destroying every keyset.**
  Found by a reviewer probing the guard added for `wh set ap --keys all`, and measured:
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

  `crates/wh-cli/src/confirm.rs`'s module doc says the two commands "reach the same destruction by
  different routes". Once this lands it is three, and the doc should say so rather than implying the
  list was complete.

- [ ] **2.26 Two regression-guard gaps in `wh keyset remove`'s announcement, each one fixture.**
  Found by a cold reviewer that built its own replay generator and drove the binary, after the
  committed behaviour had already been measured correct in both cases. **The shipped code is right;
  what is missing is a test that would notice if it stopped being.** Each is one fixture.

  **The mode count can be over-claimed on a board the three current fixtures cannot distinguish.**
  The whole-board prompt counts keys whose touch nibble moves. Two wrong predicates survive the
  suite green, counting keys with any value record, and counting keys with a MODE record. Both agree
  with the correct answer on the shipped fixtures (4 of 4, 2 of 4, 0 of 4) and diverge only on a
  whole board where every key is already at nibble 1 and holds a stray value: every key gets value
  records, no nibble moves, and the mutant prints "4 key(s) move off global travel" when none do.
  Missing fixture: that board, asserting the clause is absent.

  The under-reporting direction, which is the dangerous one, is already pinned: counting only keys
  whose owned value also moves, and counting only `Rt` transitions while missing the nibble-0
  promotion, are each killed by two tests. The prompt cannot silently omit a mode change.

  **`keyset_disappears` can be under-claimed.** Rewriting it as `leaving.len() == ks.members.len()`
  survives the suite green. Measured on two keysets, 1 holding `w,a` and 2 holding `s,d`, removing
  `w,a,s`: keyset 1 is emptied and the mutant omits "keyset 1 ceases to exist". Consequence is mild,
  since the operator still sees a `removing` line for every member, so the destruction stays
  inferable. Missing fixture: two keysets, remove all of one plus part of the other.

  Deliberately not fixed in the branch that found them. Seven fix rounds ran there and three of the
  last four introduced a defect of the class they were fixing, so an eighth round carried more risk
  than two unguarded predicates whose behaviour is measured correct.

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

- [ ] **2.13 `wh set rt --off` must clear rapid trigger keyset membership. Depends on 2.4.**
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
  `ops::rt_off_records` refuses to do. `plan` no longer emits one at all, measured against 618 MODE
  write records in the corpus of which none is at nibble 0, so routing this task through `plan` no
  longer introduces that write.

  Measured in the same review, and settling an earlier doubt: the vendor **does** reset the
  sensitivities on a per-key rapid trigger off. `rt-off-w.jsonl` shows W going from 500/500 to the
  global 100/100. `ops::rt_off_records` writes MODE alone and leaves the private value in place, so
  it is the one that diverges; routing through `Change::rt_off` removes a divergence rather than
  creating one.
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
    no global to read. Rejected alternatives: the whole board's majority, which returns some
    keyset's value wearing the global's name, and the vendor's five fixed keys (`0x29`, `0xfa`,
    `0x31`, `0x28`, `0x52`), whose disagreement behaviour is unmeasured and one of which was itself
    in a keyset.

  This gives `wh keyset create ap`, `wh keyset delete ap` and `wh set rt --off` a `--value` (or
  `--press`/`--release`) escape hatch that is optional on an agreeing board and required on a
  disagreeing one. Implemented in 2.4b and 2.13, not here.

- [ ] **2.16 Comment cleanup in `wh-device`, from the final review of the keyset layer.** All
  non-blocking, all in code files, so all wanting an implementer rather than a hand edit:
  - `keyset.rs` `Change::ap` calls the vendor's promotion unmeasured. The promotion is measured
    (`ks-value-ap`, `X` from `0x0000` to `0x0010`); whether it depends on keyset membership is not.
    Say which, see 2.14.
  - `keyset.rs` `frames()` claims a per-key group is at most 4 records. That is a property of
    `plan`'s output, not of `frames()`: `plan` takes a bare `&[u8]` with no dedup, and a repeated
    usage produces a 16-record group that does split. Unreachable through the CLI, since both
    `Selector::resolve` and `read_matrix` dedupe.
  - `keyset.rs` `value_records()` says the slice is "packed per key below the 14-record limit". It
    is flat and unpacked; packing happens in `frames()`, and a batch of exactly 14 is reachable.
  - `keyset.rs` `plan`'s divergence list presents itself as complete and omits one: the vendor
    writes MODE twice per key per operation, we write it once.
  - `keyset.rs` says the vendor reads `0x04` from five fixed keys "at the head of every capture".
    Five of the 27 contain no `0x04` read at all.
  - ~~`ops.rs` said the vendor writes MODE `0x18` on every actuation point change.~~ Closed: the
    comment now says the measured distribution across all 27 captures, `{0x10: 154, 0x18: 40,
    0x20: 376, 0x28: 24, 0x30: 12, 0x38: 10, 0x48: 2}`, and keeps the load-bearing half, that the
    vendor rewrites MODE where `ap_records` sends nothing, which holds over 469 measured echoes.
  - ~~`ops.rs` still said a hypothesis "stays a hypothesis until the hardware session tests it".~~
    Closed: the session ran on 2026-08-29, and the comment now points at `docs/keysets.md`'s own
    ranking, that the MODE marker is unlikely to be the whole of the greying story.
  - `keyset.rs`'s nibble-0 justification was rewritten to give the semantic reason and dropped the
    measurement. Both should stand: 618 MODE write records across the corpus, none at nibble 0.
  - `wh set rt --set` is a third pair of routes to one intent with different frames, alongside the
    two recorded in 2.13 and 2.14. `plan` matches the vendor here and `ops::rt_records` diverges.

- [ ] **2.17 What the `docs/keysets.md` verification pass found that touches code. Read before
  writing 2.4b.** An adversarial pass on 2026-09-03 checked every measured claim in that document
  against the frames across 78 checked claims: 46 confirmed outright, 9 confirmed in part, 23
  findings only, 29 findings in all and 8 of them flatly wrong. A second pass over the corrections
  found nine more, seven of them introduced by the rewrite. The document is now rewritten. Three
  findings change what the CLI should do rather than only what the document says.

  - **`0x16` and `0x17` are not a constant.** They are rewritten at the key's current value like
    any other non-owned layout, matching what each capture that reads them reads: `0` in all 38
    Phase 1 write records, `100` in all 580 from the profile 1 captures, and `0` in all 414 from the
    profile 2 ones. `keyset::plan` never writes them, and its stated reason, that a constant would be
    an invented value, turns out to be right for a reason it did not know. Of the 34 captures before
    `layout-16-by-profile`, 25 both read and write them, four read without writing, four do neither,
    and one writes them with no read frames in the file at all.
    If 2.4b ever adds them, read them per key. Hard-coding `100` would write `100` over `0` on a
    board that has never held a keyset.
  - **Template step 1 is a two-record cap, not one frame per distinct value.** Of 162 MODE-only
    write frames, 147 carry two records and 15 carry one, and none carries more. The vendor splits
    one value across two frames and puts two values in one. `frames()` packs whole per-key groups up
    to 14 records, so it already diverges here; the divergence is defensible, but it is now a
    measured one rather than a match, and the comment in `keyset.rs` should say so.
  - **The global rapid trigger skip is not measured as a membership test.** None of the four global
    captures reads `0xFE`. The two skipped keys are simultaneously the members of `0xFE=2` and the
    only two keys at MODE nibble `3`, so the frames cannot separate the two rules. Worse, `u` and
    `i` held `0xFE=1` at the last membership read and were written rather than skipped. Anything in
    the CLI that skips on membership is choosing one of two readings, and should say which.

  The remaining 26 findings are in the document. The ones worth knowing while writing the CLI: the
  allocation of `0xFF=9` is unexplained by any frame and is no longer offered as evidence for max
  plus one, indices are reused after a delete rather than being monotonic, and the vendor does not
  always batch two members of one create into the same frame.

- [ ] **2.18 Parked findings from task 2.4b's `wh keyset create` reviews.** Five review rounds
  closed everything that changes behaviour. The bullets below are what survived, all judged not to
  block the rest of the CLI, all measured rather than suspected. Two were added after this entry was
  written, and any struck through since have been closed.

  - ~~`verify_write_as`'s (named `verify_create` when this was written) `rt_keyset` fallback to the
    pre-write value was unpinned.~~ Closed: mutating it to compare the readback against itself fails
    `keyset_set_rt_end_to_end_catches_a_membership_drift_on_the_second_member`.
  - `value_moves`'s rapid trigger arm is pinned as a unit but neither of its two comparisons
    individually, because the fixture moves both press and release. A fixture moving only one
    closes it. Consequence if the release half is lost: a create is announced as keeping a value it
    is about to overwrite.
  - `describe_member` (renamed from `describe_loss` once it started covering a freshly-enrolled
    free key too, which loses nothing) documents a fourth outcome that appears to be
    unreachable. Reasoning, not measurement: `plan` emits value records only when MODE or a value
    moved, so if the value did not move then MODE did, and the branch above catches it. Either
    delete the branch and its doc bullet or find the case that reaches it.
  - ~~`mode_change`'s comment justified printing a `TouchMode` through `{:?}` by a precedent in
    `dump` that does not exist.~~ Closed: the comment now says `dump` prints `on`/`off` and a raw
    `mode_raw` instead, that this announcement is the only place in `wh` that names a touch mode to
    the operator, and that an unknown nibble prints rough Rust tuple-variant syntax matching
    `ops::rt_records`, meaning the behaviour, since that function builds records and prints
    nothing. The behaviour itself was already right and is unchanged.
  - `announce_steal`'s `kind` still selects what is compared, unlike `verify_create`'s. Safe inside
    `create` by construction and pinned there by three fixtures, but it is the last surviving
    instance of the pattern four rounds were spent removing. Recorded in the plan as a warning to
    the task that consumes it.
  - `verify_create`'s `op` is a `&str` with three intended values, so a delete can label itself a
    create. Cannot affect what is checked, which was proven by deleting the label and watching every
    other parameter go dead. A small enum would make it unforgeable.
  - ~~`verify_restore` was pinned as a whole and not per comparison.~~ Closed: every comparison,
    `ap`, `rt_press`, `rt_release`, `mode`, and both keyset memberships (`0xFF` and `0xFE`), is now
    its own fixture-backed fault, confirmed by disabling each one at a time against the full
    workspace and finding exactly one failing test per row.
  - `wh restore` never checks the snapshot's key usages against the board's live matrix, so a
    snapshot from a different matrix writes values and membership to usages the board may not have.
    Worse than cosmetic, because `verify_restore` reads back the snapshot's usages rather than the
    board's, so a phantom usage the firmware echoes is reported as verified rather than refused.
    Fixing it needs a policy decision (refuse, skip, or gate on a flag), a live matrix read inside
    the session, and a restructure of restore's build-everything-before-sending order.
  - The cross-layout membership check is a new hardware assumption stated nowhere: an actuation
    point create now asserts that `0xFE` is untouched, and the converse. `docs/keysets.md`'s
    separate-counters finding makes that the right assumption, and it is the only one of the six
    checks resting on the firmware not coupling two layouts. One line saying so satisfies the
    measure-never-infer rule.

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

- [ ] **2.10 Rename `Snapshot::global.travel_mm`.** Measured: it is the configurator's `"MM" CUSTOM
  VALUE`, the step size for its steppers, not the global actuation point. The real global actuation
  point is not in that record; it is what every key in no keyset holds in layout `0x04`.

  **`--mm` is reserved for this, and must not be spent elsewhere.** Ruled by the operator on
  2026-09-04 while naming 2.23's flag. `"MM" CUSTOM VALUE` is the one term the configurator uses for
  this setting, and it is the exact term this task exists to stop being confused with the actuation
  point. Any flag `wh` grows for it should be `--mm`; 2.23 uses `--base` for the actuation point so
  the two cannot collide.
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

- [ ] **Command `0x18`.** Suspected RGB or LED control, 10 request and reply pairs across five
  files. `0x2c` was resolved on 2026-08-29: it is SOCD, measured, see `docs/keysets.md`.
- [ ] **Nine `cmd 0x00` sub-orders.** All request and reply balanced, none ever failing, none needed
  by anything in Phase 1.
- [ ] **Layouts `0x16`, `0x17` and `0x19`.** `0x16` and `0x17` were recorded as never once
  observed non-zero across 1806 records. That held for Phase 1 only: they read `100` on every key of
  profile 1 from 2026-08-29 onward, measured in `layout-16-by-profile`, and `0` on every key of
  profile 2, measured in `profile-switch`, which establishes its own profile from its frames. An
  earlier revision cited `layout-16-by-profile` for both halves; it contains no profile 2 read. They are not the global rapid trigger sensitivity, measured 2026-08-29:
  they stayed at `100` through two global changes that moved `0x14`/`0x15` to `150` and then `200`.
  What moved them on profile 1 is unmeasured, and `wh` never writes them so it is not ours. `0x19`
  is still only ever `0x0000` or `0x3e2c`.
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
