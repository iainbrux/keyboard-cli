# Transport events: receiving the board's unsolicited frames

Task 3.4. The architectural prerequisite for the TUI (3.5), designed with the operator on
2026-09-06 and resting on three hardware measurements made the same night.

## Goal

`wh` can receive, keep, and act on frames the board sends without being asked, today meaning the
`cmd 0x00` sub-order `0xbe` adjust-mode edges, without changing the one-shot CLI's behaviour and
without threads.

## The measurements this design rests on

All in `docs/protocol.md` ("The board announces its own adjust mode"):

1. `be 00` entering adjust mode, `be 01` leaving, unsolicited, measured 2026-09-04. The vendor
   ignores the first and re-reads nine layouts on the second.
2. The lock is input-only (2026-09-06): a mid-lock `wh dump` answers normally and a mid-lock
   `wh set ap` is accepted, applied and survives the concluding AP press. Nothing in any frame
   reveals the lock, so passive detection is impossible; hearing the edges is the only route.
3. `Session::roundtrip` today silently discards any valid frame whose cmd is not the awaited
   reply, so an edge arriving mid-command is lost.

## Design

### Transport: unchanged

`Transport` keeps exactly `send(&[u8; 64])` and `recv(timeout) -> [u8; 64]`. Both implementations
already provide a bounded wait for one frame, which is the only primitive events need. No trait
change, no background reader, no channels: the single-threaded determinism every replay test relies
on is untouched.

### `BoardEvent`, in `wh-proto`

```rust
pub enum BoardEvent {
    AdjustModeEntered,          // cmd 0x00 reply, payload 00 be 00
    AdjustModeLeft,             // cmd 0x00 reply, payload 00 be 01
    Unknown(Vec<u8>),           // any other device-initiated frame: kept, never dropped
}
```

Parsed by a pure `wh-proto` function from a received frame, alongside a predicate the session uses
to distinguish "event" from "unexpected frame". The byte shapes come from the captures
(`board-side-ap-change`, `board-side-rt-change`); the golden fixtures embed them as literals.
`Unknown` exists because the corpus proves the board volunteers frames, and `wh` must never again
be structurally deaf to a new one: an unknown device-initiated frame becomes a visible event, not
silence.

### `Session`: a queue and one new method

- `events: VecDeque<BoardEvent>` on `Session`.
- `roundtrip`'s read loop routes device-initiated frames (per the `wh-proto` predicate) into
  `events` instead of discarding them. Frames that are neither the awaited reply nor an event keep
  today's behaviour exactly.
- `pub fn poll_event(&mut self, timeout: Duration) -> Result<Option<BoardEvent>, DeviceError>`:
  drain `events` first; else `recv(timeout)`; a timeout is `Ok(None)` (the normal idle case); a
  received frame parses to an event or, if it is not device-initiated, is handled as `roundtrip`
  would handle an unexpected frame.
- `pub fn pending_events(&mut self) -> impl Iterator<Item = BoardEvent> + '_` (a drain), so a
  finished command can ask what arrived mid-run.

### CLI behaviour: report, never act

After its work, any command whose session holds drained events prints one stderr line per edge
kind, at most once each:

```
note: the board entered its own adjust mode during this command; settings may have changed underneath it
note: the board left its own adjust mode during this command; settings may have changed underneath it
```

No refusal, no abort, no flags. Justified by measurement: mid-lock writes are safe (measurement 2),
aborting a multi-frame write midway is the known worse hazard, and a refusal the CLI can only
enforce in a race window would be a promise it cannot keep. Refusal belongs to the TUI (3.5), which
hears both edges and will refuse writes while locked with "finish the adjustment on the board
first", the operator's own ruling.

### ReplayTransport

A replay script represents an unsolicited frame as an inbound line not paired with an outbound
line. `recv` returns it when the cursor reaches it. If the current implementation requires
send-before-recv, extend it minimally to serve an inbound-at-cursor line on any `recv`; the
byte-for-byte matching of outbound frames must not be loosened in any way. The two `board-side-*`
captures become end-to-end fixtures: their `0xbe` frames plus the vendor's nine-layout re-read
sweep are real recorded traffic.

### `HidTransport`

`recv` already takes a timeout (hidapi read with timeout). Verify the timeout path returns
`DeviceError::Timeout` rather than an empty read, on hardware, once, during this task's hardware
check. No other change expected.

## What 3.5 consumes (not built here)

The TUI's event loop alternates terminal input polling with `poll_event(short timeout)`. On
`AdjustModeEntered`: show the locked banner, disable writes. On `AdjustModeLeft`: clear the banner,
re-read the board the way the vendor does. Nothing in 3.4 knows about rendering.

## Testing

- Golden: the `0xbe` byte shapes parse to the right events; every other frame in the corpus parses
  to none.
- Unit: `poll_event` returns queued events before touching the transport; timeout is `Ok(None)`;
  an event mid-`roundtrip` is queued and the roundtrip's own reply still matches; an unknown
  device-initiated frame surfaces as `Unknown`, not silence, not an error.
- End to end over replay: a script with an edge between two commands' traffic produces the stderr
  note exactly once; a script with no events produces no note (the negative half, asserted); a
  mid-roundtrip edge does not disturb the command's result but is reported.
- Mutation gates: revert the discard (events dropped) and watch the mid-roundtrip test fail; make
  the note print per-frame rather than per-kind and watch the exactly-once test fail; route
  `Unknown` to silence and watch its test fail.
- One hardware check during implementation: FN+AP with `poll_event` running, both edges received;
  the input-only lock measurements are already done (2026-09-06) and are not repeated.

## Out of scope

TUI rendering (3.5). Any interpretation of the third payload byte beyond `00`/`01`. Acting on
events in one-shot commands beyond the stderr note. The knob-versus-host write conflict, untested
and noted as such in `docs/protocol.md`.
