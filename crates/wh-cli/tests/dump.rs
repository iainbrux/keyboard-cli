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

/// The SYNC roundtrip `ops::device_info` sends, as `[out, in]` lines: `serial` and `firmware`
/// are padded into the reply payload at the offsets `cmds::parse_sync` reads them back from.
/// Factored out of `build_script` so the write-path tests below can compose the same fixture
/// shape (backup taken during `auto_backup` calls `snapshot_from_device`, which starts with
/// this exact roundtrip) without hand-copying the payload layout a second time.
fn sync_lines(serial: &str, firmware: &str) -> Vec<String> {
    let mut payload = vec![0u8; 60];
    let s = serial.as_bytes();
    payload[9..9 + s.len()].copy_from_slice(s);
    let f = firmware.as_bytes();
    payload[26..26 + f.len()].copy_from_slice(f);
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
/// own matrix read, the auto-backup phase, the AP write batch and SAVE, then the readback
/// verification's `read_key_settings` for 'w'. `readback_ap` is the AP value (micrometres) the
/// verification reads back, letting the happy-path and mismatch tests below share this builder
/// and diverge only on that one number.
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
    let save = cmds::cmd_order(cmds::order::SAVE, &[]).unwrap();
    lines.push(out_line(&save));
    lines.push(in_line(&reply(
        cmds::cmd::CMD,
        &[0x00, cmds::order::SAVE, 0x01],
    )));

    // Readback verification reads all four layouts for 'w', not just AP; MODE/press/release
    // echo back unchanged so only the AP field can drive a match or mismatch here.
    lines.extend(key_settings_lines(0x1A, readback_ap, 0x0220, 500, 500));
    lines
}

/// `set ap --keys w --set 1.2` end to end: the auto-backup phase, the write batch, SAVE, and a
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

/// `--dry-run` must send nothing at all, not even a read, so it has to work with no keyboard
/// attached at all: this runs it against a genuinely empty replay script and asserts success.
/// If `set ap --dry-run` tried to read the board's matrix (or anything else), `ReplayTransport`
/// would reject the unexpected send and the process would exit non-zero instead.
#[test]
fn set_ap_dry_run_sends_nothing_against_an_empty_script() {
    let empty_replay = write_script("set-ap-dry-run", &[]);
    let config_home = scratch_config_dir("set-ap-dry-run");

    let out = run_wh(
        &["set", "ap", "--keys", "w", "--set", "1.2", "--dry-run"],
        &empty_replay,
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

    std::fs::remove_file(empty_replay).unwrap();
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
            rt: true,
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
    let save = cmds::cmd_order(cmds::order::SAVE, &[]).unwrap();
    lines.push(out_line(&save));
    lines.push(in_line(&reply(
        cmds::cmd::CMD,
        &[0x00, cmds::order::SAVE, 0x01],
    )));

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
