//! End-to-end tests of `wh dump`, `wh get`, and the write path (`set`, `backup`, `restore`,
//! `selftest`) over replay scripts, exercising the full `snapshot_from_device`, `resolve_keys`,
//! and write pipelines without a physical keyboard, via the `WH_REPLAY` seam.

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

/// One key's MODE-only read roundtrip, the single read `ops::rt_records`/`ops::rt_off_records`
/// send per key (to preserve the advanced nibble), distinct from `key_settings_lines`' full
/// four-read `read_key_settings` sequence.
fn mode_read_lines(usage: u8, mode: u16) -> Vec<String> {
    vec![
        out_line(&cmds::read_key_layout(usage, layout::MODE)),
        in_line(&reply(
            cmds::cmd::KEY,
            &[
                0x00,
                usage,
                layout::MODE,
                (mode & 0xFF) as u8,
                (mode >> 8) as u8,
            ],
        )),
    ]
}

/// The SYNC roundtrip `ops::device_info` sends, as `[out, in]` lines: `serial` and `firmware`
/// are each written into the reply payload with the length prefix `cmds::parse_sync` reads them
/// back through (task 19b chunk 6: both strings are length-prefixed on the wire, not fixed-width).
/// Factored out of `build_script` so the write-path tests below can compose the same fixture
/// shape (backup taken during `auto_backup` calls `snapshot_from_device`, which starts with
/// this exact roundtrip) without hand-copying the payload layout a second time.
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

/// The DB read roundtrip `ops::global_travel` sends, as `[out, in]` lines, for the given
/// travel/press-dead/release-dead values in micrometres. Factored out for the same reason as
/// `sync_lines` above.
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

/// Composes, in order, exactly the frames `snapshot_from_device` sends against the two-key
/// board: the SYNC request and info reply, the global travel DB read and reply, the matrix's
/// three DEFKEY roundtrips, then four KEY reads and replies per key. Built with
/// `wh_proto::cmds` encoders, not hand-written hex, so the test breaks if an encoder changes
/// rather than silently drifting from it.
fn build_script() -> Vec<String> {
    let mut lines = Vec::new();

    lines.extend(sync_lines("SNDUMPTEST000001", "V1.0.0.001"));
    lines.extend(global_travel_lines(500, 200, 200));
    lines.extend(matrix_lines());

    // Per-key reads, in matrix order: 'w' (0x1A) then 'a' (0x04). 'w's MODE is 0x0230 (a
    // non-zero high byte, 0x02, over the Rt touch nibble 0x3 and a zero advanced nibble) so the
    // fixture actually exercises `Mode`'s full 16-bit round trip rather than only its low
    // byte, which the wire format always carried and a truncating bug could hide behind.
    lines.extend(key_settings_lines(0x1A, 1200, 0x0230, 500, 500));
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
    // test: the fixture's MODE reply is 0x0230, and mode_raw must come back exactly that, not
    // truncated to 0x30.
    assert_eq!(v["keys"][0]["mode_raw"], 0x0230);
    assert_eq!(v["keys"][1]["name"], "a");
    assert_eq!(v["keys"][1]["rt"], false);

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Pins that a K-001 board-function key (task 19b chunk 7: `0xFA`, `0xFB`, `0xD6`, `0xFC`,
/// confirmed by measurement) renders by its name in `dump` output, not as bare hex. A one-key
/// board with 'ap' (usage `0xFA`) at row 0 col 0; before chunk 7 this printed as `"0xFA"`.
#[test]
fn dump_prints_a_board_function_key_by_name_not_hex() {
    let mut lines = Vec::new();
    lines.extend(sync_lines("SNBOARDFUNC000001", "V1.0.0.001"));
    lines.extend(global_travel_lines(500, 200, 200));
    let row_pairs = [(0u8, 1u8), (2u8, 3u8), (4u8, 5u8)];
    for (i, &(a, b)) in row_pairs.iter().enumerate() {
        lines.push(out_line(&cmds::read_defkey_rows(a, b)));
        let payload = if i == 0 {
            defkey_payload(a, b, Some(0xFA), None) // row a col0 = the 'ap' board-function key
        } else {
            defkey_payload(a, b, None, None)
        };
        lines.push(in_line(&reply(cmds::cmd::DEFKEY, &payload)));
    }
    lines.extend(key_settings_lines(0xFA, 0, 0x10, 0, 0));

    let path = write_script("dump-board-func", &lines);
    let config_home = scratch_config_dir("dump-board-func");

    let out = run_wh(&["dump", "--json"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["keys"][0]["name"], "ap");

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
    lines.extend(key_settings_lines(0x1A, 1200, 0x30, 400, 600)); // 'w': rt on
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
/// universe filter stopped applying, `wh set` on top of the same `resolve_keys` would send a
/// write to the board on a selector that should have refused to run at all.
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

// --- write path: `set`, `backup`, `restore`, `selftest` -------------------------------------

/// Exactly the frames `auto_backup` sends against the two-key board (`matrix_lines`): the full
/// `snapshot_from_device` pipeline, sync, global travel, matrix, then one four-read
/// `read_key_settings` per key on the board. The AP/press/release values here (1000um for 'w',
/// the untouched defaults for 'a') are deliberately distinct from anything a write-path test
/// writes or reads back afterwards, so a script that accidentally reused this phase's frames
/// for the post-write readback could not pass by coincidence.
fn auto_backup_lines() -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(sync_lines("SNWRITETEST00001", "V1.0.0.001"));
    lines.extend(global_travel_lines(500, 200, 200));
    lines.extend(matrix_lines());
    lines.extend(key_settings_lines(0x1A, 1000, 0x0220, 500, 500)); // 'w' pre-write
    lines.extend(key_settings_lines(0x04, 1500, 0x00, 0, 0)); // 'a' pre-write
    lines
}

/// The full script for `wh set ap --keys w --set 1.2` against the two-key board: `resolve_keys`'
/// own matrix read, the auto-backup phase, the AP write batch (no SAVE follows it, see task 19b
/// chunk 4), then the readback verification's `read_key_settings` for 'w'. `readback_ap` is the
/// AP value (micrometres) the verification reads back, letting the happy-path and mismatch tests
/// below share this builder and diverge only on that one number.
fn set_ap_script(readback_ap: u16) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(matrix_lines()); // resolve_keys, ahead of auto_backup's own matrix read
    lines.extend(auto_backup_lines());

    let recs = vec![KeyRecord {
        key: 0x1A,
        layout: layout::AP,
        value: 1200,
    }];
    let batch = cmds::write_key_records(&recs);
    for f in &batch {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    // No SAVE order follows the write batch: the vendor was never observed sending one (task
    // 19b chunk 4), so `write_records` does not either.

    // Readback verification reads all four layouts for 'w', not just AP; MODE/press/release
    // echo back unchanged so only the AP field can drive a match or mismatch here.
    lines.extend(key_settings_lines(0x1A, readback_ap, 0x0220, 500, 500));
    lines
}

/// `set ap --keys w --set 1.2` end to end: the auto-backup phase, the write batch, and a
/// readback that matches (1200um = 1.20mm). Exit 0, "verified" in stdout, and a real backup
/// file on disk, not just the message claiming one.
#[test]
fn set_ap_end_to_end_backs_up_writes_and_verifies() {
    let path = write_script("set-ap-ok", &set_ap_script(1200));
    let config_home = scratch_config_dir("set-ap-ok");

    let out = run_wh(
        &["set", "ap", "--keys", "w", "--set", "1.2"],
        &path,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("verified"), "unexpected stdout: {stdout}");

    let backups = std::fs::read_dir(config_home.join("wh").join("backups"))
        .unwrap()
        .count();
    assert_eq!(backups, 1, "expected exactly one auto-backup file on disk");

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The mismatch twin of the test above: the board reads back 1100um (1.10mm) where 1200um
/// (1.20mm) was written. Non-zero exit, "mismatch" in stderr.
#[test]
fn set_ap_end_to_end_reports_mismatch_on_readback() {
    let path = write_script("set-ap-mismatch", &set_ap_script(1100));
    let config_home = scratch_config_dir("set-ap-mismatch");

    let out = run_wh(
        &["set", "ap", "--keys", "w", "--set", "1.2"],
        &path,
        &config_home,
    );
    assert!(
        !out.status.success(),
        "expected a non-zero exit, got success with stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("mismatch"), "unexpected stderr: {stderr}");

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `verify_rt` has to compare the exact MODE value `ops::rt_records` computed (touch nibble,
/// advanced nibble, and high byte), not just "is the touch mode Rt" plus the two sensitivities.
/// Here the board's write drops the advanced nibble: before the write, 'w' carries MODE 0x01
/// (touch Global, advanced nibble 1); `rt_records` reads that and builds a wanted MODE of 0x31
/// (touch Rt, nibble 3, advanced nibble 1 preserved). The scripted readback instead reports 0x30
/// (touch Rt, advanced nibble lost), with the press/release values otherwise exactly right. A
/// verification that only checked `rt_enabled()` plus press/release would call this a pass; it
/// has to be a mismatch, because the user's advanced-key configuration on 'w' was just
/// silently dropped.
#[test]
fn set_rt_end_to_end_detects_a_corrupted_advanced_nibble_on_readback() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(auto_backup_lines());

    // ops::rt_records' own pre-write MODE read: 0x01 (touch Global, advanced nibble 1).
    lines.extend(mode_read_lines(0x1A, 0x01));

    // The write batch: MODE 0x31 (touch Rt, advanced nibble 1 preserved), press/release 400um
    // (0.40mm). No SAVE order follows (task 19b chunk 4).
    let recs = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x31,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 400,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 400,
        },
    ];
    let batch = cmds::write_key_records(&recs);
    for f in &batch {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    // No SAVE order follows the write batch: the vendor was never observed sending one (task
    // 19b chunk 4), so `write_records` does not either.

    // verify_rt's readback: MODE comes back 0x30, not the 0x31 that was written, with
    // press/release otherwise matching exactly.
    lines.extend(key_settings_lines(0x1A, 1000, 0x30, 400, 400));

    let path = write_script("set-rt-nibble-mismatch", &lines);
    let config_home = scratch_config_dir("set-rt-nibble-mismatch");

    let out = run_wh(
        &["set", "rt", "--keys", "w", "--set", "0.4"],
        &path,
        &config_home,
    );
    assert!(
        !out.status.success(),
        "expected a non-zero exit, got success with stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("mismatch"), "unexpected stderr: {stderr}");

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The `verify_rt_off` sibling of the corrupted-advanced-nibble test above: `ops::rt_off_records`
/// does the same read-modify-write as `ops::rt_records`, just forcing the touch nibble to
/// Single (per-key actuation point, nibble 1) instead of Rt, so it can lose the advanced nibble
/// the same way. Before the write, 'w' carries MODE 0x31 (touch Rt, advanced nibble 1);
/// `rt_off_records` reads that and builds a wanted MODE of 0x11 (touch Single, advanced nibble 1
/// preserved: see chunk 3 of task 19b for why the real device writes nibble 1, not 0, here). The
/// scripted readback instead reports 0x10 (advanced nibble lost). A verification that only
/// checked `!rt_enabled()` would call this a pass, since the touch nibble genuinely did flip
/// away from Rt; it has to be a mismatch, because 'w's advanced-key configuration was just
/// silently dropped.
#[test]
fn set_rt_off_end_to_end_detects_a_corrupted_advanced_nibble_on_readback() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(auto_backup_lines());

    // ops::rt_off_records' own pre-write MODE read: 0x31 (touch Rt, advanced nibble 1).
    lines.extend(mode_read_lines(0x1A, 0x31));

    // The write batch: MODE 0x11 (touch Single, advanced nibble 1 preserved). No SAVE order
    // follows: the vendor was never observed sending one (task 19b chunk 4).
    let recs = vec![KeyRecord {
        key: 0x1A,
        layout: layout::MODE,
        value: 0x11,
    }];
    let batch = cmds::write_key_records(&recs);
    for f in &batch {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    // verify_rt_off's readback: MODE comes back 0x10, not the 0x11 that was written; press and
    // release are unrelated to this check and left at whatever the board otherwise reports.
    lines.extend(key_settings_lines(0x1A, 1000, 0x10, 400, 400));

    let path = write_script("set-rt-off-nibble-mismatch", &lines);
    let config_home = scratch_config_dir("set-rt-off-nibble-mismatch");

    let out = run_wh(&["set", "rt", "--keys", "w", "--off"], &path, &config_home);
    assert!(
        !out.status.success(),
        "expected a non-zero exit, got success with stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("mismatch"), "unexpected stderr: {stderr}");

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `--dry-run` means no writes and no SAVE, not "no I/O": `resolve_keys` still reads the live
/// matrix (a preview has to be of an operation that could actually happen against this board,
/// not against every key the protocol has ever heard of), and `ap_records` itself needs no
/// further device state. The script here is exactly that one read and nothing else; if the
/// implementation sent a write or a SAVE afterwards, `ReplayTransport` would reject the
/// unexpected send against the now-exhausted script and this would fail instead of passing.
#[test]
fn set_ap_dry_run_reads_the_matrix_but_sends_no_write_or_save() {
    let path = write_script("set-ap-dry-run", &matrix_lines());
    let config_home = scratch_config_dir("set-ap-dry-run");

    let out = run_wh(
        &["set", "ap", "--keys", "w", "--set", "1.2", "--dry-run"],
        &path,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("dry run"), "unexpected stdout: {stdout}");
    // Negative assertion (review round 1, chunk 4): the old message,
    // "dry run, no writes sent; save-to-flash frame {hex} would follow", also satisfied
    // `contains("dry run")`, so that check alone could not catch a regression that reinstated
    // the removed SAVE frame in the dry-run output. Pin its absence directly.
    let save = cmds::cmd_order(cmds::order::SAVE, &[]).unwrap();
    assert!(
        !stdout.contains(&hex(&save)),
        "dry-run output must not contain the SAVE frame: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The `rt` sibling of the test above, and the one variant that genuinely opens a session
/// during `--dry-run`: `ops::rt_records` reads each selected key's current MODE (one read, to
/// preserve the advanced nibble in the preview) on top of `resolve_keys`' matrix read, and
/// nothing else. The script is exactly those reads; a regression that added `ops::set_rt` or a
/// bare SAVE to this branch would try to send afterwards, and `ReplayTransport` would reject it
/// against the exhausted script.
#[test]
fn set_rt_dry_run_reads_matrix_and_mode_but_sends_no_write_or_save() {
    // 'w' (0x1A) starts at MODE 0x0220 (touch Unknown(2), advanced nibble 0, high byte 0x02,
    // i.e. not already RT) and wants 0x0230 after `rt_records` forces the touch nibble to Rt
    // (nibble 3, continuous off) while preserving the advanced nibble and high byte.
    let mut lines = matrix_lines();
    lines.extend(mode_read_lines(0x1A, 0x0220));
    let path = write_script("set-rt-dry-run", &lines);
    let config_home = scratch_config_dir("set-rt-dry-run");

    let out = run_wh(
        &["set", "rt", "--keys", "w", "--set", "0.4", "--dry-run"],
        &path,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("dry run"), "unexpected stdout: {stdout}");
    // Negative assertion (review round 1, chunk 4): see the `set ap` sibling above for why
    // "contains(\"dry run\")" alone cannot catch a reinstated SAVE frame.
    let save = cmds::cmd_order(cmds::order::SAVE, &[]).unwrap();
    assert!(
        !stdout.contains(&hex(&save)),
        "dry-run output must not contain the SAVE frame: {stdout}"
    );

    // Pins the exact previewed records, not just that something printed, bringing this test up
    // to the same standard as its `--off` sibling below: a regression in `rt_records`' touch
    // nibble choice or its high-byte/advanced-nibble preservation would otherwise only be
    // caught on the `--off` path.
    let expected = cmds::write_key_records(&[
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x0230,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 400,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 400,
        },
    ]);
    for frame in &expected {
        assert!(
            stdout.contains(&hex(frame)),
            "expected frame {} in stdout: {stdout}",
            hex(frame)
        );
    }

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The `--off` sibling of the dry-run test above, the last dry-run branch that had nothing
/// pinning it: `ops::rt_off_records` reads each selected key's current MODE (one read per key,
/// to preserve the advanced nibble) on top of `resolve_keys`' matrix read, and nothing else.
/// Both board keys are selected (`--keys all`) so this also pins the exact preview content for
/// two keys at once: 'w' (0x1A) starts at MODE 0x0231 (touch Rt, advanced nibble 1, high byte
/// 0x02) and wants 0x0211 after the touch nibble flips to Single (per-key actuation point, not
/// Global: see chunk 3 of task 19b); 'a' (0x04) starts at MODE 0x0037 (touch Rt, advanced
/// nibble 7, high byte 0) and wants 0x0017. The script is exactly those two matrix-plus-MODE
/// reads; a regression that added `ops::set_rt_off` or a bare SAVE to this branch would try to
/// send afterwards, and `ReplayTransport` would reject it against the exhausted script.
#[test]
fn set_rt_off_dry_run_reads_matrix_and_mode_but_sends_no_write_or_save() {
    let mut lines = matrix_lines();
    lines.extend(mode_read_lines(0x1A, 0x0231));
    lines.extend(mode_read_lines(0x04, 0x0037));
    let path = write_script("set-rt-off-dry-run", &lines);
    let config_home = scratch_config_dir("set-rt-off-dry-run");

    let out = run_wh(
        &["set", "rt", "--keys", "all", "--off", "--dry-run"],
        &path,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("dry run"), "unexpected stdout: {stdout}");
    // Negative assertion (review round 1, chunk 4): see the `set ap` sibling above for why
    // "contains(\"dry run\")" alone cannot catch a reinstated SAVE frame.
    let save = cmds::cmd_order(cmds::order::SAVE, &[]).unwrap();
    assert!(
        !stdout.contains(&hex(&save)),
        "dry-run output must not contain the SAVE frame: {stdout}"
    );

    // Pins the exact previewed records, not just that something printed: the touch nibble
    // must flip to Single (nibble 1, per-key actuation point) on both keys while each key's own
    // advanced nibble and high byte survive independently, the same read-modify-write
    // `verify_rt_off` checks on the real write path.
    let expected = cmds::write_key_records(&[
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x0211,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::MODE,
            value: 0x0017,
        },
    ]);
    for frame in &expected {
        assert!(
            stdout.contains(&hex(frame)),
            "expected frame {} in stdout: {stdout}",
            hex(frame)
        );
    }

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Pins that `--dry-run` previews an operation that could actually happen: 'z' (usage 0x1D) is
/// a real key in `wh_proto::keys::TABLE` but is not on this two-key fixture board (only 'w' and
/// 'a' are). A dry run that resolved selectors against the full static table instead of the
/// live matrix would happily preview writing to 'z' anyway; resolving against the live board
/// (like every other command) has to reject it with the same `NotOnDevice` error `get`/`set`
/// already give for a live write. The script is exactly the matrix read: if `--dry-run` skipped
/// the live resolution, it would never touch this script at all and still exit non-zero for an
/// unrelated reason (`--pick` aside, a live resolution is the only source of `NotOnDevice`), so
/// the finished-matrix-read plus non-zero-exit combination is what actually distinguishes this
/// from the bug.
#[test]
fn set_ap_dry_run_rejects_a_key_absent_from_the_board() {
    let path = write_script("set-ap-dry-run-absent", &matrix_lines());
    let config_home = scratch_config_dir("set-ap-dry-run-absent");

    let out = run_wh(
        &["set", "ap", "--keys", "z", "--set", "1.2", "--dry-run"],
        &path,
        &config_home,
    );
    assert!(
        !out.status.success(),
        "expected a non-zero exit, got success with stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("'z'") && stderr.contains("not a key on this device"),
        "unexpected stderr: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// A snapshot's TOML text for one key, 'w', with a caller-chosen `ap_mm`, so both the
/// out-of-range and happy-path restore tests below can share it and diverge only on that one
/// value.
fn restore_snapshot_toml(ap_mm: f64) -> String {
    let snap = wh_config::snapshot::Snapshot {
        firmware: "V1.0.0.001".into(),
        serial: "SNRESTORETEST001".into(),
        taken_at: "2026-08-28T12:00:00Z".into(),
        global: wh_config::snapshot::GlobalToml {
            travel_mm: 2.0,
            press_dead_mm: 0.2,
            release_dead_mm: 0.1,
        },
        keys: vec![wh_config::snapshot::KeyToml {
            name: "w".into(),
            usage: 0x1A,
            ap_mm,
            // Agrees with mode_raw below: 0x0220 decodes to TouchMode::Unknown(2), not Rt, so
            // `rt` is false. `restore`'s write/verify path never reads this field (it round-trips
            // mode_raw verbatim, see RestoreKey/restore_records/verify_restore in run.rs), so this
            // is purely informational, but it should still describe the snapshot it sits in.
            rt: false,
            rt_press_mm: 0.5,
            rt_release_mm: 0.6,
            mode_raw: 0x0220,
        }],
    };
    snap.to_toml().unwrap()
}

fn write_snapshot(tag: &str, ap_mm: f64) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("wh-{tag}-{}.toml", std::process::id()));
    std::fs::write(&path, restore_snapshot_toml(ap_mm)).unwrap();
    path
}

/// A snapshot whose `ap_mm` is out of range (the device's actuation point tops out at 4.00mm,
/// this one says 99.0mm) must be refused before a single frame is sent, not after. Run against
/// a genuinely empty replay script: if `restore` sent anything at all before finishing
/// validation, `ReplayTransport` would reject the unexpected send.
#[test]
fn restore_refuses_an_out_of_range_value_before_any_frame_is_sent() {
    let config_home = scratch_config_dir("restore-out-of-range");
    let snap_path = write_snapshot("restore-oor", 99.0);
    let empty_replay = write_script("restore-oor", &[]);

    let out = run_wh(
        &["restore", snap_path.to_str().unwrap()],
        &empty_replay,
        &config_home,
    );
    assert!(
        !out.status.success(),
        "expected a non-zero exit, got success with stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Names the offending key ('w') specifically, not just any error, and reports the range
    // violation, so this can't pass on an unrelated failure (e.g. a bad path or a TOML parse
    // error) that happens to also be non-zero exit.
    assert!(
        stderr.contains("key 'w'") && stderr.to_lowercase().contains("out of range"),
        "unexpected stderr: {stderr}"
    );

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(empty_replay).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `restore` from a valid snapshot: the auto-backup happens before anything is overwritten
/// (pinned by a real backup file existing on disk afterwards, not just the printed message),
/// the global travel and per-key writes land, and the readback verifies. Exit 0, "verified" in
/// stdout.
#[test]
fn restore_happy_path_backs_up_and_verifies() {
    let config_home = scratch_config_dir("restore-happy");
    let snap_path = write_snapshot("restore-happy", 1.2);

    let mut lines = Vec::new();
    lines.extend(auto_backup_lines());

    // restore_all: global travel write first (2.0/0.2/0.1mm = 2000/200/100um).
    let db_write = cmds::write_global_travel(
        wh_proto::value::Um::from_mm(2.0, 0.0, 4.0).unwrap(),
        wh_proto::value::Um::from_mm(0.2, 0.0, 4.0).unwrap(),
        wh_proto::value::Um::from_mm(0.1, 0.0, 4.0).unwrap(),
    );
    lines.push(out_line(&db_write));
    lines.push(in_line(&reply(cmds::cmd::DB, &[0x01, 0, 0])));

    // Then the per-key batch for 'w': ap, mode (verbatim), rt press, rt release.
    let recs = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 1200,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x0220,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 500,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 600,
        },
    ];
    let batch = cmds::write_key_records(&recs);
    for f in &batch {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    // No SAVE order follows the write batch: the vendor was never observed sending one (task
    // 19b chunk 4), so `write_records` does not either.

    // verify_restore reads 'w' back and finds every field matching what was restored.
    lines.extend(key_settings_lines(0x1A, 1200, 0x0220, 500, 600));

    let path = write_script("restore-happy", &lines);
    let out = run_wh(
        &["restore", snap_path.to_str().unwrap()],
        &path,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("verified"), "unexpected stdout: {stdout}");
    // Pins the singular grammar for a one-key snapshot ("1 key", not "1 keys").
    assert!(
        stdout.contains("restored 1 key from snapshot"),
        "unexpected stdout: {stdout}"
    );

    let backups = std::fs::read_dir(config_home.join("wh").join("backups"))
        .unwrap()
        .count();
    assert_eq!(
        backups, 1,
        "expected restore's auto-backup to have written exactly one file"
    );

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `selftest` must never send SAVE: it rewrites the global travel with its own current value
/// and reads it back, and the script below never includes a SAVE roundtrip. If the
/// implementation sent one anyway, `ReplayTransport` would reject the unexpected send and this
/// would fail instead of passing.
#[test]
fn selftest_sends_no_save_frame() {
    let mut lines = Vec::new();
    lines.extend(sync_lines("SNSELFTEST0000001", "V1.0.0.001"));
    lines.extend(global_travel_lines(500, 200, 200));
    let rewrite = cmds::write_global_travel(
        wh_proto::value::Um(500),
        wh_proto::value::Um(200),
        wh_proto::value::Um(200),
    );
    lines.push(out_line(&rewrite));
    lines.push(in_line(&reply(cmds::cmd::DB, &[0x01, 0, 0])));
    lines.extend(global_travel_lines(500, 200, 200));

    let path = write_script("selftest", &lines);
    let config_home = scratch_config_dir("selftest");
    let out = run_wh(&["selftest"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("selftest OK"),
        "unexpected stdout: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}
