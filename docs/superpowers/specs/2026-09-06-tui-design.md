# wh tui design (3.5)

## Goal

A terminal UI, launched as `wh tui`, that replicates terminal.wallhack.com one to one: the same
layout, the same tabs, the same gestures, driven by mouse or arrow and enter keys. Values are
populated by reading the board on open. Nothing is cached beyond the read-modify-write window the
CLI already has.

## Decisions made with the operator (2026-09-06)

1. Scope: the full vendor frame, with honest stubs. Every tab renders. A tab whose write path is
   not yet built says so instead of pretending.
2. Writes are live, through the same planning paths the CLI uses. One backup per TUI session,
   reason `auto: tui`, taken before the session's first write and not repeated for later writes
   in the same session.
3. Every destructive action gets a typed-yes modal. This deliberately guards harder than the
   vendor, which confirms only Reset Profile and Factory Reset and fires RESET KEYSETS, RESET
   LAYER and SOCD REMOVE instantly (measured in the demo walkthrough, see the screenshot index).
4. Architecture: a new `wh-tui` crate; a single-threaded event loop alternating
   `crossterm::event::poll` with `Session::poll_event`, roughly 15ms each. No threads, no async.

## Visual reference

The reference is the vendor's own bundle, snapshotted at `research/vendor-bundle/2026-09-05/` and
driven in its demo mode on 2026-09-06. Sixteen screenshots with a per-file index live in
`screenshots/` there; palette and font facts extracted from the bundle's CSS and JS are in
`styling-notes.md`. Everything below was observed in those captures, not inferred.

The default theme is `monochrome`: white on black, `#CCCCCC` secondary, `#999999` accent. The TUI
uses the terminal's default foreground and background plus dim, which reproduces it without a
colour system. The vendor body font is monospace, so the web layout is already terminal-shaped.

Frame, top to bottom:

- ASCII logo block, then `WH_TERMINAL V1.0.0 - WALLHACK 2026`, then
  `NAVIGATE WITH MOUSE OR ARROW & ENTER KEYS`.
- Device line: connection mark, `WALLHACK K-001 - V<fw>`, and a `PROFILE < n >` stepper.
- Tab row: ACTUATION POINT, RAPID TRIGGER, MAPPING, SWITCHES, ADVANCED. The selected tab renders
  boxed or inverted; the others plain; disabled things dim.
- Two-pane body: settings on the left, the 68-key ANSI-DK matrix on the right. Between them one
  prompt line: left half a `> message` status, right half the contextual actions (RESET KEYSETS,
  or ADD KEYSET [ENTER] and CANCEL [ESC] during a selection). Only ADVANCED's GAMEPAD, DEVICE and
  SHARE sub-tabs drop the keyboard pane.
- Footer: HELP drawer bottom left, EN JA CH language row, support email.

Widget inventory, in order of leverage:

- **The row primitive.** Every setting is `LABEL....dot leaders....< VALUE >` (a stepper, where
  `<` and `>` are click targets) or `LABEL....dot leaders....[BUTTON]`. Disabled rows dim whole.
  Dependent rows (RT's three sub-settings) disable while their parent toggle is OFF. This one
  widget is most of the UI.
- **Keycaps are two-line cells that carry state.** Label on top, per-key value beneath (AP
  millimetres, RT sensitivity), the value appearing only once the key is in a keyset. Selection
  inverts the cell. A keyset member gains a small `[x]` remove badge. Special bindings restyle
  the cap: SOCD renders both partner keys as linked split caps (`A D` and `D A`), Mod Tap splits
  tap over hold with `?` for unbound, DKS adds a `DKS` sub-label.
- **Keyset building is one gesture shared by AP, RT and SWITCHES.** Click keys to accumulate
  (the prompt counts them), Enter or ADD KEYSET commits, Escape or CANCEL aborts. A committed
  keyset becomes a collapsible left-pane row `[X] Q,W .... [^]` with its own indented steppers.
- **Modals are centred bordered boxes with no backdrop dim.** The vendor's reset confirm titles
  itself (`RESET PROFILE 1`), states the consequence, and focuses CANCEL. `wh tui` keeps that
  shape but requires typing `yes`, per decision 3.
- **The walkthrough is an anchored callout system**, a bordered box with a caret pointing at its
  highlighted target and a `1 OF 22` footer. Out of scope for 3.5's build (it is vendor help
  copy); the ADVANCED row for it renders as a stub.

Per-tab bodies, from the screenshots:

- **ACTUATION POINT**: Global Actuation Point stepper (2.00 MM default), "MM" custom value
  stepper (0.10 MM), then keysets. Keys show their AP value.
- **RAPID TRIGGER**: Global Rapid Trigger toggle, then Separate Press and Release, RT
  Sensitivity, Continuous Rapid Trigger, all three disabled while the global toggle is OFF.
  Keyset rows carry the same three sub-steppers.
- **MAPPING**: two levels of sub-tabs (BASE LAYER / FN LAYER, then BASE CHARACTERS / EXTENDED
  CHARACTERS / FUNCTIONS / GAMEPAD) over a character palette. Click a key then a character, in
  either order. RESET LAYER top right.
- **SWITCHES**: Calibrate Switches (START plus a three-line advisory), Current Switches dropdown,
  switch groups built with the keyset gesture.
- **ADVANCED / GENERAL**: Reset Profile and Factory Reset (SELECT), Polling Rate, LED Sleep
  Timer, LED Brightness, System Type, four ON/OFF steppers (Show Analog Output, Safety Zone,
  Show Mapped Key Labels, Localized Key Labels), SOCD, DKS and Mod Tap SELECT rows, Walkthrough.
  RESET ALL top right.
- **ADVANCED / GAMEPAD**: Gamepad Mode stepper, four toggles, and a joystick-curve point graph.
  **DEVICE**: read-only name, serial, firmware. **SHARE**: Export (COPY) and Import (IMPORT).
- **SOCD editor**: prompt-driven two-key pick, then `SOCD: A + D` with REMOVE, DONE and a
  PRIORITY stepper (LAST-INPUT). **DKS editor**: four SELECT binding rows crossed with seven
  travel-stage columns (`1↓ 2↓ 3↓ 4↕ 3↑ 2↑ 1↑`) and a legend. **MOD TAP editor**: TAP, HOLD,
  HOLD DURATION rows with CANCEL and a DONE disabled until bound.

## Live, read-only, or stub

The rule: a control is live where `wh` already has the operation, read-only where only the read
is measured, and a stub where neither exists yet. Stubs render the vendor layout with a one-line
"not built yet, see docs/tasks.md" where interaction would start. Nothing unmeasured is shown as
a value.

| Surface | State in 3.5 |
|---|---|
| AP tab (global, custom value, keysets) | Live |
| RT tab (toggle, sensitivities, keysets) | Live |
| Profile stepper | Live |
| SOCD editor | Live (`wh socd` exists) |
| Show Analog Output, Safety Zone toggles | Read-only (reads measured in 3.2; `wh` has no write op yet) |
| MAPPING | Stub until 3.6; current mapping may display (reads measured) |
| LED rows | Stub until 3.7; values may display (record decomposed) |
| SHARE | Stub until 3.8 |
| SWITCHES calibration and switch type | Stub (unmeasured) |
| DKS, Mod Tap editors | Stub (write model unmeasured) |
| GAMEPAD sub-tab | Stub |
| Walkthrough | Stub |
| Reset Profile, Factory Reset, RESET ALL | Stub in 3.5 (destructive, write model unmeasured) |

## Event loop

One loop, one thread:

1. `crossterm::event::poll(15ms)`; handle input, which may issue Session reads and writes inline.
2. `session.poll_event(15ms)`; route `AdjustModeEntered` to the locked banner,
   `AdjustModeLeft` to banner-off plus a full re-read, `Unknown` to a status-line note.
3. Redraw if dirty.

While locked (between `be 00` and `be 01`): navigation and mouse stay alive, edits are refused
with the banner's wording. Reads and writes both work mid-lock (measured in 3.4), but the board
is editing the same settings, so refusing edits avoids a host-versus-board race. The vendor does
the same by ignoring `be 00` and re-reading everything on `be 01`.

Two rulings the task-2 report addenda asked 3.5 to make:

- **Starvation**: the TUI drains the event queue every tick via `poll_event`, so the 256-edge
  accumulation cannot build in normal operation. If a roundtrip returns Timeout with events
  pending, the TUI keeps the events (they are the re-read signal) and surfaces the timeout in
  the status line. No queue cap is added in 3.5.
- **Hijack by absence**: the TUI adds no encoder for cmd 0x00 sub-order 0xbe, so the routing
  predicate's safety argument is unchanged.

## Writes and backups

Writes go through the same `keyset::plan` paths as the CLI, preserving the read-modify-write
invariant. The first write of a TUI session takes one backup with reason `auto: tui`; later
writes in the session do not. After a write, the affected keys are re-read and the UI shows the
board's values, never the intended ones.

## Replay scripting

`poll_event` on a replay script whose cursor sits on an Out entry is a loud `DeviceError::Replay`
today, so the poll-then-read loop cannot run under `WH_REPLAY`. 3.5 adds a `wait` entry to the
JSONL script format: at a `wait` entry, `recv` returns the timeout path and the entry is consumed
(a counted form, `{"wait": n}`, covers n polls). `send` at a `wait` entry is an error, because
the script said nothing should be sent yet. Byte-for-byte matching of In and Out entries is not
touched.

## Testing

- Rendering through `ratatui::backend::TestBackend`: draw, then assert the frame's text with
  whole-line assertions, per the house test discipline.
- Event-loop behaviour over `ReplayTransport` scripts using `wait` entries: open-read, a write
  gesture end to end, the locked banner on a scripted `be 00`/`be 01` pair, the re-read after
  `be 01`.
- The three gates as always; existing suites untouched.

## Out of scope for 3.5

Remap writes (3.6), lighting writes (3.7), import and export (3.8), DKS and Mod Tap write
models, calibration, the walkthrough content, and any theme system beyond the terminal's own
colours.
