//! End-to-end tests of `wh keyset list`, `create`, `set`, and `delete` over replay scripts.
//!
//! `ReplayTransport` matches each outgoing frame against the script byte for byte and rejects
//! anything else, on purpose: an unscripted, reordered, or otherwise-different send must fail
//! loudly. Loosening that match to make a test pass would defeat the harness.

use std::process::Command;
use wh_device::replay::hex;
use wh_proto::cmds::{self, layout, KeyRecord};

fn out_line(bytes: &[u8; 64]) -> String {
    format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(bytes))
}

fn in_line(bytes: &[u8; 64]) -> String {
    format!("{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(bytes))
}

/// Builds a reply frame the way the real device sends it: with the high bit
/// set on the command byte (see `wh_proto::frame::REPLY_BIT`), so fixtures
/// built through this helper are faithful to the wire.
fn reply(cmd: u8, payload: &[u8]) -> [u8; 64] {
    wh_proto::frame::frame(cmd | wh_proto::frame::REPLY_BIT, payload).unwrap()
}

/// A scratch directory unique to this test and process, used as its own `XDG_CONFIG_HOME`.
/// `run_wh` always sets one; a write-path test's auto-backup lands here, and even a `list` test,
/// which touches no config, still gets its own rather than racing another test's over the same
/// path.
fn scratch_config_dir(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("wh-cli-it-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// A DEFKEY reply payload for one row pair: `[rw, row_a, 21 usages, row_b, 21 usages]`, with
/// at most the first column of each row populated. `None` leaves a row empty (no keys), which
/// is what the third row pair of this four-key board needs.
fn defkey_payload(row_a: u8, row_b: u8, a_col0: Option<u8>, b_col0: Option<u8>) -> Vec<u8> {
    let mut payload = vec![0u8; 45];
    payload[1] = row_a;
    if let Some(u) = a_col0 {
        payload[2] = u;
    }
    payload[23] = row_b;
    if let Some(u) = b_col0 {
        payload[24] = u;
    }
    payload
}

/// The three DEFKEY roundtrips that make up `ops::read_matrix` for a four-key board: 'w' (0x1A)
/// and 'a' (0x04) in the first row pair, 's' (0x16) and 'd' (0x07) in the second, so
/// `read_matrix` reports them in exactly that order, w, a, s, d.
fn matrix_lines() -> Vec<String> {
    let mut lines = Vec::new();
    let row_pairs = [(0u8, 1u8), (2u8, 3u8), (4u8, 5u8)];
    for (i, &(a, b)) in row_pairs.iter().enumerate() {
        lines.push(out_line(&cmds::read_defkey_rows(a, b)));
        let payload = match i {
            0 => defkey_payload(a, b, Some(0x1A), Some(0x04)),
            1 => defkey_payload(a, b, Some(0x16), Some(0x07)),
            _ => defkey_payload(a, b, None, None),
        };
        lines.push(in_line(&reply(cmds::cmd::DEFKEY, &payload)));
    }
    lines
}

/// True if `s` contains a run of at least `n` consecutive hex-digit characters anywhere in it,
/// not just as the whole string.
fn contains_hex_run(s: &str, n: usize) -> bool {
    let mut run = 0usize;
    for c in s.chars() {
        if c.is_ascii_hexdigit() {
            run += 1;
            if run >= n {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

/// Every `stdout` line containing a 64-byte frame's hex, verbatim and in order. `--dry-run`
/// prints one bare frame per line, so a line that wraps a frame in other text is captured whole
/// and fails the comparison loudly instead of being silently skipped.
fn frame_lines(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|l| contains_hex_run(l, 128))
        .map(str::to_string)
        .collect()
}

fn write_script(tag: &str, lines: &[String]) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("wh-{tag}-{}.jsonl", std::process::id()));
    std::fs::write(&path, lines.join("\n")).unwrap();
    path
}

fn run_wh(
    args: &[&str],
    replay: &std::path::Path,
    config_home: &std::path::Path,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_wh"))
        .env("WH_REPLAY", replay)
        .env("XDG_CONFIG_HOME", config_home)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap()
}

/// `run_wh` with a line on stdin, for the commands that ask for a typed confirmation.
fn run_wh_stdin(
    args: &[&str],
    replay: &std::path::Path,
    config_home: &std::path::Path,
    input: &str,
) -> std::process::Output {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_wh"))
        .env("WH_REPLAY", replay)
        .env("XDG_CONFIG_HOME", config_home)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// One `read_layout_value` roundtrip: a single-record read request for `usage`/`layout`, and the
/// reply carrying `value`. Built from `cmds::read_key_layout` and the same reply shape
/// `ops::read_layout_value` parses, matching what `keyset::read_membership` and `keyset::list`
/// actually send.
fn layout_read_lines(usage: u8, layout: u8, value: u16) -> Vec<String> {
    vec![
        out_line(&cmds::read_key_layout(usage, layout)),
        in_line(&reply(
            cmds::cmd::KEY,
            &[
                0x00,
                usage,
                layout,
                (value & 0xFF) as u8,
                (value >> 8) as u8,
            ],
        )),
    ]
}

/// One key's full `read_key_settings` script, in the order it issues reads: AP, MODE, RT_PRESS,
/// RT_RELEASE, KEYSET_AP, KEYSET_RT. Matches `keyset::plan`'s own per-key read order.
#[allow(clippy::too_many_arguments)]
fn key_settings_lines(
    usage: u8,
    ap: u16,
    mode: u16,
    rt_press: u16,
    rt_release: u16,
    ap_keyset: u16,
    rt_keyset: u16,
) -> Vec<String> {
    let mut lines = Vec::new();
    for (lid, val) in [
        (layout::AP, ap),
        (layout::MODE, mode),
        (layout::RT_PRESS, rt_press),
        (layout::RT_RELEASE, rt_release),
        (layout::KEYSET_AP, ap_keyset),
        (layout::KEYSET_RT, rt_keyset),
    ] {
        lines.extend(layout_read_lines(usage, lid, val));
    }
    lines
}

/// The SYNC roundtrip `ops::device_info` sends, as `[out, in]` lines.
fn sync_lines(serial: &str, firmware: &str) -> Vec<String> {
    let mut payload = vec![0u8; 60];
    let s = serial.as_bytes();
    payload[8] = s.len() as u8;
    payload[9..9 + s.len()].copy_from_slice(s);
    let f = firmware.as_bytes();
    let fw_len_pos = 9 + s.len();
    payload[fw_len_pos] = f.len() as u8;
    let fw_start = fw_len_pos + 1;
    payload[fw_start..fw_start + f.len()].copy_from_slice(f);
    vec![
        out_line(&cmds::sync()),
        in_line(&reply(cmds::cmd::SYNC, &payload)),
    ]
}

/// The profile-read roundtrip `ops::profile` sends, `idx` the wire's zero-based index.
fn profile_lines(idx: u8) -> Vec<String> {
    vec![
        out_line(&cmds::read_profile()),
        in_line(&reply(cmds::cmd::CMD, &[0x00, 0x70, idx, 0xFF])),
    ]
}

/// The DB read roundtrip `ops::global_travel` sends, in micrometres.
fn global_travel_lines(travel_um: u16, press_um: u16, release_um: u16) -> Vec<String> {
    let mut payload = [0u8; 9];
    payload[3..5].copy_from_slice(&travel_um.to_le_bytes());
    payload[5..7].copy_from_slice(&press_um.to_le_bytes());
    payload[7..9].copy_from_slice(&release_um.to_le_bytes());
    vec![
        out_line(&cmds::read_global_travel()),
        in_line(&reply(cmds::cmd::DB, &payload)),
    ]
}

/// One key's `(ap, mode, rt_press, rt_release, ap_keyset, rt_keyset)`, the shape
/// `auto_backup_lines` takes one of per board key.
type KeyState = (u16, u16, u16, u16, u16, u16);

/// The full `snapshot_from_device` script `auto_backup` sends against the four-key board: sync,
/// profile, global travel, the matrix a further time, then each key's six-layout read in matrix
/// order (w, a, s, d).
fn auto_backup_lines(
    profile_idx: u8,
    w: KeyState,
    a: KeyState,
    s: KeyState,
    d: KeyState,
) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(sync_lines("SNKEYSETTEST0001", "V1.0.0.001"));
    lines.extend(profile_lines(profile_idx));
    lines.extend(global_travel_lines(500, 200, 200));
    lines.extend(matrix_lines());
    for (usage, (ap, mode, press, release, apks, rtks)) in
        [(0x1Au8, w), (0x04, a), (0x16, s), (0x07, d)]
    {
        lines.extend(key_settings_lines(
            usage, ap, mode, press, release, apks, rtks,
        ));
    }
    lines
}

/// `wh keyset create ap --keys w,s` over a board where w,a already hold ap keyset 1 at 0.30mm
/// and s,d are free at 2.00mm: the matrix (for `run::resolve_keys`), the matrix again and the
/// 0xFF sweep (for `keyset::read_membership`), the 0x04 reads `global_ap` performs over the free
/// keys s and d, and `plan`'s six-layout read for each selected key, w then s.
fn create_script_stealing_w_from_keyset_1() -> Vec<String> {
    let mut lines = matrix_lines();
    lines.extend(matrix_lines());
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(layout_read_lines(0x16, layout::AP, 2000));
    lines.extend(layout_read_lines(0x07, layout::AP, 2000));
    lines.extend(key_settings_lines(0x1A, 300, 0x18, 100, 150, 1, 0));
    lines.extend(key_settings_lines(0x16, 2000, 0x18, 100, 150, 0, 0));
    lines
}

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
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    // Both the target (the header) and the prior value (the loss line) are pinned, and they
    // differ (0.30mm vs 2.00mm): a mutation that printed the target where the prior value
    // belongs, or the reverse, fails at least one of these.
    assert!(
        text.contains("ap keyset 2: creating at 2.00mm"),
        "got: {text}"
    );
    assert!(text.contains("keyset 1 loses w at 0.30mm"), "got: {text}");
    // `s` already sits at the target AP, so `plan`'s skip rule gives it no value records, only a
    // membership one. Dropping `s` from the plan entirely would still pass the asserts above, so
    // pin its membership frame directly too.
    let s_membership = cmds::write_key_records_singly(&[KeyRecord {
        key: 0x16,
        layout: layout::KEYSET_AP,
        value: 2,
    }])[0];
    assert!(
        text.contains(&hex(&s_membership)),
        "s's membership frame must be in the plan too, not just w's: {text}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-create-steal"));
}

/// A stolen member already at the target value: `w` sits in ap keyset 1 at 2.00mm, exactly the
/// board's global, so a create with no `--value` gives `plan`'s skip rule nothing to write for
/// it. The announcement must say `w` keeps its value, not "loses w at 2.00mm", which would claim
/// the opposite of what is about to happen.
#[test]
fn keyset_create_announces_a_kept_value_differently_from_a_lost_one() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(layout_read_lines(0x16, layout::AP, 2000)); // s, free
    lines.extend(layout_read_lines(0x07, layout::AP, 2000)); // d, free
    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 1, 0)); // plan's read of w

    let script = write_script("keyset-create-keeps-value", &lines);
    let config_home = scratch_config_dir("keyset-create-keeps-value");
    let out = run_wh(
        &["keyset", "create", "ap", "--keys", "w", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("keyset 1 loses w (keeps 2.00mm, index only)"),
        "got: {text}"
    );
    assert!(
        !text.contains("loses w at 2.00mm"),
        "must not claim w loses a value it keeps: {text}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// A stolen member whose touch mode promotes (`Global` to `Single`) while its actuation point
/// does not move at all: `w` sits in ap keyset 1 at 2.00mm with touch `Global`, exactly the
/// board's global, so only its touch mode changes. "index only" would be wrong here, since the
/// mode moves too; "at 2.00mm" would be wrong too, since the value itself never does; and saying
/// nothing about the mode would leave the one real change unnamed.
#[test]
fn keyset_create_announces_a_promoted_key_that_keeps_its_value() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(layout_read_lines(0x16, layout::AP, 2000)); // s, free
    lines.extend(layout_read_lines(0x07, layout::AP, 2000)); // d, free
    lines.extend(key_settings_lines(0x1A, 2000, 0x00, 100, 150, 1, 0)); // plan's read of w: touch Global

    let script = write_script("keyset-create-promotes-only", &lines);
    let config_home = scratch_config_dir("keyset-create-promotes-only");
    let out = run_wh(
        &["keyset", "create", "ap", "--keys", "w", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("keyset 1 loses w (keeps 2.00mm, mode Global to Single)"),
        "got: {text}"
    );
    assert!(
        !text.contains("keeps 2.00mm, index only"),
        "more than the index changes here, the mode promotes too: {text}"
    );
    assert!(
        !text.contains("loses w at 2.00mm"),
        "the actuation point itself never moves: {text}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The rapid trigger sibling of `keyset_create_announces_the_keys_it_steals`: `w` sits in an rt
/// keyset at 0.50/0.50mm, and a create with different press/release targets is about to overwrite
/// that. Every other announcement fixture in this file creates `ap`; `value_moves`'s `Kind::Rt`
/// arm has no coverage without this one.
#[test]
fn keyset_create_announces_a_rapid_trigger_steal() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 0), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, ks));
    }
    lines.extend(key_settings_lines(0x1A, 2000, 0x38, 500, 500, 0, 1)); // plan's read of w

    let script = write_script("keyset-create-rt-steal", &lines);
    let config_home = scratch_config_dir("keyset-create-rt-steal");
    let out = run_wh(
        &[
            "keyset",
            "create",
            "rt",
            "--keys",
            "w",
            "--press",
            "0.10",
            "--release",
            "0.30",
            "--dry-run",
        ],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("rt keyset 2: creating at 0.10/0.30mm"),
        "got: {text}"
    );
    assert!(
        text.contains("keyset 1 loses w at 0.50/0.50mm"),
        "got: {text}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The common case: `wh keyset create ap --keys w,a,s,d` where none of the four is in any
/// keyset. `losing` is empty, so before the shared `announce_steal` gained a free-key line this
/// printed only its header and nothing about the four members it was about to overwrite. Each
/// key is given a distinct prior actuation point, so a mutation that reused one key's value for
/// every line, or dropped this case to silence again, fails this exact match.
#[test]
fn keyset_create_announces_every_enrolled_key_over_an_all_free_selection() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership
    for (usage, ks) in [(0x1Au8, 0u16), (0x04, 0), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 0, 0)); // plan's read of w
    lines.extend(key_settings_lines(0x04, 1900, 0x18, 100, 150, 0, 0)); // plan's read of a
    lines.extend(key_settings_lines(0x16, 1800, 0x18, 100, 150, 0, 0)); // plan's read of s
    lines.extend(key_settings_lines(0x07, 1700, 0x18, 100, 150, 0, 0)); // plan's read of d

    let script = write_script("keyset-create-all-free", &lines);
    let config_home = scratch_config_dir("keyset-create-all-free");
    let out = run_wh(
        &[
            "keyset",
            "create",
            "ap",
            "--keys",
            "w,a,s,d",
            "--value",
            "1.20",
            "--dry-run",
        ],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("ap keyset 1: creating at 1.20mm"),
        "got: {text}"
    );
    assert!(
        !text.contains("loses"),
        "no keyset exists to lose anything from: {text}"
    );
    assert!(
        text.contains("enrolling free key(s) w at 2.00mm,a at 1.90mm,s at 1.80mm,d at 1.70mm"),
        "got: {text}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `wh keyset create ap --keys w,s` with no `--value`, where the free keys s and d disagree on
/// the board's actuation point: the matrix, the matrix again and the 0xFF sweep, then the two
/// disagreeing 0x04 reads over s and d. `global_ap_or_bail` must refuse before `plan` is ever
/// called, so the script needs nothing past those two reads.
fn create_script_with_a_split_global() -> Vec<String> {
    let mut lines = matrix_lines();
    lines.extend(matrix_lines());
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(layout_read_lines(0x16, layout::AP, 1000));
    lines.extend(layout_read_lines(0x07, layout::AP, 2000));
    lines
}

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

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-create-split"));
}

/// `--press`/`--release` are meaningless on an ap create: only `--value` names an actuation
/// point. Silently ignoring them would leave a typo'd command believing it set a sensitivity
/// that was never used, so this must refuse before it even opens the replay script.
#[test]
fn keyset_create_ap_refuses_rapid_trigger_flags() {
    let config_home = scratch_config_dir("keyset-create-ap-refuse");
    let out = run_wh(
        &[
            "keyset",
            "create",
            "ap",
            "--keys",
            "w",
            "--press",
            "0.10",
            "--release",
            "0.20",
            "--dry-run",
        ],
        std::path::Path::new("/nonexistent-keyset-create-refuse.jsonl"),
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("--press") && err.contains("--release"),
        "got: {err}"
    );

    let _ = std::fs::remove_dir_all(&config_home);
}

/// A single rapid trigger flag alone must refuse too on `create`, the same one-flag hole checked
/// on `set` and `delete`: `create` is destructive too, overwriting every selected key's value.
#[test]
fn keyset_create_ap_refuses_press_alone() {
    let config_home = scratch_config_dir("keyset-create-ap-refuse-press");
    let out = run_wh(
        &["keyset", "create", "ap", "--keys", "w", "--press", "0.10"],
        std::path::Path::new("/nonexistent-keyset-create-refuse-press.jsonl"),
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--press"), "got: {err}");

    let _ = std::fs::remove_dir_all(&config_home);
}

/// The `--release`-alone mirror of the test above.
#[test]
fn keyset_create_ap_refuses_release_alone() {
    let config_home = scratch_config_dir("keyset-create-ap-refuse-release");
    let out = run_wh(
        &["keyset", "create", "ap", "--keys", "w", "--release", "0.20"],
        std::path::Path::new("/nonexistent-keyset-create-refuse-release.jsonl"),
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--release"), "got: {err}");

    let _ = std::fs::remove_dir_all(&config_home);
}

/// One key's post-write readback, varied one field at a time by the branch-coverage tests below
/// while every other field stays at the value a correct write would leave: `(ap, mode, rt_press,
/// rt_release, ap_keyset, rt_keyset)`.
#[derive(Clone, Copy)]
struct Readback {
    ap: u16,
    mode: u16,
    rt_press: u16,
    rt_release: u16,
    ap_keyset: u16,
    rt_keyset: u16,
}

/// `s`'s correct post-write readback for the ap create scripts below: AP moved to the 2.00mm
/// target, MODE unchanged (already `Single`), membership moved to keyset 1.
const S_CORRECT_AP: Readback = Readback {
    ap: 2000,
    mode: 0x18,
    rt_press: 100,
    rt_release: 150,
    ap_keyset: 1,
    rt_keyset: 0,
};

/// The full script for `wh keyset create ap --keys w,s --value 2.00` against a four-key board
/// with no existing ap keysets: `resolve_keys`' matrix read, `read_membership`'s matrix and 0xFF
/// sweep, `plan`'s six-layout read for w then s, the auto-backup snapshot, the value batch and
/// the two membership frames, then the readback for both selected keys. `w` always reads back
/// correctly; `s_readback` lets the tests below vary exactly one of `s`'s fields.
fn create_ap_write_script(s_readback: Readback) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(matrix_lines()); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    lines.extend(key_settings_lines(0x1A, 1000, 0x18, 100, 150, 0, 0)); // plan's read of w
    lines.extend(key_settings_lines(0x16, 1500, 0x18, 100, 150, 0, 0)); // plan's read of s

    lines.extend(auto_backup_lines(
        0,
        (1000, 0x18, 100, 150, 0, 0), // w
        (1200, 0x00, 0, 0, 0, 0),     // a
        (1500, 0x18, 100, 150, 0, 0), // s
        (1500, 0x00, 0, 0, 0, 0),     // d
    ));

    let value_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 150,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    for f in cmds::write_key_records(&value_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    let membership_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_AP,
            value: 1,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::KEYSET_AP,
            value: 1,
        },
    ];
    for f in cmds::write_key_records_singly(&membership_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 1, 0)); // w readback: correct
    lines.extend(key_settings_lines(
        0x16,
        s_readback.ap,
        s_readback.mode,
        s_readback.rt_press,
        s_readback.rt_release,
        s_readback.ap_keyset,
        s_readback.rt_keyset,
    )); // s readback

    lines
}

/// `wh keyset create ap --keys w,s --value 2.00` end to end, with a fully correct readback: the
/// auto-backup phase, the value batch, the two membership frames, and a readback that matches for
/// both keys. Exit 0, "verified" in stdout, and a real backup file on disk, not just the message
/// claiming one.
///
/// Byte-for-byte lever: the script's first frame after `create`'s own reads is the auto-backup's
/// SYNC read. If the write ran before the backup, the actual first frame sent there would be a
/// write frame instead, which would not match this script's next expected frame at all.
#[test]
fn keyset_create_ap_end_to_end_backs_up_writes_and_verifies() {
    let lines = create_ap_write_script(S_CORRECT_AP);
    let script = write_script("keyset-create-ap-ok", &lines);
    let config_home = scratch_config_dir("keyset-create-ap-ok");

    let out = run_wh(
        &["keyset", "create", "ap", "--keys", "w,s", "--value", "2.00"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ap keyset create: 2 keys verified"),
        "got: {stdout}"
    );
    // `create` always writes membership, but `wh restore` writes membership back too now, so the
    // old restore-coverage warning must not print, on a clean success any more than on failure.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("wh restore does not yet write keyset membership"),
        "got: {stderr}"
    );

    let backups = std::fs::read_dir(config_home.join("wh").join("backups"))
        .unwrap()
        .count();
    assert_eq!(backups, 1, "expected exactly one auto-backup file on disk");

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `s`'s actuation point write silently fails to land (the board still reports its pre-write
/// value) while everything else about it lands correctly, exactly the shape of the reviewer's
/// original repro: membership alone can't tell this apart from a real success.
///
/// Output-assertion lever: the mismatch sits on `s`, the *second* selected key, so a verifier
/// that stopped after the first key would never read it back, still claim success, and exit 0 on
/// a board that had not fully changed.
#[test]
fn keyset_create_ap_end_to_end_catches_a_value_that_never_landed() {
    let lines = create_ap_write_script(Readback {
        ap: 1500, // s still reports its pre-write AP
        ..S_CORRECT_AP
    });
    let script = write_script("keyset-create-ap-mismatch", &lines);
    let config_home = scratch_config_dir("keyset-create-ap-mismatch");

    let out = run_wh(
        &["keyset", "create", "ap", "--keys", "w,s", "--value", "2.00"],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("s: board reports ap 1.50mm, wanted 2.00mm"),
        "got: {err}"
    );
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("verified"),
        "must not claim success on a board that did not change"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `s`'s MODE write silently fails to land while its AP and membership writes both do. Every
/// earlier fixture in this file answers the MODE readback correctly, so this is the first one
/// that fails on it: a comparison that dropped the MODE check entirely would have shipped
/// undetected otherwise.
///
/// Output-assertion lever, mismatch on the second selected key, same reasoning as above.
#[test]
fn keyset_create_ap_end_to_end_catches_a_mode_that_never_landed() {
    let lines = create_ap_write_script(Readback {
        mode: 0x28, // s still reports RtGlobal touch instead of the target Single
        ..S_CORRECT_AP
    });
    let script = write_script("keyset-create-ap-mode-mismatch", &lines);
    let config_home = scratch_config_dir("keyset-create-ap-mode-mismatch");

    let out = run_wh(
        &["keyset", "create", "ap", "--keys", "w,s", "--value", "2.00"],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("s: board reports mode 0x0028 (rt on), wanted mode 0x0018 (rt off)"),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `s`'s membership write silently fails to land while its AP and MODE writes both do: the exact
/// case a membership comparison mutated away would miss, even on the branch the function is
/// named for.
///
/// Output-assertion lever, mismatch on the second selected key, same reasoning as above.
#[test]
fn keyset_create_ap_end_to_end_catches_a_membership_that_never_landed() {
    let lines = create_ap_write_script(Readback {
        ap_keyset: 0, // s still reports no ap keyset
        ..S_CORRECT_AP
    });
    let script = write_script("keyset-create-ap-membership-mismatch", &lines);
    let config_home = scratch_config_dir("keyset-create-ap-membership-mismatch");

    let out = run_wh(
        &["keyset", "create", "ap", "--keys", "w,s", "--value", "2.00"],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("s: board reports ap keyset 0, wanted 1"),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Both `w` and `s`'s membership writes silently do nothing (the board reads `ap_keyset` back as
/// 0 for both), while both keys' actuation point writes land correctly, matching the reviewer's
/// F5 repro exactly: the write frames carry index 1 for both keys, byte for byte, but neither
/// took.
///
/// `verify_write` takes no caller-supplied index any more, only `plan`, so there is no call site
/// left where a wrong index could be substituted; this pins that the index it actually checks
/// against, `1`, still comes from `plan.membership_records()` and not from any assumption baked
/// into the check itself.
#[test]
fn keyset_create_ap_end_to_end_catches_a_membership_write_that_did_nothing_for_either_key() {
    let mut lines = Vec::new();
    lines.extend(matrix_lines()); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    lines.extend(key_settings_lines(0x1A, 1000, 0x18, 100, 150, 0, 0)); // plan's read of w
    lines.extend(key_settings_lines(0x16, 1500, 0x18, 100, 150, 0, 0)); // plan's read of s

    lines.extend(auto_backup_lines(
        0,
        (1000, 0x18, 100, 150, 0, 0), // w
        (1200, 0x00, 0, 0, 0, 0),     // a
        (1500, 0x18, 100, 150, 0, 0), // s
        (1500, 0x00, 0, 0, 0, 0),     // d
    ));

    let value_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 150,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    for f in cmds::write_key_records(&value_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    let membership_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_AP,
            value: 1,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::KEYSET_AP,
            value: 1,
        },
    ];
    for f in cmds::write_key_records_singly(&membership_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    // Both keys: AP landed, membership did not.
    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 0, 0));
    lines.extend(key_settings_lines(0x16, 2000, 0x18, 100, 150, 0, 0));

    let script = write_script("keyset-create-ap-membership-neither", &lines);
    let config_home = scratch_config_dir("keyset-create-ap-membership-neither");

    let out = run_wh(
        &["keyset", "create", "ap", "--keys", "w,s", "--value", "2.00"],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("w: board reports ap keyset 0, wanted 1"),
        "got: {err}"
    );
    assert!(
        err.contains("s: board reports ap keyset 0, wanted 1"),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `s`'s correct post-write readback for the rt create scripts below: sensitivities at the
/// 0.10mm/0.30mm target, MODE moved from `Single` to `Rt`, membership moved to keyset 1. `ap`
/// stays at `s`'s unchanged pre-write value, since `Change::rt_on` never touches it.
const S_CORRECT_RT: Readback = Readback {
    ap: 1500,
    mode: 0x38,
    rt_press: 100,
    rt_release: 300,
    ap_keyset: 0,
    rt_keyset: 1,
};

/// The full script for `wh keyset create rt --keys w,s --press 0.10 --release 0.30` against a
/// four-key board with no existing rt keysets, the same shape as `create_ap_write_script`: `w`
/// always reads back correctly; `s_readback` lets the tests below vary exactly one of `s`'s
/// fields.
fn create_rt_write_script(s_readback: Readback) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(matrix_lines()); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, 0));
    }
    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 999, 999, 0, 0)); // plan's read of w
    lines.extend(key_settings_lines(0x16, 1500, 0x18, 999, 999, 0, 0)); // plan's read of s

    lines.extend(auto_backup_lines(
        0,
        (2000, 0x18, 999, 999, 0, 0), // w
        (1200, 0x00, 0, 0, 0, 0),     // a
        (1500, 0x18, 999, 999, 0, 0), // s
        (1500, 0x00, 0, 0, 0, 0),     // d
    ));

    let value_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x38,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 300,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::MODE,
            value: 0x38,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::AP,
            value: 1500,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::RT_RELEASE,
            value: 300,
        },
    ];
    for f in cmds::write_key_records(&value_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    let membership_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_RT,
            value: 1,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::KEYSET_RT,
            value: 1,
        },
    ];
    for f in cmds::write_key_records_singly(&membership_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    lines.extend(key_settings_lines(0x1A, 2000, 0x38, 100, 300, 0, 1)); // w readback: correct
    lines.extend(key_settings_lines(
        0x16,
        s_readback.ap,
        s_readback.mode,
        s_readback.rt_press,
        s_readback.rt_release,
        s_readback.ap_keyset,
        s_readback.rt_keyset,
    )); // s readback

    lines
}

/// `wh keyset create rt --keys w,s --press 0.10 --release 0.30` end to end, press and release
/// deliberately different values, with a fully correct readback.
///
/// Byte-for-byte lever: if `--press`/`--release` resolution swapped the two (e.g. bound press to
/// `release.or(value)`), the write batch's RT_PRESS record would carry 300 instead of 100, which
/// would not match this script's scripted write frame at all.
#[test]
fn keyset_create_rt_end_to_end_writes_press_and_release_independently() {
    let lines = create_rt_write_script(S_CORRECT_RT);
    let script = write_script("keyset-create-rt-ok", &lines);
    let config_home = scratch_config_dir("keyset-create-rt-ok");

    let out = run_wh(
        &[
            "keyset",
            "create",
            "rt",
            "--keys",
            "w,s",
            "--press",
            "0.10",
            "--release",
            "0.30",
        ],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("rt keyset create: 2 keys verified"),
        "got: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `s`'s press sensitivity write silently fails to land while release, MODE and membership all
/// land correctly.
///
/// Output-assertion lever, mismatch on the second selected key, same reasoning as the ap tests
/// above.
#[test]
fn keyset_create_rt_end_to_end_catches_a_press_that_never_landed() {
    let lines = create_rt_write_script(Readback {
        rt_press: 500, // s still reports the wrong press
        ..S_CORRECT_RT
    });
    let script = write_script("keyset-create-rt-press-mismatch", &lines);
    let config_home = scratch_config_dir("keyset-create-rt-press-mismatch");

    let out = run_wh(
        &[
            "keyset",
            "create",
            "rt",
            "--keys",
            "w,s",
            "--press",
            "0.10",
            "--release",
            "0.30",
        ],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "s: board reports press 0.50mm release 0.30mm, wanted press 0.10mm release 0.30mm"
        ),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `s`'s release sensitivity write silently fails to land while press, MODE and membership all
/// land correctly. This is the fixture that catches a rapid trigger comparison reduced from
/// `press || release` to `press` alone: with only `rt_press` checked, this readback would verify
/// clean.
///
/// Output-assertion lever, mismatch on the second selected key, same reasoning as the ap tests
/// above.
#[test]
fn keyset_create_rt_end_to_end_catches_a_release_that_never_landed() {
    let lines = create_rt_write_script(Readback {
        rt_release: 500, // s still reports the wrong release
        ..S_CORRECT_RT
    });
    let script = write_script("keyset-create-rt-release-mismatch", &lines);
    let config_home = scratch_config_dir("keyset-create-rt-release-mismatch");

    let out = run_wh(
        &[
            "keyset",
            "create",
            "rt",
            "--keys",
            "w,s",
            "--press",
            "0.10",
            "--release",
            "0.30",
        ],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "s: board reports press 0.10mm release 0.50mm, wanted press 0.10mm release 0.30mm"
        ),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `s`'s membership write silently fails to land while its sensitivities and MODE all land
/// correctly.
///
/// Output-assertion lever, mismatch on the second selected key, same reasoning as the ap tests
/// above.
#[test]
fn keyset_create_rt_end_to_end_catches_a_membership_that_never_landed() {
    let lines = create_rt_write_script(Readback {
        rt_keyset: 0, // s still reports no rt keyset
        ..S_CORRECT_RT
    });
    let script = write_script("keyset-create-rt-membership-mismatch", &lines);
    let config_home = scratch_config_dir("keyset-create-rt-membership-mismatch");

    let out = run_wh(
        &[
            "keyset",
            "create",
            "rt",
            "--keys",
            "w,s",
            "--press",
            "0.10",
            "--release",
            "0.30",
        ],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("s: board reports rt keyset 0, wanted 1"),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The full script for `wh keyset create ap --keys w,s --value 2.00`, where `s` already sits at
/// the target before the write: `plan`'s skip rule gives it no value records at all, only a
/// membership one, so `verify_write`'s yardstick for it is `before()`, never a record `sent`
/// returns. `w` still changes for real, matching `create_ap_write_script`'s shape otherwise.
fn create_ap_write_script_skipping_s(s_readback: Readback) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(matrix_lines()); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    lines.extend(key_settings_lines(0x1A, 1000, 0x18, 100, 150, 0, 0)); // plan's read of w
    lines.extend(key_settings_lines(
        0x16,
        S_CORRECT_AP.ap,
        S_CORRECT_AP.mode,
        S_CORRECT_AP.rt_press,
        S_CORRECT_AP.rt_release,
        0,
        0,
    )); // plan's read of s: already at the target, so the skip rule fires

    lines.extend(auto_backup_lines(
        0,
        (1000, 0x18, 100, 150, 0, 0), // w
        (1200, 0x00, 0, 0, 0, 0),     // a
        (
            S_CORRECT_AP.ap,
            S_CORRECT_AP.mode,
            S_CORRECT_AP.rt_press,
            S_CORRECT_AP.rt_release,
            0,
            0,
        ), // s
        (1500, 0x00, 0, 0, 0, 0),     // d
    ));

    let value_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    for f in cmds::write_key_records(&value_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    let membership_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_AP,
            value: 1,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::KEYSET_AP,
            value: 1,
        },
    ];
    for f in cmds::write_key_records_singly(&membership_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 1, 0)); // w readback: correct
    lines.extend(key_settings_lines(
        0x16,
        s_readback.ap,
        s_readback.mode,
        s_readback.rt_press,
        s_readback.rt_release,
        s_readback.ap_keyset,
        s_readback.rt_keyset,
    )); // s readback

    lines
}

/// `s` skipped, so `sent(MODE)` is `None` and `verify_write`'s yardstick falls back to
/// `before().mode`: `s`'s MODE moved between the write and the readback while nothing was ever
/// sent to move it. Every earlier ap fixture gives `s` a real MODE record, so `sent` is never
/// `None` in any of them; this is the only one that exercises the fallback.
#[test]
fn keyset_create_ap_end_to_end_catches_a_skipped_keys_mode_moving_on_its_own() {
    let lines = create_ap_write_script_skipping_s(Readback {
        mode: 0x28,
        ..S_CORRECT_AP
    });
    let script = write_script("keyset-create-ap-skip-mode", &lines);
    let config_home = scratch_config_dir("keyset-create-ap-skip-mode");

    let out = run_wh(
        &["keyset", "create", "ap", "--keys", "w,s", "--value", "2.00"],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("s: board reports mode 0x0028 (rt on), wanted mode 0x0018 (rt off)"),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `s` skipped, so `sent(AP)` is `None` and `verify_write`'s yardstick falls back to
/// `before().ap`: `s`'s actuation point moved between the write and the readback while nothing
/// was ever sent to move it.
#[test]
fn keyset_create_ap_end_to_end_catches_a_skipped_keys_ap_moving_on_its_own() {
    let lines = create_ap_write_script_skipping_s(Readback {
        ap: 1900,
        ..S_CORRECT_AP
    });
    let script = write_script("keyset-create-ap-skip-ap", &lines);
    let config_home = scratch_config_dir("keyset-create-ap-skip-ap");

    let out = run_wh(
        &["keyset", "create", "ap", "--keys", "w,s", "--value", "2.00"],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("s: board reports ap 1.90mm, wanted 2.00mm"),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The full script for `wh keyset create rt --keys w,s --press 0.10 --release 0.30`, where `s`
/// already sits at the target before the write, the rt sibling of
/// `create_ap_write_script_skipping_s`.
fn create_rt_write_script_skipping_s(s_readback: Readback) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(matrix_lines()); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, 0));
    }
    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 999, 999, 0, 0)); // plan's read of w
    lines.extend(key_settings_lines(
        0x16,
        S_CORRECT_RT.ap,
        S_CORRECT_RT.mode,
        S_CORRECT_RT.rt_press,
        S_CORRECT_RT.rt_release,
        0,
        0,
    )); // plan's read of s: already at the target, so the skip rule fires

    lines.extend(auto_backup_lines(
        0,
        (2000, 0x18, 999, 999, 0, 0), // w
        (1200, 0x00, 0, 0, 0, 0),     // a
        (
            S_CORRECT_RT.ap,
            S_CORRECT_RT.mode,
            S_CORRECT_RT.rt_press,
            S_CORRECT_RT.rt_release,
            0,
            0,
        ), // s
        (1500, 0x00, 0, 0, 0, 0),     // d
    ));

    let value_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x38,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 300,
        },
    ];
    for f in cmds::write_key_records(&value_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    let membership_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_RT,
            value: 1,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::KEYSET_RT,
            value: 1,
        },
    ];
    for f in cmds::write_key_records_singly(&membership_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    lines.extend(key_settings_lines(0x1A, 2000, 0x38, 100, 300, 0, 1)); // w readback: correct
    lines.extend(key_settings_lines(
        0x16,
        s_readback.ap,
        s_readback.mode,
        s_readback.rt_press,
        s_readback.rt_release,
        s_readback.ap_keyset,
        s_readback.rt_keyset,
    )); // s readback

    lines
}

/// `s` skipped, so `sent(RT_PRESS)` is `None` and `verify_write`'s yardstick falls back to
/// `before().rt_press`: `s`'s press sensitivity moved between the write and the readback while
/// nothing was ever sent to move it.
#[test]
fn keyset_create_rt_end_to_end_catches_a_skipped_keys_press_moving_on_its_own() {
    let lines = create_rt_write_script_skipping_s(Readback {
        rt_press: 500,
        ..S_CORRECT_RT
    });
    let script = write_script("keyset-create-rt-skip-press", &lines);
    let config_home = scratch_config_dir("keyset-create-rt-skip-press");

    let out = run_wh(
        &[
            "keyset",
            "create",
            "rt",
            "--keys",
            "w,s",
            "--press",
            "0.10",
            "--release",
            "0.30",
        ],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "s: board reports press 0.50mm release 0.30mm, wanted press 0.10mm release 0.30mm"
        ),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `s` skipped, so `sent(RT_RELEASE)` is `None` and `verify_write`'s yardstick falls back to
/// `before().rt_release`: `s`'s release sensitivity moved between the write and the readback
/// while nothing was ever sent to move it.
#[test]
fn keyset_create_rt_end_to_end_catches_a_skipped_keys_release_moving_on_its_own() {
    let lines = create_rt_write_script_skipping_s(Readback {
        rt_release: 500,
        ..S_CORRECT_RT
    });
    let script = write_script("keyset-create-rt-skip-release", &lines);
    let config_home = scratch_config_dir("keyset-create-rt-skip-release");

    let out = run_wh(
        &[
            "keyset",
            "create",
            "rt",
            "--keys",
            "w,s",
            "--press",
            "0.10",
            "--release",
            "0.30",
        ],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "s: board reports press 0.10mm release 0.50mm, wanted press 0.10mm release 0.30mm"
        ),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `wh keyset set ap 1 --value 1.20 --dry-run` over a board where w,a hold ap keyset 1 at 1.00mm
/// and s,d are free: `read_membership`'s matrix and 0xFF sweep, then `plan`'s six-layout read for
/// w then a. No global read at all: `set` never falls back to it.
fn set_script_for_keyset_1_over_w_and_a() -> Vec<String> {
    let mut lines = matrix_lines();
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(key_settings_lines(0x1A, 1000, 0x18, 100, 150, 1, 0));
    lines.extend(key_settings_lines(0x04, 1000, 0x18, 100, 150, 1, 0));
    lines
}

/// `wh keyset delete ap 1 --dry-run` over the same board: w holds ap keyset 1 at 0.30mm, a holds
/// it at 1.00mm (deliberately different, so an announcement or a write that mixed the two members
/// up cannot pass by printing one twice), s,d are free and agree at 2.00mm. `read_membership`'s
/// matrix and 0xFF sweep, `global_ap`'s two reads over the free keys s and d, then `plan`'s
/// six-layout read for w then a.
fn delete_script_for_keyset_1() -> Vec<String> {
    let mut lines = matrix_lines();
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(layout_read_lines(0x16, layout::AP, 2000));
    lines.extend(layout_read_lines(0x07, layout::AP, 2000));
    lines.extend(key_settings_lines(0x1A, 300, 0x18, 100, 150, 1, 0));
    lines.extend(key_settings_lines(0x04, 1000, 0x18, 100, 150, 1, 0));
    lines
}

/// Changing a keyset's value writes every member, not just the one named, at exactly the value
/// asked for, and writes no membership record at all: the keyset keeps its index. Pins the exact
/// frame, not just that a record exists at each key's coordinate: a layout-byte-only check cannot
/// see a value silently drifting from what was actually requested.
#[test]
fn keyset_set_writes_every_member_and_no_membership_record() {
    let script = write_script("keyset-set", &set_script_for_keyset_1_over_w_and_a());
    let out = run_wh(
        &["keyset", "set", "ap", "1", "--value", "1.20", "--dry-run"],
        &script,
        &scratch_config_dir("keyset-set"),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let frames = frame_lines(&String::from_utf8_lossy(&out.stdout));
    assert_eq!(frames.len(), 1, "no membership record, one value batch");

    let value_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 1200,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 150,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::AP,
            value: 1200,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    let expected = hex(&cmds::write_key_records(&value_records)[0]);
    assert_eq!(frames[0], expected, "got: {}", frames[0]);

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-set"));
}

/// An index no key holds is an error naming what the board actually has, not a silent success
/// that writes to nobody.
#[test]
fn keyset_set_on_a_missing_index_names_the_live_ones() {
    let script = write_script(
        "keyset-set-missing",
        &set_script_for_keyset_1_over_w_and_a(),
    );
    let out = run_wh(
        &["keyset", "set", "ap", "7", "--value", "1.20"],
        &script,
        &scratch_config_dir("keyset-set-missing"),
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no keyset 7"), "got: {err}");
    // Not a bare `'1'`: stderr opens with the transport line, which names a temp file path
    // containing the process id, so a lone digit can pass by matching that instead of the
    // board's own live index.
    assert!(
        err.contains("the board has 1"),
        "the live indices must be named: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-set-missing"));
}

/// A delete clears membership and returns the members to the global value, in that order:
/// values first, membership last, one record per frame. Pins the exact frames, values and the
/// cleared membership value (`0`) included, not just that something exists at each coordinate: a
/// membership record that carried the wrong index, or a value that landed on the wrong target,
/// would still satisfy a layout-byte-only check.
#[test]
fn keyset_delete_writes_values_before_clearing_membership() {
    let script = write_script("keyset-delete", &delete_script_for_keyset_1());
    let out = run_wh(
        &["keyset", "delete", "ap", "1", "--dry-run"],
        &script,
        &scratch_config_dir("keyset-delete"),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let frames = frame_lines(&String::from_utf8_lossy(&out.stdout));
    assert_eq!(
        frames.len(),
        3,
        "one value batch, then one membership frame per member"
    );

    let value_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 150,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    let expected_value = hex(&cmds::write_key_records(&value_records)[0]);
    assert_eq!(frames[0], expected_value, "got: {}", frames[0]);

    let membership_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_AP,
            value: 0,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::KEYSET_AP,
            value: 0,
        },
    ];
    let expected_membership = cmds::write_key_records_singly(&membership_records);
    assert_eq!(frames[1], hex(&expected_membership[0]));
    assert_eq!(frames[2], hex(&expected_membership[1]));

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-delete"));
}

/// A delete overwrites every member's value with the global before it writes anything, so it
/// must announce what each member currently holds and what replaces it, printed before the
/// first frame: the operator's only warning before a destructive write.
#[test]
fn keyset_delete_announces_each_members_prior_value_before_writing() {
    let script = write_script("keyset-delete-announce", &delete_script_for_keyset_1());
    let out = run_wh(
        &["keyset", "delete", "ap", "1", "--dry-run"],
        &script,
        &scratch_config_dir("keyset-delete-announce"),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ap keyset 1: deleting, returning members to 2.00mm"),
        "got: {stdout}"
    );
    assert!(stdout.contains("w at 0.30mm"), "got: {stdout}");
    assert!(stdout.contains("a at 1.00mm"), "got: {stdout}");

    let announce_at = stdout.find("deleting, returning members").unwrap();
    let first_frame_at = stdout
        .lines()
        .position(|l| contains_hex_run(l, 128))
        .expect("at least one frame line");
    let announce_line = stdout[..announce_at].lines().count();
    assert!(
        announce_line < first_frame_at,
        "announcement must print before the first frame: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-delete-announce"));
}

/// `s`'s and `d`'s state for `set`/`delete` end-to-end scripts below: free of any ap keyset,
/// their exact values only matter to the auto-backup snapshot read.
const FREE_KEY: KeyState = (1500, 0x00, 0, 0, 0, 0);

/// The full script for `wh keyset set ap 1 --value 1.20` against a board where w,a hold ap
/// keyset 1 at 1.00mm and s,d are free: `read_membership`'s matrix and 0xFF sweep, `plan`'s
/// six-layout read for w then a, the auto-backup snapshot, the value batch (no membership, `set`
/// writes none), then the readback for both members. `a_readback` lets the fault test below vary
/// exactly one of `a`'s fields.
fn set_ap_write_script(a_readback: Readback) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(key_settings_lines(0x1A, 1000, 0x18, 100, 150, 1, 0)); // plan's read of w
    lines.extend(key_settings_lines(0x04, 1000, 0x18, 100, 150, 1, 0)); // plan's read of a

    lines.extend(auto_backup_lines(
        0,
        (1000, 0x18, 100, 150, 1, 0), // w
        (1000, 0x18, 100, 150, 1, 0), // a
        FREE_KEY,                     // s
        FREE_KEY,                     // d
    ));

    let value_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 1200,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 150,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::AP,
            value: 1200,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    for f in cmds::write_key_records(&value_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    lines.extend(key_settings_lines(0x1A, 1200, 0x18, 100, 150, 1, 0)); // w readback: correct
    lines.extend(key_settings_lines(
        0x04,
        a_readback.ap,
        a_readback.mode,
        a_readback.rt_press,
        a_readback.rt_release,
        a_readback.ap_keyset,
        a_readback.rt_keyset,
    )); // a readback

    lines
}

/// `a`'s correct post-write readback for the `set` scripts above: ap moved to the 1.20mm target,
/// everything else unchanged, membership untouched at keyset 1.
const A_CORRECT_SET: Readback = Readback {
    ap: 1200,
    mode: 0x18,
    rt_press: 100,
    rt_release: 150,
    ap_keyset: 1,
    rt_keyset: 0,
};

/// `wh keyset set ap 1 --value 1.20` end to end, with a fully correct readback: the auto-backup
/// phase, the value batch with no membership records, and a readback that matches for both
/// members. Exit 0, "verified" in stdout, and a real backup file on disk.
///
/// Byte-for-byte lever: the script's first frame after `set_value`'s own reads is the
/// auto-backup's SYNC read; a write sent before the backup would not match it.
#[test]
fn keyset_set_ap_end_to_end_backs_up_writes_and_verifies() {
    let lines = set_ap_write_script(A_CORRECT_SET);
    let script = write_script("keyset-set-ap-ok", &lines);
    let config_home = scratch_config_dir("keyset-set-ap-ok");

    let out = run_wh(
        &["keyset", "set", "ap", "1", "--value", "1.20"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ap keyset set: 2 keys verified"),
        "got: {stdout}"
    );

    let backups = std::fs::read_dir(config_home.join("wh").join("backups"))
        .unwrap()
        .count();
    assert_eq!(backups, 1, "expected exactly one auto-backup file on disk");

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `a`'s actuation point write silently fails to land while everything else about it lands
/// correctly. `a` is the second member `set` writes, so a verifier that stopped after the first
/// member would never read it back.
///
/// Output-assertion lever: nothing is scripted after `a`'s readback, so only the printed
/// mismatch text can catch a verifier that missed it.
#[test]
fn keyset_set_ap_end_to_end_catches_a_value_that_never_landed_on_the_second_member() {
    let lines = set_ap_write_script(Readback {
        ap: 1000, // a still reports its pre-write ap
        ..A_CORRECT_SET
    });
    let script = write_script("keyset-set-ap-mismatch", &lines);
    let config_home = scratch_config_dir("keyset-set-ap-mismatch");

    let out = run_wh(
        &["keyset", "set", "ap", "1", "--value", "1.20"],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("a: board reports ap 1.00mm, wanted 1.20mm"),
        "got: {err}"
    );
    assert!(
        !err.contains("wh restore does not yet write keyset membership"),
        "the rollback caveat is inapt on `set`, which never writes membership: {err}"
    );
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("verified"),
        "must not claim success on a board that did not change"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `a`'s MODE write silently fails to land while its actuation point and everything else about
/// it land correctly. `a` is the second member `set` writes.
///
/// Output-assertion lever, same reasoning as the value mismatch test above.
#[test]
fn keyset_set_ap_end_to_end_catches_a_mode_that_never_landed_on_the_second_member() {
    let lines = set_ap_write_script(Readback {
        mode: 0x28, // a still reports RtGlobal touch instead of the target Single
        ..A_CORRECT_SET
    });
    let script = write_script("keyset-set-ap-mode-mismatch", &lines);
    let config_home = scratch_config_dir("keyset-set-ap-mode-mismatch");

    let out = run_wh(
        &["keyset", "set", "ap", "1", "--value", "1.20"],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("a: board reports mode 0x0028 (rt on), wanted mode 0x0018 (rt off)"),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `a`'s ap keyset membership drifts between the write and the readback while `set` never sent a
/// membership record at all: `verify_write`'s fallback to `a`'s pre-write membership is what
/// catches this, the end-to-end sibling of the unit test pinning the same fallback on a bare
/// `plan`.
///
/// Output-assertion lever, same reasoning as the value mismatch test above.
#[test]
fn keyset_set_ap_end_to_end_catches_a_membership_drift_on_the_second_member() {
    let lines = set_ap_write_script(Readback {
        ap_keyset: 0, // a's membership silently dropped, though `set` never touched it
        ..A_CORRECT_SET
    });
    let script = write_script("keyset-set-ap-membership-drift", &lines);
    let config_home = scratch_config_dir("keyset-set-ap-membership-drift");

    let out = run_wh(
        &["keyset", "set", "ap", "1", "--value", "1.20"],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("a: board reports ap keyset 0, wanted 1"),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `resolve_index`'s "no keysets of this kind exist" refusal, reached through the CLI rather than
/// called directly: a board with no ap keyset at all must say so, not just "no keyset 1".
#[test]
fn keyset_set_ap_on_a_board_with_no_keysets_of_this_kind_says_so() {
    let mut lines = matrix_lines();
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    let script = write_script("keyset-set-ap-none-exist", &lines);
    let out = run_wh(
        &["keyset", "set", "ap", "1", "--value", "1.20"],
        &script,
        &scratch_config_dir("keyset-set-ap-none-exist"),
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("no keysets of this kind exist on the board"),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-set-ap-none-exist"));
}

/// `set` with no `--value` at all is an error, not a no-op: nothing may be written to nobody's
/// benefit. Fails right after `read_membership`, before `plan` ever reads a key, so the unused
/// tail of the reused script is simply never consumed.
#[test]
fn keyset_set_ap_requires_a_value() {
    let script = write_script(
        "keyset-set-novalue",
        &set_script_for_keyset_1_over_w_and_a(),
    );
    let out = run_wh(
        &["keyset", "set", "ap", "1"],
        &script,
        &scratch_config_dir("keyset-set-novalue"),
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("pass --value"), "got: {err}");

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-set-novalue"));
}

/// `set rt` with none of `--value`/`--press`/`--release` says what is actually accepted: both
/// flags together, or `--value` to set both at once. Not "and/or", which would tell the operator
/// a single flag is enough when `set rt 1 --press 0.10` alone still refuses.
#[test]
fn keyset_set_rt_requires_press_and_release_or_value() {
    let mut lines = matrix_lines();
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, ks));
    }
    let script = write_script("keyset-set-rt-novalue", &lines);
    let out = run_wh(
        &["keyset", "set", "rt", "1"],
        &script,
        &scratch_config_dir("keyset-set-rt-novalue"),
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("pass --press and --release") && err.contains("--value"),
        "got: {err}"
    );
    assert!(!err.contains("and/or"), "got: {err}");

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-set-rt-novalue"));
}

/// `--press` alone, with neither `--release` nor `--value`, names the specific flag still
/// missing rather than repeating the "none given at all" message.
#[test]
fn keyset_set_rt_with_only_press_names_what_is_missing() {
    let mut lines = matrix_lines();
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, ks));
    }
    let script = write_script("keyset-set-rt-press-only", &lines);
    let out = run_wh(
        &["keyset", "set", "rt", "1", "--press", "0.10"],
        &script,
        &scratch_config_dir("keyset-set-rt-press-only"),
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("--press given without --release or --value"),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-set-rt-press-only"));
}

/// `--press`/`--release` are meaningless on an ap `set`, the same refusal `create` applies:
/// silently ignoring them would leave a typo'd command believing it changed a sensitivity that
/// was never used.
#[test]
fn keyset_set_ap_refuses_rapid_trigger_flags() {
    let config_home = scratch_config_dir("keyset-set-ap-refuse");
    let out = run_wh(
        &[
            "keyset",
            "set",
            "ap",
            "1",
            "--press",
            "0.10",
            "--release",
            "0.20",
        ],
        std::path::Path::new("/nonexistent-keyset-set-refuse.jsonl"),
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("--press") && err.contains("--release"),
        "got: {err}"
    );

    let _ = std::fs::remove_dir_all(&config_home);
}

/// A single rapid trigger flag alone must refuse too, not just the two together: an `&&` in the
/// refusal check would let one flag through silently, which the two-flags test above cannot see.
#[test]
fn keyset_set_ap_refuses_press_alone() {
    let config_home = scratch_config_dir("keyset-set-ap-refuse-press");
    let out = run_wh(
        &["keyset", "set", "ap", "1", "--press", "0.10"],
        std::path::Path::new("/nonexistent-keyset-set-refuse-press.jsonl"),
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--press"), "got: {err}");

    let _ = std::fs::remove_dir_all(&config_home);
}

/// The `--release`-alone mirror of the test above.
#[test]
fn keyset_set_ap_refuses_release_alone() {
    let config_home = scratch_config_dir("keyset-set-ap-refuse-release");
    let out = run_wh(
        &["keyset", "set", "ap", "1", "--release", "0.20"],
        std::path::Path::new("/nonexistent-keyset-set-refuse-release.jsonl"),
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--release"), "got: {err}");

    let _ = std::fs::remove_dir_all(&config_home);
}

/// `delete`'s own refusal, the same one-flag hole checked on `set` above.
#[test]
fn keyset_delete_ap_refuses_press_alone() {
    let config_home = scratch_config_dir("keyset-delete-ap-refuse-press");
    let out = run_wh(
        &["keyset", "delete", "ap", "1", "--press", "0.10"],
        std::path::Path::new("/nonexistent-keyset-delete-refuse-press.jsonl"),
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--press"), "got: {err}");

    let _ = std::fs::remove_dir_all(&config_home);
}

/// The `--release`-alone mirror of the test above, on `delete`.
#[test]
fn keyset_delete_ap_refuses_release_alone() {
    let config_home = scratch_config_dir("keyset-delete-ap-refuse-release");
    let out = run_wh(
        &["keyset", "delete", "ap", "1", "--release", "0.20"],
        std::path::Path::new("/nonexistent-keyset-delete-refuse-release.jsonl"),
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--release"), "got: {err}");

    let _ = std::fs::remove_dir_all(&config_home);
}

/// `wh keyset set rt 1 --press 0.10 --release 0.30 --dry-run` writes every member's press and
/// release independently, and no rapid trigger membership record: `set` never touches membership,
/// ap or rt alike. Pins the exact frame, values included, not just which layout bytes appear: a
/// press/release swap produces a byte-identical-looking pair of records with the two values
/// traded, which a layout-byte-only check cannot see.
#[test]
fn keyset_set_rt_writes_press_and_release_to_every_member() {
    let mut lines = matrix_lines(); // read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, ks));
    }
    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 999, 999, 0, 1)); // plan's read of w
    lines.extend(key_settings_lines(0x04, 2000, 0x18, 999, 999, 0, 1)); // plan's read of a

    let script = write_script("keyset-set-rt", &lines);
    let config_home = scratch_config_dir("keyset-set-rt");
    let out = run_wh(
        &[
            "keyset",
            "set",
            "rt",
            "1",
            "--press",
            "0.10",
            "--release",
            "0.30",
            "--dry-run",
        ],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let frames = frame_lines(&String::from_utf8_lossy(&out.stdout));
    assert_eq!(frames.len(), 1, "no membership record, one value batch");

    let value_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x38,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 300,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::MODE,
            value: 0x38,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_RELEASE,
            value: 300,
        },
    ];
    let expected = hex(&cmds::write_key_records(&value_records)[0]);
    assert_eq!(frames[0], expected, "got: {}", frames[0]);

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The full script for `wh keyset delete ap 1` against a board where w,a hold ap keyset 1 at
/// 0.30mm and s,d are free and agree at 2.00mm: `read_membership`'s matrix and 0xFF sweep,
/// `global_ap`'s two reads over s and d, `plan`'s six-layout read for w then a, the auto-backup
/// snapshot, the value batch, the two membership frames, then the readback for both members.
/// `a_readback` lets the fault test below vary exactly one of `a`'s fields.
fn delete_ap_write_script(a_readback: Readback) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(layout_read_lines(0x16, layout::AP, 2000)); // s, free
    lines.extend(layout_read_lines(0x07, layout::AP, 2000)); // d, free
    lines.extend(key_settings_lines(0x1A, 300, 0x18, 100, 150, 1, 0)); // plan's read of w
    lines.extend(key_settings_lines(0x04, 300, 0x18, 100, 150, 1, 0)); // plan's read of a

    lines.extend(auto_backup_lines(
        0,
        (300, 0x18, 100, 150, 1, 0), // w
        (300, 0x18, 100, 150, 1, 0), // a
        (2000, 0x00, 0, 0, 0, 0),    // s
        (2000, 0x00, 0, 0, 0, 0),    // d
    ));

    let value_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 150,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    for f in cmds::write_key_records(&value_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    let membership_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_AP,
            value: 0,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::KEYSET_AP,
            value: 0,
        },
    ];
    for f in cmds::write_key_records_singly(&membership_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 0, 0)); // w readback: correct
    lines.extend(key_settings_lines(
        0x04,
        a_readback.ap,
        a_readback.mode,
        a_readback.rt_press,
        a_readback.rt_release,
        a_readback.ap_keyset,
        a_readback.rt_keyset,
    )); // a readback

    lines
}

/// `a`'s correct post-write readback for the `delete` scripts above: ap returned to the board's
/// 2.00mm global, membership cleared to 0.
const A_CORRECT_DELETE: Readback = Readback {
    ap: 2000,
    mode: 0x18,
    rt_press: 100,
    rt_release: 150,
    ap_keyset: 0,
    rt_keyset: 0,
};

/// `wh keyset delete ap 1` end to end, with no `--value`, falling back to the board's global:
/// the auto-backup phase, the value batch, the two membership frames, and a readback that
/// matches for both members.
#[test]
fn keyset_delete_ap_end_to_end_backs_up_writes_and_verifies() {
    let lines = delete_ap_write_script(A_CORRECT_DELETE);
    let script = write_script("keyset-delete-ap-ok", &lines);
    let config_home = scratch_config_dir("keyset-delete-ap-ok");

    let out = run_wh(&["keyset", "delete", "ap", "1"], &script, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ap keyset delete: 2 keys verified"),
        "got: {stdout}"
    );

    let backups = std::fs::read_dir(config_home.join("wh").join("backups"))
        .unwrap()
        .count();
    assert_eq!(backups, 1, "expected exactly one auto-backup file on disk");

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `a`'s membership write silently fails to land while its value writes all land correctly. `a`
/// is the second member `delete` writes, so a verifier that stopped after the first member would
/// never read it back.
///
/// Output-assertion lever, same reasoning as the `set` mismatch test above.
#[test]
fn keyset_delete_ap_end_to_end_catches_a_membership_that_never_cleared_on_the_second_member() {
    let lines = delete_ap_write_script(Readback {
        ap_keyset: 1, // a still reports ap keyset 1
        ..A_CORRECT_DELETE
    });
    let script = write_script("keyset-delete-ap-mismatch", &lines);
    let config_home = scratch_config_dir("keyset-delete-ap-mismatch");

    let out = run_wh(&["keyset", "delete", "ap", "1"], &script, &config_home);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("a: board reports ap keyset 1, wanted 0"),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The mismatch itself must stay the final `error:` line on `delete`, which does write
/// membership, with nothing else, retired caveat or otherwise, pushed ahead of it. Enumerates
/// every line stderr is allowed to hold rather than only checking the last one and the retired
/// wording is absent: a differently-worded caveat inserted before the headline would still leave
/// the last line reading `error: readback mismatch` and the retired text absent, so only a check
/// on the total shape of stderr can catch it coming back under a new name.
#[test]
fn keyset_delete_ap_mismatch_keeps_the_readback_failure_as_the_headline() {
    let lines = delete_ap_write_script(Readback {
        ap_keyset: 1,
        ..A_CORRECT_DELETE
    });
    let script = write_script("keyset-delete-ap-headline", &lines);
    let config_home = scratch_config_dir("keyset-delete-ap-headline");

    let out = run_wh(&["keyset", "delete", "ap", "1"], &script, &config_home);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(
        lines.len(),
        4,
        "expected exactly transport, backup, one fault line, and the headline, nothing pushed \
         ahead of the headline: {err}"
    );
    assert!(lines[0].starts_with("transport: replay"), "got: {err}");
    assert!(lines[1].starts_with("(backed up to"), "got: {err}");
    assert!(
        lines[2].contains("a: board reports ap keyset 1, wanted 0"),
        "got: {err}"
    );
    assert!(
        lines[3].starts_with("error: readback mismatch"),
        "the readback failure must be the final, headline line: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// A board whose free keys disagree on the actuation point has no one global value, so a delete
/// with no `--value` must refuse and name the disagreement rather than picking a winner: `delete`
/// never votes.
#[test]
fn keyset_delete_ap_with_no_value_and_split_global_refuses() {
    let mut lines = matrix_lines();
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(layout_read_lines(0x16, layout::AP, 1000));
    lines.extend(layout_read_lines(0x07, layout::AP, 2000));
    let script = write_script("keyset-delete-split", &lines);
    let out = run_wh(
        &["keyset", "delete", "ap", "1"],
        &script,
        &scratch_config_dir("keyset-delete-split"),
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("disagree"), "got: {err}");
    assert!(err.contains("--value"), "the way out must be named: {err}");

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-delete-split"));
}

/// `wh keyset delete rt 1 --dry-run` with no `--press`/`--release`, falling back to the board's
/// global rapid trigger sensitivity: values go out first, membership last, membership uses the rt
/// layout (`0xFE`), not the ap one, and the touch nibble actually turns rapid trigger off. Pins
/// the exact value frame, not just which layout bytes appear: a `rt_off`/`rt_on` swap leaves the
/// touch nibble on (`0x38` instead of `0x18`) while every layout byte sent stays identical.
#[test]
fn keyset_delete_rt_clears_membership_to_the_global() {
    let mut lines = matrix_lines(); // read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, ks));
    }
    lines.extend(layout_read_lines(0x16, layout::RT_PRESS, 100)); // s, free
    lines.extend(layout_read_lines(0x16, layout::RT_RELEASE, 150));
    lines.extend(layout_read_lines(0x07, layout::RT_PRESS, 100)); // d, free
    lines.extend(layout_read_lines(0x07, layout::RT_RELEASE, 150));
    lines.extend(key_settings_lines(0x1A, 2000, 0x38, 500, 500, 0, 1)); // plan's read of w
    lines.extend(key_settings_lines(0x04, 2000, 0x38, 500, 500, 0, 1)); // plan's read of a

    let script = write_script("keyset-delete-rt", &lines);
    let config_home = scratch_config_dir("keyset-delete-rt");
    let out = run_wh(
        &["keyset", "delete", "rt", "1", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let frames = frame_lines(&String::from_utf8_lossy(&out.stdout));
    assert_eq!(
        frames.len(),
        3,
        "one value batch, then one membership frame per member"
    );

    let value_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 150,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    let expected_value = hex(&cmds::write_key_records(&value_records)[0]);
    assert_eq!(frames[0], expected_value, "got: {}", frames[0]);

    let membership_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_RT,
            value: 0,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::KEYSET_RT,
            value: 0,
        },
    ];
    let expected_membership = cmds::write_key_records_singly(&membership_records);
    assert_eq!(frames[1], hex(&expected_membership[0]));
    assert_eq!(frames[2], hex(&expected_membership[1]));

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The full script for `wh keyset set rt 1 --press 0.10 --release 0.30` against a board where
/// w,a hold rt keyset 1 and s,d are free: `read_membership`'s matrix and 0xFE sweep, `plan`'s
/// six-layout read for w then a, the auto-backup snapshot, the value batch (no membership, `set`
/// writes none), then the readback for both members. `a_readback` lets the fault test below vary
/// exactly one of `a`'s fields.
fn set_rt_write_script(a_readback: Readback) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, ks));
    }
    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 999, 999, 0, 1)); // plan's read of w
    lines.extend(key_settings_lines(0x04, 2000, 0x18, 999, 999, 0, 1)); // plan's read of a

    lines.extend(auto_backup_lines(
        0,
        (2000, 0x18, 999, 999, 0, 1), // w
        (2000, 0x18, 999, 999, 0, 1), // a
        FREE_KEY,                     // s
        FREE_KEY,                     // d
    ));

    let value_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x38,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 300,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::MODE,
            value: 0x38,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_RELEASE,
            value: 300,
        },
    ];
    for f in cmds::write_key_records(&value_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    lines.extend(key_settings_lines(0x1A, 2000, 0x38, 100, 300, 0, 1)); // w readback: correct
    lines.extend(key_settings_lines(
        0x04,
        a_readback.ap,
        a_readback.mode,
        a_readback.rt_press,
        a_readback.rt_release,
        a_readback.ap_keyset,
        a_readback.rt_keyset,
    )); // a readback

    lines
}

/// `a`'s correct post-write readback for the `set rt` scripts above: press and release moved to
/// the 0.10/0.30mm target, touch moved from `Single` to `Rt`, membership untouched at keyset 1.
const A_CORRECT_SET_RT: Readback = Readback {
    ap: 2000,
    mode: 0x38,
    rt_press: 100,
    rt_release: 300,
    ap_keyset: 0,
    rt_keyset: 1,
};

/// `wh keyset set rt 1 --press 0.10 --release 0.30` end to end, with a fully correct readback:
/// the auto-backup phase, the value batch with no membership records, and a readback that
/// matches for both members.
///
/// Byte-for-byte lever: a swapped `p`/`r` here sends the wrong value to the device, which this
/// script's exact write frame would reject outright, the same way `keyset_set_ap_end_to_end_backs_up_writes_and_verifies`
/// does for actuation point.
#[test]
fn keyset_set_rt_end_to_end_backs_up_writes_and_verifies() {
    let lines = set_rt_write_script(A_CORRECT_SET_RT);
    let script = write_script("keyset-set-rt-ok", &lines);
    let config_home = scratch_config_dir("keyset-set-rt-ok");

    let out = run_wh(
        &[
            "keyset",
            "set",
            "rt",
            "1",
            "--press",
            "0.10",
            "--release",
            "0.30",
        ],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("rt keyset set: 2 keys verified"),
        "got: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `a`'s press sensitivity write silently fails to land while release, MODE and membership all
/// land correctly. `a` is the second member `set` writes, so a verifier that stopped after the
/// first member would never read it back.
///
/// Output-assertion lever, same reasoning as the ap mismatch tests above.
#[test]
fn keyset_set_rt_end_to_end_catches_a_press_that_never_landed_on_the_second_member() {
    let lines = set_rt_write_script(Readback {
        rt_press: 999, // a still reports its pre-write press
        ..A_CORRECT_SET_RT
    });
    let script = write_script("keyset-set-rt-mismatch", &lines);
    let config_home = scratch_config_dir("keyset-set-rt-mismatch");

    let out = run_wh(
        &[
            "keyset",
            "set",
            "rt",
            "1",
            "--press",
            "0.10",
            "--release",
            "0.30",
        ],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "a: board reports press 1.00mm release 0.30mm, wanted press 0.10mm release 0.30mm"
        ),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `a`'s rt keyset membership drifts between the write and the readback while `set` never sent a
/// membership record at all, the rapid trigger sibling of the ap membership-drift test: the ap
/// and rt fallbacks in `verify_write` are two separate expressions, and this is the only fixture
/// that exercises the rt one.
///
/// Output-assertion lever, same reasoning as the ap mismatch tests above.
#[test]
fn keyset_set_rt_end_to_end_catches_a_membership_drift_on_the_second_member() {
    let lines = set_rt_write_script(Readback {
        rt_keyset: 0, // a's rt membership silently dropped, though `set` never touched it
        ..A_CORRECT_SET_RT
    });
    let script = write_script("keyset-set-rt-membership-drift", &lines);
    let config_home = scratch_config_dir("keyset-set-rt-membership-drift");

    let out = run_wh(
        &[
            "keyset",
            "set",
            "rt",
            "1",
            "--press",
            "0.10",
            "--release",
            "0.30",
        ],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("a: board reports rt keyset 0, wanted 1"),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The full script for `wh keyset delete rt 1` against a board where w,a hold rt keyset 1 at
/// 0.50/0.50mm and s,d are free and agree at 0.10/0.15mm: `read_membership`'s matrix and 0xFE
/// sweep, `global_rt`'s reads over s and d, `plan`'s six-layout read for w then a, the auto-backup
/// snapshot, the value batch, the two membership frames, then the readback for both members.
/// `a_readback` lets the fault test below vary exactly one of `a`'s fields.
fn delete_rt_write_script(a_readback: Readback) -> Vec<String> {
    const FREE_RT: KeyState = (2000, 0x00, 100, 150, 0, 0);

    let mut lines = Vec::new();
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, ks));
    }
    lines.extend(layout_read_lines(0x16, layout::RT_PRESS, 100)); // s, free
    lines.extend(layout_read_lines(0x16, layout::RT_RELEASE, 150));
    lines.extend(layout_read_lines(0x07, layout::RT_PRESS, 100)); // d, free
    lines.extend(layout_read_lines(0x07, layout::RT_RELEASE, 150));
    lines.extend(key_settings_lines(0x1A, 2000, 0x38, 500, 500, 0, 1)); // plan's read of w
    lines.extend(key_settings_lines(0x04, 2000, 0x38, 500, 500, 0, 1)); // plan's read of a

    lines.extend(auto_backup_lines(
        0,
        (2000, 0x38, 500, 500, 0, 1), // w
        (2000, 0x38, 500, 500, 0, 1), // a
        FREE_RT,                      // s
        FREE_RT,                      // d
    ));

    let value_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 150,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    for f in cmds::write_key_records(&value_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    let membership_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_RT,
            value: 0,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::KEYSET_RT,
            value: 0,
        },
    ];
    for f in cmds::write_key_records_singly(&membership_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 0, 0)); // w readback: correct
    lines.extend(key_settings_lines(
        0x04,
        a_readback.ap,
        a_readback.mode,
        a_readback.rt_press,
        a_readback.rt_release,
        a_readback.ap_keyset,
        a_readback.rt_keyset,
    )); // a readback

    lines
}

/// `a`'s correct post-write readback for the `delete rt` scripts above: press and release
/// returned to the board's 0.10/0.15mm global, touch moved from `Rt` to `Single`, membership
/// cleared to 0.
const A_CORRECT_DELETE_RT: Readback = Readback {
    ap: 2000,
    mode: 0x18,
    rt_press: 100,
    rt_release: 150,
    ap_keyset: 0,
    rt_keyset: 0,
};

/// `wh keyset delete rt 1` end to end, with no `--press`/`--release`, falling back to the
/// board's global: the auto-backup phase, the value batch, the two membership frames, and a
/// readback that matches for both members.
///
/// Byte-for-byte lever: a `rt_off`/`rt_on` swap here sends the wrong touch nibble to the device,
/// which this script's exact write frame would reject outright.
#[test]
fn keyset_delete_rt_end_to_end_backs_up_writes_and_verifies() {
    let lines = delete_rt_write_script(A_CORRECT_DELETE_RT);
    let script = write_script("keyset-delete-rt-ok", &lines);
    let config_home = scratch_config_dir("keyset-delete-rt-ok");

    let out = run_wh(&["keyset", "delete", "rt", "1"], &script, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("rt keyset delete: 2 keys verified"),
        "got: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `a`'s touch mode never actually turns rapid trigger off, exactly the observable shape of a
/// `rt_off`/`rt_on` swap: MODE still reports `Rt` (`0x38`) instead of `Single` (`0x18`) while
/// press, release and membership all land correctly. `a` is the second member `delete` writes.
///
/// Output-assertion lever, same reasoning as the ap mismatch tests above.
#[test]
fn keyset_delete_rt_end_to_end_catches_a_mode_that_never_turned_off_on_the_second_member() {
    let lines = delete_rt_write_script(Readback {
        mode: 0x38, // a's touch never actually left rapid trigger
        ..A_CORRECT_DELETE_RT
    });
    let script = write_script("keyset-delete-rt-mismatch", &lines);
    let config_home = scratch_config_dir("keyset-delete-rt-mismatch");

    let out = run_wh(&["keyset", "delete", "rt", "1"], &script, &config_home);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("a: board reports mode 0x0038 (rt on), wanted mode 0x0018 (rt off)"),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The measured `ks-remove-one-rt` shape (`docs/keysets.md`): removing a member from a rapid
/// trigger keyset turns rapid trigger off (touch nibble 1, not nibble 2, following the global) and
/// resets the sensitivities to the global, but leaves the key's own actuation point untouched.
/// `w`'s AP, `1.10mm`, is the whole point of this test: a rewrite that reused the actuation point
/// branch would send the global `2.00mm` there instead, and every other record in the frame would
/// still look right. This is also the ordinary shape `wh keyset remove rt` runs against, a member
/// with its own non-base sensitivity, so the announcement must name the mode transition alongside
/// the new value, not only in the free-key case where the value happens to sit at the base already.
#[test]
fn keyset_remove_rt_turns_rapid_trigger_off_and_keeps_the_actuation_point() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, ks));
    }
    // global_rt reads the press/release sensitivity of the keys outside every keyset: s and d.
    lines.extend(layout_read_lines(0x16, layout::RT_PRESS, 100));
    lines.extend(layout_read_lines(0x16, layout::RT_RELEASE, 100));
    lines.extend(layout_read_lines(0x07, layout::RT_PRESS, 100));
    lines.extend(layout_read_lines(0x07, layout::RT_RELEASE, 100));
    // plan's own per-key read of w: own AP 1.10mm, rt keyset 1, press/release 0.30/0.40mm.
    lines.extend(key_settings_lines(0x1A, 1100, 0x30, 300, 400, 0, 1));

    let script = write_script("keyset-remove-rt", &lines);
    let config_home = scratch_config_dir("keyset-remove-rt");
    let out = run_wh(
        &["keyset", "remove", "rt", "--keys", "w", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(
            "rt: removing w from keyset 1, 0.30/0.40mm to 0.10/0.10mm, mode Rt to Single"
        ),
        "got: {stdout}"
    );

    let value_records = [
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x10,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 1100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 100,
        },
    ];
    let mut expected: Vec<String> = cmds::write_key_records(&value_records)
        .iter()
        .map(|f| hex(f))
        .collect();
    expected.extend(
        cmds::write_key_records_singly(&[KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_RT,
            value: 0,
        }])
        .iter()
        .map(|f| hex(f)),
    );
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "MODE goes to touch nibble 1, AP stays at w's own 1100, not the global: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The full script for `wh keyset remove ap --keys w`, over a board where w,a,s hold ap keyset 3
/// at 1.20mm and d is free at 2.00mm: `resolve_keys`'s matrix read, `read_membership`'s matrix and
/// 0xFF sweep, `global_ap_excluding`'s one read over the free key d, `plan`'s six-layout read for w
/// alone, the auto-backup snapshot, the value batch, the one membership frame, then the readback
/// for w.
fn remove_ap_end_to_end_script() -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(matrix_lines()); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 3u16), (0x04, 3), (0x16, 3), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(layout_read_lines(0x07, layout::AP, 2000)); // d, free
    lines.extend(key_settings_lines(0x1A, 1200, 0x18, 100, 150, 3, 0)); // plan's read of w

    lines.extend(auto_backup_lines(
        0,
        (1200, 0x18, 100, 150, 3, 0), // w
        (1200, 0x18, 100, 150, 3, 0), // a
        (1200, 0x18, 100, 150, 3, 0), // s
        (2000, 0x00, 0, 0, 0, 0),     // d
    ));

    let value_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    for f in cmds::write_key_records(&value_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    let membership_records = vec![KeyRecord {
        key: 0x1A,
        layout: layout::KEYSET_AP,
        value: 0,
    }];
    for f in cmds::write_key_records_singly(&membership_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 0, 0)); // w readback: correct
    lines
}

/// Removing one member of a three-key keyset must leave the other two exactly where they were.
/// The proof is in the first run: its byte-exact script only accepts frames naming `w`, so a
/// `remove` that rewrote or cleared `a` or `s` too (the same shape as `delete`) fails there, on a
/// replay send mismatch, before any assertion below even runs. The second run, a fresh
/// `wh keyset list ap` call against a hand-authored script fixed at compile time, proves something
/// narrower: that a board left in the state `remove` is supposed to produce still lists keyset 3
/// correctly, with `a` and `s` and no `w`. It cannot observe what the first run actually wrote.
#[test]
fn keyset_remove_leaves_the_keyset_alive_when_others_remain() {
    let write_lines = remove_ap_end_to_end_script();
    let write_script_path = write_script("keyset-remove-alive-write", &write_lines);
    let write_config_home = scratch_config_dir("keyset-remove-alive-write");
    let write_out = run_wh(
        &["keyset", "remove", "ap", "--keys", "w"],
        &write_script_path,
        &write_config_home,
    );
    assert!(
        write_out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&write_out.stdout),
        String::from_utf8_lossy(&write_out.stderr)
    );
    // The negative every "ceases to exist" test up to now has been missing: this keyset really
    // does survive, with `a` and `s` still members, so the announcement must not claim otherwise.
    // A `keyset_disappears` that always answered `true` would satisfy every other assertion in
    // this file and only fail here.
    let write_stdout = String::from_utf8_lossy(&write_out.stdout);
    assert!(
        !write_stdout.contains("ceases to exist"),
        "keyset 3 keeps a and s; it must not be announced as destroyed: {write_stdout}"
    );

    let mut list_lines = matrix_lines();
    for (usage, ks) in [(0x1Au8, 0u16), (0x04, 3), (0x16, 3), (0x07, 0)] {
        list_lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    list_lines.extend(layout_read_lines(0x04, layout::AP, 1200));
    list_lines.extend(layout_read_lines(0x16, layout::AP, 1200));
    let list_script_path = write_script("keyset-remove-alive-list", &list_lines);
    let list_config_home = scratch_config_dir("keyset-remove-alive-list");
    let list_out = run_wh(
        &["keyset", "list", "ap"],
        &list_script_path,
        &list_config_home,
    );
    assert!(
        list_out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&list_out.stderr)
    );
    let text = String::from_utf8_lossy(&list_out.stdout);
    assert!(text.contains("3 1.20mm  a,s"), "got: {text}");
    assert!(
        !text
            .split(|c: char| c.is_whitespace() || c == ',')
            .any(|tok| tok == "w"),
        "w left keyset 3, it must not still be listed as a member: {text}"
    );

    std::fs::remove_file(write_script_path).unwrap();
    let _ = std::fs::remove_dir_all(&write_config_home);
    std::fs::remove_file(list_script_path).unwrap();
    let _ = std::fs::remove_dir_all(&list_config_home);
}

/// `--help` output is never covered by the run-over-replay tests above, since it exits before
/// `WH_REPLAY` is ever read. `set rt`'s value help once claimed a single `--press` or `--release`
/// sufficed, the exact false claim B5 removed from the error message; this pins that it cannot
/// come back unnoticed.
#[test]
fn keyset_set_help_does_not_claim_a_single_rt_flag_suffices() {
    let out = Command::new(env!("CARGO_BIN_EXE_wh"))
        .args(["keyset", "set", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!text.contains("and/or"), "got: {text}");
    assert!(!text.contains("at least one of"), "got: {text}");
    // Pins the statement the code's own error messages actually enforce, not the earlier
    // "must be given together" wording, which was false: `--press` alone plus `--value` is
    // accepted, since `--value` supplies the missing release half.
    assert!(
        text.contains("--press requires --release or --value"),
        "got: {text}"
    );
    assert!(
        text.contains("--release requires --press or --value"),
        "got: {text}"
    );
}

/// The same check on `create` and `delete`, which never carried the false claim but are the two
/// other commands sharing this flag pattern.
#[test]
fn keyset_create_and_delete_help_do_not_claim_a_single_rt_flag_suffices() {
    for sub in ["create", "delete"] {
        let out = Command::new(env!("CARGO_BIN_EXE_wh"))
            .args(["keyset", sub, "--help"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let text = String::from_utf8_lossy(&out.stdout);
        // A positive anchor: the two `contains(false)` checks below pass just as well if
        // `--value`'s whole doc comment vanished, which is exactly what happened to `create`'s
        // when this test was last written without one.
        assert!(
            text.contains("Value in mm"),
            "{sub}: --value help missing: {text}"
        );
        assert!(!text.contains("and/or"), "{sub} got: {text}");
        assert!(!text.contains("at least one of"), "{sub} got: {text}");
    }
}

/// `<INDEX>` had no help text on `set` or `delete`, unlike every other argument in the tree.
#[test]
fn keyset_index_has_help_on_set_and_delete() {
    for sub in ["set", "delete"] {
        let out = Command::new(env!("CARGO_BIN_EXE_wh"))
            .args(["keyset", sub, "--help"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let text = String::from_utf8_lossy(&out.stdout);
        // The `Usage:` line also contains the bare word `<INDEX>`; the two-space indent marks
        // the `Arguments:` section's own entry for it instead.
        let index_at = text
            .find("\n  <INDEX>\n")
            .expect("INDEX must be listed in Arguments");
        let after = &text[index_at + 1..];
        let next_line = after.lines().nth(1).unwrap_or("").trim();
        assert!(
            !next_line.is_empty(),
            "{sub}: <INDEX> has no help text: {text}"
        );
    }
}

/// `wh keyset list ap` groups the board's 0xFF values into keysets and prints each one's members
/// by name. The script gives four keys, two of them at index 1 and one at index 2, so an
/// implementation that printed every non-zero key as its own keyset would fail here.
#[test]
fn keyset_list_ap_groups_members_by_index() {
    let mut lines = matrix_lines();
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    // one AP read per member of each keyset, for the value column: w and a agree at 2.00mm, s is
    // the only member of keyset 2.
    lines.extend(layout_read_lines(0x1A, layout::AP, 2000));
    lines.extend(layout_read_lines(0x04, layout::AP, 2000));
    lines.extend(layout_read_lines(0x16, layout::AP, 1200));
    let script = write_script("keyset-list-ap", &lines);
    let out = run_wh(
        &["keyset", "list", "ap"],
        &script,
        &scratch_config_dir("keyset-list-ap"),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("1 2.00mm  w,a"), "got: {text}");
    assert!(text.contains("2 1.20mm  s"), "got: {text}");
    assert!(
        !text
            .split(|c: char| c.is_whitespace() || c == ',')
            .any(|tok| tok == "d"),
        "key d must not appear as a member name, it holds 0 and is in no keyset: {text}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-list-ap"));
}

/// A board with no keysets prints so rather than printing an empty heading.
#[test]
fn keyset_list_says_none_when_no_key_holds_a_keyset() {
    let mut lines = matrix_lines();
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    let script = write_script("keyset-list-empty", &lines);
    let out = run_wh(
        &["keyset", "list", "ap"],
        &script,
        &scratch_config_dir("keyset-list-empty"),
    );
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("ap keysets: none"));

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-list-empty"));
}

/// The disagreement case: two members of the same keyset read different actuation points.
/// `wh keyset list` must show the disagreement, not print one member's value as though both
/// agreed, which is exactly the defect this test guards against.
#[test]
fn keyset_list_ap_shows_a_disagreement_instead_of_one_members_value() {
    let mut lines = matrix_lines();
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(layout_read_lines(0x1A, layout::AP, 2000));
    lines.extend(layout_read_lines(0x04, layout::AP, 1200));
    let script = write_script("keyset-list-ap-disagree", &lines);
    let out = run_wh(
        &["keyset", "list", "ap"],
        &script,
        &scratch_config_dir("keyset-list-ap-disagree"),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("1 disagree: w at 2.00mm, a at 1.20mm"),
        "got: {text}"
    );
    assert!(
        !text.contains("1 2.00mm  w,a"),
        "must not print one member's value as though both agreed: {text}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-list-ap-disagree"));
}

/// `Kind::Rt`'s own formatting: press/release, distinct from the ap column's bare millimetres.
#[test]
fn keyset_list_rt_formats_press_and_release() {
    let mut lines = matrix_lines();
    for (usage, ks) in [(0x1Au8, 0u16), (0x04, 0), (0x16, 5), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, ks));
    }
    lines.extend(layout_read_lines(0x16, layout::RT_PRESS, 250));
    lines.extend(layout_read_lines(0x16, layout::RT_RELEASE, 310));
    let script = write_script("keyset-list-rt", &lines);
    let out = run_wh(
        &["keyset", "list", "rt"],
        &script,
        &scratch_config_dir("keyset-list-rt"),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("5 0.25/0.31mm  s"), "got: {text}");

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-list-rt"));
}

/// `wh keyset list` with no kind argument lists ap then rt, each its own full membership read:
/// `wh` caches nothing, so the two kinds are two independent passes over the board.
#[test]
fn keyset_list_with_no_kind_lists_ap_then_rt() {
    let mut lines = matrix_lines();
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 0), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(layout_read_lines(0x1A, layout::AP, 2000));
    lines.extend(matrix_lines());
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, 0));
    }
    let script = write_script("keyset-list-both", &lines);
    let out = run_wh(
        &["keyset", "list"],
        &script,
        &scratch_config_dir("keyset-list-both"),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    let ap_at = text.find("ap keysets:").expect("ap heading missing");
    let rt_at = text.find("rt keysets:").expect("rt heading missing");
    assert!(ap_at < rt_at, "ap heading must come before rt: {text}");
    assert!(text.contains("1 2.00mm  w"), "got: {text}");
    assert!(text.contains("rt keysets: none"), "got: {text}");

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-list-both"));
}

/// A rapid trigger keyset whose members share press but differ on release. Equal press forces the
/// comparison onto the whole pair, not press alone, the shape a partial `wh set rt` write leaves.
/// Output-assertion lever: reading fewer frames than scripted here produces no later frame
/// mismatch (there is nothing scripted after it), so only the printed text can catch it.
#[test]
fn keyset_list_rt_shows_a_disagreement_when_only_release_differs() {
    let mut lines = matrix_lines();
    for (usage, ks) in [(0x1Au8, 7u16), (0x04, 7), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, ks));
    }
    lines.extend(layout_read_lines(0x1A, layout::RT_PRESS, 200));
    lines.extend(layout_read_lines(0x1A, layout::RT_RELEASE, 150));
    lines.extend(layout_read_lines(0x04, layout::RT_PRESS, 200));
    lines.extend(layout_read_lines(0x04, layout::RT_RELEASE, 300));
    let script = write_script("keyset-list-rt-disagree", &lines);
    let out = run_wh(
        &["keyset", "list", "rt"],
        &script,
        &scratch_config_dir("keyset-list-rt-disagree"),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("7 disagree: w at 0.20/0.15mm, a at 0.20/0.30mm"),
        "got: {text}"
    );
    assert!(
        !text.contains("0.20/0.15mm  w,a"),
        "must not print one member's pair as though both agreed: {text}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(scratch_config_dir("keyset-list-rt-disagree"));
}

/// The measured vendor shape for taking one key out of a keyset: the removed key gets the whole
/// per-key template ending in `0xFF = 0`, and the members that stay get no records at all
/// (`ks-remove-one-key`, `docs/keysets.md`). Exact frame equality is what pins the second half:
/// `a` and `s` must appear nowhere in the plan, which a rewrite that rewrote every member of the
/// keyset would break while still clearing `w`'s membership correctly.
#[test]
fn keyset_remove_ap_writes_only_the_removed_key_and_clears_its_membership() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 3u16), (0x04, 3), (0x16, 3), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    // global_ap reads the actuation point of the keys outside every keyset. `d` is the only one.
    lines.extend(layout_read_lines(0x07, layout::AP, 2000));
    // plan's own per-key read of w, in plan's read order.
    lines.extend(key_settings_lines(0x1A, 1200, 0x18, 100, 150, 3, 0));

    let script = write_script("keyset-remove-ap", &lines);
    let config_home = scratch_config_dir("keyset-remove-ap");
    let out = run_wh(
        &["keyset", "remove", "ap", "--keys", "w", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    // The keyset it leaves, its prior value and the value it returns to are all pinned, and all
    // three differ, so a mutation that printed any one of them in another's place fails here.
    assert!(
        stdout.contains("ap: removing w from keyset 3, 1.20mm to 2.00mm"),
        "got: {stdout}"
    );

    let value_records = [
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    let mut expected: Vec<String> = cmds::write_key_records(&value_records)
        .iter()
        .map(|f| hex(f))
        .collect();
    expected.extend(
        cmds::write_key_records_singly(&[KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_AP,
            value: 0,
        }])
        .iter()
        .map(|f| hex(f)),
    );
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "only w is written, membership last and alone: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// A selection mixing a keyset member with a free key: `wh keyset remove ap --keys w,d` where `w`
/// is in keyset 3 and `d` is already free but away from the base. Both are now included in the
/// same `plan`: `d` gets its own value group and its own membership-clear record, even though it
/// was never in a keyset, because `remove`'s job is a destination every selected key reaches, not
/// only a transition for the ones that were members. `d` is also the only free key on this board,
/// so it and `w` are the whole selection: no free key is left outside it to read a base from, and
/// the resolved target is `NO_SIGNAL_BASE`, not a read value.
#[test]
fn keyset_remove_writes_a_free_key_selected_alongside_a_member() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 3u16), (0x04, 3), (0x16, 3), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    // No base read at all: w and d are both in the selection, and d was the only free key, so
    // none is left outside the selection to read from.
    lines.extend(key_settings_lines(0x1A, 1200, 0x18, 100, 150, 3, 0)); // plan's read of w
    lines.extend(key_settings_lines(0x07, 1100, 0x18, 100, 150, 0, 0)); // plan's read of d

    let script = write_script("keyset-remove-mixed", &lines);
    let config_home = scratch_config_dir("keyset-remove-mixed");
    let out = run_wh(
        &["keyset", "remove", "ap", "--keys", "w,d", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Both w and d are excluded from the base read (both are in this selection), and d was the
    // only free key on the board, so nothing is left to read: the base is the invented default,
    // and both lines say so.
    assert!(
        stdout.contains("ap: removing w from keyset 3, 1.20mm to 2.00mm (no key outside a keyset to read a base from, using the default)"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("ap: returning d to 2.00mm (no key outside a keyset to read a base from, using the default), already in no ap keyset"),
        "got: {stdout}"
    );

    let value_records = [
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 150,
        },
        KeyRecord {
            key: 0x07,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x07,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x07,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x07,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    let mut expected: Vec<String> = cmds::write_key_records(&value_records)
        .iter()
        .map(|f| hex(f))
        .collect();
    expected.extend(
        cmds::write_key_records_singly(&[
            KeyRecord {
                key: 0x1A,
                layout: layout::KEYSET_AP,
                value: 0,
            },
            KeyRecord {
                key: 0x07,
                layout: layout::KEYSET_AP,
                value: 0,
            },
        ])
        .iter()
        .map(|f| hex(f)),
    );
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "d is free but selected, and now gets its own value group and membership clear too: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

// -- remove resets to the base, rather than only reversing membership --

/// The case the whole task exists for: `w` was never in a keyset, so today's `remove` refuses it
/// outright. Here it must reach the base read from the other free keys and clear its own
/// membership, even though that membership was already 0: `plan`'s membership write is
/// unconditional per selected key (`plan_skip_rule_skips_a_key_already_at_every_target_but_still_writes_membership`
/// in `wh-device`), so a selected key always carries a `0xFF` record, whether or not it changes.
#[test]
fn keyset_remove_returns_a_free_key_to_the_base() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    // global_ap_excluding reads the AP of every free key except w: a, s, d.
    lines.extend(layout_read_lines(0x04, layout::AP, 2000));
    lines.extend(layout_read_lines(0x16, layout::AP, 2000));
    lines.extend(layout_read_lines(0x07, layout::AP, 2000));
    lines.extend(key_settings_lines(0x1A, 1100, 0x18, 100, 150, 0, 0)); // plan's read of w

    let script = write_script("keyset-remove-free-to-base", &lines);
    let config_home = scratch_config_dir("keyset-remove-free-to-base");
    let out = run_wh(
        &["keyset", "remove", "ap", "--keys", "w", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ap: returning w to 2.00mm, already in no ap keyset"),
        "got: {stdout}"
    );

    let value_records = [
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    let mut expected: Vec<String> = cmds::write_key_records(&value_records)
        .iter()
        .map(|f| hex(f))
        .collect();
    expected.extend(
        cmds::write_key_records_singly(&[KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_AP,
            value: 0,
        }])
        .iter()
        .map(|f| hex(f)),
    );
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "today's remove refuses this key outright; it must instead reach the base: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// A free key already at the base gets no value record, `plan`'s own skip rule. It still carries
/// the membership-clear record `plan` writes unconditionally for every selected key
/// (`plan_skip_rule_skips_a_key_already_at_every_target_but_still_writes_membership`), so the wire
/// is not literally silent: the announcement says "membership rewritten, value unchanged", not
/// "nothing to do", since a real frame is sent even though it is idempotent and destroys nothing.
/// `wh-device`'s own comment on that record ("whether the vendor skips such a key is unmeasured,
/// and an unconditional rewrite is non-destructive") is why `remove` does not try to suppress it.
#[test]
fn keyset_remove_writes_only_membership_for_a_key_already_at_the_base() {
    let mut lines = matrix_lines();
    lines.extend(matrix_lines());
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    lines.extend(layout_read_lines(0x04, layout::AP, 2000));
    lines.extend(layout_read_lines(0x16, layout::AP, 2000));
    lines.extend(layout_read_lines(0x07, layout::AP, 2000));
    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 0, 0));

    let script = write_script("keyset-remove-already-base", &lines);
    let config_home = scratch_config_dir("keyset-remove-already-base");
    let out = run_wh(
        &["keyset", "remove", "ap", "--keys", "w", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(
            "ap: w already at 2.00mm in no ap keyset, membership rewritten, value unchanged"
        ),
        "got: {stdout}"
    );

    let expected: Vec<String> = cmds::write_key_records_singly(&[KeyRecord {
        key: 0x1A,
        layout: layout::KEYSET_AP,
        value: 0,
    }])
    .iter()
    .map(|f| hex(f))
    .collect();
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "no value record, only the unconditional membership clear: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The commoner case the mode-only fix missed: a free key with its *own* non-base sensitivity, so
/// the value genuinely moves and the mode transition would otherwise be silent, dropped by
/// `value_moves` taking priority with nothing to append it. `w` is outside every rt keyset at
/// 0.20/0.25mm, touch nibble `Rt` (own settings); the base, read from the other free keys, is
/// 0.10/0.15mm. `remove`'s `rt_off` change both resets the sensitivity and turns rapid trigger off,
/// MODE `0x0038` to `0x0018`, in the same write, and the announcement must say both: a value that
/// changed from 0.20/0.25mm to 0.10/0.15mm reads as "the new rapid trigger setting" unless the mode
/// clause makes clear there will be no rapid trigger at all.
#[test]
fn keyset_remove_rt_names_the_mode_transition_alongside_a_value_that_also_moves() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, 0));
    }
    // remove_base_rt reads the press/release of every free key except w: a, s, d.
    for usage in [0x04u8, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::RT_PRESS, 100));
        lines.extend(layout_read_lines(usage, layout::RT_RELEASE, 150));
    }
    // plan's own read of w: rt keyset 0 (free), MODE 0x38 (Rt, own settings), own sensitivity
    // 0.20/0.25mm, away from the base.
    lines.extend(key_settings_lines(0x1A, 2000, 0x38, 200, 250, 0, 0));

    let script = write_script("keyset-remove-rt-value-and-mode", &lines);
    let config_home = scratch_config_dir("keyset-remove-rt-value-and-mode");
    let out = run_wh(
        &["keyset", "remove", "rt", "--keys", "w", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout
            .contains("rt: returning w to 0.10/0.15mm, mode Rt to Single, already in no rt keyset"),
        "got: {stdout}"
    );

    let value_records = [
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    let mut expected: Vec<String> = cmds::write_key_records(&value_records)
        .iter()
        .map(|f| hex(f))
        .collect();
    expected.extend(
        cmds::write_key_records_singly(&[KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_RT,
            value: 0,
        }])
        .iter()
        .map(|f| hex(f)),
    );
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "MODE 0x0038 to 0x0018 and the sensitivity reset both really are on the wire: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The measured case a plain owned-value comparison gets wrong: `w` is outside every rt keyset,
/// already at the base sensitivity (0.10/0.15mm), but its own touch nibble still says `Rt` (its
/// own settings), not `Single`. `remove`'s `rt_off` change turns that off, which is a real frame
/// on the wire, MODE `0x0038` to `0x0018`, even though the press/release pair it carries never
/// moves. The announcement must name the mode transition, not call this "returning" (a value that
/// never moved is not being "returned" anywhere) and not "nothing to do" (rapid trigger really did
/// switch off): a comparison that only checked the owned press/release pair against the target
/// would have missed that entirely, silent in both directions.
#[test]
fn keyset_remove_rt_names_a_mode_only_change_as_a_mode_transition() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, 0));
    }
    // remove_base_rt reads the press/release of every free key except w: a, s, d.
    for usage in [0x04u8, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::RT_PRESS, 100));
        lines.extend(layout_read_lines(usage, layout::RT_RELEASE, 150));
    }
    // plan's own read of w: rt keyset 0 (free), MODE 0x38 (Rt, own settings), already at the
    // base sensitivity.
    lines.extend(key_settings_lines(0x1A, 2000, 0x38, 100, 150, 0, 0));

    let script = write_script("keyset-remove-rt-mode-only", &lines);
    let config_home = scratch_config_dir("keyset-remove-rt-mode-only");
    let out = run_wh(
        &["keyset", "remove", "rt", "--keys", "w", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("nothing to do"),
        "a frame turning rapid trigger off must never be reported as a no-op: {stdout}"
    );
    assert!(
        !stdout.contains("returning w"),
        "the sensitivity never moves, so this is not a value returning anywhere: {stdout}"
    );
    assert!(
        stdout.contains("rt: w keeps 0.10/0.15mm, mode Rt to Single, already in no rt keyset"),
        "got: {stdout}"
    );

    let value_records = [
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    let mut expected: Vec<String> = cmds::write_key_records(&value_records)
        .iter()
        .map(|f| hex(f))
        .collect();
    expected.extend(
        cmds::write_key_records_singly(&[KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_RT,
            value: 0,
        }])
        .iter()
        .map(|f| hex(f)),
    );
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "MODE 0x0038 to 0x0018 really is on the wire despite the unchanged sensitivity: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The mirror case on the actuation point side, and the one the operator's own board hit
/// (`docs/backlog.md`): a free key already at the base value but still on touch nibble 0, "follow
/// global travel". `remove`'s `Change::ap` promotes that to `Single`, MODE `0x0008` to `0x0018`, a
/// real frame even though the actuation point itself, already `2.00mm`, never moves. The
/// announcement must name the mode transition, not the "returning" wording a plain value
/// comparison would produce.
#[test]
fn keyset_remove_ap_names_a_mode_only_change_as_a_mode_transition() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    // remove_base_ap reads the actuation point of every free key except w: a, s, d.
    lines.extend(layout_read_lines(0x04, layout::AP, 2000));
    lines.extend(layout_read_lines(0x16, layout::AP, 2000));
    lines.extend(layout_read_lines(0x07, layout::AP, 2000));
    // plan's own read of w: ap keyset 0 (free), MODE 0x08 (Global, touch nibble 0), already at
    // the base actuation point.
    lines.extend(key_settings_lines(0x1A, 2000, 0x08, 100, 150, 0, 0));

    let script = write_script("keyset-remove-ap-mode-only", &lines);
    let config_home = scratch_config_dir("keyset-remove-ap-mode-only");
    let out = run_wh(
        &["keyset", "remove", "ap", "--keys", "w", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("nothing to do"),
        "a frame promoting the key off global travel must never be reported as a no-op: {stdout}"
    );
    assert!(
        !stdout.contains("returning w"),
        "the actuation point never moves, so this is not a value returning anywhere: {stdout}"
    );
    assert!(
        stdout.contains("ap: w keeps 2.00mm, mode Global to Single, already in no ap keyset"),
        "got: {stdout}"
    );

    let value_records = [
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    let mut expected: Vec<String> = cmds::write_key_records(&value_records)
        .iter()
        .map(|f| hex(f))
        .collect();
    expected.extend(
        cmds::write_key_records_singly(&[KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_AP,
            value: 0,
        }])
        .iter()
        .map(|f| hex(f)),
    );
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "MODE 0x0008 to 0x0018 really is on the wire despite the unchanged actuation point: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `w` disagrees with the rest of the board (1.10mm against 2.00mm), which would ordinarily
/// `Split`. Because `w` is itself being reset, `global_ap_excluding` leaves it out of the reading,
/// so the remaining free keys agree and the removal can proceed. Without excluding it, this is
/// exactly the case that would wrongly refuse.
#[test]
fn keyset_remove_takes_the_base_from_the_keys_it_is_not_resetting() {
    let mut lines = matrix_lines();
    lines.extend(matrix_lines());
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    lines.extend(layout_read_lines(0x04, layout::AP, 2000));
    lines.extend(layout_read_lines(0x16, layout::AP, 2000));
    lines.extend(layout_read_lines(0x07, layout::AP, 2000));
    lines.extend(key_settings_lines(0x1A, 1100, 0x18, 100, 150, 0, 0));

    let script = write_script("keyset-remove-excludes-self", &lines);
    let config_home = scratch_config_dir("keyset-remove-excludes-self");
    let out = run_wh(
        &["keyset", "remove", "ap", "--keys", "w", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ap: returning w to 2.00mm, already in no ap keyset"),
        "an un-excluded reading would see w disagree with itself and Split: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `w` is being reset, but `a` (1.50mm) is left behind disagreeing with `s` and `d` (2.00mm): the
/// remaining free keys genuinely disagree, so `remove` must refuse rather than invent a winner.
#[test]
fn keyset_remove_refuses_when_the_remaining_free_keys_disagree() {
    let mut lines = matrix_lines();
    lines.extend(matrix_lines());
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    lines.extend(layout_read_lines(0x04, layout::AP, 1500));
    lines.extend(layout_read_lines(0x16, layout::AP, 2000));
    lines.extend(layout_read_lines(0x07, layout::AP, 2000));

    let script = write_script("keyset-remove-remaining-disagree", &lines);
    let config_home = scratch_config_dir("keyset-remove-remaining-disagree");
    let out = run_wh(
        &["keyset", "remove", "ap", "--keys", "w"],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("1.50mm"), "got: {err}");
    assert!(err.contains("2.00mm"), "got: {err}");
    assert!(err.contains("1 key(s) at 1.50mm"), "got: {err}");
    assert!(err.contains("2 key(s) at 2.00mm"), "got: {err}");
    // Not just the counts: `create`/`set`/`delete`'s own pre-existing `split_message` also
    // produces "N key(s) at X.XXmm" text, so a `remove` that wrongly called that helper instead of
    // `remove_split_message` would still pass the two assertions above. This clause is the one
    // only `remove`'s own refusal emits.
    assert!(
        err.contains("include them in the selection so they are reset too"),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Every key on this four-key board is in a keyset and every one is selected, so no free key is
/// left to read a base from at all: `global_ap_excluding` reports `NoneOutsideAKeyset` and
/// `remove` falls back to `NO_SIGNAL_BASE`, 2.00mm.
#[test]
fn keyset_remove_uses_the_base_constant_when_no_free_key_is_left() {
    let mut lines = matrix_lines();
    lines.extend(matrix_lines());
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 1), (0x07, 1)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    // No free-key read at all: every key is in the selection, so none is left outside it.
    for (usage, ap) in [(0x1Au8, 1200u16), (0x04, 1200), (0x16, 1200), (0x07, 1200)] {
        lines.extend(key_settings_lines(usage, ap, 0x18, 100, 150, 1, 0));
    }

    let script = write_script("keyset-remove-no-signal", &lines);
    let config_home = scratch_config_dir("keyset-remove-no-signal");
    // `--dry-run` also means this whole-board selection never reaches the confirmation prompt,
    // so no stdin is needed here; `keyset_remove_over_the_whole_board_requires_a_typed_yes`
    // covers that gate on its own.
    let out = run_wh(
        &["keyset", "remove", "ap", "--keys", "w,a,s,d", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ap: removing w from keyset 1, 1.20mm to 2.00mm"),
        "got: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The rapid trigger mirror of the case above refuses instead of falling back: there is no
/// measured default sensitivity, unlike the actuation point's `2000`. Every key on this four-key
/// board genuinely is in an rt keyset, so `global_rt_excluding` reports `NoneOutsideAKeyset` for a
/// selection of just `w`, and `remove_base_rt` must bail rather than inventing `0x14 = 2000,
/// 0x15 = 2000`, a pair no capture has ever shown written. Pins the "no key is outside" wording
/// specifically, since that is only true of a board shaped exactly like this one: every key really
/// is in a keyset here, which the mirror test below is not.
///
/// This is also `remove rt`'s half of the divergence from `delete`, which `reset_change` must
/// never collapse: `delete` in the same situation refuses too, but names `--press and --release`
/// as the way out, flags `wh keyset remove` does not have. See
/// `keyset_delete_ap_refuses_where_remove_would_invent_a_base` for `delete`'s half.
#[test]
fn keyset_remove_rt_refuses_when_no_free_key_is_left_to_read_a_sensitivity_from() {
    let mut lines = matrix_lines();
    lines.extend(matrix_lines());
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 1), (0x07, 1)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, ks));
    }
    // No further reads at all: every key is in a keyset, so `global_rt_excluding` finds nothing
    // outside one to read even before considering the selection.

    let script = write_script("keyset-remove-rt-no-signal", &lines);
    let config_home = scratch_config_dir("keyset-remove-rt-no-signal");
    let out = run_wh(
        &["keyset", "remove", "rt", "--keys", "w"],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("no key is outside a rapid trigger keyset"),
        "got: {err}"
    );
    assert!(err.contains("no default is measured"), "got: {err}");

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The case the wording above gets wrong: no rt keyset exists on this board at all, every key
/// genuinely free, but every one of them is also in the selection, so `global_rt_excluding` still
/// reports `NoneOutsideAKeyset`. "No key is outside a rapid trigger keyset" would be false here,
/// and would send an operator looking for keysets that plainly do not exist. `m.entries()` already
/// distinguishes the two causes with no extra device read.
#[test]
fn keyset_remove_rt_refuses_when_every_free_key_is_also_selected() {
    let mut lines = matrix_lines();
    lines.extend(matrix_lines());
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, 0));
    }
    // No further reads: every free key is also in the selection, so `global_rt_excluding` finds
    // nothing left outside it to read, the same `NoneOutsideAKeyset` report as the test above, for
    // a genuinely different reason.

    let script = write_script("keyset-remove-rt-all-selected", &lines);
    let config_home = scratch_config_dir("keyset-remove-rt-all-selected");
    let out = run_wh(
        &["keyset", "remove", "rt", "--keys", "w,a,s,d"],
        &script,
        &config_home,
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !err.contains("no key is outside a rapid trigger keyset"),
        "every key here is outside a keyset; the wrong cause must not be named: {err}"
    );
    assert!(
        err.contains("every key outside a rapid trigger keyset is also in this selection"),
        "got: {err}"
    );
    assert!(err.contains("no default is measured"), "got: {err}");

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Selecting every key in the board's live matrix triggers the confirmation, regardless of the
/// literal text used to name them: this run spells out `w,a,s,d` rather than any `all` keyword.
/// The first run declines and must leave the board untouched (the script accepts no frame past
/// the confirmation prompt's own reads); the second run accepts and proceeds to write.
#[test]
fn keyset_remove_over_the_whole_board_requires_a_typed_yes() {
    let mut decline_lines = matrix_lines();
    decline_lines.extend(matrix_lines());
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 1), (0x07, 1)] {
        decline_lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    // `plan` is built before the confirmation now, so it reads all four keys even on the
    // declined half: only the write that follows a `yes` is what the decline never reaches.
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        decline_lines.extend(key_settings_lines(usage, 1200, 0x18, 100, 150, 1, 0));
    }
    let decline_script = write_script("keyset-remove-whole-board-no", &decline_lines);
    let decline_config_home = scratch_config_dir("keyset-remove-whole-board-no");
    let decline_out = run_wh_stdin(
        &["keyset", "remove", "ap", "--keys", "w,a,s,d"],
        &decline_script,
        &decline_config_home,
        "no\n",
    );
    assert!(!decline_out.status.success());
    // The prompt itself is on stderr, a diagnostic, not stdout: the refusal that follows it,
    // once the reader answers `no`, is also the command's own error on stderr.
    let decline_err = String::from_utf8_lossy(&decline_out.stderr);
    assert!(
        decline_err.contains("ap keyset(s) 1 will cease to exist"),
        "got: {decline_err}"
    );
    // The value clause, not only the keyset clause: this is what a call-site refactor could
    // drop while every unit test on `confirm_whole_board_remove` itself stays green, since
    // those only prove the string is built correctly, not that it reaches the operator.
    assert!(
        decline_err.contains("every key moves to 2.00mm"),
        "got: {decline_err}"
    );
    assert!(
        decline_err.contains("was not confirmed"),
        "got: {decline_err}"
    );

    std::fs::remove_file(decline_script).unwrap();
    let _ = std::fs::remove_dir_all(&decline_config_home);

    // `--dry-run` would skip the confirmation entirely, so this half must be a real run: the
    // script covers `plan`'s reads, the auto-backup snapshot, the actual write frames, and the
    // readback verification, the whole pipeline `yes` unlocks.
    let mut accept_lines = matrix_lines();
    accept_lines.extend(matrix_lines());
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 1), (0x07, 1)] {
        accept_lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        accept_lines.extend(key_settings_lines(usage, 1200, 0x18, 100, 150, 1, 0));
    }
    accept_lines.extend(auto_backup_lines(
        0,
        (1200, 0x18, 100, 150, 1, 0),
        (1200, 0x18, 100, 150, 1, 0),
        (1200, 0x18, 100, 150, 1, 0),
        (1200, 0x18, 100, 150, 1, 0),
    ));
    let value_records: Vec<KeyRecord> = [0x1Au8, 0x04, 0x16, 0x07]
        .iter()
        .flat_map(|&usage| {
            [
                KeyRecord {
                    key: usage,
                    layout: layout::MODE,
                    value: 0x18,
                },
                KeyRecord {
                    key: usage,
                    layout: layout::AP,
                    value: 2000,
                },
                KeyRecord {
                    key: usage,
                    layout: layout::RT_PRESS,
                    value: 100,
                },
                KeyRecord {
                    key: usage,
                    layout: layout::RT_RELEASE,
                    value: 150,
                },
            ]
        })
        .collect();
    // `frames()` never splits one key's own group across a report boundary: 16 records at 4 per
    // key exceeds the 14-record limit, so it batches whole groups (w, a, s: 12) into one frame
    // and starts a fresh one (d: 4) rather than splitting mid-key the way a plain 14-record
    // chunking of the flat list would.
    for f in cmds::write_key_records(&value_records[..12]) {
        accept_lines.push(out_line(&f));
        accept_lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    for f in cmds::write_key_records(&value_records[12..]) {
        accept_lines.push(out_line(&f));
        accept_lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    let membership_records: Vec<KeyRecord> = [0x1Au8, 0x04, 0x16, 0x07]
        .iter()
        .map(|&usage| KeyRecord {
            key: usage,
            layout: layout::KEYSET_AP,
            value: 0,
        })
        .collect();
    for f in cmds::write_key_records_singly(&membership_records) {
        accept_lines.push(out_line(&f));
        accept_lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        accept_lines.extend(key_settings_lines(usage, 2000, 0x18, 100, 150, 0, 0));
    }

    let accept_script = write_script("keyset-remove-whole-board-yes", &accept_lines);
    let accept_config_home = scratch_config_dir("keyset-remove-whole-board-yes");
    let accept_out = run_wh_stdin(
        &["keyset", "remove", "ap", "--keys", "w,a,s,d"],
        &accept_script,
        &accept_config_home,
        "yes\n",
    );
    assert!(
        accept_out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&accept_out.stdout),
        String::from_utf8_lossy(&accept_out.stderr)
    );
    let accept_stdout = String::from_utf8_lossy(&accept_out.stdout);
    assert!(
        accept_stdout.contains("ap: removing w from keyset 1, 1.20mm to 2.00mm"),
        "got: {accept_stdout}"
    );

    std::fs::remove_file(accept_script).unwrap();
    let _ = std::fs::remove_dir_all(&accept_config_home);
}

/// The negative half is what actually guards the split: asserting the prompt is in stderr does
/// not stop a future change sending it to both streams, only `!stdout.contains(..)` does.
#[test]
fn keyset_remove_prompt_goes_to_stderr_not_stdout() {
    let mut lines = matrix_lines();
    lines.extend(matrix_lines());
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 1), (0x07, 1)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(key_settings_lines(usage, 1200, 0x18, 100, 150, 1, 0));
    }
    let script = write_script("keyset-remove-prompt-stream", &lines);
    let config_home = scratch_config_dir("keyset-remove-prompt-stream");
    let out = run_wh_stdin(
        &["keyset", "remove", "ap", "--keys", "w,a,s,d"],
        &script,
        &config_home,
        "no\n",
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stderr.contains("type yes to continue"),
        "the prompt must reach stderr: got stderr: {stderr}"
    );
    assert!(
        !stdout.contains("type yes to continue"),
        "the prompt must not also reach stdout: got stdout: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The measured board a per-key announcement alone cannot save: four free keys, no ap keysets,
/// every one already at 2.00mm. The value clause and the keyset clause are both literally true and
/// both read as a no-op. What actually happens, promoting every key off touch nibble 0 ("follow
/// global travel") onto its own pinned actuation point, only shows up in the per-key lines that
/// print after the prompt is already answered. The prompt itself must name it, as a count, so the
/// operator has it in front of them before answering rather than after.
#[test]
fn keyset_remove_whole_board_prompt_names_a_mode_transition_a_no_op_value_would_hide() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    // No base read at all: every key is in the selection, so none is left outside it. `plan`'s own
    // six-layout read for every key: already at 2.00mm, MODE 0x08 (Global, touch nibble 0).
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(key_settings_lines(usage, 2000, 0x08, 100, 150, 0, 0));
    }

    let script = write_script("keyset-remove-whole-board-mode-only-prompt", &lines);
    let config_home = scratch_config_dir("keyset-remove-whole-board-mode-only-prompt");
    let out = run_wh_stdin(
        &["keyset", "remove", "ap", "--keys", "w,a,s,d"],
        &script,
        &config_home,
        "no\n",
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("every key moves to 2.00mm"),
        "the value clause alone reads as a no-op here: {stderr}"
    );
    assert!(
        stderr.contains("no ap keysets exist to lose"),
        "got: {stderr}"
    );
    assert!(
        stderr.contains("4 key(s) move off global travel onto their own actuation point"),
        "the mode transition must be in the prompt itself, before the operator answers: {stderr}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The mixed case a single all-moving fixture cannot distinguish from "count everything": two of
/// the four keys are already on touch nibble 1 (Single), so only `w` and `a` actually move. A
/// mutant that counted `plan.before().len()` instead of filtering by `mode_change` would pass the
/// all-four-move test above unchanged and only be caught here, where the count and the selection
/// size differ.
#[test]
fn keyset_remove_whole_board_prompt_counts_only_the_keys_that_actually_move() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    // w and a are still on touch nibble 0 and will promote; s and d are already Single.
    lines.extend(key_settings_lines(0x1A, 2000, 0x08, 100, 150, 0, 0));
    lines.extend(key_settings_lines(0x04, 2000, 0x08, 100, 150, 0, 0));
    lines.extend(key_settings_lines(0x16, 2000, 0x18, 100, 150, 0, 0));
    lines.extend(key_settings_lines(0x07, 2000, 0x18, 100, 150, 0, 0));

    let script = write_script("keyset-remove-whole-board-mixed-mode", &lines);
    let config_home = scratch_config_dir("keyset-remove-whole-board-mixed-mode");
    let out = run_wh_stdin(
        &["keyset", "remove", "ap", "--keys", "w,a,s,d"],
        &script,
        &config_home,
        "no\n",
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("2 key(s) move off global travel onto their own actuation point"),
        "two of four move, not four of four: {stderr}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The other edge the single all-moving fixture cannot reach: every key already on touch nibble 1,
/// so nothing moves at all. A mutant counting `plan.before().len()` would print "4 key(s) move off
/// global travel" over a selection where not one of them does; the clause must be absent entirely.
#[test]
fn keyset_remove_whole_board_prompt_omits_the_mode_clause_when_nothing_moves() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    // Every key already on touch nibble 1 (Single) and already at the base: nothing moves.
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(key_settings_lines(usage, 2000, 0x18, 100, 150, 0, 0));
    }

    let script = write_script("keyset-remove-whole-board-no-mode-change", &lines);
    let config_home = scratch_config_dir("keyset-remove-whole-board-no-mode-change");
    let out = run_wh_stdin(
        &["keyset", "remove", "ap", "--keys", "w,a,s,d"],
        &script,
        &config_home,
        "no\n",
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("this selects every key on the board: every key moves to 2.00mm, and no ap keysets exist to lose"),
        "the prompt must end there, with no mode clause appended: {stderr}"
    );
    assert!(
        !stderr.contains("move off global travel"),
        "nothing moves here, the clause must be absent entirely, not \"0 key(s)\": {stderr}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The gap the two fixtures above cannot separate: a board where every key already sits on touch
/// nibble 1 (Single) but every key's actuation point still differs from the base, so `plan` still
/// bundles a MODE record for each key, echoing the unchanged nibble back, because the bundle is
/// emitted whenever any of MODE/AP/RT_PRESS/RT_RELEASE differs and the nibble-0 omission only
/// applies to a nibble that stays `Global`. Counting keys with any value record, or counting keys
/// with a MODE record, both see this record and wrongly count every key; only counting an actual
/// touch nibble change, what `moved_mode_count` does, correctly reports zero.
#[test]
fn keyset_remove_whole_board_prompt_omits_the_mode_clause_when_only_the_value_moves() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    // Every key already Single (nibble 1) but away from the base 2.00mm: the AP change alone
    // triggers the bundle, and since the nibble is not `Global` the MODE record still goes out,
    // echoing the same value back unchanged.
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(key_settings_lines(usage, 1200, 0x18, 100, 150, 0, 0));
    }

    let script = write_script("keyset-remove-whole-board-value-only-mode-echo", &lines);
    let config_home = scratch_config_dir("keyset-remove-whole-board-value-only-mode-echo");
    let out = run_wh_stdin(
        &["keyset", "remove", "ap", "--keys", "w,a,s,d"],
        &script,
        &config_home,
        "no\n",
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("this selects every key on the board: every key moves to 2.00mm, and no ap keysets exist to lose"),
        "the prompt must end there, with no mode clause appended: {stderr}"
    );
    assert!(
        !stderr.contains("move off global travel"),
        "every key here already holds touch nibble 1; only the value moves, so the clause must be absent entirely: {stderr}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The measured case where an invented value renders exactly like a real one: every key on this
/// board is in ap keyset 1, so no free key exists anywhere, not merely outside this selection.
/// `wh keyset remove ap --keys w` reaches `NoneOutsideAKeyset` from a single-key selection, no
/// whole-board confirmation gate in the way, and the announcement must say the value was invented
/// rather than let `2.00mm` print indistinguishably from a value the board actually held.
///
/// This is also `remove ap`'s half of the divergence from `delete`, which `reset_change` must
/// never collapse: `delete` on this same board refuses outright and names `--value`, a flag
/// `wh keyset remove` does not have, rather than inventing anything. See
/// `keyset_delete_ap_refuses_where_remove_would_invent_a_base` for `delete`'s half.
#[test]
fn keyset_remove_ap_names_the_base_as_invented_when_every_key_is_already_in_a_keyset() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 1));
    }
    // No free-key read at all: every key on the board is in keyset 1, none outside it anywhere.
    lines.extend(key_settings_lines(0x1A, 300, 0x18, 100, 150, 1, 0)); // plan's read of w

    let script = write_script("keyset-remove-ap-invented-base", &lines);
    let config_home = scratch_config_dir("keyset-remove-ap-invented-base");
    let out = run_wh(
        &["keyset", "remove", "ap", "--keys", "w", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ap: removing w from keyset 1, 0.30mm to 2.00mm (no key outside a keyset to read a base from, using the default)"),
        "got: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Taking a keyset's only member empties it, the same fact the whole-board prompt already names
/// for every keyset at once. `w` alone is ap keyset 1; `a`, `s`, `d` are free and agree at 2.00mm.
/// The per-key announcement must say the keyset ceases to exist, not just that `w` left it, since a
/// partial removal (this is not a whole-board selection at all) triggers no confirmation to say so
/// any other way.
#[test]
fn keyset_remove_ap_names_a_keyset_that_ceases_to_exist_from_a_partial_removal() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 0), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    // global_ap_excluding reads the actuation point of the free keys: a, s, d, all agreeing.
    lines.extend(layout_read_lines(0x04, layout::AP, 2000));
    lines.extend(layout_read_lines(0x16, layout::AP, 2000));
    lines.extend(layout_read_lines(0x07, layout::AP, 2000));
    // plan's own read of w, the keyset's only member.
    lines.extend(key_settings_lines(0x1A, 1200, 0x18, 100, 150, 1, 0));

    let script = write_script("keyset-remove-ap-last-member", &lines);
    let config_home = scratch_config_dir("keyset-remove-ap-last-member");
    let out = run_wh(
        &["keyset", "remove", "ap", "--keys", "w", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ap: removing w from keyset 1, 1.20mm to 2.00mm, keyset 1 ceases to exist"),
        "got: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The gap the fixture above cannot reach: two keysets, 1 holding `w,a` and 2 holding `s,d`,
/// removing `w,a,s`. Keyset 1 loses every member and must be announced as gone; keyset 2 keeps `d`
/// and must not be. The mutant `leaving.len() == ks.members.len()` compares the whole selection's
/// size against one keyset's own member count instead of checking that keyset's own members are
/// the ones leaving, and gets keyset 1 wrong here: `leaving` holds three entries in total (w, a and
/// s, across both keysets) against keyset 1's two members, so the mutant answers `false` where the
/// real predicate, matching each of keyset 1's members against `leaving` by index, answers `true`.
#[test]
fn keyset_remove_ap_names_a_keyset_that_ceases_to_exist_from_a_partial_removal_of_two_keysets() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    // No base read at all: every key on this board is in keyset 1 or keyset 2, so none is free
    // regardless of who is selected.
    lines.extend(key_settings_lines(0x1A, 1200, 0x18, 100, 150, 1, 0)); // plan's read of w
    lines.extend(key_settings_lines(0x04, 1200, 0x18, 100, 150, 1, 0)); // plan's read of a
    lines.extend(key_settings_lines(0x16, 1200, 0x18, 100, 150, 2, 0)); // plan's read of s

    let script = write_script("keyset-remove-ap-two-keysets", &lines);
    let config_home = scratch_config_dir("keyset-remove-ap-two-keysets");
    let out = run_wh(
        &["keyset", "remove", "ap", "--keys", "w,a,s", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ap: removing w from keyset 1, 1.20mm to 2.00mm (no key outside a keyset to read a base from, using the default), keyset 1 ceases to exist"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("ap: removing s from keyset 2, 1.20mm to 2.00mm (no key outside a keyset to read a base from, using the default)"),
        "got: {stdout}"
    );
    assert!(
        !stdout.contains("keyset 2 ceases to exist"),
        "d stays in keyset 2, it must not be announced as destroyed: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// All three suffixes at once, pinning their order: the base parenthetical must sit right beside
/// the value it qualifies, not after the mode clause, where it would read as qualifying the mode
/// transition instead. `w` alone is ap keyset 1 (so removing it empties that keyset), `a`, `s` and
/// `d` are all in keyset 2 (so no free key exists anywhere and the base is the invented default),
/// and `w`'s own touch nibble is 0 (so it promotes too).
#[test]
fn keyset_remove_ap_orders_the_invented_suffix_beside_the_value_not_the_mode_clause() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 2), (0x16, 2), (0x07, 2)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    // No free-key read at all: every key on the board is in a keyset, none outside any of them.
    lines.extend(key_settings_lines(0x1A, 300, 0x08, 100, 150, 1, 0)); // plan's read of w

    let script = write_script("keyset-remove-ap-suffix-order", &lines);
    let config_home = scratch_config_dir("keyset-remove-ap-suffix-order");
    let out = run_wh(
        &["keyset", "remove", "ap", "--keys", "w", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ap: removing w from keyset 1, 0.30mm to 2.00mm (no key outside a keyset to read a base from, using the default), mode Global to Single, keyset 1 ceases to exist"),
        "got: {stdout}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `delete` and `remove` build the same per-key template from an already-resolved value, and they
/// resolve that value differently. Only the first of those is shared code (`reset_change`); the
/// second must stay apart, and this pins `delete`'s half of the divergence. On a board where every
/// key sits in a keyset there is no global to read, and `delete` refuses and names `--value`, its
/// escape hatch. It must not reach for `remove`'s `NO_SIGNAL_BASE`, which would write 2.00mm over
/// both members with nobody having asked for it.
///
/// The two `remove` halves are pinned by
/// `keyset_remove_ap_names_the_base_as_invented_when_every_key_is_already_in_a_keyset` and
/// `keyset_remove_rt_refuses_when_no_free_key_is_left_to_read_a_sensitivity_from`.
///
/// The work here is done by the status and the refusal sentence: a `delete` resolving through
/// `remove_base_ap` succeeds, and emits neither that sentence nor any other. The script carries
/// the two member reads such a `delete` would go on to make, which changes no detection, only the
/// failure message: without them the run dies on an exhausted script, and with them the failing
/// assertion prints the announcement and the frames the operator would have got. That padding is
/// latent rather than safe. Nothing on the CLI path asserts `ReplayTransport::finished()` today;
/// if `run_wh` ever gained that check, a natural strengthening here, this test would fail on its
/// passing path for a reason unrelated to what it guards, and the script is the reason.
#[test]
fn keyset_delete_ap_refuses_where_remove_would_invent_a_base() {
    let mut lines = matrix_lines(); // read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    // No free-key read at all: every key is in keyset 1 or 2, so `global_ap` finds nothing
    // outside a keyset to read and refuses before any further frame goes out. The two member
    // reads below are the ones a `delete` wrongly falling back would send next, present only so
    // that such a run reaches its announcement and fails with it on screen.
    lines.extend(key_settings_lines(0x1A, 1200, 0x18, 100, 150, 1, 0));
    lines.extend(key_settings_lines(0x04, 1200, 0x18, 100, 150, 1, 0));

    let script = write_script("keyset-delete-ap-no-global", &lines);
    let config_home = scratch_config_dir("keyset-delete-ap-no-global");
    let out = run_wh(
        &["keyset", "delete", "ap", "1", "--dry-run"],
        &script,
        &config_home,
    );
    assert!(
        !out.status.success(),
        "delete resolved a value it was never given: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "every key on the board is in a keyset, so there is no global actuation point to \
             read; pass --value to say which value to use"
        ),
        "got: {err}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}
