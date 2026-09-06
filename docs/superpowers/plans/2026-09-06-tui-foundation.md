# TUI Foundation (3.5, plan 1 of 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** A read-only `wh tui` that opens the board, renders the vendor configurator's full frame
with live values, tracks the board's own adjust mode, and quits cleanly; every write path is an
honest stub for plan 2.

**Architecture:** A new `wh-tui` library crate holding a single-threaded ratatui app (App state,
pure draw functions, a BoardModel read over the existing `wh-device` ops), launched from a new
`wh tui` subcommand in `wh-cli`. Two foundations land in `wh-device` first: a `wait` entry in the
replay script format so a polling loop is scriptable, and a row-preserving matrix read so the key
grid renders from board-derived geometry.

**Tech Stack:** Rust, ratatui 0.29, crossterm 0.28, the existing wh-proto and wh-device crates.
(`wh-config` was declared here at first and never used: nothing in this plan touches a snapshot,
so it was dropped, and the editing plan re-adds it when the backup lands.)

**Spec:** `docs/superpowers/specs/2026-09-06-tui-design.md`. 3.5 executes as two plans; this is
plan 1. Plan 2 (editing: steppers that write, the keyset gesture, SOCD editor, profile switch,
typed-yes modals, the one `auto: tui` backup) is written after this plan lands, against its real
code.

## Global Constraints

- No em dashes or en dashes anywhere: code, comments, docs, commit messages.
- Commit messages: one line, `[type] - Message`, no body, no trailers of any kind.
- Every test fixture frame is built through `wh_proto::frame::frame` or a `wh_proto::cmds`
  encoder. Never hand-type frame hex; two hand-typed checksums have both been wrong here.
- Assert output by whole line (exact string equality per line, or a named constant), never by
  `contains` on a fragment something else can also emit.
- Never loosen `ReplayTransport`'s byte-for-byte frame matching.
- `wh-tui` never encodes frames by hand and never opens a transport itself; it receives a
  `Session` and calls `wh-device` ops.
- The banner line is exactly `WALLHACK TERMINAL BY "@BRUX" - V<wh version>` (operator's ruling,
  2026-09-06).
- The TUI renders Wallhack's own mark, extracted verbatim from the vendor bundle, used with
  Wallhack's permission (held by the operator, stated 2026-09-06): the earlier own-mark ruling
  in this bullet is overturned. The logo block in Task 4 must be the vendor's own bytes, never
  redrawn.
- Gates before every commit: `cargo test --workspace --no-fail-fast`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`.
- This plan performs no device writes anywhere. Every fixture script is reads only.

---

### Task 1: The `wait` replay entry

`Session::poll_event` against a script whose cursor sits on an `Out` entry is a loud
`DeviceError::Replay`, so a poll-then-read loop cannot be scripted. A `wait` entry says "the next
recv gets nothing": `recv` returns `Err(DeviceError::Timeout)` (which `poll_event` maps to
`Ok(None)`) and the entry is consumed. `send` at a `wait` entry stays a loud error.

**Files:**
- Modify: `crates/wh-device/src/replay.rs`

**Interfaces:**
- Consumes: existing `Entry`, `parse_jsonl`, `ReplayTransport`, `hex`.
- Produces: `Entry::Wait(u32)`; JSONL forms `{"dir":"wait"}` (count 1) and
  `{"dir":"wait","count":N}` with N >= 1. Later tasks' scripts rely on this exact shape.

- [ ] **Step 1: Write the failing tests** (in `replay.rs`'s test module, alongside the existing
  ones; build frames with the module's existing frame helpers, or
  `wh_proto::frame::frame(0x80, &[0x00, 0xbe, 0x00]).unwrap()` for an event frame):

```rust
#[test]
fn wait_entry_serves_one_timeout_then_advances() {
    let f = wh_proto::frame::frame(0x80, &[0x00, 0xbe, 0x00]).unwrap();
    let script = format!("{{\"dir\":\"wait\"}}\n{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(&f));
    let mut t = ReplayTransport::from_jsonl(&script).unwrap();
    assert!(matches!(t.recv(Duration::from_millis(1)), Err(DeviceError::Timeout)));
    assert_eq!(t.recv(Duration::from_millis(1)).unwrap(), f);
    assert!(t.finished());
}

#[test]
fn counted_wait_serves_that_many_timeouts() {
    // A trailing `In` entry after the wait, and a `finished()` check at every step, so an
    // implementation that consumes the whole `Wait` on the first `recv` cannot pass by having
    // its extra recv calls land on an exhausted script (which is Timeout too).
    let f = wh_proto::frame::frame(0x80, &[0x00, 0xbe, 0x00]).unwrap();
    let script = format!(
        "{{\"dir\":\"wait\",\"count\":3}}\n{{\"dir\":\"in\",\"hex\":\"{}\"}}",
        hex(&f)
    );
    let mut t = ReplayTransport::from_jsonl(&script).unwrap();
    assert!(!t.finished());
    assert!(matches!(t.recv(Duration::from_millis(1)), Err(DeviceError::Timeout)));
    assert!(!t.finished());
    assert!(matches!(t.recv(Duration::from_millis(1)), Err(DeviceError::Timeout)));
    assert!(!t.finished());
    assert!(matches!(t.recv(Duration::from_millis(1)), Err(DeviceError::Timeout)));
    assert!(!t.finished());
    assert_eq!(t.recv(Duration::from_millis(1)).unwrap(), f);
    assert!(t.finished());
}

#[test]
fn send_at_a_wait_entry_is_a_loud_replay_error() {
    let mut t = ReplayTransport::from_jsonl("{\"dir\":\"wait\"}").unwrap();
    let out = wh_proto::cmds::read_profile();
    match t.send(&out) {
        Err(DeviceError::Replay(msg)) => assert_eq!(
            msg,
            "unexpected send at 0: script expects 1 more empty polls here"
        ),
        other => panic!("expected Replay error, got {other:?}"),
    }
}

#[test]
fn wait_count_zero_is_rejected_at_parse() {
    assert!(matches!(
        ReplayTransport::from_jsonl("{\"dir\":\"wait\",\"count\":0}"),
        Err(DeviceError::Replay(_))
    ));
}

#[test]
fn poll_event_over_a_wait_script_returns_none_then_the_edge() {
    let f = wh_proto::frame::frame(0x80, &[0x00, 0xbe, 0x00]).unwrap();
    let script = format!("{{\"dir\":\"wait\"}}\n{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(&f));
    let t = ReplayTransport::from_jsonl(&script).unwrap();
    let mut s = crate::session::Session::new(t);
    assert!(s.poll_event(Duration::from_millis(1)).unwrap().is_none());
    assert!(matches!(
        s.poll_event(Duration::from_millis(1)).unwrap(),
        Some(wh_proto::event::BoardEvent::AdjustModeEntered)
    ));
}
```

- [ ] **Step 2: Run them, watch them fail**

Run: `cargo test -p wh-device wait_entry`
Expected: FAIL to compile or parse error at `{"dir":"wait"}` (unknown dir), which is the current
behaviour the task changes.

- [ ] **Step 3: Implement**

In `Entry`, add `Wait(u32)`. In the JSONL parser, accept `"dir":"wait"` with an optional
`"count"` field defaulting to 1 and rejecting 0 (`DeviceError::Replay("wait count 0 at line N")`
in the parser's existing error style). In the `Transport` impl:

- `recv` at an `Entry::Wait(n)` cursor: if `n > 1`, decrement it in place; if `n == 1`, advance
  the cursor. Either way return `Err(DeviceError::Timeout)`.
- `send` at an `Entry::Wait(_)` cursor: return the same "unexpected send at {pos}" `Replay` error
  shape the `In`-cursor case uses, with wording that names the wait (for example
  `unexpected send at {pos}: script expects {n} more empty polls here`).
- The JSONL writer (`Entry` to line, used by `RecordingTransport::jsonl`) gains a
  `Wait(n)` arm producing `{"dir":"wait","count":N}`. `RecordingTransport` itself keeps
  recording only real sends and receives; it never synthesises `Wait` entries, and a one-line
  comment on the writer arm says so.
- `finished()` needs no change: a pending `Wait` means not finished, which `pos == len` already
  expresses.

Note `recv`'s mutation: `Wait` is the first entry `recv` edits rather than only advancing past.
Keep the cursor logic in one match so the three cursor cases (In, Out, Wait) read together.

- [ ] **Step 4: Run the tests, watch them pass**

Run: `cargo test -p wh-device`
Expected: PASS, including every pre-existing replay test unchanged.

- [ ] **Step 5: Gates, then commit**

```bash
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add crates/wh-device/src/replay.rs
git commit -m "[feat] - Add a wait entry to replay scripts so polling loops are scriptable"
```

---

### Task 2: Row-preserving matrix read

The TUI's key grid needs the board's own row and column placement, which `parse_defkey` already
decodes and `ops::read_matrix` currently flattens away.

**Files:**
- Modify: `crates/wh-device/src/ops.rs`

**Interfaces:**
- Consumes: `cmds::{read_defkey_rows, parse_defkey, DefKeyRow, MATRIX_ROWS}`.
- Produces: `pub fn read_matrix_rows<T: Transport>(s: &mut Session<T>) -> Result<Vec<DefKeyRow>, DeviceError>`
  sending exactly the same three DEFKEY roundtrips as `read_matrix`, returning the six decoded
  rows in wire order with their column data intact. `read_matrix` becomes a flatten over it, so
  the two can never disagree on order.

- [ ] **Step 1: Write the failing test** (in `ops.rs`'s test module or wh-device's existing test
  home for ops; build the script with the same encoder-built fixtures the existing matrix tests
  use):

```rust
#[test]
fn read_matrix_rows_preserves_rows_and_matches_read_matrix_order() {
    // Two scripts with identical frames: read_matrix_rows consumes one, read_matrix the other.
    let lines = matrix_script_two_keys(); // this module's existing encoder-built DEFKEY fixture
    let mut s = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
    let rows = read_matrix_rows(&mut s).unwrap();
    let mut s2 = Session::new(ReplayTransport::from_jsonl(&lines).unwrap());
    let flat = read_matrix(&mut s2).unwrap();
    let flattened: Vec<u8> = rows.iter().flat_map(|r| r.keys.iter().map(|&(_, u)| u)).collect();
    assert_eq!(flattened, flat);
    assert_eq!(rows.len(), 6);
}
```

If no encoder-built DEFKEY fixture helper exists in wh-device's tests yet, write one in the test
module the same way `crates/wh-cli/tests/dump.rs` builds `matrix_lines()` (three
`cmds::read_defkey_rows` requests, each answered by a `cmds::cmd::DEFKEY` reply built through
`wh_proto::frame::frame` with the reply bit set). Do not hand-type hex.

Check `DefKeyRow`'s tuple order in `cmds.rs` before writing the flatten: the pair is as
`parse_defkey` builds it, and the test must destructure it the same way.

- [ ] **Step 2: Run it, watch it fail**

Run: `cargo test -p wh-device read_matrix_rows`
Expected: FAIL, `read_matrix_rows` not found.

- [ ] **Step 3: Implement**

Lift `read_matrix`'s body into `read_matrix_rows` (same three roundtrips, keep the decoded
`DefKeyRow`s instead of flattening), then reimplement `read_matrix` as the flatten of
`read_matrix_rows`. The wire traffic is byte-identical, so every existing replay fixture still
matches; if any fixture stops matching, the change is wrong, not the fixture.

- [ ] **Step 4: Run the tests, watch them pass**

Run: `cargo test -p wh-device`
Expected: PASS, including every existing test that scripts a matrix read.

- [ ] **Step 5: Gates, then commit**

```bash
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add crates/wh-device/src/ops.rs
git commit -m "[feat] - Expose the DEFKEY rows from the matrix read for board-derived geometry"
```

---

### Task 3: The wh-tui crate and the `wh tui` subcommand

**Files:**
- Create: `crates/wh-tui/Cargo.toml`, `crates/wh-tui/src/lib.rs`, `crates/wh-tui/src/app.rs`
- Modify: `Cargo.toml` (workspace members), `crates/wh-cli/Cargo.toml`,
  `crates/wh-cli/src/cli.rs`, `crates/wh-cli/src/run.rs`
- Test: `crates/wh-tui/src/app.rs` (unit), `crates/wh-cli/tests/dump.rs` (one refusal test)

**Interfaces:**
- Consumes: `wh_device::{session::Session, transport::Transport}`.
- Produces: `wh_tui::run<T: Transport>(session: &mut Session<T>, wh_version: &str) -> anyhow::Result<()>`
  (terminal setup, loop, teardown; borrowing, so it composes with `with_session`); `wh_tui::app::App` with `pub fn new(wh_version: &str) -> Self`,
  `pub fn banner(&self) -> String`, `pub fn handle_key(&mut self, code: KeyCode)` (quit on `q`,
  `Esc`, `Ctrl+c` handled in `run`), `pub quit: bool`; `wh_tui::app::draw(f: &mut Frame, app: &mut App)`.
  Task 4 extends `App::new` to take the board model; later tasks extend `draw`.

- [ ] **Step 1: Scaffold the crate**

`crates/wh-tui/Cargo.toml`:

```toml
[package]
name = "wh-tui"
version = "0.1.0"
edition.workspace = true
license.workspace = true
publish = false

[dependencies]
wh-proto = { path = "../wh-proto" }
wh-device = { path = "../wh-device" }
anyhow = "1"
ratatui = "0.29"
crossterm = "0.28"
```

Add `"crates/wh-tui"` to the workspace `members`. Add `wh-tui = { path = "../wh-tui" }` to
`crates/wh-cli/Cargo.toml`.

- [ ] **Step 2: Write the failing unit test** (in `app.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Every rendered line of the buffer, right-trimmed, so tests assert whole lines.
    pub(crate) fn buffer_lines(terminal: &Terminal<TestBackend>) -> Vec<String> {
        let buf = terminal.backend().buffer().clone();
        let area = buf.area;
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn the_banner_line_renders_whole_and_exact() {
        let mut app = App::new("0.5.0-alpha");
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let lines = buffer_lines(&terminal);
        assert!(
            lines.iter().any(|l| l == "WALLHACK TERMINAL BY \"@BRUX\" - V0.5.0-alpha"),
            "banner missing or wrong: {lines:?}"
        );
    }

    #[test]
    fn q_and_esc_quit() {
        let mut app = App::new("x");
        app.handle_key(crossterm::event::KeyCode::Char('q'));
        assert!(app.quit);
        let mut app = App::new("x");
        app.handle_key(crossterm::event::KeyCode::Esc);
        assert!(app.quit);
    }
}
```

- [ ] **Step 3: Run it, watch it fail**

Run: `cargo test -p wh-tui`
Expected: FAIL to compile, `App` undefined.

- [ ] **Step 4: Implement `app.rs` and `lib.rs`**

`app.rs`:

```rust
use crossterm::event::KeyCode;
use ratatui::prelude::*;

/// The project's own mark, not the vendor's: Wallhack's logo is Wallhack's.
pub const LOGO: &[&str] = &[
    "00     00  00   00",
    "00  0  00  00   00",
    "00 000 00  0000000",
    "0000 0000  00   00",
    " 000 000   00   00",
    "  00 00    00   00",
];

pub struct App {
    pub wh_version: String,
    pub quit: bool,
}

impl App {
    pub fn new(wh_version: &str) -> Self {
        Self { wh_version: wh_version.to_string(), quit: false }
    }

    pub fn banner(&self) -> String {
        format!("WALLHACK TERMINAL BY \"@BRUX\" - V{}", self.wh_version)
    }

    pub fn handle_key(&mut self, code: KeyCode) {
        if matches!(code, KeyCode::Char('q') | KeyCode::Esc) {
            self.quit = true;
        }
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let mut y = 0u16;
    for line in LOGO {
        f.render_widget(Line::raw(*line), Rect::new(0, y, area.width, 1));
        y += 1;
    }
    y += 1;
    f.render_widget(Line::raw(app.banner()), Rect::new(0, y, area.width, 1));
    y += 2;
    f.render_widget(
        Line::raw("NAVIGATE WITH MOUSE OR ARROW & ENTER KEYS"),
        Rect::new(0, y, area.width, 1),
    );
}
```

`lib.rs`:

```rust
pub mod app;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyEventKind, KeyModifiers};
use std::time::Duration;
use wh_device::session::Session;
use wh_device::transport::Transport;

pub fn run<T: Transport>(session: &mut Session<T>, wh_version: &str) -> Result<()> {
    let _ = session; // the board model read arrives in the next task
    let mut terminal = match ratatui::try_init() {
        Ok(t) => t,
        Err(e) => {
            ratatui::restore();
            return Err(e).context("could not enter the alternate screen");
        }
    };
    let result = event_loop(&mut terminal, wh_version);
    // Always restore, whether the loop returned an error or not.
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, wh_version: &str) -> Result<()> {
    let mut app = app::App::new(wh_version);
    while !app.quit {
        terminal.draw(|f| app::draw(f, &mut app))?;
        if event::poll(Duration::from_millis(15))? {
            if let Event::Key(k) = event::read()? {
                if matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    if k.modifiers.contains(KeyModifiers::CONTROL)
                        && k.code == event::KeyCode::Char('c')
                    {
                        app.quit = true;
                    } else {
                        app.handle_key(k.code);
                    }
                }
            }
        }
    }
    Ok(())
}
```

Wire the subcommand. In `cli.rs`, add to `Cmd`:

```rust
/// Open the full-screen terminal UI (read-only in this phase)
Tui,
```

In `run.rs`, add the dispatch arm `Cmd::Tui => tui_cmd(),` and the handler, following `picker`'s
own terminal refusal (`crates/wh-cli/src/picker.rs` has the pattern):

```rust
fn tui_cmd() -> Result<()> {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        bail!("wh tui needs an interactive terminal, but stdout here is redirected or piped");
    }
    with_session(|s| wh_tui::run(s, env!("CARGO_PKG_VERSION")).map_err(Into::into))
}
```

The public API is the borrowing form,
`pub fn run<T: Transport>(session: &mut Session<T>, wh_version: &str) -> Result<()>`: it composes
with `with_session` directly, and `with_session`'s existing event drain and note printing still
run after the TUI exits, so edges the TUI did not consume surface as the CLI's own stderr notes
after quit. Task 9 relies on that drain still running.

- [ ] **Step 5: Add the refusal test to `crates/wh-cli/tests/dump.rs`** (the binary under test
  runs with piped stdout, so this is deterministic):

```rust
#[test]
fn tui_refuses_without_an_interactive_terminal() {
    let dir = scratch_config_dir("tui-refuse");
    let script = write_script("tui-refuse", &[]);
    let out = run_wh(&["tui"], &script, &dir);
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.lines().any(|l| l
            == "Error: wh tui needs an interactive terminal, but stdout here is redirected or piped"),
        "unexpected stderr: {stderr}"
    );
}
```

Check how anyhow errors print in this binary before finalising the expected line (run any failing
command once and read its stderr shape); the assertion must be the whole line the operator sees.
The error line alone is not enough, and the shipped test says so: it also asserts stderr carries no
`transport:` line, since moving this check inside `with_session` would print the same refusal
having first taken the exclusive vendor HID collection.

- [ ] **Step 6: Run the tests, watch them pass**

Run: `cargo test -p wh-tui && cargo test -p wh-cli --test dump`
Expected: PASS. Remember the scoped-run warning: only the workspace run proves the crates compile.

- [ ] **Step 7: Gates, then commit**

```bash
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add Cargo.toml crates/wh-tui crates/wh-cli/Cargo.toml crates/wh-cli/src/cli.rs crates/wh-cli/src/run.rs crates/wh-cli/tests/dump.rs
git commit -m "[feat] - Scaffold the wh-tui crate and the wh tui subcommand"
```

---

### Task 4: BoardModel, the full read

**Files:**
- Create: `crates/wh-tui/src/board.rs`, `crates/wh-tui/tests/support/mod.rs`,
  `crates/wh-tui/tests/board.rs`
- Modify: `crates/wh-tui/src/lib.rs` (declare module), `crates/wh-tui/src/app.rs` (App holds the
  model)

**Interfaces:**
- Consumes: `wh_device::ops::{device_info, profile, global_travel, read_matrix_rows,
  read_key_settings, KeySettings}`, `wh_proto::cmds::{DefKeyRow, GlobalTravel, ProfileNumber}`.
- Produces:

```rust
pub struct BoardModel {
    pub serial: String,
    pub firmware: String,
    pub profile: ProfileNumber,
    pub global: GlobalTravel,
    pub rows: Vec<DefKeyRow>,
    pub keys: Vec<KeySettings>, // matrix order, same flatten as ops::read_matrix
}
impl BoardModel {
    pub fn read<T: Transport>(s: &mut Session<T>) -> Result<Self, DeviceError>;
    pub fn key(&self, usage: u8) -> Option<&KeySettings>;
}
pub enum GlobalValue<T> { Agreed(T), Mixed, NoneOutside }
pub fn global_ap(keys: &[KeySettings]) -> GlobalValue<Um>;      // keys with ap_keyset == 0
pub fn global_rt(keys: &[KeySettings]) -> GlobalValue<(Um, Um)>; // rt-enabled keys with rt_keyset == 0; NoneOutside means global RT is off
pub struct KeysetView { pub index: u16, pub members: Vec<u8> }
pub fn ap_keysets(keys: &[KeySettings]) -> Vec<KeysetView>;      // grouped by ap_keyset != 0, sorted by index
pub fn rt_keysets(keys: &[KeySettings]) -> Vec<KeysetView>;      // same, over rt_keyset
```

App gains `pub board: BoardModel`; `App::new(board: BoardModel, wh_version: &str) -> Self`. The
device line and profile line render from it in this task; tabs come in Task 5.

- [ ] **Step 1: Write the test support module**, `crates/wh-tui/tests/support/mod.rs`. Copy the
  fixture-builder pattern from `crates/wh-cli/tests/dump.rs` (out_line, in_line, reply,
  defkey_payload, matrix_lines, key_settings_lines, sync_lines, profile_lines,
  global_travel_lines, build_script for the two-key board: 'w' 0x1A then 'a' 0x04), adjusted only
  in that these tests exercise the library, not the binary: no `run_wh`, scripts feed
  `ReplayTransport::from_jsonl` directly. Add one new builder this crate needs:

```rust
/// The unsolicited adjust-mode edge frames, exactly as measured in docs/protocol.md.
pub fn adjust_edge_line(entering: bool) -> String {
    let third = if entering { 0x00 } else { 0x01 };
    in_line(&reply(0x00, &[0x00, 0xbe, third]))
}
```

Every builder goes through `wh_proto::frame::frame` / `wh_proto::cmds` encoders exactly as the
dump.rs originals do. Copying rather than sharing is deliberate: the two crates' suites must be
able to drift apart only loudly, by a fixture mismatch, not silently through a shared helper
edit; a two-line comment at the top of the module says where the pattern comes from.

- [ ] **Step 2: Write the failing tests**, `crates/wh-tui/tests/board.rs`:

```rust
mod support;
use support::*;
use wh_device::replay::ReplayTransport;
use wh_device::session::Session;
use wh_tui::board::{ap_keysets, global_ap, BoardModel, GlobalValue};

#[test]
fn board_model_reads_the_same_wire_sequence_as_snapshot_from_device() {
    let lines = build_script(); // sync, profile 0, global travel, matrix, six reads per key
    let t = ReplayTransport::from_jsonl(&lines.join("\n")).unwrap();
    let mut s = Session::new(t);
    let m = BoardModel::read(&mut s).unwrap();
    assert_eq!(m.firmware, "V1.0.0.001");
    assert_eq!(m.profile.one_based(), 1);
    assert_eq!(m.keys.len(), 2);
    assert_eq!(m.key(0x1A).unwrap().ap.0, 1200);
    assert!(s.into_inner().finished(), "script not fully consumed");
}

#[test]
fn global_ap_agrees_only_over_keys_outside_ap_keysets() {
    // built from KeySettings literals, no wire involved: two keys agreed at 2.00mm outside
    // keysets is Agreed; one of them moved into a keyset with a different value stays Agreed;
    // two disagreeing outside keys is Mixed; all keys in keysets is NoneOutside.
}

#[test]
fn ap_keysets_group_by_index_and_sort() {
    // three keys, two sharing ap_keyset 2, one in 1: expect [{1,[..]}, {2,[..,..]}]
}
```

Fill the two sketched tests with real `KeySettings` literals (the struct is plain data; build
values with `Um(2000)` and `Mode::from_value(0x0010)`). The commonest measured MODE is `0x10`:
use `0x0010` for the outside-keyset keys, not `0x18` (see CLAUDE.md on the fixture skew).

- [ ] **Step 3: Run them, watch them fail**

Run: `cargo test -p wh-tui --test board`
Expected: FAIL, `wh_tui::board` unresolved.

- [ ] **Step 4: Implement `board.rs`**

`BoardModel::read` calls, in order: `ops::device_info`, `ops::profile`, `ops::global_travel`,
`ops::read_matrix_rows`, then `ops::read_key_settings` per usage in flatten order. This is the
same wire order as `snapshot_from_device` in `crates/wh-cli/src/run.rs:181` (which uses
`read_matrix`; Task 2 made the frames identical), so existing capture knowledge transfers. The
pure functions are folds over `keys`; `GlobalValue` derives `Debug, PartialEq` for tests.

- [ ] **Step 5: Wire into App**: `App::new(board, wh_version)`; `draw` adds, under the nav line,
  the device line and profile line:

```
[X] WALLHACK K-001 - V1.0.0.001
PROFILE < 1 >
```

The device line's firmware comes from `board.firmware`, the profile from
`board.profile` (its `Display` prints one-based). Update `lib.rs`'s `run` to call
`BoardModel::read(session)` before entering the loop, and update Task 3's unit tests to build an
`App` through a small `#[cfg(test)]` fixture (a `BoardModel` literal with two keys; no wire).
Add one whole-line TestBackend assertion each for the device line and the profile line.

- [ ] **Step 6: Run the tests, watch them pass**

Run: `cargo test -p wh-tui`
Expected: PASS.

- [ ] **Step 7: Gates, then commit**

```bash
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add crates/wh-tui
git commit -m "[feat] - Read the whole board into the TUI's BoardModel on open"
```

---

### Task 5: Chrome, tabs, and footer

**Files:**
- Modify: `crates/wh-tui/src/app.rs` (Tab enum, selection, hit rects), `crates/wh-tui/src/lib.rs`
  (mouse capture, mouse events)
- Test: `crates/wh-tui/tests/chrome.rs`

**Interfaces:**
- Produces: `pub enum Tab { ActuationPoint, RapidTrigger, Mapping, Switches, Advanced }` with
  `pub fn title(self) -> &'static str` returning the exact uppercase vendor titles
  (`"ACTUATION POINT"`, `"RAPID TRIGGER"`, `"MAPPING"`, `"SWITCHES"`, `"ADVANCED"`);
  `App { pub tab: Tab, .. }`; `App::handle_key` gains Left/Right cycling over tabs;
  `pub fn handle_mouse(&mut self, kind: MouseEventKind, col: u16, row: u16)` with click-to-select
  over recorded tab rects; `App { tab_rects: Vec<(Rect, Tab)> }` filled during `draw`.
  Later tasks reuse the same rect-recording pattern for keys and buttons.

- [ ] **Step 1: Write the failing tests**, `crates/wh-tui/tests/chrome.rs` (TestBackend 160x40,
  App built from the two-key `BoardModel` fixture; expose that fixture from `support` as
  `pub fn two_key_board() -> BoardModel` built from plain literals so chrome tests need no wire):

```rust
#[test]
fn the_tab_row_renders_all_five_titles_and_marks_the_selected_one() {
    // draw, take buffer_lines, find the tab row line: it contains all five titles in order,
    // and the selected one is wrapped in the selection markers chosen in Step 3 (assert the
    // whole tab-row line against one exact expected string, not fragments).
}

#[test]
fn right_and_left_arrows_cycle_the_tabs_without_wrapping_past_the_ends() {
    // Right from ActuationPoint selects RapidTrigger ... Right at Advanced stays at Advanced;
    // Left at ActuationPoint stays.
}

#[test]
fn clicking_a_tab_title_selects_it() {
    // draw once to fill tab_rects, click inside the MAPPING rect, assert app.tab == Mapping.
}

#[test]
fn the_footer_renders_help_language_and_support() {
    // whole-line assertions for the footer line(s): "HELP", "EN JA CH", "SUPPORT@WALLHACK.COM".
}
```

Write these as real code against the interfaces above; the bodies are described here because
they depend on the exact expected strings chosen in Step 3, and the task is not done until each
assertion is a whole-line equality against a literal.

- [ ] **Step 2: Run them, watch them fail** (`cargo test -p wh-tui --test chrome`).

- [ ] **Step 3: Implement**

The tab row renders each title separated by two spaces; the selected tab renders inverted
(`Style::default().add_modifier(Modifier::REVERSED)`), matching the vendor's inverted selected
tab, and is additionally wrapped in `[` `]` so the selection also survives in plain text (the
TestBackend assertions read symbols, not styles; the brackets are what they pin). The footer is
the last row: `HELP` left, then `EN JA CH`, then `SUPPORT@WALLHACK.COM`. `draw` records each
tab title's Rect into `app.tab_rects` before rendering. `lib.rs` enables mouse capture after
`try_init` (`crossterm::execute!(std::io::stdout(), EnableMouseCapture)`) and disables it before
`ratatui::restore()`, in a shape where the disable runs even when the loop errors; the event
loop routes `Event::Mouse(m)` with `MouseEventKind::Down(MouseButton::Left)` to
`app.handle_mouse`.

- [ ] **Step 4: Run the tests, watch them pass** (`cargo test -p wh-tui`).

- [ ] **Step 5: Gates, then commit**

```bash
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add crates/wh-tui
git commit -m "[feat] - Render the TUI chrome with clickable, arrow-cyclable tabs"
```

---

### Task 6: The key matrix widget

**Files:**
- Create: `crates/wh-tui/src/matrix.rs`
- Modify: `crates/wh-tui/src/app.rs` (render it on the right pane, record key rects)
- Test: `crates/wh-tui/tests/matrix.rs`

**Interfaces:**
- Consumes: `BoardModel::{rows, keys}`, `wh_proto::keys::label`.
- Produces:

```rust
/// Width of one key cap in units of a standard cap, from the key's name. Everything not
/// listed is 1.0. These are the ANSI-DK proportions read off the vendor's own rendering
/// (research/vendor-bundle/2026-09-05/screenshots/), not a standard to import.
pub fn cap_units(usage: u8) -> f32;
pub struct CapValue { pub show: bool, pub text: String }
pub fn render_matrix(
    area: Rect, buf: &mut Buffer,
    rows: &[DefKeyRow], value_of: impl Fn(u8) -> CapValue,
    selected: &HashSet<u8>, rects: &mut Vec<(Rect, u8)>,
);
pub fn key_at(rects: &[(Rect, u8)], col: u16, row: u16) -> Option<u8>;
```

App gains `pub selection: HashSet<u8>` (rendered now, driven by clicks in plan 2) and
`key_rects: Vec<(Rect, u8)>`.

- [ ] **Step 1: Write the failing tests**, `crates/wh-tui/tests/matrix.rs`, against a
  four-key board fixture (`support::wasd_board()`, the wasd shape from dump.rs's
  `matrix_lines_wasd`, as literals):

```rust
#[test]
fn caps_render_label_over_value() {
    // render_matrix into a Buffer; assert the cap for 'w' contains a line with "W" centred
    // and the line below it "2.00" (whole cap-cell content, coordinates from the returned rect).
}

#[test]
fn value_hidden_when_value_of_says_so() { /* CapValue{show:false,..} renders a blank value line */ }

#[test]
fn key_at_resolves_a_click_inside_a_cap_and_misses_between_caps() { }

#[test]
fn a_selected_cap_renders_reversed() {
    // assert the style modifier on a cell inside the selected cap's rect.
}
```

Write the four with real coordinates taken from the returned `rects`, so the tests do not
hard-code geometry that the width table may tune later.

- [ ] **Step 2: Run them, watch them fail** (`cargo test -p wh-tui --test matrix`).

- [ ] **Step 3: Implement**

Each cap is a bordered cell 4 rows tall (top border, label line, value line, bottom border),
`round(cap_units(u) * 7.0)` columns wide, laid out left to right per `DefKeyRow` in column
order, rows stacked with no gap. `cap_units` by name (via `wh_proto::keys::name_for_usage`):
`backspace` 2.0, `tab` 1.5, `backslash` 1.5, `caps` 1.75, `enter` 2.25, left `shift` 2.25,
right `shift` 1.75, `space` 6.25, everything else 1.0 (check the actual name strings in
`wh_proto::keys::TABLE` for the modifier and special keys before matching on them; the table's
names are the authority, and left and right variants have distinct usages). Label is
`wh_proto::keys::label(usage)` uppercased, truncated to the cap's inner width. Selected caps
render with `Modifier::REVERSED` over the whole cell. If `area` is too narrow for a row,
`render_matrix` draws nothing and records no rects, and `app::draw` renders
`matrix::too_narrow_text` in its place, word-wrapped so the whole sentence reaches the operator:
in the matrix pane when that pane can hold the message's longest word, in the left pane below the
note row otherwise (at 56 columns or less there is no matrix pane at all). The message names the
frame width the board's own rows need, 169 columns for a full ANSI-DK 68-key board.
`app::draw` splits the body horizontally (left pane 56 columns, rest to the matrix) and calls
`render_matrix` with `value_of` chosen by tab: ActuationPoint shows every key's `ap` as
`format!("{:.2}", um.to_mm())`; RapidTrigger shows `rt_press` only for keys with
`rt_keyset != 0 && rt_enabled()`; other tabs show none. ADVANCED's GAMEPAD, DEVICE and SHARE
sub-tabs drop the keyboard pane entirely and give the left pane the full width.

Controller ruling, 2026-09-06, overturning this task's own earlier text: the predicate was
`rt_keyset != 0` alone, which prints a sensitivity for a key whose rapid trigger is off.
`global_rt` in the same crate already used the stronger predicate, and it is the correct one.

- [ ] **Step 4: Run the tests, watch them pass** (`cargo test -p wh-tui`).

- [ ] **Step 5: Gates, then commit**

```bash
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add crates/wh-tui
git commit -m "[feat] - Render the key matrix from board-derived rows with hit testing"
```

---

### Task 7: Settings rows, keyset rows, and the prompt line

**Files:**
- Create: `crates/wh-tui/src/rows.rs`
- Modify: `crates/wh-tui/src/app.rs` (AP and RT tab bodies, prompt line)
- Test: `crates/wh-tui/tests/rows.rs`

**Interfaces:**
- Produces:

```rust
pub enum Control { Stepper { value: String }, Button { label: String } }
pub struct SettingRow { pub label: String, pub control: Control, pub disabled: bool, pub indent: u16 }
/// Renders "LABEL" + dot leaders + the control, vendor shape: stepper "< VALUE >",
/// button "[LABEL]". Disabled rows render DIM over the whole row.
pub fn render_row(area: Rect, buf: &mut Buffer, row: &SettingRow);
```

App renders per tab: the rows described below, then one prompt line under the left pane
(`> ` + status text left, action buttons right), reusing `render_row`'s button rendering for the
right-side actions and recording their rects for plan 2.

- [ ] **Step 1: Write the failing tests**, `crates/wh-tui/tests/rows.rs`:

```rust
#[test]
fn a_stepper_row_renders_label_leaders_and_value_as_one_exact_line() {
    // 60-wide area: assert the whole rendered line equals
    // "GLOBAL ACTUATION POINT.............................< 2.00 MM >"
    // built once by hand here as the pinned expectation.
}

#[test]
fn a_disabled_row_is_dim_across_its_whole_width() { }

#[test]
fn the_ap_tab_body_renders_global_custom_value_and_keyset_rows() {
    // App over a board fixture with one AP keyset: assert the two global rows and the
    // keyset row "[X] W,A" line render (whole lines), and the prompt line reads
    // "> CLICK ON THE KEYS TO MAKE A KEYSET" with "[RESET KEYSETS]" at the right.
}

#[test]
fn rt_sub_rows_render_dim_while_global_rt_is_off() { }
```

- [ ] **Step 2: Run them, watch them fail** (`cargo test -p wh-tui --test rows`).

- [ ] **Step 3: Implement**

AP tab rows: `GLOBAL ACTUATION POINT` stepper from `global_ap(&board.keys)` (`Agreed` renders
`{:.2} MM`, `Mixed` renders `MIXED`, `NoneOutside` renders `-`), `"MM" CUSTOM VALUE` stepper
from `board.global.travel`. Then one row per `ap_keysets` entry:
label `[X] {members as comma-joined labels}`, control `Button { label: "^" }` (collapse marker;
collapse state itself is plan 2 alongside interaction). RT tab rows: `GLOBAL RAPID TRIGGER`
stepper, a toggle reading `ON`, `OFF` or `MIXED` off `global_rt_on`, then
`SEPARATE PRESS AND RELEASE`, `RT SENSITIVITY`, `CONTINUOUS RAPID TRIGGER` steppers, all three
`disabled` while the global row is `OFF`, then `rt_keysets` rows in the same shape as AP's.
Steppers render values only; arrow interaction is plan 2, and nothing in this plan writes.
Prompt line per tab: AP and RT read `> CLICK ON THE KEYS TO MAKE A KEYSET`, and the right side
renders `[RESET KEYSETS]`, disabled (dim) when the tab's keyset list is empty, inert either way
in this plan.

Controller ruling, 2026-09-06, overturning this task's own earlier text: `GLOBAL RAPID TRIGGER`
was to render `{:.2} MM` for `Agreed((p, r))`. That put the same millimetres under two labels
(`RT SENSITIVITY` already carries them) and left a toggle that could never read `ON`. The vendor
renders it as a toggle, `GLOBAL RAPID TRIGGER < OFF >`, with the millimetres on the row below.
`CONTINUOUS RAPID TRIGGER` folds through `GlobalValue` the same way, so a board whose outside
keys disagree reads `MIXED` rather than `ON` off a single key.

- [ ] **Step 4: Run the tests, watch them pass** (`cargo test -p wh-tui`).

- [ ] **Step 5: Gates, then commit**

```bash
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add crates/wh-tui
git commit -m "[feat] - Render the settings rows, keyset rows and prompt line for AP and RT"
```

---

### Task 8: The honest stub tabs

**Files:**
- Modify: `crates/wh-tui/src/app.rs`
- Test: `crates/wh-tui/tests/stubs.rs`

**Interfaces:**
- Produces: ADVANCED sub-tab state `pub enum AdvancedTab { General, Gamepad, Device, Share }` on
  App, cycled and clicked exactly like `Tab` (same rect-recording pattern).

- [ ] **Step 1: Write the failing tests**, `crates/wh-tui/tests/stubs.rs`, whole-line
  assertions each:

```rust
#[test]
fn mapping_renders_its_subtab_labels_and_the_stub_line() {
    // "BASE LAYER  FN LAYER" and "BASE CHARACTERS  EXTENDED CHARACTERS  FUNCTIONS  GAMEPAD"
    // render as labels, and the body line is
    // "> MAPPING EDITS ARE NOT BUILT IN WH YET (3.6 IN DOCS/TASKS.MD)".
}

#[test]
fn switches_renders_rows_and_the_stub_line() { /* "> SWITCH SETTINGS ARE NOT BUILT IN WH YET" */ }

#[test]
fn advanced_device_shows_live_name_serial_firmware() {
    // three whole lines from the BoardModel fixture, no stub text on this sub-tab.
}

#[test]
fn advanced_general_rows_render_with_stub_markers_where_unbuilt() {
    // rows render; SOCD row present with "[SELECT]" disabled; LED and polling rows show "-"
    // values and the line "> EDITING THESE ARRIVES WITH THE TUI'S EDITING PHASE" is present.
}

#[test]
fn advanced_gamepad_and_share_render_their_stub_lines() { }
```

- [ ] **Step 2: Run, watch them fail** (`cargo test -p wh-tui --test stubs`).

- [ ] **Step 3: Implement**

MAPPING: the two sub-tab label rows render as plain (non-interactive) labels, the character
palette is omitted, and the body is the single stub line above. SWITCHES: `CALIBRATE SWITCHES`
row with a disabled `[START]`, `CURRENT SWITCHES` row with value `-`, and its stub line.
ADVANCED: the sub-tab row (GENERAL GAMEPAD DEVICE SHARE) is interactive; GENERAL renders the
vendor's row list (Reset Profile, Factory Reset, Polling Rate, LED Sleep Timer, LED Brightness,
System Type, Show Analog Output, Safety Zone, Show Mapped Key Labels, Localized Key Labels,
SOCD, Dynamic Keystroke (DKS), Mod Tap, Walkthrough) with every control disabled and every
unread value as `-`, plus the stub line; DEVICE renders `NAME`, `SERIAL NUMBER`,
`FIRMWARE VERSION` rows live from BoardModel; GAMEPAD and SHARE render their row labels disabled
with one stub line each. Stub wording is exact and shared: define the strings as `pub const`s in
`app.rs` so tests pin the constant and the rendering cannot drift from it silently, and keep the
claim honest: the text says the edit is not built, never that the feature does not exist.

- [ ] **Step 4: Run the tests, watch them pass** (`cargo test -p wh-tui`).

- [ ] **Step 5: Gates, then commit**

```bash
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add crates/wh-tui
git commit -m "[feat] - Render MAPPING, SWITCHES and ADVANCED with honest stubs and live device info"
```

---

### Task 9: Adjust-mode events in the loop

**Files:**
- Modify: `crates/wh-tui/src/app.rs`, `crates/wh-tui/src/lib.rs`
- Test: `crates/wh-tui/tests/events.rs`

**Interfaces:**
- Consumes: `Session::poll_event`, `wh_proto::event::BoardEvent`, Task 1's `wait` entries.
- Produces:

```rust
impl App {
    /// One event-poll step of the loop: at most one poll_event, routing edges.
    /// Returns Ok(true) when the display changed (a redraw is due).
    pub fn tick<T: Transport>(
        &mut self,
        s: &mut Session<T>,
        redraw: &mut impl FnMut(&mut App),
    ) -> Result<bool, DeviceError>;
}
pub const LOCKED_BANNER: &str =
    "BOARD LOCKED: ADJUSTING ON THE KEYBOARD ITSELF. IT WILL NOT TYPE UNTIL THE KEY IS PRESSED AGAIN.";
```

App gains `pub locked: bool` and `pub status: Option<String>`.

- [ ] **Step 1: Write the failing tests**, `crates/wh-tui/tests/events.rs`:

```rust
mod support;
use support::*;

#[test]
fn a_be00_edge_raises_the_locked_banner_and_be01_rereads_and_lowers_it() {
    // Script: build_script() (the open read), then {"dir":"wait"}, then adjust_edge_line(true),
    // then {"dir":"wait"}, then adjust_edge_line(false), then build_script() again (the re-read).
    // Drive: BoardModel::read, App::new, then tick() four times.
    // Assert, in order: tick1 (wait) -> no change, not locked; tick2 -> locked, draw renders
    // LOCKED_BANNER as a whole line; tick3 (wait) -> still locked; tick4 -> not locked, the
    // re-read consumed the script's second read block (session.into_inner().finished()).
}

#[test]
fn an_unknown_event_lands_in_the_status_line_and_does_not_lock() {
    // Script: build_script(), then an In line built as in_line(&reply(0x00, &[0x00,0xbe,0x02])).
    // tick once: locked stays false, app.status is Some(STATUS_UNKNOWN_EVENT), and draw
    // renders that status as a whole line.
}
```

Write both in full; the first is this plan's crown test, proving the wait entry, the event
routing, and the re-read compose. It must fail before Step 3 for a reason named in the report
(no `tick`, no banner), not for a fixture mistake.

- [ ] **Step 2: Run, watch them fail** (`cargo test -p wh-tui --test events`).

- [ ] **Step 3: Implement**

`tick` calls `s.poll_event(Duration::from_millis(15))` once (tests pass a script; the timeout is
ignored by ReplayTransport). `AdjustModeEntered`: `locked = true`. `AdjustModeLeft`:
`locked = false`, then `self.board = BoardModel::read(s)?` (the vendor re-reads everything on
this edge; so do we). `Unknown(_)`: `status = Some(STATUS_UNKNOWN_EVENT.to_string())` where
`pub const STATUS_UNKNOWN_EVENT: &str = "NOTE: UNRECOGNISED BOARD EVENT RECEIVED"`. While
`locked`, `draw` renders `LOCKED_BANNER` in the prompt line's place, and `handle_key`/
`handle_mouse` ignore everything except quit and tab navigation. If a `BoardModel::read` inside
`tick` fails with `DeviceError::Timeout`, set
`status = Some("NOTE: A READ TIMED OUT; VALUES MAY BE STALE".to_string())` and keep running with
the old model: queued events stay queued in the Session (the starvation ruling from the spec).
`lib.rs`'s loop calls `app.tick(session, &mut redraw)` between the input poll and the redraw,
redrawing when either reports a change. `tick` calls `redraw` itself once before the
`AdjustModeLeft` re-read, with `STATUS_REREADING` showing: that read blocks the single thread,
Ctrl-C included, so the frozen frame has to say what it is waiting for. See `tick`'s own comment
for the worst case.

- [ ] **Step 4: Run the tests, watch them pass** (`cargo test -p wh-tui`).

- [ ] **Step 5: Gates, then commit**

```bash
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add crates/wh-tui
git commit -m "[feat] - Route the board's adjust-mode edges into the TUI's banner and re-read"
```

---

### Task 10: Docs

**Files:**
- Modify: `docs/tasks.md`, `CLAUDE.md`, `docs/superpowers/specs/2026-09-06-tui-design.md`

- [ ] **Step 1: docs/tasks.md.** In the 3.5 entry: record that the foundation plan
  (`docs/superpowers/plans/2026-09-06-tui-foundation.md`) delivered the read-only TUI, that the
  replay blocker paragraph is resolved by the `wait` script entry (rewrite that paragraph to
  state the solution as shipped, past tense), and that the editing plan (writes, keyset gesture,
  SOCD editor, profile switch, typed-yes modals, the `auto: tui` backup) remains before 3.5
  closes. Do not tick the 3.5 checkbox.

- [ ] **Step 2: CLAUDE.md.** Three edits, each one sentence-sized: add a `wh-tui` row to the
  crate table (Owns: "The `wh tui` full-screen UI: app state, widgets, the event loop"; Never
  does: "Encode frames by hand, open a transport itself"); update the integration-suite sentence
  in the Commands section to include wh-tui's suites (count them at edit time rather than
  trusting this plan); check the sentence "long-running features are backlogged rather than
  built" against the TUI now existing, and reword it if the TUI makes it false (the exclusivity
  claim stays true; only the "no long-running features" phrasing may need care).

- [ ] **Step 3: The spec.** Its visual-reference section calls the banner "the one deliberate
  departure from the vendor's chrome". The logo ruling in this plan's Global Constraints makes
  that two. Correct the sentence to name both departures (banner text, own logo), citing the
  trademark reason for the logo.

- [ ] **Step 4: Repeats check and gates**

```bash
python3 scripts/check-doc-repeats.py $(git ls-files '*.md' ':!research/*')
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add docs/tasks.md CLAUDE.md docs/superpowers/specs/2026-09-06-tui-design.md
git commit -m "[docs] - Record the TUI foundation: read-only wh tui, wait entries, two departures"
```
