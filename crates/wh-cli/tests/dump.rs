//! End-to-end tests of `wh dump` and `wh get` over replay scripts, exercising the full
//! `snapshot_from_device` and `resolve_keys` pipelines without a physical keyboard, via the
//! `WH_REPLAY` seam.

use std::process::Command;
use wh_device::replay::hex;
use wh_proto::cmds::{self, layout};

fn out_line(bytes: &[u8; 64]) -> String {
    format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(bytes))
}

fn in_line(bytes: &[u8; 64]) -> String {
    format!("{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(bytes))
}

fn reply(cmd: u8, payload: &[u8]) -> [u8; 64] {
    wh_proto::frame::frame(cmd, payload).unwrap()
}

/// A scratch directory unique to this test and this process, mirroring the `test_dir` helper
/// `run.rs`'s own unit tests use. Each test that spawns `wh` gets its own `XDG_CONFIG_HOME`
/// rather than sharing the bare system temp directory: a shared config directory is harmless
/// for `dump`, which writes nothing, but `wh keys group` here writes a real `config.toml`, and
/// a later task's backup/restore tests would rotate a shared `backups/` directory across
/// concurrent or repeated runs, deleting another test's fixtures out from under it.
fn scratch_config_dir(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("wh-cli-it-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// A DEFKEY reply payload for one row pair: `[rw, row_a, 21 usages, row_b, 21 usages]`, with
/// at most the first column of each row populated. `None` leaves a row empty (no keys), which
/// is what the second and third row pairs of this two-key board need.
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

/// The three DEFKEY roundtrips that make up `ops::read_matrix` for a two-key board ('w' at
/// usage 0x1A, 'a' at usage 0x04): only the first row pair carries keys, the other two are
/// empty. Shared by every test in this file that needs a matrix read, so `dump`'s full script
/// and `get`'s narrower ones can't silently drift apart on what "the board" looks like.
fn matrix_lines() -> Vec<String> {
    let mut lines = Vec::new();
    let row_pairs = [(0u8, 1u8), (2u8, 3u8), (4u8, 5u8)];
    for (i, &(a, b)) in row_pairs.iter().enumerate() {
        lines.push(out_line(&cmds::read_defkey_rows(a, b)));
        let payload = if i == 0 {
            defkey_payload(a, b, Some(0x1A), Some(0x04)) // row a col0 = 'w', row b col0 = 'a'
        } else {
            defkey_payload(a, b, None, None)
        };
        lines.push(in_line(&reply(cmds::cmd::DEFKEY, &payload)));
    }
    lines
}

/// One key's [AP, MODE, RT_PRESS, RT_RELEASE] roundtrips, in the exact order
/// `ops::read_key_settings` sends them.
fn key_settings_lines(
    usage: u8,
    ap: u16,
    mode: u16,
    rt_press: u16,
    rt_release: u16,
) -> Vec<String> {
    let mut lines = Vec::new();
    for (layout_id, value) in [
        (layout::AP, ap),
        (layout::MODE, mode),
        (layout::RT_PRESS, rt_press),
        (layout::RT_RELEASE, rt_release),
    ] {
        lines.push(out_line(&cmds::read_key_layout(usage, layout_id)));
        let payload = [
            0x00,
            usage,
            layout_id,
            (value & 0xFF) as u8,
            (value >> 8) as u8,
        ];
        lines.push(in_line(&reply(cmds::cmd::KEY, &payload)));
    }
    lines
}

/// Composes, in order, exactly the frames `snapshot_from_device` sends against the two-key
/// board: the SYNC request and info reply, the global travel DB read and reply, the matrix's
/// three DEFKEY roundtrips, then four KEY reads and replies per key. Built with
/// `wh_proto::cmds` encoders, not hand-written hex, so the test breaks if an encoder changes
/// rather than silently drifting from it.
fn build_script() -> Vec<String> {
    let mut lines = Vec::new();

    // SYNC: device_info
    lines.push(out_line(&cmds::sync()));
    let mut sync_payload = vec![0u8; 60];
    sync_payload[9..25].copy_from_slice(b"SNDUMPTEST000001");
    sync_payload[26..36].copy_from_slice(b"V1.0.0.001");
    lines.push(in_line(&reply(cmds::cmd::SYNC, &sync_payload)));

    // DB read: global_travel
    lines.push(out_line(&cmds::read_global_travel()));
    let db_payload = [0x00, 0, 0, 0xF4, 0x01, 0xC8, 0x00, 0xC8, 0x00]; // 500/200/200 um
    lines.push(in_line(&reply(cmds::cmd::DB, &db_payload)));

    lines.extend(matrix_lines());

    // Per-key reads, in matrix order: 'w' (0x1A) then 'a' (0x04). 'w's MODE is 0x0220 (a
    // non-zero high byte, 0x02, over the RT touch nibble and a zero advanced nibble) so the
    // fixture actually exercises `Mode`'s full 16-bit round trip rather than only its low
    // byte, which the wire format always carried and a truncating bug could hide behind.
    lines.extend(key_settings_lines(0x1A, 1200, 0x0220, 500, 500));
    lines.extend(key_settings_lines(0x04, 1500, 0x00, 0, 0));

    lines
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
        .output()
        .unwrap()
}

#[test]
fn dump_json_via_replay() {
    let path = write_script("dump", &build_script());
    let config_home = scratch_config_dir("dump-json");

    let out = run_wh(&["dump", "--json"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["serial"], "SNDUMPTEST000001");
    assert_eq!(v["firmware"], "V1.0.0.001");
    assert_eq!(v["global"]["travel_mm"], 0.5);
    assert_eq!(v["keys"][0]["name"], "w");
    assert_eq!(v["keys"][0]["rt"], true);
    // Pins the `Mode` high-byte fix at the CLI's own boundary, not just in wh-proto's unit
    // test: the fixture's MODE reply is 0x0220, and mode_raw must come back exactly that, not
    // truncated to 0x20.
    assert_eq!(v["keys"][0]["mode_raw"], 0x0220);
    assert_eq!(v["keys"][1]["name"], "a");
    assert_eq!(v["keys"][1]["rt"], false);

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `wh get rt --keys w`: pins that `resolve_keys` and `get` work end to end over a replay
/// script, not just `dump`. Without this, nothing in the committed suite ever exercises
/// `resolve_keys` for a *present* key, and Task 16's write commands build directly on it.
#[test]
fn get_rt_via_replay() {
    let mut lines = matrix_lines();
    // Press and release are deliberately distinct (0.40mm / 0.60mm, not the same value
    // twice): equal fixture values can't catch the two being swapped anywhere between the
    // wire reply and the printed line.
    lines.extend(key_settings_lines(0x1A, 1200, 0x20, 400, 600)); // 'w': rt on
    let path = write_script("get-rt", &lines);
    let config_home = scratch_config_dir("get-rt");

    let out = run_wh(&["get", "rt", "--keys", "w"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("w: rt on press 0.40mm release 0.60mm"),
        "unexpected stdout: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// A selector that resolves to a real, stored group, but one whose keys are all absent from
/// this board's matrix, must fail loudly rather than silently write nothing or, worse, write
/// to keys the board doesn't have. This pins the specific guard at the end of `resolve_keys`
/// (`if usages.is_empty() { bail!(...) }`): if a later change dropped that check or the
/// universe filter stopped applying, `wh set` on top of the same `resolve_keys` would burn a
/// flash SAVE cycle on a selector that should have refused to run at all.
#[test]
fn get_on_a_group_absent_from_the_board_is_rejected() {
    let config_home = scratch_config_dir("offboard-group");

    // Define the group against the CLI's own static key table (no device needed for this
    // half), the same way a user would with `wh keys group`.
    let empty_replay = write_script("offboard-group-setup", &[]);
    let group = run_wh(
        &["keys", "group", "offboard", "arrows"],
        &empty_replay,
        &config_home,
    );
    assert!(
        group.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&group.stdout),
        String::from_utf8_lossy(&group.stderr)
    );

    // The board itself only has 'w' and 'a': none of "arrows" (up/down/left/right) is present.
    let path = write_script("offboard-group-get", &matrix_lines());
    let out = run_wh(&["get", "rt", "--keys", "offboard"], &path, &config_home);
    assert!(
        !out.status.success(),
        "expected a non-zero exit, got success with stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("selector matches no keys on this board"),
        "unexpected stderr: {stderr}"
    );

    std::fs::remove_file(empty_replay).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}
