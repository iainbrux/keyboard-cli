# Transport Events Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** `wh` receives, keeps, and reports the board's unsolicited `0xbe` adjust-mode frames
instead of silently discarding them.

**Architecture:** A pure `BoardEvent` codec in `wh-proto`; an event queue plus `poll_event` on
`wh-device`'s `Session`, with `roundtrip` routing device-initiated frames into the queue before
its reply match; a once-per-kind stderr note in `wh-cli` after every command. `Transport` is
untouched and no threads are introduced.

**Tech Stack:** Existing workspace only. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-09-06-transport-events-design.md`

## Global Constraints

- The `Transport` trait is not modified in any way.
- `ReplayTransport`'s byte-for-byte outbound matching is not loosened in any way.
- No threads, no channels, no async.
- Events never change what a command writes or whether it proceeds; the only CLI effect is the
  stderr note, worded exactly as in Task 3, at most once per edge kind per run.
- The `0xbe` byte shapes come from `captures/board-side-ap-change.jsonl` and are embedded as
  literals in fixtures, since `captures/` is gitignored and absent in CI. The two frames open
  `5c 03 80 14 00 be 00` and `5c 03 80 15 00 be 01`, zero-padded to 64 bytes; Task 1 gives them
  as code and they must be used verbatim.
- Comments two lines by default, four max; no task numbers in comments; no em or en dashes
  anywhere; commit messages one line `[type] - Message`, no trailers.
- Multi-part edits are written one file-write per edit, never batched behind a final assert.

---

### Task 1: `BoardEvent` in `wh-proto`

**Files:**
- Create: `crates/wh-proto/src/event.rs`
- Modify: `crates/wh-proto/src/lib.rs` (add `pub mod event;`)

**Interfaces:**
- Consumes: `wh_proto::frame::{parse, REPLY_BIT}` (existing; `parse` returns
  `Reply { cmd: u8, payload: &[u8] }`).
- Produces (Tasks 2 and 3 rely on these exact names):
  `pub enum BoardEvent { AdjustModeEntered, AdjustModeLeft, Unknown(Vec<u8>) }` (derives
  `Debug, Clone, PartialEq, Eq`);
  `pub fn adjust_event(report: &[u8; 64]) -> Option<BoardEvent>` returning `Some` only for the
  known `0xbe` shapes;
  `pub fn any_event(report: &[u8; 64]) -> BoardEvent` for idle-context frames, mapping non-`0xbe`
  frames to `BoardEvent::Unknown(payload.to_vec())` (and unparseable reports to
  `Unknown(report.to_vec())`).

- [ ] **Step 1: Write the failing tests**

In `crates/wh-proto/src/event.rs` (module plus tests in one new file, the crate's house pattern):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The two 0xbe frames, byte for byte as the board sent them in
    /// `captures/board-side-ap-change.jsonl` (embedded because captures/ is gitignored).
    fn entering() -> [u8; 64] {
        let mut f = [0u8; 64];
        f[..7].copy_from_slice(&[0x5c, 0x03, 0x80, 0x14, 0x00, 0xbe, 0x00]);
        f
    }
    fn leaving() -> [u8; 64] {
        let mut f = [0u8; 64];
        f[..7].copy_from_slice(&[0x5c, 0x03, 0x80, 0x15, 0x00, 0xbe, 0x01]);
        f
    }

    #[test]
    fn adjust_event_parses_both_measured_edges() {
        assert_eq!(adjust_event(&entering()), Some(BoardEvent::AdjustModeEntered));
        assert_eq!(adjust_event(&leaving()), Some(BoardEvent::AdjustModeLeft));
    }

    #[test]
    fn adjust_event_ignores_an_ordinary_reply() {
        // A bd poll reply, the commonest frame on the wire: cmd 0x80, payload 00 bd 01 ff.
        let mut f = [0u8; 64];
        f[..8].copy_from_slice(&[0x5c, 0x04, 0x80, 0x53, 0x00, 0xbd, 0x01, 0xff]);
        assert_eq!(adjust_event(&f), None);
    }

    #[test]
    fn adjust_event_keeps_an_unmeasured_third_byte_out_of_the_known_edges() {
        // be 02 has never been observed; it must not read as either measured edge.
        let mut f = [0u8; 64];
        f[..7].copy_from_slice(&[0x5c, 0x03, 0x80, 0x16, 0x00, 0xbe, 0x02]);
        assert_eq!(adjust_event(&f), None);
        assert_eq!(any_event(&f), BoardEvent::Unknown(vec![0x00, 0xbe, 0x02]));
    }

    #[test]
    fn any_event_wraps_a_non_be_frame_as_unknown_with_its_payload() {
        let mut f = [0u8; 64];
        f[..8].copy_from_slice(&[0x5c, 0x04, 0x80, 0x53, 0x00, 0xbd, 0x01, 0xff]);
        assert_eq!(any_event(&f), BoardEvent::Unknown(vec![0x00, 0xbd, 0x01, 0xff]));
    }

    #[test]
    fn any_event_wraps_an_unparseable_report_whole() {
        let f = [0xffu8; 64];
        assert_eq!(any_event(&f), BoardEvent::Unknown(f.to_vec()));
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p wh-proto event`
Expected: FAIL to compile, `event` module not found.

- [ ] **Step 3: Implement**

```rust
//! Frames the board sends without being asked. Measured 2026-09-04: `cmd 0x00` sub-order `0xbe`
//! announces the board's own adjust mode, `be 00` entering and `be 01` leaving
//! (`docs/protocol.md`, "The board announces its own adjust mode").

use crate::frame::{parse, REPLY_BIT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardEvent {
    AdjustModeEntered,
    AdjustModeLeft,
    /// A device-initiated frame this build does not recognise. Kept, never dropped: the corpus
    /// proves the board volunteers frames, and a new one must surface rather than vanish.
    Unknown(Vec<u8>),
}

/// The two measured adjust-mode edges, and nothing else: an unmeasured third byte is not an
/// edge. Callers in a roundtrip use this, so only certainly-unsolicited frames leave the reply
/// path.
pub fn adjust_event(report: &[u8; 64]) -> Option<BoardEvent> {
    let reply = parse(report).ok()?;
    if reply.cmd != REPLY_BIT && reply.cmd != (0x00 | REPLY_BIT) {
        return None;
    }
    match reply.payload {
        [0x00, 0xbe, 0x00, ..] => Some(BoardEvent::AdjustModeEntered),
        [0x00, 0xbe, 0x01, ..] => Some(BoardEvent::AdjustModeLeft),
        _ => None,
    }
}

/// Any frame received with nothing awaited is device-initiated by definition: the known edges
/// parse as themselves, everything else is `Unknown` carrying its payload (or the whole report
/// when it does not even frame).
pub fn any_event(report: &[u8; 64]) -> BoardEvent {
    if let Some(e) = adjust_event(report) {
        return e;
    }
    match parse(report) {
        Ok(reply) => BoardEvent::Unknown(reply.payload.to_vec()),
        Err(_) => BoardEvent::Unknown(report.to_vec()),
    }
}
```

Note `0x00 | REPLY_BIT` == `REPLY_BIT`; write the single form `reply.cmd != REPLY_BIT` and let
the comment say it means "a `cmd 0x00` reply frame". The double form above is illustrative only.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p wh-proto event`
Expected: PASS, 5 tests.

- [ ] **Step 5: Mutation checks, then commit**

Mutate `[0x00, 0xbe, 0x01, ..]` to also match `0x00` (collapse the arms): the two-edge test
fails. Mutate `any_event`'s unknown arm to return `Unknown(vec![])`: the payload test fails.
Restore both by inverse edit, verify with `git diff`, then:

```bash
git add crates/wh-proto/src/event.rs crates/wh-proto/src/lib.rs
git commit -m "[feat] - Model the board's unsolicited adjust-mode frames as BoardEvent"
```

---

### Task 2: the event queue and `poll_event` on `Session`

**Files:**
- Modify: `crates/wh-device/src/session.rs` (struct at :13-24, `roundtrip` at :26-50)

**Interfaces:**
- Consumes: `wh_proto::event::{adjust_event, any_event, BoardEvent}` from Task 1.
- Produces (Task 3 relies on these):
  `Session::poll_event(&mut self, timeout: Duration) -> Result<Option<BoardEvent>, DeviceError>`;
  `Session::pending_events(&mut self) -> std::collections::vec_deque::Drain<'_, BoardEvent>`.

**Decisions pinned here** (the spec left them to this plan):
- In a roundtrip, only `adjust_event` frames are routed to the queue; other mismatched frames
  keep today's skip, since a stray frame mid-roundtrip may be a late reply and calling it
  device-initiated would be an inference. The routing check runs BEFORE the reply-cmd match:
  a `0xbe` frame during a `cmd 0x00` roundtrip would otherwise satisfy `cmd == 0x80` and be
  returned as the awaited reply, a live mis-match hazard found while planning.
- In `poll_event`, nothing is awaited, so every received frame is an event: the known edges as
  themselves, everything else through `any_event` as `Unknown`. Timeout is `Ok(None)`.
- `ReplayTransport` needs no change: `recv` already returns an `In` entry at the cursor with no
  preceding send (`crates/wh-device/src/replay.rs:101-113`), and script exhaustion returns
  `Timeout`, which `poll_event` maps to `Ok(None)`. A `poll_event` against a script whose cursor
  sits at an `Out` entry stays a loud replay error, deliberately: in 3.4 nothing polls
  mid-script, and if the TUI's loop needs "no event pending, next is my send" in 3.5, that is a
  scripting question to solve then, not silently now.

- [ ] **Step 1: Write the failing tests**

In `crates/wh-device/src/session.rs`'s test module (it exists; follow its `Entry`-building
helpers in `replay.rs` tests, or construct `ReplayTransport` from JSONL text as the existing
tests do):

```rust
// The 0xbe edge frames, byte for byte from captures/board-side-ap-change.jsonl.
fn be_entering() -> [u8; 64] { /* as Task 1's entering() */ }
fn be_leaving() -> [u8; 64] { /* as Task 1's leaving() */ }

#[test]
fn poll_event_returns_a_scripted_edge_and_then_none_on_exhaustion() {
    // Script: one inbound 0xbe frame, nothing else.
    let mut s = Session::new(replay_with(vec![Entry::In(be_entering())]));
    assert_eq!(
        s.poll_event(Duration::from_millis(1)).unwrap(),
        Some(BoardEvent::AdjustModeEntered)
    );
    assert_eq!(s.poll_event(Duration::from_millis(1)).unwrap(), None);
}

#[test]
fn poll_event_wraps_a_non_be_frame_as_unknown() {
    let mut s = Session::new(replay_with(vec![Entry::In(bd_reply())]));
    match s.poll_event(Duration::from_millis(1)).unwrap() {
        Some(BoardEvent::Unknown(p)) => assert_eq!(p, vec![0x00, 0xbd, 0x01, 0xff]),
        other => panic!("wanted Unknown, got {other:?}"),
    }
}

#[test]
fn roundtrip_queues_an_edge_and_still_matches_its_reply() {
    // Script: out request, in 0xbe edge, in real reply. The edge arrives mid-roundtrip.
    let req = wh_proto::cmds::read_profile();
    let mut s = Session::new(replay_with(vec![
        Entry::Out(req),
        Entry::In(be_entering()),
        Entry::In(profile_reply_frame()),
    ]));
    let payload = s.roundtrip(&req).unwrap();
    assert_eq!(payload[..3], [0x00, 0x70, 0x00]);
    let events: Vec<_> = s.pending_events().collect();
    assert_eq!(events, vec![BoardEvent::AdjustModeEntered]);
}

#[test]
fn roundtrip_does_not_return_an_edge_as_a_cmd_zero_reply() {
    // The hazard: a bd poll's reply match is cmd 0x80, which a 0xbe frame also carries. The
    // edge must be queued and the real bd reply returned, not the edge returned as the reply.
    let req = wh_proto::cmds::poll_bd(); // whatever encoder sends payload bd 01 ff ff; if none
                                         // exists, hand-build the request frame as the captures
                                         // show it: 5c 04 00 53 bd 01 ff ff zero-padded.
    let mut s = Session::new(replay_with(vec![
        Entry::Out(req),
        Entry::In(be_entering()),
        Entry::In(bd_reply()),
    ]));
    let payload = s.roundtrip(&req).unwrap();
    assert_eq!(payload[..2], [0x00, 0xbd]);
    assert_eq!(s.pending_events().collect::<Vec<_>>(), vec![BoardEvent::AdjustModeEntered]);
}

#[test]
fn pending_events_drains_once() {
    let mut s = Session::new(replay_with(vec![Entry::In(be_leaving())]));
    let _ = s.poll_event(Duration::from_millis(1)).unwrap();
    // poll_event returned it; the queue holds nothing, and a second drain is empty.
    assert_eq!(s.pending_events().count(), 0);
}
```

Where `replay_with`, `bd_reply` and `profile_reply_frame` are small local helpers; build frames
with the real checksum rule `(0x35 + 0x5C + len + cmd + payload.last()) & 0xFF` or reuse the
crate's existing test helpers (grep `fn reply(` in `crates/wh-device/src`, they exist).

If `Entry` is private to `replay.rs`, construct the transport from JSONL text via
`ReplayTransport::from_jsonl` exactly as `session.rs`'s existing tests do; the script lines are
`{"dir":"in","data":"<hex>"}` / `{"dir":"out","data":"<hex>"}`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p wh-device --no-fail-fast poll_event roundtrip_queues pending_events`
Expected: FAIL to compile, `poll_event` not found.

- [ ] **Step 3: Implement**

In `Session`: add `events: std::collections::VecDeque<wh_proto::event::BoardEvent>` (init in
`new`). In `roundtrip`'s loop, after a successful `recv` and before `parse`'s reply match:

```rust
if let Some(e) = wh_proto::event::adjust_event(&report) {
    self.events.push_back(e);
    continue;
}
```

New methods:

```rust
/// One bounded listen for a device-initiated frame. Drains queued events first; a quiet wire
/// is `Ok(None)`, the normal idle case.
pub fn poll_event(
    &mut self,
    timeout: Duration,
) -> Result<Option<wh_proto::event::BoardEvent>, DeviceError> {
    if let Some(e) = self.events.pop_front() {
        return Ok(Some(e));
    }
    match self.t.recv(timeout) {
        Ok(report) => Ok(Some(wh_proto::event::any_event(&report))),
        Err(DeviceError::Timeout) => Ok(None),
        Err(e) => Err(e),
    }
}

/// What arrived uninvited while this session worked, drained: a caller reports each event
/// once, and a second call answers nothing.
pub fn pending_events(
    &mut self,
) -> std::collections::vec_deque::Drain<'_, wh_proto::event::BoardEvent> {
    self.events.drain(..)
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p wh-device --no-fail-fast`
Expected: PASS, all existing tests untouched plus the five new.

- [ ] **Step 5: Mutation checks, then commit**

Mutate the routing to run AFTER the reply-cmd match: `roundtrip_does_not_return_an_edge_as_a_cmd_zero_reply`
fails with the edge payload returned. Mutate the routing away entirely (restore the plain skip):
`roundtrip_queues_an_edge_and_still_matches_its_reply` fails on the empty drain. Restore by
inverse edit, verify with `git diff`, then:

```bash
git add crates/wh-device/src/session.rs
git commit -m "[feat] - Queue the board's unsolicited edges in Session and add poll_event"
```

---

### Task 3: the once-per-kind stderr note, end to end

**Files:**
- Modify: `crates/wh-cli/src/run.rs` (`with_session` at :49-76)
- Test: `crates/wh-cli/tests/dump.rs`
- Modify: `docs/tasks.md` (close 3.4), `README.md` (one paragraph on the note)

**Interfaces:**
- Consumes: `Session::pending_events()` from Task 2.

- [ ] **Step 1: Write the failing tests**

In `crates/wh-cli/tests/dump.rs`, following the house replay-script helpers:

```rust
/// An 0xbe edge arriving mid-command surfaces as exactly one stderr note, the command's own
/// work is untouched, and stdout carries no trace of it.
#[test]
fn a_mid_command_adjust_edge_prints_one_stderr_note_and_changes_nothing_else() {
    // Script: wh get ap --keys w, with the entering edge injected between the matrix read's
    // request and its reply (an In line the command never asked for).
    // ... build matrix_lines() for w, then splice Entry::In(be_entering) before one reply ...
    let out = run_wh(&["get", "ap", "--keys", "w"], &path, &config_home);
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        stderr.matches("note: the board entered its own adjust mode during this command; settings may have changed underneath it").count(),
        1,
        "got: {stderr}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("adjust mode"), "the note must stay off stdout: {stdout}");
}

/// Two edges of the same kind still print one note; both kinds print one each.
#[test]
fn repeated_edges_of_one_kind_print_once_and_both_kinds_print_one_each() {
    // Script carries be 00, be 00, be 01 spliced across the command's replies.
    // Assert entered-note count == 1, left-note count == 1.
}

/// The negative half: a command whose script carries no edge prints no note.
#[test]
fn a_command_with_no_edges_prints_no_adjust_note() {
    // Reuse any passing fixture's script; assert !stderr.contains("adjust mode").
}
```

The splice point matters: the edge goes between an outbound request and its scripted reply, which
is exactly where `roundtrip` will read it, matching how the real board interleaves (measured: the
edges arrived alone, but mid-roundtrip arrival is what the discard bug loses today).

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p wh-cli --test dump adjust`
Expected: the note tests fail (no note printed); the negative test passes trivially, which is
fine, its job is guarding the others later.

- [ ] **Step 3: Implement**

In `with_session`, after `f(&mut s)` returns and regardless of its result:

```rust
let result = f(&mut s);
let mut entered = false;
let mut left = false;
for e in s.pending_events() {
    match e {
        wh_proto::event::BoardEvent::AdjustModeEntered => entered = true,
        wh_proto::event::BoardEvent::AdjustModeLeft => left = true,
        wh_proto::event::BoardEvent::Unknown(_) => {}
    }
}
if entered {
    best_effort_eprintln(
        "note: the board entered its own adjust mode during this command; settings may have \
         changed underneath it",
    );
}
if left {
    best_effort_eprintln(
        "note: the board left its own adjust mode during this command; settings may have \
         changed underneath it",
    );
}
result
```

`Unknown` events are intentionally not printed by one-shot commands: nothing in the corpus
produces one mid-roundtrip (only `0xbe` shapes are routed there), and `poll_event`'s consumers
own them in 3.5. Say that in a two-line comment.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p wh-cli --test dump adjust`, then the full four gates:

```
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
scripts/check-doc-repeats.py $(git ls-files '*.md' ':!research/*')
```

Then rebuild `cargo build -p wh-cli --release --target x86_64-pc-windows-gnu` and re-run the
workspace, since the shim test executes the real `wh.exe`.

- [ ] **Step 5: Mutation checks**

Print per-frame instead of per-kind (drop the boolean dedupe): the repeated-edges test fails.
Route the drain to stdout: the stays-off-stdout assertion fails. Remove the drain call: both
positive tests fail. Restore each by inverse edit, verify with `git diff`.

- [ ] **Step 6: Docs and commit**

Close 3.4 in `docs/tasks.md` (strike the entry, note the mid-roundtrip mis-match hazard found
and fixed, and that the hardware check remains for the operator: FN+AP with `poll_event` running,
expected both edges, to be run before 3.5 relies on it). Add one README paragraph beside the
existing "board can change under you" material describing the note and what it means. Check
`docs/protocol.md`'s adjust-mode section needs no change (it should not; the design cites it).

```bash
git add crates/wh-cli/src/run.rs crates/wh-cli/tests/dump.rs docs/tasks.md README.md
git commit -m "[feat] - Report the board's adjust-mode edges after every command, once per kind"
```
