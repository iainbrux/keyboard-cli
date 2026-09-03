# Keyset CLI (task 2.4b) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `wh` a `keyset` command tree that reads, creates, values and deletes keysets the way
terminal.wallhack.com does, teach `wh set ap` to split a keyset the way the vendor UI does, and make
`wh restore` put membership back.

**Architecture:** Almost nothing new in `wh-device`; task 2.4a already shipped
`crates/wh-device/src/keyset.rs` and nothing calls it yet. This plan is almost entirely `wh-cli`: a new `Cmd::Keyset` subtree in
`cli.rs`, a new `crates/wh-cli/src/keyset.rs` holding its handlers, and three edits into existing
`run.rs` paths. Two small additions to `wh-device` are needed and are called out where they occur:
one helper that builds membership records from per-key indices (for restore), and a split of
`ops::restore_all` so membership goes out last, one record per frame.

**Tech Stack:** Rust 2021, clap 4 derive, `ReplayTransport` for every test.

**Spec:** `docs/keysets.md` is the measured evidence base and the binding authority for wire
behaviour. `docs/tasks.md` entries 2.4b, 2.14, 2.15 and 2.17 carry the decisions. The Phase 2 design
doc, `docs/superpowers/specs/2026-08-29-phase-2-design.md`, deliberately left 2.4 unspecified and is
not an authority here.

## Global Constraints

Copied from `CLAUDE.md` and the task entries. Every task's requirements implicitly include these.

- **No em dashes or en dashes anywhere**, in code, comments, docs or commit messages.
- Commit messages are one line, `[type] - Message`. **No trailers of any kind.**
- Comments default to one or two lines, four is the ceiling. Never cite a task number, review round
  or chunk number in a comment.
- **Never loosen `ReplayTransport`'s byte-for-byte frame matching** to make a test pass. If a
  fixture stops matching, the code changed under it and the fixture is what should change.
- All three gates pass before every commit: `cargo test --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`.
- **Commit before mutation testing.** Restoring a mutation with `git checkout --` destroys
  uncommitted work; this has already happened once in this repository.
- Establish that each new test **fails when the code is wrong**: mutate the thing the test claims to
  check, watch it fail, restore, and say so in the report.
- Comments and docs may state what was **measured**. They may not state an inference as though it
  were measured. Say which it is. This plan marks its own inferences; carry those markings into the
  code comments verbatim rather than upgrading them.
- Do not dispatch subagents. Review arrives from the controller after your report.
- `captures/` is gitignored and absent on any machine but the operator's. No test may read it.

## What 2.4a already provides

Read `crates/wh-device/src/keyset.rs` before starting. Its API changed six times during 2.4a and no
earlier brief describes it correctly. The signatures this plan calls:

```rust
pub use wh_proto::cmds::KeysetKind as Kind;          // Kind::Ap | Kind::Rt, .layout() -> 0xFF | 0xFE
pub struct Keyset { pub index: u16, pub members: Vec<u8> }
pub struct Membership { /* private */ }              // .kind(), .entries() -> &[(u8, u16)]
pub struct KeysetIndex { /* private */ }             // .kind(), .value()
pub struct Change { /* private */ }                  // .kind()
pub struct WritePlan { /* private */ }               // .value_records(), .membership_records(),
                                                     // .before(), .frames(), .is_empty()
pub enum Global<T> { Agreed(T), Split(Vec<(T, usize)>), NoneOutsideAKeyset }

pub fn read_membership<T: Transport>(s: &mut Session<T>, kind: Kind)
    -> Result<Membership, DeviceError>;               // reads the matrix itself, whole board
pub fn group(m: &Membership) -> Vec<Keyset>;          // ascending by index, excludes 0
pub fn next_index(m: &Membership) -> Result<KeysetIndex, DeviceError>;
impl KeysetIndex {
    pub fn clear(kind: Kind) -> Self;                 // value 0
    pub fn restoring(kind: Kind, value: u16) -> Self; // for wh restore only
}
impl Change {
    pub fn ap(value: Um) -> Self;                     // promotes Global -> Single
    pub fn ap_keeping_touch(value: Um) -> Self;
    pub fn rt_on(press: Um, release: Um) -> Self;
    pub fn rt_off(press: Um, release: Um) -> Self;
    pub fn membership_only(kind: Kind) -> Self;
}
pub fn plan<T: Transport>(s: &mut Session<T>, usages: &[u8], change: &Change,
                          membership: Option<KeysetIndex>) -> Result<WritePlan, DeviceError>;
pub fn apply<T: Transport>(s: &mut Session<T>, plan: &WritePlan) -> Result<(), DeviceError>;
pub fn global_ap<T: Transport>(s: &mut Session<T>, m: &Membership)
    -> Result<Global<Um>, DeviceError>;
pub fn global_rt<T: Transport>(s: &mut Session<T>, m: &Membership)
    -> Result<Global<(Um, Um)>, DeviceError>;
```

`plan` sends only reads, so every command below can dry run by building a plan and printing
`plan.frames()` without calling `apply`.

## Decisions this plan implements

These were settled outside it. Do not relitigate them; if one looks wrong, report it and continue.

1. **`wh set ap` emits one shape, always** (task 2.14). It routes through `keyset::plan` with
   `Change::ap`, whether or not the key is in a keyset. `ops::ap_records` becomes the divergent
   path, not a second supported one. Measured: `ap-wasd-1.2` emits the same template on keys with no
   keyset traffic in the file, three times over.
2. **A `Split` or absent global refuses rather than guesses** (task 2.15). Where a command needs the
   board's global value and `global_ap`/`global_rt` returns `Split` or `NoneOutsideAKeyset`, the
   command errors, names the disagreement, and points at `--value` (or `--press`/`--release`). It
   never picks a majority.
3. **A new keyset starts at the global value**, not at its members' existing values, and creating
   one is therefore destructive to those values. The command must say so before it writes.
4. **Splitting is automatic and announced.** When `wh set ap` targets a strict subset of a keyset's
   members, it allocates a new index for the selected keys and prints what it did, matching the
   vendor UI's behaviour.

## Wire rules this plan must not break

From `docs/keysets.md`. `plan` already enforces the first four; the fifth and sixth are this plan's
to get right.

- Values always precede membership, and membership is written one record per frame, always last.
- A key gets the whole value template if any owned value differs, and nothing at all if none does.
- Non-owned layouts are rewritten at the key's current value, read first.
- Nothing writes touch nibble `0`.
- Allocation is max plus one over **live** membership, so a freed index returns to the pool. Always
  allocate from a fresh `read_membership`, never from a cached or partial view.
- Layouts `0x16` and `0x17` are **not** a constant. `plan` does not write them and this plan does
  not add them. If you find yourself hard-coding `100`, stop: that would write `100` over `0` on a
  board that has never held a keyset.

## File structure

| File | Responsibility | Task |
|---|---|---|
| `crates/wh-cli/src/cli.rs` | The `Cmd::Keyset` subtree and its args. Parsing only | 1 |
| `crates/wh-cli/src/keyset.rs` | **New.** Every `wh keyset` handler, plus the shared index and split helpers | 1 to 3 |
| `crates/wh-cli/src/run.rs` | Dispatch into the new module; the `set ap` split; restore membership | 1, 4, 5 |
| `crates/wh-cli/tests/keyset.rs` | **New.** End-to-end replay tests for the `wh keyset` tree | 1 to 3 |
| `crates/wh-cli/tests/dump.rs` | End-to-end tests for the `set ap` split and restore | 4, 5 |
| `crates/wh-device/src/keyset.rs` | One new helper, `membership_records` | 5 |
| `crates/wh-device/src/ops.rs` | `read_layout_value` becomes `pub`; `restore_all` splits values from membership | 1, 5 |
| `README.md` | The user-facing command reference for the whole tree | 6 |
| `docs/tasks.md` | Closing 2.4b | 6 |

`crates/wh-cli/src/run.rs` is already 1500 lines. Everything new that is not a two-line dispatch arm
goes in `crates/wh-cli/src/keyset.rs`.

---

### Task 1: The `wh keyset` subtree and `wh keyset list`

Establishes the command shape, the `Kind` argument, and the replay test harness the next two tasks
reuse. Read-only, so nothing here can damage a board.

**Files:**
- Modify: `crates/wh-cli/src/cli.rs`
- Create: `crates/wh-cli/src/keyset.rs`
- Modify: `crates/wh-cli/src/run.rs` (module declaration and one dispatch arm)
- Test: `crates/wh-cli/tests/keyset.rs` (new)

**Interfaces:**
- Consumes: `keyset::read_membership`, `keyset::group`, `keyset::global_ap`, `keyset::global_rt`,
  `keyset::Global`, `run::key_label`, `run::with_session`.
- Produces, for tasks 2 and 3:
  ```rust
  pub(crate) fn resolve_index(sets: &[wh_device::keyset::Keyset], index: u16)
      -> anyhow::Result<wh_device::keyset::Keyset>;
  pub(crate) fn global_ap_or_bail<T: Transport>(
      s: &mut Session<T>, m: &wh_device::keyset::Membership, flag: &str)
      -> anyhow::Result<wh_proto::value::Um>;
  pub(crate) fn global_rt_or_bail<T: Transport>(
      s: &mut Session<T>, m: &wh_device::keyset::Membership, flag: &str)
      -> anyhow::Result<(wh_proto::value::Um, wh_proto::value::Um)>;
  ```

- [ ] **Step 1: Add the command surface to `cli.rs`**

Add to `Cmd`:

```rust
    /// Read and write keysets (grouped actuation point and rapid trigger settings)
    Keyset {
        #[command(subcommand)]
        what: KeysetWhat,
    },
```

Add, after `BackupsWhat`:

```rust
/// Which of the two independent keyset groupings a command operates on. They have separate
/// indices, so every keyset command names one.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum KeysetKindArg {
    /// Actuation point keysets (layout 0xFF)
    Ap,
    /// Rapid trigger keysets (layout 0xFE)
    Rt,
}

#[derive(Subcommand)]
pub enum KeysetWhat {
    /// List keysets and their members: wh keyset list ap
    List {
        /// Omit to list both kinds
        kind: Option<KeysetKindArg>,
    },
    /// Create a keyset over selected keys: wh keyset create ap --keys u,i,o,p
    Create {
        kind: KeysetKindArg,
        #[command(flatten)]
        keys: KeysArg,
        /// Actuation point in mm for a new ap keyset. Defaults to the board's global, and is
        /// required when the keys outside every keyset disagree on it.
        #[arg(long)]
        value: Option<f64>,
        /// Press sensitivity in mm for a new rt keyset. Defaults to the board's global.
        #[arg(long)]
        press: Option<f64>,
        /// Release sensitivity in mm for a new rt keyset. Defaults to the board's global.
        #[arg(long)]
        release: Option<f64>,
        /// Print the exact reports without sending
        #[arg(long)]
        dry_run: bool,
    },
    /// Change an existing keyset's value: wh keyset set ap 3 --value 1.2
    Set {
        kind: KeysetKindArg,
        index: u16,
        #[arg(long)]
        value: Option<f64>,
        #[arg(long)]
        press: Option<f64>,
        #[arg(long)]
        release: Option<f64>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Delete a keyset, returning its members to the global value: wh keyset delete ap 3
    Delete {
        kind: KeysetKindArg,
        index: u16,
        /// Value in mm to return ap members to. Defaults to the board's global, and is required
        /// when the keys outside every keyset disagree on it.
        #[arg(long)]
        value: Option<f64>,
        #[arg(long)]
        press: Option<f64>,
        #[arg(long)]
        release: Option<f64>,
        #[arg(long)]
        dry_run: bool,
    },
}
```

- [ ] **Step 2: Write the failing parse tests**

Add to `cli.rs`'s test module:

```rust
    #[test]
    fn keyset_list_takes_an_optional_kind() {
        let c = Cli::try_parse_from(["wh", "keyset", "list"]).unwrap();
        match c.cmd {
            Cmd::Keyset {
                what: KeysetWhat::List { kind },
            } => assert!(kind.is_none()),
            _ => panic!("wrong parse"),
        }
        assert!(Cli::try_parse_from(["wh", "keyset", "list", "ap"]).is_ok());
        assert!(Cli::try_parse_from(["wh", "keyset", "list", "nonsense"]).is_err());
    }

    #[test]
    fn keyset_create_requires_a_kind_and_a_selector() {
        assert!(Cli::try_parse_from(["wh", "keyset", "create", "--keys", "w"]).is_err());
        assert!(Cli::try_parse_from(["wh", "keyset", "create", "ap"]).is_err());
        assert!(Cli::try_parse_from(["wh", "keyset", "create", "ap", "--keys", "w"]).is_ok());
    }

    #[test]
    fn keyset_set_and_delete_take_a_decimal_index() {
        let c = Cli::try_parse_from(["wh", "keyset", "set", "ap", "3", "--value", "1.2"]).unwrap();
        match c.cmd {
            Cmd::Keyset {
                what: KeysetWhat::Set { index, value, .. },
            } => {
                assert_eq!(index, 3);
                assert_eq!(value, Some(1.2));
            }
            _ => panic!("wrong parse"),
        }
        assert!(Cli::try_parse_from(["wh", "keyset", "delete", "rt", "2"]).is_ok());
    }
```

Run: `cargo test -p wh-cli --lib keyset_`
Expected: FAIL to compile, `KeysetWhat` not found, until step 1's code is in.

- [ ] **Step 3: Create `crates/wh-cli/src/keyset.rs` with `list` and the shared helpers**

```rust
//! The `wh keyset` command tree. Every handler here reads the board's live membership first;
//! `wh` caches no device state, and allocation is max plus one over live membership, so a stale
//! view could hand out an index a key already holds.

use anyhow::{bail, Result};
use std::io::Write;
use wh_device::keyset::{self, Global, Keyset, Kind, Membership};
use wh_device::session::Session;
use wh_device::transport::Transport;
use wh_proto::value::Um;

use crate::cli::KeysetKindArg;
use crate::run::key_label;

pub(crate) fn kind_of(arg: KeysetKindArg) -> Kind {
    match arg {
        KeysetKindArg::Ap => Kind::Ap,
        KeysetKindArg::Rt => Kind::Rt,
    }
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Ap => "ap",
        Kind::Rt => "rt",
    }
}

/// The keyset holding `index`, or an error naming what is actually there. A caller that let a
/// missing index through would allocate nothing and write membership to no keys, succeeding
/// silently.
pub(crate) fn resolve_index(sets: &[Keyset], index: u16) -> Result<Keyset> {
    if index == 0 {
        bail!("0 is not a keyset index; it is the value a key outside every keyset holds");
    }
    match sets.iter().find(|k| k.index == index) {
        Some(k) => Ok(k.clone()),
        None if sets.is_empty() => bail!("no keysets of this kind exist on the board"),
        None => {
            let live: Vec<String> = sets.iter().map(|k| k.index.to_string()).collect();
            bail!("no keyset {index}; the board has {}", live.join(", "))
        }
    }
}

/// The board's global actuation point, or an error the operator can act on. `wh` never picks a
/// winner when the keys outside every keyset disagree: a majority vote would write a value
/// nobody typed over every member of the keyset being created.
pub(crate) fn global_ap_or_bail<T: Transport>(
    s: &mut Session<T>,
    m: &Membership,
    flag: &str,
) -> Result<Um> {
    match keyset::global_ap(s, m)? {
        Global::Agreed(v) => Ok(v),
        Global::Split(counts) => bail!("{}", split_message("actuation point", &counts, flag)),
        Global::NoneOutsideAKeyset => bail!(
            "every key on the board is in a keyset, so there is no global actuation point to \
             read; pass {flag} to say which value to use"
        ),
    }
}

pub(crate) fn global_rt_or_bail<T: Transport>(
    s: &mut Session<T>,
    m: &Membership,
    flag: &str,
) -> Result<(Um, Um)> {
    match keyset::global_rt(s, m)? {
        Global::Agreed(v) => Ok(v),
        Global::Split(counts) => {
            let shown: Vec<(String, usize)> = counts
                .iter()
                .map(|((p, r), n)| (format!("{:.2}/{:.2}mm", p.to_mm(), r.to_mm()), *n))
                .collect();
            bail!("{}", split_message_str("rapid trigger sensitivity", &shown, flag))
        }
        Global::NoneOutsideAKeyset => bail!(
            "every key on the board is in a keyset, so there is no global rapid trigger \
             sensitivity to read; pass {flag} to say which value to use"
        ),
    }
}

fn split_message(what: &str, counts: &[(Um, usize)], flag: &str) -> String {
    let shown: Vec<(String, usize)> = counts
        .iter()
        .map(|(v, n)| (format!("{:.2}mm", v.to_mm()), *n))
        .collect();
    split_message_str(what, &shown, flag)
}

fn split_message_str(what: &str, shown: &[(String, usize)], flag: &str) -> String {
    let parts: Vec<String> = shown
        .iter()
        .map(|(v, n)| format!("{n} key(s) at {v}"))
        .collect();
    format!(
        "the keys outside every keyset disagree on the global {what} ({}), so there is no one \
         global value to use; pass {flag} to say which",
        parts.join(", ")
    )
}

/// Lists one kind's keysets with their members and value. Every member is read and compared: an
/// agreeing keyset prints its one value, a disagreeing one names each distinct value and which
/// keys hold it. `wh` never picks a winner here, the same refusal `global_ap` and `global_rt`
/// apply to the board's global value.
pub(crate) fn list<T: Transport>(
    out: &mut impl Write,
    s: &mut Session<T>,
    kind: Kind,
) -> Result<()> {
    let m = keyset::read_membership(s, kind)?;
    let sets = keyset::group(&m);
    if sets.is_empty() {
        writeln!(out, "{} keysets: none", kind_name(kind))?;
        return Ok(());
    }
    writeln!(out, "{} keysets:", kind_name(kind))?;
    for ks in &sets {
        let line = keyset_line(s, kind, ks)?;
        writeln!(out, "  {} {}", ks.index, line)?;
    }
    Ok(())
}
```

`keyset_line` reads one layout per member for `Kind::Ap` and two for `Kind::Rt`, through
`ops::read_layout_value`, and hands the pairs to an `agreement_line` helper that prints one value
when every member holds it and `disagree: <keys> at <value>, ...` when they do not.

**This block is what shipped, after task 1's review.** An earlier version of this plan read
`ks.members[0]` alone and carried a comment claiming that printing it was how a caller sees a
keyset whose members have drifted apart. The review proved with a replay script that a divergence
was not merely unreported but never observed. **Any later task that reads a keyset's value reads
every member.** Do not reintroduce the one-member read.

- [ ] **Step 4: Dispatch it from `run.rs`**

Add `mod keyset;` beside the other module declarations in `main.rs`, and this arm to `run`'s match:

```rust
        Cmd::Keyset { what } => keyset_cmd(what),
```

and the handler, next to `profile_cmd`:

```rust
fn keyset_cmd(what: crate::cli::KeysetWhat) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    with_session(|s| match what {
        crate::cli::KeysetWhat::List { kind } => match kind {
            Some(k) => crate::keyset::list(&mut out, s, crate::keyset::kind_of(k)),
            None => {
                crate::keyset::list(&mut out, s, wh_device::keyset::Kind::Ap)?;
                crate::keyset::list(&mut out, s, wh_device::keyset::Kind::Rt)
            }
        },
        _ => bail!("not yet implemented"),
    })
}
```

The `_ => bail!` arm is temporary and task 2 replaces it. Do not leave it behind.

Make `key_label` reachable: it is already `pub(crate)`.

- [ ] **Step 5: Write the end-to-end replay test**

Create `crates/wh-cli/tests/keyset.rs`. Copy `out_line`, `in_line`, `reply`, `matrix_lines`,
`key_settings_lines`, `write_script`, `run_wh` and `scratch_config_dir` from
`crates/wh-cli/tests/dump.rs` verbatim rather than importing them; integration test binaries do not
share a module. Then:

```rust
/// `wh keyset list ap` groups the board's 0xFF values into keysets and prints each one's members
/// by name. The script gives four keys, two of them at index 1 and one at index 2, so a
/// implementation that printed every non-zero key as its own keyset would fail here.
#[test]
fn keyset_list_ap_groups_members_by_index() {
    let mut lines = matrix_lines();
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    // one AP read per member, for the value column
    lines.extend(key_settings_lines(0x1A, 2000, 0x0018, 100, 100, 1, 0));
    lines.extend(key_settings_lines(0x16, 1200, 0x0018, 100, 100, 2, 0));
    let script = write_script("keyset-list-ap", &lines);
    let out = run_wh(&["keyset", "list", "ap"], &script, &scratch_config_dir("keyset-list-ap"));
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("1 2.00mm  w,a"), "got: {text}");
    assert!(text.contains("2 1.20mm  s"), "got: {text}");
    assert!(!text.contains("d"), "key d holds 0 and is in no keyset: {text}");
}

/// A board with no keysets prints so rather than printing an empty heading.
#[test]
fn keyset_list_says_none_when_no_key_holds_a_keyset() {
    let mut lines = matrix_lines();
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    let script = write_script("keyset-list-empty", &lines);
    let out = run_wh(&["keyset", "list", "ap"], &script, &scratch_config_dir("keyset-list-empty"));
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("ap keysets: none"));
}
```

`layout_read_lines` is a new local helper; write it beside the copied ones:

```rust
/// One `read_layout_value` roundtrip: a single-record read request for `usage`/`layout`, and the
/// reply carrying `value`.
fn layout_read_lines(usage: u8, layout: u8, value: u16) -> Vec<String> {
    let req = cmds::read_key_records(&[(usage, layout)]);
    let rec = KeyRecord { key: usage, layout, value };
    vec![out_line(&req), in_line(&reply(cmds::cmd::KEY, &cmds::key_record_payload(&[rec])))]
}
```

If `cmds::read_key_records` or `cmds::key_record_payload` do not exist under those names, find what
`ops::read_layout_value` actually sends and build the fixture from that. **Do not add a public
encoder to `wh-proto` just to make a test convenient**; build the frame from the existing one.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p wh-cli keyset`
Expected: PASS. Then all three gates.

- [ ] **Step 7: Prove the tests bite**

Change `group`'s `if index == 0 { continue; }` to `if false { continue; }` in
`crates/wh-device/src/keyset.rs`, run `cargo test -p wh-cli keyset`, and confirm
`keyset_list_ap_groups_members_by_index` fails on key `d`. Restore, confirm
`git status --porcelain` is empty, and say so in your report.

- [ ] **Step 8: Commit**

```bash
git add crates/wh-cli/src/cli.rs crates/wh-cli/src/keyset.rs crates/wh-cli/src/main.rs \
        crates/wh-cli/src/run.rs crates/wh-cli/tests/keyset.rs
git commit -m "[feat] - Add the wh keyset command tree and wh keyset list"
```

---

### Task 2: `wh keyset create`

**Files:**
- Modify: `crates/wh-cli/src/keyset.rs`
- Modify: `crates/wh-cli/src/run.rs` (replace the temporary `_ => bail!` arm)
- Test: `crates/wh-cli/tests/keyset.rs`

**Interfaces:**
- Consumes: task 1's `resolve_index`, `global_ap_or_bail`, `global_rt_or_bail`, `kind_of`.
- Produces, for task 4: `pub(crate) fn announce_steal(out: &mut impl Write, kind: Kind,
  losing: &[(u16, Vec<u8>)], new_index: u16) -> std::io::Result<()>;`

**What it must do, in order:**

1. Resolve the selector against the live matrix (`run::resolve_keys`).
2. `read_membership(s, kind)` for the whole board. Allocation must see every key.
3. `next_index(&m)`.
4. Work out the value: `--value` (or `--press`/`--release`) if given, else the global via
   `global_ap_or_bail`/`global_rt_or_bail`, which errors on `Split` or `NoneOutsideAKeyset`.
5. Work out which existing keysets lose members to this create, from `group(&m)` intersected with
   the selection, and print that **before** writing.
6. `plan(s, &usages, &change, Some(index))`, then `auto_backup`, then `apply`.
7. Verify by re-reading: every selected key holds the new index and the new value.

Step 5 matters because creating a keyset is destructive to its members' values: they are overwritten
with the global, not carried in. The operator must see which keys are about to lose what.

- [ ] **Step 1: Write the failing test for the announcement**

```rust
/// Creating a keyset over keys that already belong to one must say which keysets lose members
/// before it writes, because a create overwrites its members' values with the global rather than
/// carrying them in.
#[test]
fn keyset_create_announces_the_keys_it_steals() {
    // board: w,a in ap keyset 1 at 0.30mm; s,d free at 2.00mm. Create over w,s.
    let lines = create_script_stealing_w_from_keyset_1();
    let script = write_script("keyset-create-steal", &lines);
    let out = run_wh(
        &["keyset", "create", "ap", "--keys", "w,s", "--dry-run"],
        &script,
        &scratch_config_dir("keyset-create-steal"),
    );
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("keyset 1 loses w"), "got: {text}");
    assert!(text.contains("keyset 2"), "the new index must be named: {text}");
}
```

Write `create_script_stealing_w_from_keyset_1` beside it, using `matrix_lines`,
`layout_read_lines` and `key_settings_lines`. A dry run sends reads only, so the script needs: the
matrix, the `0xFF` sweep for every key on the board, the `0x04` reads `global_ap` performs over the
free keys, and `plan`'s six-layout read per selected key. Build it by running the command against a
short script first and reading `ReplayTransport`'s rejection message, which names the frame it did
not expect; that is the fastest way to get a fixture right and is how the existing `dump.rs`
fixtures were built.

Run: `cargo test -p wh-cli keyset_create`
Expected: FAIL, "not yet implemented".

- [ ] **Step 2: Implement `create`**

```rust
/// Creates a keyset over `usages` at the global value, or at an explicit one. Announces which
/// existing keysets lose members first: a create overwrites its members' values with the global
/// rather than carrying them in, so the operator sees what is about to go.
pub(crate) fn create<T: Transport>(
    out: &mut impl Write,
    s: &mut Session<T>,
    kind: Kind,
    usages: &[u8],
    value: Option<Um>,
    rt: Option<(Um, Um)>,
) -> Result<CreatePlan> {
    let m = keyset::read_membership(s, kind)?;
    let index = keyset::next_index(&m)?;
    let change = match kind {
        Kind::Ap => {
            let v = match value {
                Some(v) => v,
                None => global_ap_or_bail(s, &m, "--value")?,
            };
            keyset::Change::ap(v)
        }
        Kind::Rt => {
            let (p, r) = match rt {
                Some(v) => v,
                None => global_rt_or_bail(s, &m, "--press and --release")?,
            };
            keyset::Change::rt_on(p, r)
        }
    };
    let losing = losing_members(&keyset::group(&m), usages);
    announce_steal(out, kind, &losing, index.value())?;
    let plan = keyset::plan(s, usages, &change, Some(index))?;
    Ok(CreatePlan { index, plan })
}

/// Existing keysets that would lose members to a create over `usages`, as (index, the members
/// it loses), ascending by index.
fn losing_members(sets: &[Keyset], usages: &[u8]) -> Vec<(u16, Vec<u8>)> {
    sets.iter()
        .filter_map(|ks| {
            let taken: Vec<u8> = ks
                .members
                .iter()
                .copied()
                .filter(|u| usages.contains(u))
                .collect();
            (!taken.is_empty()).then(|| (ks.index, taken))
        })
        .collect()
}

pub(crate) fn announce_steal(
    out: &mut impl Write,
    kind: Kind,
    losing: &[(u16, Vec<u8>)],
    new_index: u16,
) -> std::io::Result<()> {
    writeln!(out, "{} keyset {new_index}: creating", kind_name(kind))?;
    for (index, taken) in losing {
        let names: Vec<String> = taken.iter().map(|&u| key_label(u)).collect();
        writeln!(out, "  keyset {index} loses {}", names.join(","))?;
    }
    Ok(())
}

pub(crate) struct CreatePlan {
    pub index: keyset::KeysetIndex,
    pub plan: keyset::WritePlan,
}
```

The `run.rs` arm calls `create`, then either prints `plan.frames()` on `--dry-run` or takes an
auto-backup and calls `apply` followed by the verification in step 4.

- [ ] **Step 3: Write the failing test for the refusal**

```rust
/// A board whose free keys disagree on the actuation point has no one global value, so a create
/// with no --value must refuse and name the disagreement rather than picking a winner.
#[test]
fn keyset_create_refuses_a_split_global_and_names_it() {
    let lines = create_script_with_a_split_global();
    let script = write_script("keyset-create-split", &lines);
    let out = run_wh(
        &["keyset", "create", "ap", "--keys", "w,s"],
        &script,
        &scratch_config_dir("keyset-create-split"),
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("disagree"), "got: {err}");
    assert!(err.contains("--value"), "the way out must be named: {err}");
}
```

- [ ] **Step 4: Verify the write landed**

Add to `keyset.rs`:

```rust
/// Re-reads every key the create touched and confirms it holds the new index. Reads the board
/// back rather than trusting the write's echo, the same way every other write path in `wh`
/// verifies. `read_key_settings` already returns both keyset layouts, so no new device call is
/// needed.
pub(crate) fn verify_membership<T: Transport>(
    out: &mut impl Write,
    s: &mut Session<T>,
    kind: Kind,
    usages: &[u8],
    want: u16,
) -> Result<()> {
    let mut bad = Vec::new();
    for &u in usages {
        let ks = wh_device::ops::read_key_settings(s, u)?;
        let got = match kind {
            Kind::Ap => ks.ap_keyset,
            Kind::Rt => ks.rt_keyset,
        };
        if got != want {
            bad.push(format!(
                "{}: board reports keyset {got}, wanted {want}",
                key_label(u)
            ));
        }
    }
    crate::run::report_verification(out, "keyset", usages, &bad)
}
```

`report_verification` is private to `run.rs`; make it `pub(crate)`. That is the only visibility
change this plan needs: `ops::read_key_settings` is already `pub` and already returns `ap_keyset`
and `rt_keyset`.

Run: `cargo test -p wh-cli keyset`
Expected: PASS. Then all three gates.

- [ ] **Step 5: Prove the tests bite**

Mutate `losing_members`'s `usages.contains(u)` to `!usages.contains(u)` and confirm
`keyset_create_announces_the_keys_it_steals` fails. Then mutate `global_ap_or_bail`'s `Split` arm to
return the first count's value instead of bailing, and confirm
`keyset_create_refuses_a_split_global_and_names_it` fails. Restore both, confirm
`git status --porcelain` is empty, and say so in your report.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "[feat] - Add wh keyset create with steal announcement and split refusal"
```

---

### Task 3: `wh keyset set` and `wh keyset delete`

One task, because both resolve an existing index to its members and then run a single `plan` over
them. They differ only in where the value comes from and whether membership is cleared.

**Files:**
- Modify: `crates/wh-cli/src/keyset.rs`, `crates/wh-cli/src/run.rs`
- Test: `crates/wh-cli/tests/keyset.rs`

**Interfaces:**
- Consumes: task 1's `resolve_index` and the two `*_or_bail` helpers, task 2's `verify_membership`.

**Semantics:**

| Command | Members | Value written | Membership |
|---|---|---|---|
| `keyset set ap N --value X` | every member of N | `X` | untouched (`None`) |
| `keyset set rt N --press P --release R` | every member of N | `P`/`R` | untouched |
| `keyset delete ap N` | every member of N | the global, or `--value` | `KeysetIndex::clear(Kind::Ap)` |
| `keyset delete rt N` | every member of N | the global, or `--press`/`--release` | `KeysetIndex::clear(Kind::Rt)` |

`set` with no value at all is an error, not a no-op: "pass --value" for `ap`, "pass --press and/or
--release" for `rt`. `delete` with no value falls back to the global and refuses on `Split`.

`delete` uses `Change::ap` for `Kind::Ap`, not `Change::ap_keeping_touch`. That is deliberate and
matches `docs/keysets.md`'s delete row, which owns `0x04` and rewrites MODE.

- [ ] **Step 1: Write the failing tests**

```rust
/// Changing a keyset's value writes every member, not just the one named, and writes no
/// membership record at all: the keyset keeps its index.
#[test]
fn keyset_set_writes_every_member_and_no_membership_record() {
    let script = write_script("keyset-set", &set_script_for_keyset_1_over_w_and_a());
    let out = run_wh(
        &["keyset", "set", "ap", "1", "--value", "1.20", "--dry-run"],
        &script,
        &scratch_config_dir("keyset-set"),
    );
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let frames = frame_lines(&String::from_utf8_lossy(&out.stdout));
    let joined = frames.join("\n");
    assert!(joined.contains("1a04"), "w's ap record must be present: {joined}");
    assert!(joined.contains("0404"), "a's ap record must be present: {joined}");
    assert!(!joined.contains("1aff"), "no 0xFF record may be sent: {joined}");
}

/// An index no key holds is an error naming what the board actually has, not a silent success
/// that writes to nobody.
#[test]
fn keyset_set_on_a_missing_index_names_the_live_ones() {
    let script = write_script("keyset-set-missing", &set_script_for_keyset_1_over_w_and_a());
    let out = run_wh(
        &["keyset", "set", "ap", "7", "--value", "1.20"],
        &script,
        &scratch_config_dir("keyset-set-missing"),
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no keyset 7"), "got: {err}");
    assert!(err.contains('1'), "the live indices must be named: {err}");
}

/// A delete clears membership and returns the members to the global value, in that order:
/// values first, membership last, one record per frame.
#[test]
fn keyset_delete_writes_values_before_clearing_membership() {
    let script = write_script("keyset-delete", &delete_script_for_keyset_1());
    let out = run_wh(
        &["keyset", "delete", "ap", "1", "--dry-run"],
        &script,
        &scratch_config_dir("keyset-delete"),
    );
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let frames = frame_lines(&String::from_utf8_lossy(&out.stdout));
    let membership_at = frames.iter().position(|f| f.contains("1aff")).expect("0xFF record");
    let value_at = frames.iter().position(|f| f.contains("1a04")).expect("0x04 record");
    assert!(value_at < membership_at, "values must precede membership");
    let after: Vec<&String> = frames[membership_at..].iter().collect();
    assert_eq!(after.len(), 2, "one membership record per frame, two members");
}
```

Run: `cargo test -p wh-cli keyset_set keyset_delete`
Expected: FAIL, "not yet implemented".

- [ ] **Step 2: Implement both**

```rust
/// Changes an existing keyset's value across every member. Membership is untouched: the keyset
/// keeps its index and its member list.
pub(crate) fn set_value<T: Transport>(
    s: &mut Session<T>,
    kind: Kind,
    index: u16,
    value: Option<Um>,
    rt: Option<(Um, Um)>,
) -> Result<(Vec<u8>, keyset::WritePlan)> {
    let m = keyset::read_membership(s, kind)?;
    let ks = resolve_index(&keyset::group(&m), index)?;
    let change = match kind {
        Kind::Ap => keyset::Change::ap(value.ok_or_else(|| {
            anyhow::anyhow!("pass --value to say what this keyset's actuation point becomes")
        })?),
        Kind::Rt => {
            let (p, r) = rt.ok_or_else(|| {
                anyhow::anyhow!("pass --press and --release to say what this keyset's rapid trigger sensitivity becomes")
            })?;
            keyset::Change::rt_on(p, r)
        }
    };
    let plan = keyset::plan(s, &ks.members, &change, None)?;
    Ok((ks.members, plan))
}

/// Deletes a keyset: its members return to the global value and their membership is cleared.
/// Values go out first and membership last, which is `plan`'s own ordering.
pub(crate) fn delete<T: Transport>(
    s: &mut Session<T>,
    kind: Kind,
    index: u16,
    value: Option<Um>,
    rt: Option<(Um, Um)>,
) -> Result<(Vec<u8>, keyset::WritePlan)> {
    let m = keyset::read_membership(s, kind)?;
    let ks = resolve_index(&keyset::group(&m), index)?;
    let change = match kind {
        Kind::Ap => {
            let v = match value {
                Some(v) => v,
                None => global_ap_or_bail(s, &m, "--value")?,
            };
            keyset::Change::ap(v)
        }
        Kind::Rt => {
            let (p, r) = match rt {
                Some(v) => v,
                None => global_rt_or_bail(s, &m, "--press and --release")?,
            };
            keyset::Change::rt_off(p, r)
        }
    };
    let plan = keyset::plan(s, &ks.members, &change, Some(keyset::KeysetIndex::clear(kind)))?;
    Ok((ks.members, plan))
}
```

Wire both into `run.rs`'s `keyset_cmd`, deleting the temporary `_ => bail!` arm. Each non-dry run
takes an `auto_backup(s, store, "keyset set")` / `"keyset delete"` before `apply`, the same as
`set ap` does.

- [ ] **Step 3: Run the tests and the gates**

Run: `cargo test -p wh-cli keyset`, then all three gates.
Expected: PASS.

- [ ] **Step 4: Prove the tests bite**

Change `delete` to pass `None` for membership instead of `Some(clear)` and confirm
`keyset_delete_writes_values_before_clearing_membership` fails on the missing `0xFF` record. Change
`set_value` to pass `Some(...)` and confirm
`keyset_set_writes_every_member_and_no_membership_record` fails. Restore both, confirm
`git status --porcelain` is empty, and say so.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "[feat] - Add wh keyset set and wh keyset delete"
```

---

### Task 4: `wh set ap` splits a keyset, and says so

**Files:**
- Modify: `crates/wh-cli/src/run.rs` (the `SetWhat::Ap` arm), `crates/wh-cli/src/keyset.rs`
- Test: `crates/wh-cli/tests/dump.rs`

**Interfaces:**
- Consumes: task 2's `losing_members` and `announce_steal`, `keyset::next_index`,
  `keyset::read_membership`.

**The rule, and where each half comes from:**

| Selection, against `0xFF` | What `wh set ap` does | Basis |
|---|---|---|
| No selected key is in a keyset | `plan` with `Change::ap`, membership `None` | **Measured.** `ap-wasd-1.2` writes no `0xFF` record at all |
| The selection is exactly one keyset's members | `plan` with `Change::ap`, membership `None` | **Measured.** `ks-value-ap` writes no `0xFF` record |
| Anything else | Allocate `next_index`, write it to every selected key, and announce the split | **Inferred**, from the vendor UI's observed behaviour in a screenshot, not from frames |

The third row is an inference and its comment in the code must say so. No capture in the corpus
shows the vendor splitting a keyset. What was observed is the configurator's own UI copying a
selection that included two existing members into a new keyset.

The second row's "exactly" is set equality, not "every selected key is in keyset N". A selection of
`w,s` where keyset 1 holds `w,a,s,d` is a strict subset and takes the third row.

- [ ] **Step 1: Write the failing tests**

```rust
/// `wh set ap` over keys in no keyset writes no membership record. Measured behaviour: the
/// vendor's own actuation point change in ap-wasd-1.2 sends no 0xFF record at all.
#[test]
fn set_ap_on_free_keys_writes_no_membership_record() {
    let script = write_script("set-ap-free", &set_ap_script_all_keys_free());
    let out = run_wh(
        &["set", "ap", "--keys", "w,a", "--set", "1.20", "--dry-run"],
        &script,
        &scratch_config_dir("set-ap-free"),
    );
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let joined = frame_lines(&String::from_utf8_lossy(&out.stdout)).join("\n");
    assert!(!joined.contains("1aff"), "no 0xFF record may be sent: {joined}");
}

/// `wh set ap` over exactly one keyset's members changes its value in place, keeping the index.
#[test]
fn set_ap_over_a_whole_keyset_keeps_its_index() {
    let script = write_script("set-ap-whole", &set_ap_script_w_and_a_in_keyset_1());
    let out = run_wh(
        &["set", "ap", "--keys", "w,a", "--set", "1.20", "--dry-run"],
        &script,
        &scratch_config_dir("set-ap-whole"),
    );
    assert!(out.status.success());
    let joined = frame_lines(&String::from_utf8_lossy(&out.stdout)).join("\n");
    assert!(!joined.contains("1aff"), "the keyset keeps its index: {joined}");
}

/// `wh set ap` over a strict subset of a keyset's members splits it, and says which keys moved
/// and where they went before it writes.
#[test]
fn set_ap_over_part_of_a_keyset_splits_it_and_announces_the_split() {
    let script = write_script("set-ap-split", &set_ap_script_wasd_in_keyset_1());
    let out = run_wh(
        &["set", "ap", "--keys", "w,s", "--set", "1.20", "--dry-run"],
        &script,
        &scratch_config_dir("set-ap-split"),
    );
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("keyset 1 loses w,s"), "got: {text}");
    assert!(text.contains("keyset 2"), "the new index must be named: {text}");
    assert!(frame_lines(&text).join("\n").contains("1aff"), "a 0xFF record must be sent");
}
```

Run: `cargo test -p wh-cli set_ap_`
Expected: the first two FAIL on the current `ops::ap_records` path (no membership read in the
script), the third FAILs outright.

- [ ] **Step 2: Implement the split decision**

Add to `crates/wh-cli/src/keyset.rs`:

```rust
/// What `wh set ap` should do about membership for `usages`. Two of the three cases are measured
/// and one is not: no capture in the corpus shows the vendor splitting a keyset, only its UI
/// copying a mixed selection into a new one.
pub(crate) enum ApMembership {
    /// Leave membership alone: either no selected key is in a keyset, or the selection is
    /// exactly one keyset's members and it keeps its index.
    Keep,
    /// Move every selected key into a newly allocated keyset, taking these members from these
    /// existing keysets.
    Split {
        index: keyset::KeysetIndex,
        losing: Vec<(u16, Vec<u8>)>,
    },
}

pub(crate) fn ap_membership_for(m: &Membership, usages: &[u8]) -> Result<ApMembership> {
    let sets = keyset::group(m);
    let losing = losing_members(&sets, usages);
    if losing.is_empty() {
        return Ok(ApMembership::Keep);
    }
    if losing.len() == 1 {
        let (index, taken) = &losing[0];
        let whole = sets
            .iter()
            .find(|k| k.index == *index)
            .is_some_and(|k| k.members.len() == taken.len());
        if whole && taken.len() == usages.len() {
            return Ok(ApMembership::Keep);
        }
    }
    Ok(ApMembership::Split {
        index: keyset::next_index(m)?,
        losing,
    })
}
```

Then rewrite `run.rs`'s `SetWhat::Ap` arm to read membership, call `ap_membership_for`, announce a
`Split` before writing, build the plan through `keyset::plan` with `Change::ap`, and print
`plan.frames()` on a dry run or `auto_backup` then `apply` otherwise.

`ops::ap_records` and `ops::set_ap` stay where they are; they are no longer on the `wh set ap` path.
Leave them and their tests alone. Removing them is not this task.

- [ ] **Step 3: Update the existing `set ap` fixtures**

`crates/wh-cli/tests/dump.rs` already has `set ap` tests written against `ops::ap_records`' frames.
The route changed, so their scripts change: `plan` reads six layouts per key where `ap_records` read
fewer, and a whole-board `0xFF` sweep now precedes everything. Rewrite the fixtures to match the new
traffic. **Do not loosen the replay matching to avoid rewriting them.**

- [ ] **Step 4: Run the tests and the gates**

Run: `cargo test --workspace`, then clippy and fmt.
Expected: PASS.

- [ ] **Step 5: Prove the tests bite**

Change `ap_membership_for`'s `taken.len() == usages.len()` to `taken.len() <= usages.len()` and
confirm `set_ap_over_part_of_a_keyset_splits_it_and_announces_the_split` fails, since a strict
subset would now be treated as the whole keyset. Restore, confirm `git status --porcelain` is empty,
and say so.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "[feat] - Route wh set ap through the keyset plan and split a keyset when it must"
```

---

### Task 5: `wh restore` restores membership

**Files:**
- Modify: `crates/wh-device/src/keyset.rs` (one new function), `crates/wh-device/src/ops.rs`
  (`restore_all`), `crates/wh-cli/src/run.rs` (`restore_records`, `verify_restore`)
- Test: `crates/wh-device/src/keyset.rs` unit tests, `crates/wh-cli/tests/dump.rs`

**Interfaces:**
- Produces:
  ```rust
  pub fn membership_records(entries: &[(u8, KeysetIndex)])
      -> Result<Vec<KeyRecord>, DeviceError>;
  ```

`wh restore` cannot go through `plan`: `plan` writes one index to every key it is given, and a
restore writes a different index per key. It also must reproduce indices with gaps that allocation
never reuses, which is what `KeysetIndex::restoring` exists for and its only caller.

- [ ] **Step 1: Add the helper to `wh-device`**

```rust
/// Membership records for keys that each carry their own index, which `plan` cannot express: it
/// writes one index to every key it is given. `wh restore` is the only caller, since a snapshot's
/// indices can include gaps allocation never reuses. Errors if the entries mix kinds, so one
/// layout's index can never be written to the other's layout.
pub fn membership_records(entries: &[(u8, KeysetIndex)]) -> Result<Vec<KeyRecord>, DeviceError> {
    let Some((_, first)) = entries.first() else {
        return Ok(Vec::new());
    };
    let kind = first.kind;
    for (_, idx) in entries {
        if idx.kind != kind {
            return Err(DeviceError::KeysetKindMismatch {
                expected: kind,
                found: idx.kind,
            });
        }
    }
    Ok(entries
        .iter()
        .map(|&(key, idx)| KeyRecord {
            key,
            layout: kind.layout(),
            value: idx.value,
        })
        .collect())
}
```

- [ ] **Step 2: Write its failing unit test**

```rust
    /// Mixing an actuation point index with a rapid trigger one in one call must error rather
    /// than writing one layout's index into the other layout, which would silently move keys
    /// between groupings.
    #[test]
    fn membership_records_refuses_mixed_kinds() {
        let entries = [
            (0x1Au8, KeysetIndex::restoring(Kind::Ap, 1)),
            (0x04, KeysetIndex::restoring(Kind::Rt, 1)),
        ];
        assert!(matches!(
            membership_records(&entries),
            Err(DeviceError::KeysetKindMismatch { .. })
        ));
    }

    /// Every record carries its own key's index and the layout its kind names.
    #[test]
    fn membership_records_carries_one_index_per_key() {
        let entries = [
            (0x1Au8, KeysetIndex::restoring(Kind::Ap, 3)),
            (0x04, KeysetIndex::restoring(Kind::Ap, 0)),
        ];
        let r = membership_records(&entries).unwrap();
        assert_eq!(r[0], KeyRecord { key: 0x1A, layout: layout::KEYSET_AP, value: 3 });
        assert_eq!(r[1], KeyRecord { key: 0x04, layout: layout::KEYSET_AP, value: 0 });
    }
```

Run: `cargo test -p wh-device membership_records`
Expected: FAIL until step 1 is in.

- [ ] **Step 3: Split `restore_all`**

```rust
/// Writes the global record, then every key's values batched, then membership one record per
/// frame, last. That ordering is the vendor's and is measured; batching membership with the
/// values would be a divergence for no gain.
pub fn restore_all<T: Transport>(
    s: &mut Session<T>,
    global: &cmds::GlobalTravel,
    records: &[KeyRecord],
    membership: &[KeyRecord],
) -> Result<(), DeviceError> {
    s.roundtrip(&cmds::write_global_travel(
        global.travel,
        global.press_dead,
        global.release_dead,
    ))?;
    write_records(s, records)?;
    for frame in cmds::write_key_records_singly(membership) {
        s.roundtrip(&frame)?;
    }
    Ok(())
}
```

`restore_all` has exactly one production caller, `crates/wh-cli/src/run.rs:1062`, and two unit
tests in `ops.rs` (`restore_all_writes_global_travel_then_key_batches_and_sends_no_save` and
`restore_all_skips_key_batch_when_there_are_no_records`). All three pass three arguments today and
need the fourth. Give the first test a membership record and extend its script, so the new argument
is exercised rather than always empty.

- [ ] **Step 4: Build the membership records in `run.rs`**

`RestoreKey` gains `ap_keyset: u16` and `rt_keyset: u16`, populated in `validate_restore_keys` from
`KeyToml`'s existing fields. Add:

```rust
/// Membership records for a restore, actuation point first then rapid trigger, each built
/// through `KeysetIndex::restoring` so an index from a snapshot can never be mistaken for one
/// allocation produced.
fn restore_membership_records(keys: &[RestoreKey]) -> Result<Vec<KeyRecord>> {
    let ap: Vec<(u8, KeysetIndex)> = keys
        .iter()
        .map(|k| (k.usage, KeysetIndex::restoring(Kind::Ap, k.ap_keyset)))
        .collect();
    let rt: Vec<(u8, KeysetIndex)> = keys
        .iter()
        .map(|k| (k.usage, KeysetIndex::restoring(Kind::Rt, k.rt_keyset)))
        .collect();
    let mut out = wh_device::keyset::membership_records(&ap)?;
    out.extend(wh_device::keyset::membership_records(&rt)?);
    Ok(out)
}
```

Extend `verify_restore` to read both layouts back per key and add them to its mismatch line.

- [ ] **Step 5: Write the failing end-to-end test**

```rust
/// A restore puts membership back, values first and membership last, one record per frame. A
/// snapshot that recorded a key in ap keyset 3 must leave the board with that key in keyset 3.
#[test]
fn restore_writes_keyset_membership_after_the_values() {
    let snapshot = snapshot_json_with_keysets(); // w in ap keyset 3, a in none
    let script = write_script("restore-keysets", &restore_script_with_keyset_readback());
    let out = run_wh(&["restore", &snapshot], &script, &scratch_config_dir("restore-keysets"));
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("restore:"));
}
```

The replay script is what actually asserts the ordering: `ReplayTransport` matches byte for byte, so
a membership frame sent before the value frames, or batched with them, fails the script.

- [ ] **Step 6: Update `docs/tasks.md`**

Strike through 2.4b and add a line under it saying `wh restore` now writes membership. Correct the
`KeyToml` doc comment in `crates/wh-config/src/snapshot.rs`, which currently says "`wh restore`
ignores them"; that becomes false with this task. This is exactly the kind of statement `CLAUDE.md`
requires fixing when a change makes it wrong.

- [ ] **Step 7: Run the tests and the gates**

Run: `cargo test --workspace`, then clippy and fmt.
Expected: PASS.

- [ ] **Step 8: Prove the tests bite**

Reorder `restore_all` to send membership before the values and confirm the end-to-end test fails on
a replay mismatch. Restore, confirm `git status --porcelain` is empty, and say so.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "[feat] - Restore keyset membership after the values, one record per frame"
```

---

### Task 6: Document the `wh keyset` tree for users

The whole feature otherwise lands undocumented: `README.md`'s command reference is where users read
what `wh` can do, and nothing in tasks 1 to 5 touches it.

**Files:**
- Modify: `README.md`, `docs/tasks.md`

**Interfaces:** none. This task adds no code.

- [ ] **Step 1: Add the tree to the command reference**

Add a `wh keyset` block to `README.md`'s `## Commands` section, in the style of the blocks already
there: one line per subcommand with a worked example, and one sentence each on the two behaviours a
user cannot guess. Those two are that **creating a keyset overwrites its members' values with the
board's global**, not with their own, and that **`wh set ap` over part of a keyset splits it into a
new one and says so**.

- [ ] **Step 2: Correct the statement this feature makes false**

`README.md:189` says "keyset membership is not yet something `wh` writes". That is true today and
false once task 2 lands. Rewrite it to say what `wh` now writes and what it still does not.
`CLAUDE.md` requires this: a change that makes an existing statement false must fix it, and this
repository has been caught by exactly that kind of leftover twice.

- [ ] **Step 3: Strike through 2.4b in `docs/tasks.md`**

Tick it, and list underneath what shipped: the `wh keyset` tree, `wh set ap` splitting, and
`wh restore` writing membership. Note that `ops::ap_records` and `ops::set_ap` remain in the tree
and are no longer on the `wh set ap` path, since a later reader will otherwise assume the live route
is the one they can see called from `run.rs`.

- [ ] **Step 4: Gates and commit**

Run all three gates. No code changed, so the test count should be unchanged.

```bash
git add README.md docs/tasks.md
git commit -m "[docs] - Document the wh keyset command tree and close 2.4b"
```

---

## What this plan deliberately leaves out

- **Task 2.13**, routing `wh set rt --off` through `keyset::plan` so it clears `0xFE`. It depends on
  this plan's interfaces but is its own task and its own entry.
- **Task 2.17**, the three code-relevant findings from the `docs/keysets.md` verification pass. The
  `0x16`/`0x17` one binds this plan (see Wire rules) and the other two are comment work.
- **Task 2.16**, comment cleanup in `wh-device`.
- **Removing `ops::ap_records` and `ops::set_ap`.** Task 4 takes them off the `wh set ap` path and
  leaves them in place. Deleting them is a separate decision.
- **A `--json` output mode for `wh keyset list`.** `wh keys list` and `wh backups list` both print
  human text; this matches them.

## Ruling recorded after task 1's review

The review found that this plan scheduled no user-facing documentation anywhere, so the feature
would have shipped with `README.md` silently claiming `wh` does not write keyset membership. Task 6
above was added for that. It is last because the command reference is only worth writing once the
tree is complete.

## Self-review

**Spec coverage.** The three things `docs/tasks.md` lists under 2.4b are the `wh keyset` tree
(tasks 1 to 3), `wh set ap` splitting (task 4), and `wh restore` writing membership plus
`verify_restore` checking it (task 5). All four decisions in "Decisions this plan implements" have a
task that implements them: 2.14 in task 4, 2.15 in tasks 2 and 3, the destructive-create warning in
task 2, the split announcement in tasks 2 and 4.

**Placeholders.** Every code step carries real code. The two places that say "build it by running
the command against a short script and reading the rejection" are fixture construction, where the
byte sequence depends on code the implementer will have just written; the method is specified, and
it is how the existing fixtures were built.

**Type consistency.** `Kind`, `KeysetIndex`, `Change`, `WritePlan` and `Global` are used with the
signatures quoted from `keyset.rs` at the top. `kind_of` converts the clap enum to the device one
once, in task 1, and every later task uses `Kind`. `ApMembership` is introduced in task 4 and used
only there. `CreatePlan` is introduced in task 2 and used only there.

**Two visibility changes.** `wh_device::ops::read_layout_value` went from `pub(crate)` to `pub`
in task 1, so `wh keyset list` can read one layout per member rather than the six
`read_key_settings` returns. `run::report_verification` goes from private to `pub(crate)` in task 2.
Task 2's verification itself needs no new device call: `ops::read_key_settings` already returns
`ap_keyset` and `rt_keyset`.
