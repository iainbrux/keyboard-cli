//! End-to-end tests of `wh dump`, `wh get`, and the write path (`set`, `backup`, `restore`,
//! `selftest`) over replay scripts, exercising the full `snapshot_from_device`, `resolve_keys`,
//! and write pipelines without a physical keyboard, via the `WH_REPLAY` seam.
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

/// A scratch directory unique to this test and process, used as its own `XDG_CONFIG_HOME`.
/// Sharing one config directory across tests would be harmless for `dump`, which writes
/// nothing, but `wh keys group` writes a real `config.json` and `backup`/`restore` rotate a
/// shared `backups/` directory, so concurrent or repeated runs would delete each other's fixtures.
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

/// One key's [AP, MODE, RT_PRESS, RT_RELEASE, KEYSET_AP, KEYSET_RT] roundtrips, in the exact
/// order `ops::read_key_settings` sends them.
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
    for (layout_id, value) in [
        (layout::AP, ap),
        (layout::MODE, mode),
        (layout::RT_PRESS, rt_press),
        (layout::RT_RELEASE, rt_release),
        (layout::KEYSET_AP, ap_keyset),
        (layout::KEYSET_RT, rt_keyset),
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
/// six-read `read_key_settings` sequence.
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

/// One `read_layout_value` roundtrip: a single-record read request for `usage`/`layout_id`, and
/// the reply carrying `value`. Distinct from `mode_read_lines`, which is MODE-only; this covers
/// any layout, needed for `keyset::read_membership`'s own `KEYSET_AP` sweep.
fn layout_read_lines(usage: u8, layout_id: u8, value: u16) -> Vec<String> {
    vec![
        out_line(&cmds::read_key_layout(usage, layout_id)),
        in_line(&reply(
            cmds::cmd::KEY,
            &[
                0x00,
                usage,
                layout_id,
                (value & 0xFF) as u8,
                (value >> 8) as u8,
            ],
        )),
    ]
}

/// The three DEFKEY roundtrips that make up `ops::read_matrix` for a four-key board: 'w' (0x1A)
/// and 'a' (0x04) in the first row pair, 's' (0x16) and 'd' (0x07) in the second. Only the
/// keyset-split `set ap` test below needs more than the two-key board every other test here uses.
fn matrix_lines_wasd() -> Vec<String> {
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

/// The profile-read roundtrip `ops::profile` sends, as `[out, in]` lines: `idx` is the
/// zero-based index the board replies with (the wire's own numbering; `snapshot_from_device`
/// converts it to the UI's one-based numbering before storing it in `Snapshot::profile`).
fn profile_lines(idx: u8) -> Vec<String> {
    vec![
        out_line(&cmds::read_profile()),
        in_line(&reply(cmds::cmd::CMD, &[0x00, 0x70, idx, 0xFF])),
    ]
}

/// The SYNC roundtrip `ops::device_info` sends, as `[out, in]` lines: `serial` and `firmware` are
/// each written with the length prefix `cmds::parse_sync` reads back (both strings are
/// length-prefixed on the wire, not fixed-width). Factored out so write-path tests below can
/// compose the same fixture shape without hand-copying the payload layout.
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
/// board: sync, profile, global travel, matrix, then six KEY reads per key. Built with
/// `wh_proto::cmds` encoders, not hand-written hex, so the test breaks if an encoder changes.
fn build_script() -> Vec<String> {
    let mut lines = Vec::new();

    lines.extend(sync_lines("SNDUMPTEST000001", "V1.0.0.001"));
    lines.extend(profile_lines(0)); // board reports profile index 0, i.e. UI "profile 1"
    lines.extend(global_travel_lines(500, 200, 200));
    lines.extend(matrix_lines());

    // Per-key reads, in matrix order: 'w' (0x1A) then 'a' (0x04). 'w's MODE is 0x0230 (a
    // non-zero high byte, 0x02, over the Rt touch nibble 0x3 and a zero advanced nibble) so the
    // fixture actually exercises `Mode`'s full 16-bit round trip rather than only its low
    // byte, which the wire format always carried and a truncating bug could hide behind.
    // 'w' carries a non-zero AP keyset (1) so `dump_json_via_replay` can assert the raw value is
    // read through, distinct from 'a', which carries none (0).
    lines.extend(key_settings_lines(0x1A, 1200, 0x0230, 500, 500, 1, 0));
    lines.extend(key_settings_lines(0x04, 1500, 0x00, 0, 0, 0, 0));

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
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap()
}

/// `run_wh` with a line on stdin, for the commands that ask for a typed confirmation. Stdin is
/// dropped before waiting on the child: a piped stdin the child never reads (e.g. `--dry-run`,
/// which never prompts) would otherwise leave the write end open and deadlock the wait.
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

#[test]
fn dump_json_via_replay() {
    let path = write_script("dump", &build_script());
    let config_home = scratch_config_dir("dump-json");

    let out = run_wh(&["dump"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["serial"], "SNDUMPTEST000001");
    assert_eq!(v["firmware"], "V1.0.0.001");
    // The board replied with the wire's zero-based index 0; the JSON field carries the same
    // one-based value ("profile 1") the human-readable dump text below shows.
    assert_eq!(v["profile"], 1);
    assert_eq!(v["global"]["travel_mm"], 0.5);
    assert_eq!(v["keys"][0]["name"], "w");
    assert_eq!(v["keys"][0]["rt"], true);
    // The fixture's MODE reply is 0x0230; mode_raw must come back exactly that, not truncated
    // to 0x30.
    assert_eq!(v["keys"][0]["mode_raw"], 0x0230);
    // 'w' carries AP keyset 1, 'a' carries none (0): distinct per key, not a constant, and
    // carried raw rather than coerced to a boolean.
    assert_eq!(v["keys"][0]["ap_keyset"], 1);
    assert_eq!(v["keys"][0]["rt_keyset"], 0);
    assert_eq!(v["keys"][1]["name"], "a");
    assert_eq!(v["keys"][1]["rt"], false);
    assert_eq!(v["keys"][1]["ap_keyset"], 0);
    assert_eq!(v["keys"][1]["rt_keyset"], 0);

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `wh dump` must report rapid trigger as on for a key following the board's global rapid
/// trigger (nibble 2), not only for a key with its own rapid trigger settings (nibble 3): a
/// board with the global switch on would otherwise dump every such key as `rt: false`.
#[test]
fn dump_reports_rt_true_for_a_key_following_the_global_rapid_trigger() {
    let mut lines = Vec::new();
    lines.extend(sync_lines("SNDUMPTEST000002", "V1.0.0.001"));
    lines.extend(profile_lines(0));
    lines.extend(global_travel_lines(500, 200, 200));
    lines.extend(matrix_lines());
    lines.extend(key_settings_lines(0x1A, 1200, 0x0220, 200, 200, 0, 0));
    lines.extend(key_settings_lines(0x04, 1500, 0x00, 0, 0, 0, 0));

    let path = write_script("dump-global-rt", &lines);
    let config_home = scratch_config_dir("dump-global-rt");

    let out = run_wh(&["dump"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["keys"][0]["name"], "w");
    assert_eq!(v["keys"][0]["mode_raw"], 0x0220);
    assert_eq!(
        v["keys"][0]["rt"], true,
        "nibble 2 (RtGlobal) must dump as rt: true, not false"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `wh dump` with no flags is JSON. This is the format change: JSON is canonical, and the
/// human table is opt-in.
#[test]
fn dump_with_no_flags_is_json() {
    let path = write_script("dump-default-json", &build_script());
    let config_home = scratch_config_dir("dump-default-json");

    let out = run_wh(&["dump"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.trim_start().starts_with('{'),
        "dump must default to JSON, got: {stdout}"
    );
    serde_json::from_str::<serde_json::Value>(&stdout).expect("dump output must parse as JSON");

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The table survives behind `--table`, since nothing else renders 68 keys readably until the
/// TUI exists.
#[test]
fn dump_table_flag_prints_the_human_table() {
    let path = write_script("dump-table", &build_script());
    let config_home = scratch_config_dir("dump-table");

    let out = run_wh(&["dump", "--table"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("key"), "table header missing: {stdout}");
    assert!(
        !stdout.trim_start().starts_with('{'),
        "--table must not be JSON"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The table's two new columns: `apks` and `rtks`, printing the raw keyset value ('w' has AP
/// keyset 1) or `-` for the value read outside any keyset (both of 'a's, and 'w's RT keyset).
#[test]
fn dump_table_prints_the_keyset_columns() {
    let path = write_script("dump-table-keyset", &build_script());
    let config_home = scratch_config_dir("dump-table-keyset");

    let out = run_wh(&["dump", "--table"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("apks") && stdout.contains("rtks"),
        "table header must carry the keyset columns: {stdout}"
    );
    let w_line = stdout
        .lines()
        .find(|l| l.trim_start().starts_with("w "))
        .unwrap_or_else(|| panic!("no 'w' row in table: {stdout}"));
    assert!(
        w_line.contains(" 1 "),
        "'w's ap keyset (1) must appear in its row: {w_line}"
    );
    let a_line = stdout
        .lines()
        .find(|l| l.trim_start().starts_with("a "))
        .unwrap_or_else(|| panic!("no 'a' row in table: {stdout}"));
    assert!(
        a_line.contains("-"),
        "'a's keysets (both 0) must print as '-': {a_line}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `with_session` announces which transport it opened, on stderr, one line: a run believing it
/// is a replay must never silently be a hardware write, and the reverse must never be silent
/// either. This only exercises the replay half, since the host-built test binary never takes the
/// hardware branch; see `bin_wh_shim_propagates_wh_replay_and_never_touches_hardware` below for
/// the end-to-end proof through the actual shim and Windows binary.
#[test]
fn dump_via_replay_announces_the_replay_transport_on_stderr() {
    let path = write_script("dump-transport-announce", &build_script());
    let config_home = scratch_config_dir("dump-transport-announce");

    let out = run_wh(&["dump"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("transport: replay"),
        "unexpected stderr, missing the transport announcement: {stderr}"
    );
    // Kept off stdout: `dump`'s default JSON output must stay valid, parseable on its own.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_ok(),
        "the transport announcement must not have leaked into stdout: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The human-readable sibling of `dump_json_via_replay`'s `v["profile"]` assertion above: both
/// read the exact same fixture, so a fix that mixed the JSON field's and the printed text's
/// numbering conventions would show up as a mismatch between the two tests.
#[test]
fn dump_text_prints_the_one_based_profile_number() {
    let path = write_script("dump-profile-text", &build_script());
    let config_home = scratch_config_dir("dump-profile-text");

    let out = run_wh(&["dump", "--table"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("profile 1"), "unexpected stdout: {stdout}");

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `wh backup --to <file>` records the board's current profile in the file it writes. `backup
/// --to` is the path an operator actually uses to keep a snapshot, so it gets its own assertion,
/// read back off the real file `backup` wrote, not off stdout.
#[test]
fn backup_to_writes_the_profile_into_the_file() {
    let path = write_script("backup-profile", &build_script());
    let config_home = scratch_config_dir("backup-profile");
    let out_path =
        std::env::temp_dir().join(format!("wh-backup-profile-{}.json", std::process::id()));

    let out = run_wh(
        &["backup", "--to", out_path.to_str().unwrap()],
        &path,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let text = std::fs::read_to_string(&out_path).unwrap();
    let snap = wh_config::snapshot::Snapshot::from_json(&text).unwrap();
    // `build_script()` scripts the board replying with wire index 0, i.e. UI profile 1.
    assert_eq!(
        snap.profile,
        Some(cmds::ProfileNumber::from_wire_index(0).unwrap()),
        "backup --to must record the board's profile in the file: {text}"
    );

    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(out_path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Pins that a K-001 board-function key (`0xFA`, `0xFB`, `0xD6`, `0xFC`, confirmed by
/// measurement) renders by its name in `dump` output, not as bare hex. A one-key board with 'ap'
/// (usage `0xFA`) at row 0 col 0.
#[test]
fn dump_prints_a_board_function_key_by_name_not_hex() {
    let mut lines = Vec::new();
    lines.extend(sync_lines("SNBOARDFUNC000001", "V1.0.0.001"));
    lines.extend(profile_lines(0));
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
    lines.extend(key_settings_lines(0xFA, 0, 0x10, 0, 0, 0, 0));

    let path = write_script("dump-board-func", &lines);
    let config_home = scratch_config_dir("dump-board-func");

    let out = run_wh(&["dump"], &path, &config_home);
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
/// script, not just `dump`. Nothing else in the suite exercises `resolve_keys` for a present key,
/// and the write commands build directly on it.
#[test]
fn get_rt_via_replay() {
    let mut lines = matrix_lines();
    // Press and release are deliberately distinct (0.40mm / 0.60mm, not the same value
    // twice): equal fixture values can't catch the two being swapped anywhere between the
    // wire reply and the printed line. RT keyset 2, non-zero, so the printed suffix exercises
    // the "keyset N" branch rather than "keyset none".
    lines.extend(key_settings_lines(0x1A, 1200, 0x30, 400, 600, 0, 2)); // 'w': rt on
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
        stdout.contains("w: rt on press 0.40mm release 0.60mm keyset 2"),
        "unexpected stdout: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The `wh get ap` sibling: 'w's AP keyset is 0 (none) here, so the printed suffix exercises the
/// "keyset none" branch, the other half of `get_rt_via_replay`'s "keyset N" coverage above.
#[test]
fn get_ap_prints_keyset_none_when_the_key_has_no_ap_keyset() {
    let mut lines = matrix_lines();
    lines.extend(key_settings_lines(0x1A, 1200, 0x30, 400, 600, 0, 0));
    let path = write_script("get-ap-keyset-none", &lines);
    let config_home = scratch_config_dir("get-ap-keyset-none");

    let out = run_wh(&["get", "ap", "--keys", "w"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("w: ap 1.20mm keyset none"),
        "unexpected stdout: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// A selector resolving to a real, stored group whose keys are all absent from this board's
/// matrix must fail loudly, not silently write nothing or write to keys the board doesn't have.
/// Pins the `if usages.is_empty() { bail!(...) }` guard at the end of `resolve_keys`, which `wh
/// set` relies on too.
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

/// `wh keys list` must render a group member that has no `TABLE` entry as hex, not silently drop
/// it: this listing is the operator's only recovery route once `SelectError::AmbiguousWithGroup`
/// refuses to resolve a stale group. The unnamed usage is written into a legacy `config.toml`
/// fixture directly, which the store still reads, rather than through `wh keys group`.
#[test]
fn keys_list_renders_an_unnamed_group_member_as_hex_not_dropping_it() {
    let config_home = scratch_config_dir("keys-list-unnamed");
    let wh_dir = config_home.join("wh");
    std::fs::create_dir_all(&wh_dir).unwrap();
    let unnamed = (0u8..=u8::MAX)
        .find(|&u| wh_proto::keys::name_for_usage(u).is_none())
        .expect("wh_proto::keys::TABLE does not occupy every u8 usage code");
    std::fs::write(
        wh_dir.join("config.toml"),
        format!("[groups]\nstale = [26, {unnamed}]\n"), // 26 = 0x1A = 'w'
    )
    .unwrap();
    let empty_replay = write_script("keys-list-unnamed", &[]);

    let out = run_wh(&["keys", "list"], &empty_replay, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let expected_hex = format!("0x{unnamed:02X}");
    assert!(
        stdout.contains(&expected_hex),
        "unnamed usage {expected_hex} must still be listed, got: {stdout}"
    );
    assert!(
        stdout.contains(&format!("w,{expected_hex}")),
        "named and unnamed members should both appear, in order: {stdout}"
    );

    std::fs::remove_file(empty_replay).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

// --- write path: `set`, `backup`, `restore`, `selftest` -------------------------------------

/// Exactly the frames `auto_backup` sends: sync, profile, global travel, matrix, then one
/// six-read `read_key_settings` per key. Its AP/press/release values are deliberately distinct
/// from anything a write-path test writes or reads back, so reused frames couldn't pass by
/// coincidence. `profile_idx` lets `restore`'s profile-safety tests script a board profile that
/// matches or differs from the snapshot being restored.
fn auto_backup_lines(profile_idx: u8) -> Vec<String> {
    auto_backup_lines_with_modes(profile_idx, 0x0220, 0x00)
}

/// Like `auto_backup_lines`, but lets a caller pick 'w' and 'a''s pre-write MODE values, for a
/// scenario that needs a specific touch nibble already on the board (e.g. rapid trigger on 'w').
fn auto_backup_lines_with_modes(profile_idx: u8, mode_w: u16, mode_a: u16) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(sync_lines("SNWRITETEST00001", "V1.0.0.001"));
    lines.extend(profile_lines(profile_idx));
    lines.extend(global_travel_lines(500, 200, 200));
    lines.extend(matrix_lines());
    lines.extend(key_settings_lines(0x1A, 1000, mode_w, 500, 500, 0, 0)); // 'w' pre-write
    lines.extend(key_settings_lines(0x04, 1500, mode_a, 0, 0, 0, 0)); // 'a' pre-write
    lines
}

/// The full script for `wh set ap --keys w --set 1.2` against the two-key board, routed through
/// `keyset::plan`: `resolve_keys`' matrix read, `keyset::read_membership`'s own matrix read and
/// `0xFF` sweep over both keys (neither is a member), `plan`'s own six-layout read of 'w', the
/// auto-backup phase, the value write batch, the membership write allocating keyset 1 for 'w'
/// since it was free, then the readback verification. `readback_ap` lets the happy-path and
/// mismatch tests below share this builder and diverge only on that one number.
///
/// 'w' reads back MODE 0x0220 (touch `RtGlobal`), which the actuation point promotion never
/// touches. `plan` still sends MODE in the write batch, echoing 0x0220 back: it only drops MODE
/// when the touch nibble would stay literally `Global` (0) unchanged, and `RtGlobal` (2) is not
/// that nibble. Only the AP field can drive a match or mismatch on readback here.
fn set_ap_script(readback_ap: u16) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(matrix_lines()); // resolve_keys
    lines.extend(matrix_lines()); // keyset::read_membership's own matrix read
    lines.extend(layout_read_lines(0x1A, layout::KEYSET_AP, 0)); // w, free
    lines.extend(layout_read_lines(0x04, layout::KEYSET_AP, 0)); // a, free
    lines.extend(key_settings_lines(0x1A, 1000, 0x0220, 500, 500, 0, 0)); // plan's read of w
    lines.extend(auto_backup_lines(0));

    let recs = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x0220,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 1200,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 500,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 500,
        },
    ];
    let batch = cmds::write_key_records(&recs);
    for f in &batch {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    // No SAVE order follows the write batch: the vendor was never observed sending one.

    // 'w' was free, so the plan allocates keyset 1 and writes it.
    let membership = cmds::write_key_records_singly(&[KeyRecord {
        key: 0x1A,
        layout: layout::KEYSET_AP,
        value: 1,
    }]);
    for f in &membership {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    // verify_write's readback: all six layouts for 'w', ap_keyset now 1.
    lines.extend(key_settings_lines(
        0x1A,
        readback_ap,
        0x0220,
        500,
        500,
        1,
        0,
    ));
    lines
}

/// `set ap --keys w --set 1.2` end to end: 'w' was free, so giving it its own value creates keyset 1 for
/// it, then the auto-backup phase, the write batch, and a readback that matches (1200um =
/// 1.20mm). Exit 0, "verified" in stdout, and a real backup file on disk, not just the message
/// claiming one.
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
    // Exact label, not a bare "verified": the label must name the keyset the create allocated,
    // and the announcement must name 'w' as the free key it enrolled.
    assert!(
        stdout.contains("ap keyset 1: creating at 1.20mm"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("enrolling free key(s) w at 1.00mm"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("ap keyset 1 at 1.20mm: 1 key verified"),
        "unexpected stdout: {stdout}"
    );

    let backups = std::fs::read_dir(config_home.join("wh").join("backups"))
        .unwrap()
        .count();
    assert_eq!(backups, 1, "expected exactly one auto-backup file on disk");

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The mismatch twin of the test above: the board reads back 1100um (1.10mm) where 1200um
/// (1.20mm) was written. Non-zero exit, and the per-key fault line naming both values, not just
/// the word "mismatch": `ReplayTransport`'s own violation wording also contains "mismatch" (a
/// script that never got extended to cover an added membership frame reads as this same word), so
/// a bare `contains("mismatch")` cannot tell a real readback mismatch from a broken fixture.
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
    assert!(
        stderr.contains("w: ") && stderr.contains("1.10mm") && stderr.contains("1.20mm"),
        "the failure must name the key and both ap values, got: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The end-to-end promotion path: `wh set ap --keys a` against 'a' (0x04), whose MODE reads back
/// `Global` (0x00, advanced nibble 0). `plan`'s own six-layout read repeats that same value, so
/// the write batch gains a MODE record (nibble promoted to `Single`, advanced nibble 0 preserved,
/// 0x10) alongside AP, press and release, covering the promotion path end to end and not only in
/// `keyset::plan`'s own unit tests.
fn set_ap_promotes_script(readback_ap: u16) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(matrix_lines()); // resolve_keys
    lines.extend(matrix_lines()); // keyset::read_membership's own matrix read
    lines.extend(layout_read_lines(0x1A, layout::KEYSET_AP, 0)); // w, free
    lines.extend(layout_read_lines(0x04, layout::KEYSET_AP, 0)); // a, free
    lines.extend(key_settings_lines(0x04, 1500, 0x00, 0, 0, 0, 0)); // plan's read of a: Global
    lines.extend(auto_backup_lines(0));

    let recs = vec![
        KeyRecord {
            key: 0x04,
            layout: layout::MODE,
            value: 0x10,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::AP,
            value: 1200,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_PRESS,
            value: 0,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_RELEASE,
            value: 0,
        },
    ];
    let batch = cmds::write_key_records(&recs);
    for f in &batch {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    // No SAVE order follows the write batch: the vendor was never observed sending one.

    // 'a' was free, so the plan allocates keyset 1 and writes it.
    let membership = cmds::write_key_records_singly(&[KeyRecord {
        key: 0x04,
        layout: layout::KEYSET_AP,
        value: 1,
    }]);
    for f in &membership {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    // verify_write's readback: all six layouts for 'a'; MODE now comes back 0x10, reflecting the
    // promotion just written, and ap_keyset now 1.
    lines.extend(key_settings_lines(0x04, readback_ap, 0x10, 0, 0, 1, 0));
    lines
}

/// `set ap --keys a --set 1.2` against a `Global` key: the write batch gains a MODE record
/// (nibble promoted to `Single`), 'a' was free so giving it its own value also allocates keyset 1 for
/// it, and the run still succeeds and verifies.
#[test]
fn set_ap_end_to_end_promotes_a_global_key_to_single() {
    let path = write_script("set-ap-promote", &set_ap_promotes_script(1200));
    let config_home = scratch_config_dir("set-ap-promote");

    let out = run_wh(
        &["set", "ap", "--keys", "a", "--set", "1.2"],
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
    assert!(
        stdout.contains("ap keyset 1 at 1.20mm: 1 key verified"),
        "unexpected stdout: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The end-to-end sibling of `keyset::plan`'s own rapid-trigger-preserving tests, driven through
/// `run.rs` to the wire: `wh set ap --keys w` against 'w' (0x1A), whose MODE reads back `Rt`
/// (0x38). `plan` echoes that same value back in the write batch rather than omitting it: it only
/// drops MODE when the touch nibble would stay literally `Global` (0) unchanged, and `Rt` (3) is
/// not that nibble. If `plan` ever forced the touch nibble to `Single` here, the frame it
/// actually sent would carry a different MODE value and `ReplayTransport` would reject it as a
/// send mismatch, proving rapid trigger survives a depth change through the real command path.
fn set_ap_preserves_rapid_trigger_script(readback_ap: u16, readback_mode: u16) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(matrix_lines()); // resolve_keys
    lines.extend(matrix_lines()); // keyset::read_membership's own matrix read
    lines.extend(layout_read_lines(0x1A, layout::KEYSET_AP, 0)); // w, free
    lines.extend(layout_read_lines(0x04, layout::KEYSET_AP, 0)); // a, free
    lines.extend(key_settings_lines(0x1A, 1000, 0x38, 500, 500, 0, 0)); // plan's read of w: Rt
    lines.extend(auto_backup_lines_with_modes(0, 0x38, 0x00));

    let recs = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x38,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 1200,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: 500,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 500,
        },
    ];
    let batch = cmds::write_key_records(&recs);
    for f in &batch {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    // No SAVE order follows the write batch: the vendor was never observed sending one.

    // 'w' was free, so the plan allocates keyset 1 and writes it.
    let membership = cmds::write_key_records_singly(&[KeyRecord {
        key: 0x1A,
        layout: layout::KEYSET_AP,
        value: 1,
    }]);
    for f in &membership {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    // verify_write's readback: all six layouts for 'w'. `readback_mode` is compared against the
    // 0x38 MODE the write batch actually sent, not a fallback, since MODE was sent this time.
    // ap_keyset now 1.
    lines.extend(key_settings_lines(
        0x1A,
        readback_ap,
        readback_mode,
        500,
        500,
        1,
        0,
    ));
    lines
}

/// `set ap --keys w --set 1.2` against a key with rapid trigger on: the write batch's MODE record
/// echoes 0x38 back unchanged, so rapid trigger survives because the same value was resent, not
/// because MODE was omitted. 'w' was free, so giving it its own value also allocates keyset 1 for it. The
/// run still succeeds and verifies AP and MODE both.
#[test]
fn set_ap_end_to_end_preserves_rapid_trigger() {
    let path = write_script(
        "set-ap-preserve-rt",
        &set_ap_preserves_rapid_trigger_script(1200, 0x38),
    );
    let config_home = scratch_config_dir("set-ap-preserve-rt");

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
    assert!(
        stdout.contains("ap keyset 1 at 1.20mm: 1 key verified"),
        "unexpected stdout: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The twin that matters most on this branch: same script, but the board reports MODE 0x18
/// (`Single`) on readback where the write batch sent 0x38 (`Rt`). The firmware ignored or
/// clobbered the very MODE record `plan` sent to preserve rapid trigger, and `wh` must say so
/// instead of printing "verified".
#[test]
fn set_ap_end_to_end_fails_when_the_board_clears_rapid_trigger_by_itself() {
    let path = write_script(
        "set-ap-rt-cleared",
        &set_ap_preserves_rapid_trigger_script(1200, 0x18),
    );
    let config_home = scratch_config_dir("set-ap-rt-cleared");

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
    assert!(
        stderr.contains("w: ") && stderr.contains("0x0018") && stderr.contains("0x0038"),
        "the failure must name the key and both mode values, got: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `verify_rt` has to compare the exact MODE value `ops::rt_records` computed, not just
/// "is the touch mode Rt" plus the two sensitivities. The scripted readback drops the advanced
/// nibble (0x30 instead of the written 0x31) with press/release otherwise exactly right; a
/// verification that only checked `rt_enabled()` plus press/release would wrongly pass this.
#[test]
fn set_rt_end_to_end_detects_a_corrupted_advanced_nibble_on_readback() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(auto_backup_lines(0));

    // ops::rt_records' own pre-write MODE read: 0x01 (touch Global, advanced nibble 1).
    lines.extend(mode_read_lines(0x1A, 0x01));

    // The write batch: MODE 0x31 (touch Rt, advanced nibble 1 preserved), press/release 400um
    // (0.40mm). No SAVE order follows.
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
    // No SAVE order follows the write batch: the vendor was never observed sending one.

    // verify_rt's readback: MODE comes back 0x30, not the 0x31 that was written, with
    // press/release otherwise matching exactly.
    lines.extend(key_settings_lines(0x1A, 1000, 0x30, 400, 400, 0, 0));

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
/// does the same read-modify-write, forcing the touch nibble to Single instead of Rt, so it can
/// lose the advanced nibble the same way. The scripted readback drops it (0x10 instead of the
/// written 0x11); a verification that only checked `!rt_enabled()` would wrongly pass this.
#[test]
fn set_rt_off_end_to_end_detects_a_corrupted_advanced_nibble_on_readback() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(auto_backup_lines(0));

    // ops::rt_off_records' own pre-write MODE read: 0x31 (touch Rt, advanced nibble 1).
    lines.extend(mode_read_lines(0x1A, 0x31));

    // The write batch: MODE 0x11 (touch Single, advanced nibble 1 preserved from the 0x31 read).
    // No SAVE order follows: the vendor was never observed sending one.
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
    lines.extend(key_settings_lines(0x1A, 1000, 0x10, 400, 400, 0, 0));

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
/// matrix, and `keyset::read_membership` and `keyset::plan` still read the board, since a preview
/// has to be of an operation that could actually happen against this board. 'w' reads back MODE
/// 0x18 (`Single`, not `Global`), so `plan` still echoes it back in the write batch: dropping MODE
/// only happens when the touch nibble would stay literally `Global` (0) unchanged. 'w' is free, so
/// giving a free key its own value also previews a membership record allocating keyset 1. The script is exactly
/// those reads; a stray write or SAVE afterwards would hit the exhausted script and
/// `ReplayTransport` would reject it.
#[test]
fn set_ap_dry_run_reads_the_matrix_but_sends_no_write_or_save() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // keyset::read_membership's own matrix read
    lines.extend(layout_read_lines(0x1A, layout::KEYSET_AP, 0)); // w, free
    lines.extend(layout_read_lines(0x04, layout::KEYSET_AP, 0)); // a, free
    lines.extend(key_settings_lines(0x1A, 1000, 0x18, 500, 500, 0, 0)); // plan's read of w
    let path = write_script("set-ap-dry-run", &lines);
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

    // The exact frame set, not just that some frame appears: an added, removed, or reordered
    // frame, including a reinstated SAVE frame, would not otherwise be caught.
    let mut expected: Vec<String> = cmds::write_key_records(&[
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
            value: 500,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: 500,
        },
    ])
    .iter()
    .map(|f| hex(f))
    .collect();
    expected.extend(
        cmds::write_key_records_singly(&[KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_AP,
            value: 1,
        }])
        .iter()
        .map(|f| hex(f)),
    );
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "dry run must print exactly the frames a real run would send, and no others: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The ruling: a selection where every key is free must still allocate a keyset, so giving a
/// free key its own actuation point always enrolls it. The one piece of counter-evidence,
/// recorded rather than softened: `ap-wasd-1.2` measures the absence of a `0xFF` write over an
/// actuation point change on four keys with no keyset traffic anywhere in the file; whether those
/// keys were actually free is itself unmeasured (`docs/keysets.md`), but if they were, that
/// capture contradicts the rule this test pins, and the operator ruled anyway. Asserted by exact
/// frame equality against a hand-built `cmds::write_key_records`/`write_key_records_singly`, not
/// a substring check, so the create is proved by the whole frame sequence rather than the
/// presence of one string.
#[test]
fn set_ap_dry_run_over_free_keys_creates_a_keyset() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // keyset::read_membership's own matrix read
    lines.extend(layout_read_lines(0x1A, layout::KEYSET_AP, 0)); // w, free
    lines.extend(layout_read_lines(0x04, layout::KEYSET_AP, 0)); // a, free
    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 0, 0)); // plan's read of w
    lines.extend(key_settings_lines(0x04, 2000, 0x18, 100, 150, 0, 0)); // plan's read of a

    let path = write_script("set-ap-free-keys", &lines);
    let config_home = scratch_config_dir("set-ap-free-keys");
    let out = run_wh(
        &["set", "ap", "--keys", "w,a", "--set", "1.20", "--dry-run"],
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

    assert!(
        stdout.contains("ap keyset 1: creating at 1.20mm"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("enrolling free key(s) w at 2.00mm,a at 2.00mm"),
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
    let membership_records = [
        KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_AP,
            value: 1,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::KEYSET_AP,
            value: 1,
        },
    ];
    let mut expected: Vec<String> = cmds::write_key_records(&value_records)
        .iter()
        .map(|f| hex(f))
        .collect();
    expected.extend(
        cmds::write_key_records_singly(&membership_records)
            .iter()
            .map(|f| hex(f)),
    );
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "both free keys must be enrolled into the newly allocated keyset: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The mirror case to the free-key create above: the selection is exactly one keyset's members, so
/// it keeps its index and the write still carries no `0xFF` record. `ks-value-ap` measures a value
/// change over three keys writing no `0xFF` record; whether the selection was exactly one keyset's
/// members is not itself measured. `docs/keysets.md` gives two readings of that capture: a
/// selection that is exactly one keyset's members, this test's scenario, or a mixed selection of
/// free and already-member keys, the scenario the split test below covers and which now writes a
/// `0xFF` record under the ruling. Same board and value records as the free-key test above, but a
/// different pre-write `ap_keyset` (1 for both, not 0): that test's board state allocates a keyset
/// and writes two membership records, this one's, already a whole keyset, writes none, so together
/// the pair prove membership state before the write, not the depth being written, is what decides
/// whether a `0xFF` record appears.
#[test]
fn set_ap_dry_run_over_a_whole_keyset_keeps_its_index() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // keyset::read_membership's own matrix read
    lines.extend(layout_read_lines(0x1A, layout::KEYSET_AP, 1)); // w, keyset 1
    lines.extend(layout_read_lines(0x04, layout::KEYSET_AP, 1)); // a, keyset 1
    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 1, 0)); // plan's read of w
    lines.extend(key_settings_lines(0x04, 2000, 0x18, 100, 150, 1, 0)); // plan's read of a

    let path = write_script("set-ap-whole-keyset", &lines);
    let config_home = scratch_config_dir("set-ap-whole-keyset");
    let out = run_wh(
        &["set", "ap", "--keys", "w,a", "--set", "1.20", "--dry-run"],
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

    let value_records = [
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
    let expected: Vec<String> = cmds::write_key_records(&value_records)
        .iter()
        .map(|f| hex(f))
        .collect();
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "selecting a whole keyset must keep its index, not rewrite the 0xFF record: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The third case of the `wh set ap` membership rule: the selection is a strict subset of keyset 1
/// (`w,s` out of `w,a,s,d`), so `wh set ap` allocates a new index, writes it to every selected
/// key, and announces the split first. Unlike the vendor's own create flow, no capture in the
/// corpus shows the vendor splitting a keyset this way; what was observed is its UI copying a
/// mixed selection into a new one, so this case is inferred, not measured (`docs/keysets.md`).
///
/// 'w' and 's' are given different prior actuation points (2.00mm and 1.80mm) on purpose: a
/// fixture where both members hold the same value cannot tell a correct announcement from one
/// that prints the first member's value for every key, which is exactly the defect found in an
/// earlier version of `describe_member` (then named `describe_loss`).
#[test]
fn set_ap_over_part_of_a_keyset_splits_it_and_announces_the_split() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 1), (0x07, 1)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 1, 0)); // plan's read of w
    lines.extend(key_settings_lines(0x16, 1800, 0x18, 100, 150, 1, 0)); // plan's read of s

    let path = write_script("set-ap-split", &lines);
    let config_home = scratch_config_dir("set-ap-split");
    let out = run_wh(
        &["set", "ap", "--keys", "w,s", "--set", "1.50", "--dry-run"],
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

    assert!(
        stdout.contains("ap keyset 2: creating at 1.50mm"),
        "got: {stdout}"
    );
    // Both stolen keys named on the same line, each with its own distinct prior value: a
    // mutation that reused 'w's value for 's' too would fail this exact match.
    assert!(
        stdout.contains("keyset 1 loses w at 2.00mm,s at 1.80mm"),
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
            value: 1500,
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
            value: 150,
        },
    ];
    let membership_records = [
        KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_AP,
            value: 2,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::KEYSET_AP,
            value: 2,
        },
    ];
    let mut expected: Vec<String> = cmds::write_key_records(&value_records)
        .iter()
        .map(|f| hex(f))
        .collect();
    expected.extend(
        cmds::write_key_records_singly(&membership_records)
            .iter()
            .map(|f| hex(f)),
    );
    // The new index (2) is pinned by equality, not merely asserted present, so a plan that wrote
    // the wrong index, or wrote membership for the wrong keys, fails this comparison.
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "the split must move exactly w and s to the new index, nothing more: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The escalation the review raised and the operator decided: `wh set ap --keys all` on a board
/// where one keyset exists sweeps every key, including every free one, into a single new keyset,
/// destroying the old one's separate identity. The behaviour is kept, since it is the vendor's
/// own and the operator chose "split automatically but inform the user" when this was designed.
/// What was incomplete was the information: the announcement must name the free keys being
/// enrolled, not only the keyset losing members, or the operator only sees half the change.
///
/// Each of the four keys is given a distinct prior actuation point, for the same reason the split
/// test above does: a fixture where they share a value cannot tell a correct announcement from
/// one that reuses the first key's value for every line.
#[test]
fn set_ap_dry_run_over_all_keys_enrolls_free_keys_and_says_so() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 0), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 1, 0)); // plan's read of w
    lines.extend(key_settings_lines(0x04, 1900, 0x18, 100, 150, 1, 0)); // plan's read of a
    lines.extend(key_settings_lines(0x16, 1800, 0x18, 100, 150, 0, 0)); // plan's read of s
    lines.extend(key_settings_lines(0x07, 1700, 0x18, 100, 150, 0, 0)); // plan's read of d

    let path = write_script("set-ap-split-all", &lines);
    let config_home = scratch_config_dir("set-ap-split-all");
    let out = run_wh(
        &["set", "ap", "--keys", "all", "--set", "1.50", "--dry-run"],
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

    assert!(
        stdout.contains("ap keyset 2: creating at 1.50mm"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("keyset 1 loses w at 2.00mm,a at 1.90mm"),
        "got: {stdout}"
    );
    // The line the escalation was about: s and d were never in any keyset, and the operator must
    // still be told they are being moved, each with its own distinct prior value.
    assert!(
        stdout.contains("enrolling free key(s) s at 1.80mm,d at 1.70mm"),
        "got: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Task 2.19's own gap: a selection spanning **two** existing keysets, `w,a` wholly consuming
/// keyset 1 and `s` wholly consuming keyset 2, with `d` a free key riding along. `ap_membership_for`
/// only special-cases `Keep` when exactly one keyset loses members and the selection is exactly
/// that keyset, so two distinct losing indices always fall through to `Split`; nothing before this
/// test drove `wh set ap` over more than one non-zero membership index at once. A rewrite that
/// generalises the single-keyset case to "every losing keyset is wholly consumed, so keep the
/// lowest index" reuses index 1 for the merge instead of allocating a fresh one, and folds `d`
/// into it uninvited; this pins the allocated index (3, one past the board's live maximum) and
/// both losing lines against exactly that rewrite.
///
/// A second, narrower rewrite survives the free key alone: "every losing keyset wholly consumed
/// *and* the selection covers exactly their union" still returns `Keep`, since dropping `d` makes
/// `total taken (3) == usages.len() (3)` true again. That rewrite would leave the board's two
/// keysets untouched and print no announcement at all on `--keys w,a,s`, so a second `run_wh` call
/// over exactly that selection, with the same board and no free key riding along, is what closes
/// it: the merge must still allocate a fresh index even when nothing free is enrolled.
#[test]
fn set_ap_over_a_selection_spanning_two_keysets_merges_them_into_a_new_index() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 1, 0)); // plan's read of w
    lines.extend(key_settings_lines(0x04, 1900, 0x18, 100, 150, 1, 0)); // plan's read of a
    lines.extend(key_settings_lines(0x16, 1800, 0x18, 100, 150, 2, 0)); // plan's read of s
    lines.extend(key_settings_lines(0x07, 1700, 0x18, 100, 150, 0, 0)); // plan's read of d

    let path = write_script("set-ap-split-two-indices", &lines);
    let config_home = scratch_config_dir("set-ap-split-two-indices");
    let out = run_wh(
        &[
            "set",
            "ap",
            "--keys",
            "w,a,s,d",
            "--set",
            "1.50",
            "--dry-run",
        ],
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

    // The allocated index is the board's live maximum (2) plus one, never a reuse of either
    // losing index.
    assert!(
        stdout.contains("ap keyset 3: creating at 1.50mm"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("keyset 1 loses w at 2.00mm,a at 1.90mm"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("keyset 2 loses s at 1.80mm"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("enrolling free key(s) d at 1.70mm"),
        "got: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);

    // Same board, `d` left out of the selection entirely: `w,a,s` is exactly the union of the two
    // losing keysets, so `read_membership`'s sweep and w/a/s's plan reads are unchanged, but `plan`
    // is never built over `d`, and no free-key line should print.
    let mut lines2 = matrix_lines_wasd(); // resolve_keys
    lines2.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 0)] {
        lines2.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines2.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 1, 0)); // plan's read of w
    lines2.extend(key_settings_lines(0x04, 1900, 0x18, 100, 150, 1, 0)); // plan's read of a
    lines2.extend(key_settings_lines(0x16, 1800, 0x18, 100, 150, 2, 0)); // plan's read of s

    let path2 = write_script("set-ap-split-two-indices-no-free", &lines2);
    let config_home2 = scratch_config_dir("set-ap-split-two-indices-no-free");
    let out2 = run_wh(
        &["set", "ap", "--keys", "w,a,s", "--set", "1.50", "--dry-run"],
        &path2,
        &config_home2,
    );
    assert!(
        out2.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out2.stdout),
        String::from_utf8_lossy(&out2.stderr)
    );
    let stdout2 = String::from_utf8_lossy(&out2.stdout);
    assert!(
        stdout2.contains("ap keyset 3: creating at 1.50mm"),
        "got: {stdout2}"
    );
    assert!(
        stdout2.contains("keyset 1 loses w at 2.00mm,a at 1.90mm"),
        "got: {stdout2}"
    );
    assert!(
        stdout2.contains("keyset 2 loses s at 1.80mm"),
        "got: {stdout2}"
    );
    assert!(
        !stdout2.contains("enrolling"),
        "no free key rides along this selection: {stdout2}"
    );

    std::fs::remove_file(path2).unwrap();
    let _ = std::fs::remove_dir_all(&config_home2);
}

/// One key's `(ap, mode, rt_press, rt_release, ap_keyset, rt_keyset)`, the shape
/// `auto_backup_lines_wasd` takes one of per board key.
type WasdKeyState = (u16, u16, u16, u16, u16, u16);

/// Like `auto_backup_lines`, but against the four-key w/a/s/d board `matrix_lines_wasd` reports,
/// for the real (non-dry-run) split test below: sync, profile, global travel, the matrix, then
/// one six-read `read_key_settings` per key in matrix order (w, a, s, d).
fn auto_backup_lines_wasd(
    profile_idx: u8,
    w: WasdKeyState,
    a: WasdKeyState,
    s: WasdKeyState,
    d: WasdKeyState,
) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(sync_lines("SNWRITETEST00002", "V1.0.0.001"));
    lines.extend(profile_lines(profile_idx));
    lines.extend(global_travel_lines(500, 200, 200));
    lines.extend(matrix_lines_wasd());
    for (usage, (ap, mode, press, release, apks, rtks)) in
        [(0x1Au8, w), (0x04, a), (0x16, s), (0x07, d)]
    {
        lines.extend(key_settings_lines(
            usage, ap, mode, press, release, apks, rtks,
        ));
    }
    lines
}

/// The non-dry-run sibling of `set_ap_over_part_of_a_keyset_splits_it_and_announces_the_split`:
/// the same board and selection, but driving the real write, auto-backup, and readback
/// verification, not just the preview. `wh restore` now writes membership too, so a split no
/// longer needs a warning that it is unrecoverable; this pins the negative, that the old warning
/// does not print any more, rather than leaving that regression uncovered.
#[test]
fn set_ap_end_to_end_splits_a_keyset_and_prints_no_stale_restore_warning() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 1), (0x07, 1)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 1, 0)); // plan's read of w
    lines.extend(key_settings_lines(0x16, 1800, 0x18, 100, 150, 1, 0)); // plan's read of s
    lines.extend(auto_backup_lines_wasd(
        0,
        (2000, 0x18, 100, 150, 1, 0), // w
        (2000, 0x18, 100, 150, 1, 0), // a, untouched
        (1800, 0x18, 100, 150, 1, 0), // s
        (2000, 0x18, 100, 150, 1, 0), // d, untouched
    ));

    let value_records = [
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 1500,
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
            value: 150,
        },
    ];
    for f in &cmds::write_key_records(&value_records) {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    let membership_records = [
        KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_AP,
            value: 2,
        },
        KeyRecord {
            key: 0x16,
            layout: layout::KEYSET_AP,
            value: 2,
        },
    ];
    for f in &cmds::write_key_records_singly(&membership_records) {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    // verify_write_as's readback: both keys landed at the new depth and the new index.
    lines.extend(key_settings_lines(0x1A, 1500, 0x18, 100, 150, 2, 0));
    lines.extend(key_settings_lines(0x16, 1500, 0x18, 100, 150, 2, 0));

    let path = write_script("set-ap-split-real", &lines);
    let config_home = scratch_config_dir("set-ap-split-real");
    let out = run_wh(
        &["set", "ap", "--keys", "w,s", "--set", "1.5"],
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
    assert!(
        stdout.contains("ap keyset 2 at 1.50mm: 2 keys verified"),
        "unexpected stdout: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    // The old warning claimed `wh restore` could not cover a split. `wh restore` writes
    // membership now, so the claim is false and the warning must be gone, not merely stale.
    assert!(
        !stderr.contains("wh restore does not yet write keyset membership"),
        "unexpected stderr: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `wh set ap --keys all` moves every key into one new keyset, so every existing keyset loses
/// all its members and ceases to exist. Nothing warned before this task; `wh keyset remove
/// --keys all` already carries the same typed confirmation for the same underlying hazard
/// reached a different route, and this pins the sibling guard on `set ap`.
///
/// Board: two keysets partition the whole four-key board, `w,a` in keyset 1 and `s,d` in keyset
/// 2, so both cease to exist and every member moves into a freshly allocated index (3, one past
/// the higher of the two).
#[test]
fn set_ap_over_the_whole_board_requires_a_typed_yes() {
    let mut decline_lines = matrix_lines_wasd(); // resolve_keys
    decline_lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        decline_lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    let decline_script = write_script("set-ap-whole-board-no", &decline_lines);
    let decline_config_home = scratch_config_dir("set-ap-whole-board-no");
    let decline_out = run_wh_stdin(
        &["set", "ap", "--keys", "all", "--set", "1.50"],
        &decline_script,
        &decline_config_home,
        "no\n",
    );
    assert!(!decline_out.status.success());
    // The prompt itself is a diagnostic, on stderr, not stdout; the refusal that follows it once
    // the reader answers `no` is also on stderr.
    let decline_err = String::from_utf8_lossy(&decline_out.stderr);
    assert!(
        decline_err.contains("ap: --keys all moves every key into one new keyset, keyset 3"),
        "got: {decline_err}"
    );
    assert!(
        decline_err.contains("keysets 1, 2 will cease to exist, their members absorbed"),
        "got: {decline_err}"
    );
    assert!(
        decline_err.contains("wh set ap --base 1.50"),
        "got: {decline_err}"
    );
    assert!(
        decline_err.contains("was not confirmed"),
        "got: {decline_err}"
    );

    std::fs::remove_file(decline_script).unwrap();
    let _ = std::fs::remove_dir_all(&decline_config_home);

    // `yes` proceeds: the same board, but the whole pipeline this time, `plan`'s reads, the
    // auto-backup snapshot, the actual write frames, and the readback verification.
    let mut accept_lines = matrix_lines_wasd();
    accept_lines.extend(matrix_lines_wasd());
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        accept_lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        accept_lines.extend(key_settings_lines(usage, 1200, 0x18, 100, 150, ks, 0));
    }
    accept_lines.extend(auto_backup_lines_wasd(
        0,
        (1200, 0x18, 100, 150, 1, 0),
        (1200, 0x18, 100, 150, 1, 0),
        (1200, 0x18, 100, 150, 2, 0),
        (1200, 0x18, 100, 150, 2, 0),
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
                    value: 1500,
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
    // Same 12/4 batching as any other 4-key whole-board write: `frames()` never splits one
    // key's own group across a report boundary, and 16 records at 4 per key exceeds the
    // 14-record limit.
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
            value: 3,
        })
        .collect();
    for f in cmds::write_key_records_singly(&membership_records) {
        accept_lines.push(out_line(&f));
        accept_lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        accept_lines.extend(key_settings_lines(usage, 1500, 0x18, 100, 150, 3, 0));
    }

    let accept_script = write_script("set-ap-whole-board-yes", &accept_lines);
    let accept_config_home = scratch_config_dir("set-ap-whole-board-yes");
    let accept_out = run_wh_stdin(
        &["set", "ap", "--keys", "all", "--set", "1.50"],
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
        accept_stdout.contains("ap keyset 3 at 1.50mm: 4 keys verified"),
        "got: {accept_stdout}"
    );

    std::fs::remove_file(accept_script).unwrap();
    let _ = std::fs::remove_dir_all(&accept_config_home);
}

/// Three keysets this time, each with a single member, so all three must be named, not just the
/// first or the last: the board this guards is 68 keys wide, and a prompt that dropped one
/// keyset from the list would understate what is actually about to be lost.
///
/// Selected by spelling out every usage (`w,a,s,d`), not the literal word `all`: the trigger is
/// the resolved selection covering the board's matrix, not that one spelling of it, so this is
/// also the fixture that would catch a rewrite checking `--keys` for the literal string `all`
/// instead of comparing against the membership read the arm already performs.
#[test]
fn set_ap_over_the_whole_board_names_every_keyset_that_will_cease_to_exist() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 2), (0x16, 3), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    let script = write_script("set-ap-whole-board-three-keysets", &lines);
    let config_home = scratch_config_dir("set-ap-whole-board-three-keysets");
    let out = run_wh_stdin(
        &["set", "ap", "--keys", "w,a,s,d", "--set", "1.50"],
        &script,
        &config_home,
        "no\n",
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ap: --keys all moves every key into one new keyset, keyset 4"),
        "got: {stderr}"
    );
    assert!(
        stderr.contains("keysets 1, 2, 3 will cease to exist, their members absorbed"),
        "got: {stderr}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `--dry-run` never prompts, even over the whole board: it writes nothing, so there is nothing
/// to confirm yet. Empty stdin (`run_wh`'s default) would hang the process if the guard fired
/// here regardless of `--dry-run`, so a clean, successful exit is itself the proof it did not.
#[test]
fn set_ap_over_the_whole_board_does_not_prompt_on_dry_run() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        lines.extend(key_settings_lines(usage, 1200, 0x18, 100, 150, ks, 0));
    }
    let path = write_script("set-ap-whole-board-dry-run", &lines);
    let config_home = scratch_config_dir("set-ap-whole-board-dry-run");
    let out = run_wh(
        &["set", "ap", "--keys", "all", "--set", "1.50", "--dry-run"],
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
    assert!(stdout.contains("dry run, no writes sent"), "got: {stdout}");
    assert!(
        !frame_lines(&stdout).is_empty(),
        "dry run must still print frames: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stdout.contains("type yes to continue") && !stderr.contains("type yes to continue"),
        "dry run must never prompt: stdout {stdout}\nstderr {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The regression guard for not over-triggering: a selection short of the whole matrix must
/// never prompt, even over a real write with no `yes` waiting on stdin (`run_wh`'s default,
/// `Stdio::null()`). A guard that mistakenly fired here would either hang reading an exhausted
/// stdin or refuse with "was not confirmed"; a clean, successful exit rules out both.
#[test]
fn set_ap_over_a_partial_selection_does_not_prompt() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    lines.extend(key_settings_lines(0x1A, 2000, 0x18, 100, 150, 0, 0)); // plan's read of w
    lines.extend(key_settings_lines(0x04, 2000, 0x18, 100, 150, 0, 0)); // plan's read of a
    lines.extend(auto_backup_lines_wasd(
        0,
        (2000, 0x18, 100, 150, 0, 0),
        (2000, 0x18, 100, 150, 0, 0),
        (2000, 0x18, 100, 150, 0, 0),
        (2000, 0x18, 100, 150, 0, 0),
    ));
    let value_records = [
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
    for f in &cmds::write_key_records(&value_records) {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    let membership_records = [
        KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_AP,
            value: 1,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::KEYSET_AP,
            value: 1,
        },
    ];
    for f in &cmds::write_key_records_singly(&membership_records) {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    lines.extend(key_settings_lines(0x1A, 1200, 0x18, 100, 150, 1, 0));
    lines.extend(key_settings_lines(0x04, 1200, 0x18, 100, 150, 1, 0));

    let path = write_script("set-ap-partial-no-prompt", &lines);
    let config_home = scratch_config_dir("set-ap-partial-no-prompt");
    let out = run_wh(
        &["set", "ap", "--keys", "w,a", "--set", "1.20"],
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
    assert!(
        stdout.contains("ap keyset 1 at 1.20mm: 2 keys verified"),
        "got: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The `rt` sibling of the test above: `ops::rt_records` reads each selected key's current MODE
/// (to preserve the advanced nibble in the preview) on top of `resolve_keys`' matrix read, and
/// nothing else. The script is exactly those reads; a regression that sent a write or SAVE here
/// would hit the exhausted script and `ReplayTransport` would reject it.
#[test]
fn set_rt_dry_run_reads_matrix_and_mode_but_sends_no_write_or_save() {
    // 'w' (0x1A) starts at MODE 0x0220 (touch RtGlobal, advanced nibble 0, high byte 0x02:
    // rapid trigger already on, following the global settings) and wants 0x0230 after
    // `rt_records` collapses the touch nibble to Rt (nibble 3, the key's own setting now).
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

    // The exact frame set, not just that each expected frame appears somewhere: a bare SAVE, or
    // a reordered/duplicated frame, would not otherwise be caught. Also pins the exact previewed
    // records, so a regression in the touch nibble or high-byte/advanced-nibble preservation
    // can't hide behind only the `--off` sibling catching it.
    let expected: Vec<String> = cmds::write_key_records(&[
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
    ])
    .iter()
    .map(|f| hex(f))
    .collect();
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "dry run must print exactly the frames a real run would send, and no others: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The `--off` sibling of the dry-run test above, pinning the preview for both board keys at
/// once (`--keys all`): `ops::rt_off_records` reads each key's current MODE on top of
/// `resolve_keys`' matrix read, and nothing else. A stray write or SAVE would hit the exhausted
/// script and `ReplayTransport` would reject it.
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

    // The exact frame set, not just that each expected frame appears somewhere: pins that each
    // key's advanced nibble and high byte survive independently, the same read-modify-write
    // `verify_rt_off` checks on the real write path.
    let expected: Vec<String> = cmds::write_key_records(&[
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
    ])
    .iter()
    .map(|f| hex(f))
    .collect();
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "dry run must print exactly the frames a real run would send, and no others: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// A key following the board's global rapid trigger (nibble 2) must still get a MODE frame
/// turning it off: before nibble 2 was modelled as `RtGlobal`, `rt_off_records` had no match arm
/// for it, folded it into `Unknown`, and sent nothing at all for `wh set rt --off`.
#[test]
fn set_rt_off_turns_off_a_key_following_the_global_rapid_trigger() {
    let mut lines = matrix_lines();
    lines.extend(mode_read_lines(0x1A, 0x0220));
    let path = write_script("set-rt-off-global", &lines);
    let config_home = scratch_config_dir("set-rt-off-global");

    let out = run_wh(
        &["set", "rt", "--keys", "w", "--off", "--dry-run"],
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

    let expected: Vec<String> = cmds::write_key_records(&[KeyRecord {
        key: 0x1A,
        layout: layout::MODE,
        value: 0x0210,
    }])
    .iter()
    .map(|f| hex(f))
    .collect();
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "a nibble-2 key must still get a MODE frame turning rapid trigger off: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Pins that `--dry-run` previews an operation that could actually happen: 'z' is a real key in
/// `wh_proto::keys::TABLE` but is not on this two-key fixture board. A dry run resolving against
/// the full static table instead of the live matrix would happily preview writing to it anyway,
/// so this must reject it with the same `NotOnDevice` error a live write gives.
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

/// A snapshot's JSON text for one key, 'w', with a caller-chosen `ap_mm` and `profile` (one-based,
/// or `None` for a snapshot that predates profile recording), so the out-of-range, happy-path,
/// and profile-safety restore tests below can all share it and diverge only on those two values.
fn restore_snapshot_json(ap_mm: f64, profile: Option<u8>) -> String {
    // `profile` is one-based (matching every other profile number in this file); built via
    // `from_one_based`, not `from_wire_index(p - 1)`, which would underflow-panic on `Some(0)`.
    let profile = profile.map(|p| cmds::ProfileNumber::from_one_based(p).unwrap());
    let snap = wh_config::snapshot::Snapshot {
        firmware: "V1.0.0.001".into(),
        serial: "SNRESTORETEST001".into(),
        taken_at: "2026-08-28T12:00:00Z".into(),
        profile,
        origin: None,
        global: wh_config::snapshot::GlobalToml {
            travel_mm: 2.0,
            press_dead_mm: 0.2,
            release_dead_mm: 0.1,
        },
        keys: vec![wh_config::snapshot::KeyToml {
            name: "w".into(),
            usage: 0x1A,
            ap_mm,
            // Agrees with mode_raw below: 0x0220 decodes to TouchMode::RtGlobal, rapid trigger on
            // following the global settings, so `rt` is true. `restore` never reads this field,
            // it round-trips mode_raw verbatim, but it should still describe the snapshot it sits in.
            rt: true,
            rt_press_mm: 0.5,
            rt_release_mm: 0.6,
            mode_raw: 0x0220,
            ap_keyset: Some(0),
            rt_keyset: Some(0),
        }],
    };
    snap.to_json().unwrap()
}

fn write_snapshot(tag: &str, ap_mm: f64, profile: Option<u8>) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("wh-{tag}-{}.json", std::process::id()));
    std::fs::write(&path, restore_snapshot_json(ap_mm, profile)).unwrap();
    path
}

/// A snapshot whose `ap_mm` is out of range (the device's actuation point tops out at 4.00mm,
/// this one says 99.0mm) must be refused before a single frame is sent, not after. Run against
/// a genuinely empty replay script: if `restore` sent anything at all before finishing
/// validation, `ReplayTransport` would reject the unexpected send.
#[test]
fn restore_refuses_an_out_of_range_value_before_any_frame_is_sent() {
    let config_home = scratch_config_dir("restore-out-of-range");
    let snap_path = write_snapshot("restore-oor", 99.0, Some(1));
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

/// The frames `ops::restore_all` sends plus `verify_restore`'s readback: global travel write
/// first, then the per-key batch for 'w' (ap, mode verbatim, rt press, rt release), no SAVE, then
/// a matching readback. Shared by the happy-path and force-rescue tests below so both restore the
/// identical snapshot content and diverge only on the profile-safety fixture around it.
/// `membership` is `true` for a snapshot that recorded 'w' at keyset `0` (an explicit "no
/// keyset", `Some(0)`), which sends the two membership frames below, and `false` for one that
/// predates keyset recording at all (`None`), which must send neither: the fields differ only in
/// what `restore` knows about 'w's membership, not in what it reads back, so both call sites can
/// still share the same value batch and readback.
fn restore_write_and_verify_lines(membership: bool) -> Vec<String> {
    let mut lines = Vec::new();
    let db_write = cmds::write_global_travel(
        wh_proto::value::Um::from_mm(2.0, 0.0, 4.0).unwrap(),
        wh_proto::value::Um::from_mm(0.2, 0.0, 4.0).unwrap(),
        wh_proto::value::Um::from_mm(0.1, 0.0, 4.0).unwrap(),
    );
    lines.push(out_line(&db_write));
    lines.push(in_line(&reply(cmds::cmd::DB, &[0x01, 0, 0])));

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

    if membership {
        // Membership last, one record per frame: 'w' carries an explicit no-keyset (`Some(0)`)
        // in this snapshot, so the write puts it back to none rather than skipping it.
        let membership = vec![
            KeyRecord {
                key: 0x1A,
                layout: layout::KEYSET_AP,
                value: 0,
            },
            KeyRecord {
                key: 0x1A,
                layout: layout::KEYSET_RT,
                value: 0,
            },
        ];
        for f in &cmds::write_key_records_singly(&membership) {
            lines.push(out_line(f));
            lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
        }
    }
    // No SAVE order follows: the vendor was never observed sending one.

    // verify_restore reads 'w' back and finds every field matching what was restored.
    lines.extend(key_settings_lines(0x1A, 1200, 0x0220, 500, 600, 0, 0));
    lines
}

/// `restore` from a valid snapshot: the auto-backup happens before anything is overwritten
/// (pinned by a real backup file existing on disk afterwards, not just the printed message),
/// the board's profile (1) matches the snapshot's recorded profile (1), the global travel and
/// per-key writes land, and the readback verifies. Exit 0, "verified" in stdout.
#[test]
fn restore_happy_path_backs_up_and_verifies() {
    let config_home = scratch_config_dir("restore-happy");
    let snap_path = write_snapshot("restore-happy", 1.2, Some(1));

    // `restore` reads the board's profile as its own, independent roundtrip before ever calling
    // `auto_backup`, whose own `snapshot_from_device` pipeline reads it again internally; both
    // replies report the same board profile index 0 (UI profile 1), matching the snapshot.
    let mut lines = profile_lines(0);
    lines.extend(auto_backup_lines(0));
    lines.extend(restore_write_and_verify_lines(true));

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

/// A hand-written `.toml` snapshot, restored by naming its path explicitly: proves `restore`
/// still picks the TOML parser off the extension via `Snapshot::from_file_text`, not always
/// JSON. Written by hand, not through any serializer, since `to_toml` no longer exists and this
/// is what a real Phase 1 backup looks like. Same values as `restore_write_and_verify_lines`
/// expects, so this shares that fixture with the JSON happy path above.
#[test]
fn restore_from_an_explicit_toml_file_still_works() {
    let config_home = scratch_config_dir("restore-toml-explicit");
    let snap_path =
        std::env::temp_dir().join(format!("wh-restore-toml-{}.toml", std::process::id()));
    std::fs::write(
        &snap_path,
        r#"firmware = "V1.0.0.001"
serial = "SNRESTORETEST001"
taken_at = "2026-08-28T12:00:00Z"
profile = 1

[global]
travel_mm = 2.0
press_dead_mm = 0.2
release_dead_mm = 0.1

[[keys]]
name = "w"
usage = 26
ap_mm = 1.2
rt = false
rt_press_mm = 0.5
rt_release_mm = 0.6
mode_raw = 544
"#,
    )
    .unwrap();

    let mut lines = profile_lines(0);
    lines.extend(auto_backup_lines(0));
    lines.extend(restore_write_and_verify_lines(false));
    let path = write_script("restore-toml-explicit", &lines);

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
    // This TOML predates keyset recording, so `restore` must say it left 'w's membership alone
    // rather than silently assert `0` for it (`restore_write_and_verify_lines(false)` above sent
    // no membership frame at all; `ReplayTransport` would have rejected one if it had).
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no recorded actuation point keyset for 1 key")
            && stderr.contains("no recorded rapid trigger keyset for 1 key"),
        "unexpected stderr: {stderr}"
    );

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `--last` restoring a `.toml` backup: the newest file in the store's `backups/` directory is a
/// pre-existing `.toml` backup, not one this run wrote, so `load_backup`'s returned path has to
/// carry through to `from_file_text` for the TOML parser to be picked at all. This is the one
/// path the JSON-only happy path above never exercises.
#[test]
fn restore_last_from_a_toml_backup_still_works() {
    let config_home = scratch_config_dir("restore-last-toml");
    let backups = config_home.join("wh").join("backups");
    std::fs::create_dir_all(&backups).unwrap();
    std::fs::write(
        backups.join("00000000001756000000.000000000.toml"),
        r#"firmware = "V1.0.0.001"
serial = "SNRESTORETEST001"
taken_at = "2026-08-28T12:00:00Z"
profile = 1

[global]
travel_mm = 2.0
press_dead_mm = 0.2
release_dead_mm = 0.1

[[keys]]
name = "w"
usage = 26
ap_mm = 1.2
rt = false
rt_press_mm = 0.5
rt_release_mm = 0.6
mode_raw = 544
"#,
    )
    .unwrap();

    let mut lines = profile_lines(0);
    lines.extend(auto_backup_lines(0));
    lines.extend(restore_write_and_verify_lines(false));
    let path = write_script("restore-last-toml", &lines);

    let out = run_wh(&["restore", "--last"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("verified"), "unexpected stdout: {stdout}");

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The profile-safety check, end to end: the snapshot recorded profile 1 but the board is on
/// profile 2. `restore` must refuse before `ops::restore_all` ever runs: the script ends right
/// after the auto-backup phase, so a global-travel write or key batch reaching the wire would
/// hit `ReplayTransport`'s unscripted-send rejection instead.
#[test]
fn restore_refuses_when_the_boards_profile_differs_from_the_snapshots() {
    let config_home = scratch_config_dir("restore-profile-mismatch");
    let snap_path = write_snapshot("restore-profile-mismatch", 1.2, Some(1));
    // restore's own direct profile read (board profile index 1 = UI profile 2) is the entire
    // script: refusal happens right after it, before `auto_backup` is ever called.
    let path = write_script("restore-profile-mismatch", &profile_lines(1));

    let out = run_wh(
        &["restore", snap_path.to_str().unwrap()],
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
        stderr.contains("profile 1") && stderr.contains("profile 2"),
        "unexpected stderr: {stderr}"
    );

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `--force` must not rescue a recorded mismatch: identical fixture to the test above, `--force`
/// added, same refusal expected. The script still ends right after restore's own direct profile
/// read, so a write reaching the wire would fail against the unscripted send.
#[test]
fn restore_force_does_not_rescue_a_profile_mismatch() {
    let config_home = scratch_config_dir("restore-profile-mismatch-force");
    let snap_path = write_snapshot("restore-profile-mismatch-force", 1.2, Some(1));
    let path = write_script("restore-profile-mismatch-force", &profile_lines(1));

    let out = run_wh(
        &["restore", snap_path.to_str().unwrap(), "--force"],
        &path,
        &config_home,
    );
    assert!(
        !out.status.success(),
        "expected a non-zero exit even with --force, got success with stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("profile 1") && stderr.contains("profile 2"),
        "unexpected stderr: {stderr}"
    );

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The other refusal case: no recorded profile at all (an older snapshot). Refused without
/// `--force`, before `auto_backup` or `ops::restore_all` ever run; same "script ends right after
/// restore's own direct profile read" reasoning as the mismatch tests above.
#[test]
fn restore_refuses_an_unrecorded_profile_without_force() {
    let config_home = scratch_config_dir("restore-profile-unrecorded");
    let snap_path = write_snapshot("restore-profile-unrecorded", 1.2, None);
    let path = write_script("restore-profile-unrecorded", &profile_lines(0));

    let out = run_wh(
        &["restore", snap_path.to_str().unwrap()],
        &path,
        &config_home,
    );
    assert!(
        !out.status.success(),
        "expected a non-zero exit, got success with stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    // `stderr.contains("--force")` alone would pass on an unrelated clap usage dump; matching
    // the actual refusal text (profile number and "no recorded profile" phrasing) discriminates.
    assert!(
        stderr.contains("no recorded profile") && stderr.contains("profile 1"),
        "unexpected stderr: {stderr}"
    );
    // `None` covers two causes, an older pre-recording snapshot and one whose board reported an
    // unrecognised index, and the message must name both rather than only the first.
    assert!(
        stderr.contains("does not recognise"),
        "message must also cover the unrecognised-index cause, not just predates-recording: {stderr}"
    );
    assert!(
        stderr.to_lowercase().contains("--force"),
        "unexpected stderr: {stderr}"
    );

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The rescue half of the unrecorded-profile case: identical fixture to the test above,
/// `--force` added, and this time the restore actually proceeds all the way through the write
/// and verification, unlike the mismatch case's `--force`, which never rescues anything.
#[test]
fn restore_force_rescues_an_unrecorded_profile() {
    let config_home = scratch_config_dir("restore-profile-unrecorded-force");
    let snap_path = write_snapshot("restore-profile-unrecorded-force", 1.2, None);

    // Same shape as the happy path above: restore's own direct profile read first, then the
    // full auto-backup pipeline (which reads the profile again, internally), then the write and
    // verify tail, all the way through since `--force` rescues the unrecorded-profile case.
    let mut lines = profile_lines(0);
    lines.extend(auto_backup_lines(0));
    lines.extend(restore_write_and_verify_lines(true));
    let path = write_script("restore-profile-unrecorded-force", &lines);

    let out = run_wh(
        &["restore", snap_path.to_str().unwrap(), "--force"],
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

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `restore`'s own direct profile read is a hard refusal on a wire index the board could never
/// report under the four measured profiles: unlike `dump`/`backup`/`set`'s auto-backup, `restore`
/// cannot compare what it cannot interpret, so it keeps aborting rather than degrading to
/// "unknown provenance".
#[test]
fn restore_refuses_when_the_boards_profile_index_is_out_of_range() {
    let config_home = scratch_config_dir("restore-profile-out-of-range");
    let snap_path = write_snapshot("restore-profile-out-of-range", 1.2, Some(1));
    let path = write_script("restore-profile-out-of-range", &profile_lines(0xFE));

    let out = run_wh(
        &["restore", snap_path.to_str().unwrap()],
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
        stderr.contains("254") && stderr.contains("4 profiles"),
        "unexpected stderr: {stderr}"
    );

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The `snapshot_from_device` sibling of the test above: the same out-of-range wire index
/// (0xFE), reached through `backup --to`, must not abort the command. It degrades to
/// `profile = None` ("unknown provenance", the same case an older pre-recording snapshot
/// carries) with a warning on stderr naming the bad index, rather than hard-failing.
#[test]
fn backup_degrades_to_no_profile_on_an_out_of_range_index() {
    let mut lines = sync_lines("SNOUTOFRANGE0001", "V1.0.0.001");
    lines.extend(profile_lines(0xFE));
    lines.extend(global_travel_lines(500, 200, 200));
    lines.extend(matrix_lines());
    lines.extend(key_settings_lines(0x1A, 1200, 0x0230, 500, 500, 0, 0));
    lines.extend(key_settings_lines(0x04, 1500, 0x00, 0, 0, 0, 0));

    let path = write_script("backup-out-of-range", &lines);
    let config_home = scratch_config_dir("backup-out-of-range");
    let out_path = std::env::temp_dir().join(format!("wh-backup-oor-{}.json", std::process::id()));

    let out = run_wh(
        &["backup", "--to", out_path.to_str().unwrap()],
        &path,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("254") && stderr.to_lowercase().contains("unknown"),
        "unexpected stderr: {stderr}"
    );

    let text = std::fs::read_to_string(&out_path).unwrap();
    let snap = wh_config::snapshot::Snapshot::from_json(&text).unwrap();
    assert_eq!(
        snap.profile, None,
        "an out-of-range index must record no profile, not a bogus one: {text}"
    );

    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(out_path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The distinction that justifies `DeviceError::ProfileOutOfRange` existing as its own variant,
/// separate from `DeviceError::Decode`: a profile reply that fails to decode for a reason other
/// than an out-of-range index (here, a payload too short to hold the index at all) must still
/// hard-fail `backup`, unlike the out-of-range case the test above covers.
#[test]
fn backup_hard_fails_on_a_profile_reply_too_short_to_decode() {
    let mut lines = sync_lines("SNSHORTPROFILE01", "V1.0.0.001");
    lines.push(out_line(&cmds::read_profile()));
    // Two payload bytes, `[status, sub-order]`: shaped like the start of a profile reply but
    // missing the index byte `parse_profile` needs, so it fails with `DecodeError::Short`, not
    // `DecodeError::ProfileOutOfRange`.
    lines.push(in_line(&reply(cmds::cmd::CMD, &[0x00, 0x70])));

    let path = write_script("backup-short-profile", &lines);
    let config_home = scratch_config_dir("backup-short-profile");
    let out_path =
        std::env::temp_dir().join(format!("wh-backup-short-{}.toml", std::process::id()));

    let out = run_wh(
        &["backup", "--to", out_path.to_str().unwrap()],
        &path,
        &config_home,
    );
    assert!(
        !out.status.success(),
        "a garbled profile reply must hard-fail backup, not degrade to unknown provenance: \
         stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("decode"),
        "expected a decode failure naming the short payload: {stderr}"
    );
    assert!(
        !out_path.exists(),
        "backup must not write a partial snapshot file when it fails before finishing"
    );

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

/// WSL only forwards an environment variable across the WSL/Windows boundary `bin/wh` execs
/// through when it is named in `WSLENV`; a `bin/wh` that forgot to set it once let `wh restore
/// --force` silently fall back to a real device while the operator believed `WH_REPLAY` made it
/// safe. Runs the actual shim against the actual release Windows binary, since `cargo test`'s
/// host-built binary never crosses that boundary. Skips cleanly outside WSL or before `wh.exe`
/// has been cross-built. A fake fixture serial on stdout proves replay worked end to end through
/// the shim, not just up to opening the transport.
#[test]
fn bin_wh_shim_propagates_wh_replay_and_never_touches_hardware() {
    if std::process::Command::new("wslpath")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("no wslpath on PATH (not running under WSL), skipping");
        return;
    }
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let shim = repo_root.join("bin/wh");
    let exe = repo_root.join("target/x86_64-pc-windows-gnu/release/wh.exe");
    if !shim.exists() || !exe.exists() {
        eprintln!(
            "bin/wh or the release x86_64-pc-windows-gnu build is not present, skipping \
             (run: cargo build --release --workspace --target x86_64-pc-windows-gnu)"
        );
        return;
    }

    let path = write_script("bin-wh-shim", &build_script());

    // No `XDG_CONFIG_HOME` here, deliberately, unlike every other test in this file: setting it
    // would be misleading isolation, since `Store::open`'s `directories::ProjectDirs` ignores it
    // on Windows and resolves `%APPDATA%\wh\config` regardless, exactly the mechanism behind a
    // real incident where a verification run believed it had isolation and wrote a real key group
    // into the operator's live config. Safe here only because this test's `dump` (default JSON)
    // reads and touches nothing on disk; a future test that writes needs a real `Store::open`
    // override.
    let out = std::process::Command::new(&shim)
        .args(["dump"])
        .env("WH_REPLAY", &path)
        .output()
        .unwrap();

    // An absent or held device is an environment condition, not a test bug: if `WH_REPLAY`
    // genuinely reaches `wh.exe`, this branch is unreachable, since `with_session` never opens
    // hardware. A regression with a free board still opens hardware instead of replay, which the
    // `transport: replay` assertion below still catches, so skipping here loses no coverage.
    let stderr_early = String::from_utf8_lossy(&out.stderr);
    if !out.status.success()
        && (stderr_early.contains("no Wallhack keyboard found")
            || stderr_early.contains("could not open the config interface"))
    {
        eprintln!("no keyboard reachable (absent, or held by the web configurator), skipping: {stderr_early}");
        std::fs::remove_file(path).unwrap();
        return;
    }

    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("transport: replay"),
        "unexpected stderr, WH_REPLAY may not have reached wh.exe: {stderr}"
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["serial"],
        "SNDUMPTEST000001",
        "expected the fixture's fake serial, not a real device's identity: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    std::fs::remove_file(path).unwrap();
}

/// A minimal, valid backup snapshot with a caller-chosen origin, for `wh backups list` tests
/// that don't need a full key set, just a file that parses.
fn sample_backup_snapshot(origin: &str) -> wh_config::snapshot::Snapshot {
    wh_config::snapshot::Snapshot {
        firmware: "V1.0.0.001".into(),
        serial: "SNBACKUPLIST0001".into(),
        taken_at: "2026-08-28T12:00:00Z".into(),
        profile: Some(cmds::ProfileNumber::from_one_based(1).unwrap()),
        origin: Some(origin.into()),
        global: wh_config::snapshot::GlobalToml {
            travel_mm: 2.0,
            press_dead_mm: 0.2,
            release_dead_mm: 0.2,
        },
        keys: vec![],
    }
}

/// A corrupt backup file sitting between two good ones must not hide either: `wh backups list`
/// warns on stderr about the corrupt one and still prints both good ones. A corrupt file at the
/// end would pass even with an implementation that aborted the whole listing on the first
/// parse failure, so it has to sit in the middle.
#[test]
fn backups_list_skips_a_corrupt_file_between_two_good_ones() {
    let config_home = scratch_config_dir("backups-list-corrupt");
    let backups = config_home.join("wh").join("backups");
    std::fs::create_dir_all(&backups).unwrap();

    let mut older = sample_backup_snapshot("auto: set rt");
    older.taken_at = "2026-08-28T10:00:00Z".into();
    std::fs::write(
        backups.join("1756000000.000000000.json"),
        older.to_json().unwrap(),
    )
    .unwrap();

    // Corrupt: a valid extension and non-empty, but not parseable as JSON at all.
    std::fs::write(
        backups.join("1756000005.000000000.json"),
        "{ this is not valid json",
    )
    .unwrap();

    let mut newer = sample_backup_snapshot("manual");
    newer.taken_at = "2026-08-28T11:00:00Z".into();
    std::fs::write(
        backups.join("1756000010.000000000.json"),
        newer.to_json().unwrap(),
    )
    .unwrap();

    let empty_replay = write_script("backups-list-corrupt", &[]);
    let out = run_wh(&["backups", "list"], &empty_replay, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("2026-08-28T10:00:00Z") && stdout.contains("auto: set rt"),
        "older good backup missing from the listing: {stdout}"
    );
    assert!(
        stdout.contains("2026-08-28T11:00:00Z") && stdout.contains("manual"),
        "newer good backup missing from the listing: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("1756000005.000000000.json"),
        "the corrupt file must be named in a warning: {stderr}"
    );

    std::fs::remove_file(empty_replay).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `wh restore --last` must name what it picked before restoring, on stderr: the flag keeps
/// meaning "the newest snapshot, whatever took it", so the fix is visibility, not a new flag.
/// The chosen file's path and its recorded origin must both appear.
#[test]
fn restore_last_prints_the_picked_snapshot_and_its_origin() {
    let config_home = scratch_config_dir("restore-last-origin");
    let backups = config_home.join("wh").join("backups");
    std::fs::create_dir_all(&backups).unwrap();

    let snap = wh_config::snapshot::Snapshot {
        firmware: "V1.0.0.001".into(),
        serial: "SNRESTORETEST001".into(),
        taken_at: "2026-08-28T12:00:00Z".into(),
        profile: Some(cmds::ProfileNumber::from_one_based(1).unwrap()),
        origin: Some("manual".into()),
        global: wh_config::snapshot::GlobalToml {
            travel_mm: 2.0,
            press_dead_mm: 0.2,
            release_dead_mm: 0.1,
        },
        keys: vec![wh_config::snapshot::KeyToml {
            name: "w".into(),
            usage: 0x1A,
            ap_mm: 1.2,
            // mode_raw 0x0220 decodes to TouchMode::RtGlobal: rapid trigger on, so `rt` is true.
            rt: true,
            rt_press_mm: 0.5,
            rt_release_mm: 0.6,
            mode_raw: 0x0220,
            ap_keyset: Some(0),
            rt_keyset: Some(0),
        }],
    };
    std::fs::write(
        backups.join("1756000000.000000000.json"),
        snap.to_json().unwrap(),
    )
    .unwrap();

    let mut lines = profile_lines(0);
    lines.extend(auto_backup_lines(0));
    lines.extend(restore_write_and_verify_lines(true));
    let path = write_script("restore-last-origin", &lines);

    let out = run_wh(&["restore", "--last"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("1756000000.000000000.json") && stderr.contains("origin: manual"),
        "unexpected stderr: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// A snapshot's JSON text for two keys, 'w' recorded in ap keyset 3 and 'a' recorded in none, so
/// the restore-membership test below has a real index to write back next to a zero one, telling
/// "restored to this keyset" apart from "restored to no keyset" rather than exercising only one.
fn snapshot_json_with_keysets() -> String {
    let snap = wh_config::snapshot::Snapshot {
        firmware: "V1.0.0.001".into(),
        serial: "SNRESTOREKEYSET1".into(),
        taken_at: "2026-08-28T12:00:00Z".into(),
        profile: Some(cmds::ProfileNumber::from_one_based(1).unwrap()),
        origin: None,
        global: wh_config::snapshot::GlobalToml {
            travel_mm: 2.0,
            press_dead_mm: 0.2,
            release_dead_mm: 0.1,
        },
        keys: vec![
            wh_config::snapshot::KeyToml {
                name: "w".into(),
                usage: 0x1A,
                ap_mm: 1.2,
                rt: false,
                rt_press_mm: 0.5,
                rt_release_mm: 0.6,
                mode_raw: 0x0018,
                ap_keyset: Some(3),
                rt_keyset: Some(0),
            },
            wh_config::snapshot::KeyToml {
                name: "a".into(),
                usage: 0x04,
                ap_mm: 1.5,
                rt: false,
                rt_press_mm: 0.0,
                rt_release_mm: 0.0,
                mode_raw: 0x0000,
                ap_keyset: Some(0),
                rt_keyset: Some(0),
            },
        ],
    };
    snap.to_json().unwrap()
}

/// One key's six-field wire readback in `restore_script_with_keyset_readback`'s fixture, in
/// `key_settings_lines`' own parameter order, so a test overrides exactly one field via
/// `..W_CORRECT_READBACK`/`..A_CORRECT_READBACK` and leaves the rest matching what was actually
/// restored. Mirrors `tests/keyset.rs`'s own `Readback` pattern.
#[derive(Clone, Copy)]
struct KeyReadback {
    ap: u16,
    mode: u16,
    rt_press: u16,
    rt_release: u16,
    ap_keyset: u16,
    rt_keyset: u16,
}

/// 'w's readback when the restore of `snapshot_json_with_keysets` landed exactly as sent.
const W_CORRECT_READBACK: KeyReadback = KeyReadback {
    ap: 1200,
    mode: 0x0018,
    rt_press: 500,
    rt_release: 600,
    ap_keyset: 3,
    rt_keyset: 0,
};

/// 'a's readback when the restore of `snapshot_json_with_keysets` landed exactly as sent.
const A_CORRECT_READBACK: KeyReadback = KeyReadback {
    ap: 1500,
    mode: 0x0000,
    rt_press: 0,
    rt_release: 0,
    ap_keyset: 0,
    rt_keyset: 0,
};

/// The full script `wh restore` sends for `snapshot_json_with_keysets`: its own profile read, the
/// auto-backup, the global travel write, the per-key value batch, then membership one record per
/// frame last, ap over both keys before rt over both, matching `restore_membership_records`' own
/// order, then the readback `verify_restore` does per key. `w`/`a` are each key's full six-field
/// readback: `W_CORRECT_READBACK`/`A_CORRECT_READBACK` for a clean match, or one field changed via
/// `..CORRECT` to script exactly one fault. Deliberately independent per key: a corruption on
/// `keys[0]` alone cannot tell a `verify_restore` that checks every key apart from one that only
/// ever checks the first, which `keys.iter().take(1)` proved indistinguishable from correct when
/// the only corrupted fixture was 'w'.
fn restore_script_with_keyset_readback(w: KeyReadback, a: KeyReadback) -> Vec<String> {
    let mut lines = profile_lines(0);
    lines.extend(auto_backup_lines(0));

    let db_write = cmds::write_global_travel(
        wh_proto::value::Um(2000),
        wh_proto::value::Um(200),
        wh_proto::value::Um(100),
    );
    lines.push(out_line(&db_write));
    lines.push(in_line(&reply(cmds::cmd::DB, &[0x01, 0, 0])));

    let value_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 1200,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x0018,
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
        KeyRecord {
            key: 0x04,
            layout: layout::AP,
            value: 1500,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::MODE,
            value: 0x0000,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_PRESS,
            value: 0,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_RELEASE,
            value: 0,
        },
    ];
    for f in &cmds::write_key_records(&value_records) {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    // Membership last, one record per frame: ap for every key, then rt for every key, matching
    // `restore_membership_records`' own build order rather than interleaving per key.
    let membership_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::KEYSET_AP,
            value: 3,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::KEYSET_AP,
            value: 0,
        },
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
    for f in &cmds::write_key_records_singly(&membership_records) {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    // verify_restore's readback: both keys land at the restored values and keyset indices,
    // unless the caller asked for exactly one field to disagree.
    lines.extend(key_settings_lines(
        0x1A,
        w.ap,
        w.mode,
        w.rt_press,
        w.rt_release,
        w.ap_keyset,
        w.rt_keyset,
    ));
    lines.extend(key_settings_lines(
        0x04,
        a.ap,
        a.mode,
        a.rt_press,
        a.rt_release,
        a.ap_keyset,
        a.rt_keyset,
    ));
    lines
}

/// A restore puts membership back, values first and membership last, one record per frame. A
/// snapshot that recorded a key in ap keyset 3 must leave the board with that key in keyset 3.
/// `ReplayTransport` matches byte for byte, so a membership frame sent before the value frames,
/// or batched with them, or in the wrong per-key order, fails the script rather than the
/// assertions below: the ordering is what this test actually pins.
#[test]
fn restore_writes_keyset_membership_after_the_values() {
    let config_home = scratch_config_dir("restore-keysets");
    let snap_path =
        std::env::temp_dir().join(format!("wh-restore-keysets-{}.json", std::process::id()));
    std::fs::write(&snap_path, snapshot_json_with_keysets()).unwrap();

    let lines = restore_script_with_keyset_readback(W_CORRECT_READBACK, A_CORRECT_READBACK);
    let path = write_script("restore-keysets", &lines);

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
    assert!(
        stdout.contains("restored 2 keys from snapshot") && stdout.contains("verified"),
        "unexpected stdout: {stdout}"
    );

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Runs `wh restore` against `snapshot_json_with_keysets()` with the given per-key readback and
/// asserts it fails, names `expected_fault` in stderr, and never claims `verified`. Shared by
/// every single-field mismatch test below, each of which differs from
/// `W_CORRECT_READBACK`/`A_CORRECT_READBACK` in exactly one field, so each test can only fail
/// because of the one comparison it exists to pin, not several at once.
fn assert_restore_reports_one_fault(
    tag: &str,
    w: KeyReadback,
    a: KeyReadback,
    expected_fault: &str,
) {
    let config_home = scratch_config_dir(tag);
    let snap_path = std::env::temp_dir().join(format!("wh-{tag}-{}.json", std::process::id()));
    std::fs::write(&snap_path, snapshot_json_with_keysets()).unwrap();

    let lines = restore_script_with_keyset_readback(w, a);
    let path = write_script(tag, &lines);

    let out = run_wh(
        &["restore", snap_path.to_str().unwrap()],
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
        stderr.contains(expected_fault),
        "unexpected stderr: {stderr}"
    );
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("verified"),
        "must not claim success while the board disagrees: {stderr}"
    );

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `verify_restore`'s ap keyset comparison is otherwise unpinned: nothing before this test fails
/// if it is deleted. Corrupts 'a's ap keyset readback (`5` instead of the `0` the restore actually
/// wrote), leaving every other field correct. On `keys[1]`, not `keys[0]`: a corruption on the
/// first key alone cannot tell a loop that checks every key apart from one that checks only the
/// first, which `keys.iter().take(1)` proved indistinguishable from correct when this was 'w'.
#[test]
fn restore_reports_an_ap_keyset_mismatch() {
    assert_restore_reports_one_fault(
        "restore-ap-keyset-mismatch",
        W_CORRECT_READBACK,
        KeyReadback {
            ap_keyset: 5,
            ..A_CORRECT_READBACK
        },
        "a: board reports ap keyset 5, wanted 0",
    );
}

/// The rapid trigger keyset comparison's sibling: `verify_restore`'s `rt_keyset` block is
/// otherwise unpinned. Removing it entirely leaves the whole workspace green, since nothing
/// before this test scripts a mismatched `0xFE` readback. Corrupts 'a's rt keyset readback (`7`
/// instead of `0`), on the second key for the same reason as the ap keyset test above.
#[test]
fn restore_reports_an_rt_keyset_mismatch() {
    assert_restore_reports_one_fault(
        "restore-rt-keyset-mismatch",
        W_CORRECT_READBACK,
        KeyReadback {
            rt_keyset: 7,
            ..A_CORRECT_READBACK
        },
        "a: board reports rt keyset 7, wanted 0",
    );
}

/// `verify_restore`'s `ap` comparison is otherwise unpinned: nothing before this test fails if it
/// is deleted from the value/mode condition. Corrupts only 'w's ap readback (`1300` instead of
/// `1200`), leaving every other field, including membership, correct.
#[test]
fn restore_reports_an_ap_mismatch() {
    assert_restore_reports_one_fault(
        "restore-ap-mismatch",
        KeyReadback {
            ap: 1300,
            ..W_CORRECT_READBACK
        },
        A_CORRECT_READBACK,
        "w: board reports ap 1.30mm, wanted 1.20mm",
    );
}

/// `verify_restore`'s `rt_press` comparison is otherwise unpinned: it can disappear without a
/// test noticing. Corrupts only 'w's rt press readback (`550` instead of `500`).
#[test]
fn restore_reports_an_rt_press_mismatch() {
    assert_restore_reports_one_fault(
        "restore-rt-press-mismatch",
        KeyReadback {
            rt_press: 550,
            ..W_CORRECT_READBACK
        },
        A_CORRECT_READBACK,
        "w: board reports rt press 0.55mm, wanted 0.50mm",
    );
}

/// `verify_restore`'s `rt_release` comparison is otherwise unpinned: it can disappear without a
/// test noticing. Corrupts only 'w's rt release readback (`650` instead of `600`).
#[test]
fn restore_reports_an_rt_release_mismatch() {
    assert_restore_reports_one_fault(
        "restore-rt-release-mismatch",
        KeyReadback {
            rt_release: 650,
            ..W_CORRECT_READBACK
        },
        A_CORRECT_READBACK,
        "w: board reports rt release 0.65mm, wanted 0.60mm",
    );
}

/// `verify_restore`'s `mode` comparison is otherwise unpinned, and it is the one that matters
/// most: `mode_raw` exists precisely so advanced-key modes survive a round trip, so a bug that
/// drops only this comparison would silently leave an unrestored advanced mode reported as
/// verified. Corrupts only 'w's mode readback (`0x0020` instead of `0x0018`).
#[test]
fn restore_reports_a_mode_mismatch() {
    assert_restore_reports_one_fault(
        "restore-mode-mismatch",
        KeyReadback {
            mode: 0x0020,
            ..W_CORRECT_READBACK
        },
        A_CORRECT_READBACK,
        "w: board reports mode 0x0020, wanted 0x0018",
    );
}

/// A snapshot with no `ap_keyset`/`rt_keyset` fields at all, the shape of a genuinely pre-2.1
/// backup, restored against a board holding live, nonzero keyset membership on 'w' (ap keyset 4,
/// rt keyset 2). The fix this pins: `restore` must not write `0` to either layout for 'w', which
/// would dissolve whatever keyset it actually belongs to, and it must tell the operator it left
/// membership alone rather than saying nothing. `ReplayTransport` matches byte for byte and this
/// script contains no `0xff`/`0xfe` write for 'w' at all, so a regression that sent one would fail
/// on the unscripted send rather than merely on an assertion below.
#[test]
fn restore_from_a_snapshot_that_predates_keysets_leaves_live_membership_untouched() {
    let config_home = scratch_config_dir("restore-predates-keysets");
    let snap_path = std::env::temp_dir().join(format!(
        "wh-restore-predates-keysets-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &snap_path,
        r#"{
  "firmware": "V1.0.0.001",
  "serial": "SNRESTORETEST001",
  "taken_at": "2026-08-28T12:00:00Z",
  "profile": 1,
  "global": { "travel_mm": 2.0, "press_dead_mm": 0.2, "release_dead_mm": 0.1 },
  "keys": [
    { "name": "w", "usage": 26, "ap_mm": 1.2, "rt": false, "rt_press_mm": 0.5,
      "rt_release_mm": 0.6, "mode_raw": 24 }
  ]
}"#,
    )
    .unwrap();

    let mut lines = profile_lines(0);
    // The auto-backup's own live read: 'w' holds ap keyset 4 and rt keyset 2, 'a' holds neither.
    lines.extend(sync_lines("SNWRITETEST00001", "V1.0.0.001"));
    lines.extend(profile_lines(0));
    lines.extend(global_travel_lines(500, 200, 200));
    lines.extend(matrix_lines());
    lines.extend(key_settings_lines(0x1A, 1000, 0x0220, 500, 500, 4, 2));
    lines.extend(key_settings_lines(0x04, 1500, 0x00, 0, 0, 0, 0));

    // `restore`'s own writes: global travel, then 'w's value batch. No membership frame at all.
    let db_write = cmds::write_global_travel(
        wh_proto::value::Um(2000),
        wh_proto::value::Um(200),
        wh_proto::value::Um(100),
    );
    lines.push(out_line(&db_write));
    lines.push(in_line(&reply(cmds::cmd::DB, &[0x01, 0, 0])));
    let value_records = vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 1200,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x0018,
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
    for f in &cmds::write_key_records(&value_records) {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }

    // verify_restore's readback: 'w' still holds its live keyset 4/2, untouched, and the values
    // that were actually restored. The keyset fields are not compared (the snapshot never
    // recorded them), so whatever they read here cannot itself pass or fail the run.
    lines.extend(key_settings_lines(0x1A, 1200, 0x0018, 500, 600, 4, 2));

    let path = write_script("restore-predates-keysets", &lines);

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
    assert!(
        stdout.contains("restored 1 key from snapshot") && stdout.contains("verified"),
        "unexpected stdout: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no recorded actuation point keyset for 1 key")
            && stderr.contains("no recorded rapid trigger keyset for 1 key"),
        "unexpected stderr: {stderr}"
    );

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The skip note must not print for a restore that never runs. A snapshot recorded on profile 1,
/// board on profile 2, predates keyset recording exactly as the test above: `restore` must refuse
/// on the profile mismatch, the same as it does for any snapshot, and the skip note (which
/// describes membership being left as the board already has it) must not appear, since nothing
/// about this restore happened at all. The script ends right after the profile read, so a write
/// reaching the wire would fail against the unscripted send.
#[test]
fn restore_refusal_before_any_write_prints_no_membership_skip_note() {
    let config_home = scratch_config_dir("restore-predates-keysets-refused");
    let snap_path = std::env::temp_dir().join(format!(
        "wh-restore-predates-keysets-refused-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &snap_path,
        r#"{
  "firmware": "V1.0.0.001",
  "serial": "SNRESTORETEST001",
  "taken_at": "2026-08-28T12:00:00Z",
  "profile": 1,
  "global": { "travel_mm": 2.0, "press_dead_mm": 0.2, "release_dead_mm": 0.1 },
  "keys": [
    { "name": "w", "usage": 26, "ap_mm": 1.2, "rt": false, "rt_press_mm": 0.5,
      "rt_release_mm": 0.6, "mode_raw": 24 }
  ]
}"#,
    )
    .unwrap();

    // Board reports wire index 1 (UI profile 2); the snapshot recorded profile 1, so `restore`
    // refuses right after this one read, before `auto_backup` or any write is ever attempted.
    let path = write_script("restore-predates-keysets-refused", &profile_lines(1));

    let out = run_wh(
        &["restore", snap_path.to_str().unwrap()],
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
        stderr.contains("profile 1") && stderr.contains("profile 2"),
        "unexpected stderr: {stderr}"
    );
    assert!(
        !stderr.contains("no recorded actuation point keyset"),
        "the skip note must not describe a restore that never ran: {stderr}"
    );

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}
