//! End-to-end tests of `wh keyset list` and `wh keyset create` over replay scripts.
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
    assert!(text.contains("keyset 1 loses w"), "got: {text}");
    assert!(
        text.contains("keyset 2"),
        "the new index must be named: {text}"
    );
    // `s` already sits at the target AP, so `plan`'s skip rule gives it no value records, only a
    // membership one. Dropping `s` from the plan entirely would still pass the two asserts above,
    // so pin its membership frame directly too.
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

/// The full script for `wh keyset create ap --keys w,s --value 2.00` against a four-key board
/// with no existing ap keysets: `resolve_keys`' matrix read, `read_membership`'s matrix and 0xFF
/// sweep, `plan`'s six-layout read for w then s, the auto-backup snapshot, the value batch and
/// the two membership frames, then the readback for both selected keys. `readback_s_ap` lets the
/// happy-path and mismatch tests below share this builder and diverge only on what `s` reads
/// back as after the write.
fn create_ap_write_script(readback_s_ap: u16) -> Vec<String> {
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
        readback_s_ap,
        0x18,
        100,
        150,
        1,
        0,
    )); // s readback

    lines
}

/// `wh keyset create ap --keys w,s --value 2.00` end to end: the auto-backup phase, the value
/// batch, the two membership frames, and a readback that matches for both keys. Exit 0,
/// "verified" in stdout, and a real backup file on disk, not just the message claiming one.
///
/// Byte-for-byte lever: the script's first frame after `create`'s own reads is the auto-backup's
/// SYNC read. If the write ran before the backup, the actual first frame sent there would be a
/// write frame instead, which would not match this script's next expected frame at all.
#[test]
fn keyset_create_ap_end_to_end_backs_up_writes_and_verifies() {
    let lines = create_ap_write_script(2000);
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
    assert!(stdout.contains("keyset: 2 keys verified"), "got: {stdout}");

    let backups = std::fs::read_dir(config_home.join("wh").join("backups"))
        .unwrap()
        .count();
    assert_eq!(backups, 1, "expected exactly one auto-backup file on disk");

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `s`'s actuation point write silently fails to land (the board still reports its pre-write
/// value) while its membership write does, exactly the shape of the reviewer's original repro:
/// membership alone can't tell this apart from a real success.
///
/// Output-assertion lever: the mismatch sits on `s`, the *second* selected key, so a verifier
/// that stopped after the first key would never read it back, still print "2 keys verified", and
/// exit 0 on a board that had not fully changed.
#[test]
fn keyset_create_ap_end_to_end_catches_a_value_that_never_landed() {
    let lines = create_ap_write_script(1500); // s still reports its pre-write AP
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

/// `wh keyset create rt --keys w --press 0.10 --release 0.30` end to end, with press and release
/// deliberately different values.
///
/// Byte-for-byte lever: if `--press`/`--release` resolution swapped the two (e.g. bound press to
/// `release.or(value)`), the write batch's RT_PRESS record would carry 300 instead of 100, which
/// would not match this script's scripted write frame at all.
#[test]
fn keyset_create_rt_end_to_end_writes_press_and_release_independently() {
    let mut lines = Vec::new();
    lines.extend(matrix_lines()); // resolve_keys
    lines.extend(matrix_lines()); // read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, 0));
    }
    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 999, 999, 0, 0)); // plan's read of w

    lines.extend(auto_backup_lines(
        0,
        (2000, 0x18, 999, 999, 0, 0), // w
        (1200, 0x00, 0, 0, 0, 0),     // a
        (1500, 0x18, 100, 150, 0, 0), // s
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
    let membership_records = vec![KeyRecord {
        key: 0x1A,
        layout: layout::KEYSET_RT,
        value: 1,
    }];
    for f in cmds::write_key_records_singly(&membership_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    lines.extend(key_settings_lines(0x1A, 2000, 0x38, 100, 300, 0, 1)); // w readback

    let script = write_script("keyset-create-rt-ok", &lines);
    let config_home = scratch_config_dir("keyset-create-rt-ok");

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
    assert!(stdout.contains("keyset: 1 key verified"), "got: {stdout}");

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
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
