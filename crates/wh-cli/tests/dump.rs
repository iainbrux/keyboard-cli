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
use wh_proto::value::Um;

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

/// One key's MODE-only read roundtrip, the single read `ops::rt_records` sends per key (to
/// preserve the advanced nibble), distinct from `key_settings_lines`' full six-read
/// `read_key_settings` sequence. `wh set rt --set` is the last path that reads this way: `--off`
/// goes through `keyset::plan`, which reads all six layouts.
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
    assert_eq!(v["global"]["custom_value_mm"], 0.5);
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
    // The global line names the setting for what it is, the configurator's `"MM" CUSTOM VALUE`,
    // rather than calling it travel: the actuation point is not in that record at all.
    // `build_script` scripts the board reading 500um for it.
    assert!(
        stdout.contains("global: custom value 0.50mm, dead 0.20/0.20mm"),
        "the global line must name the custom value: {stdout}"
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

/// The global record's first field is the configurator's `"MM" CUSTOM VALUE`, not the global
/// actuation point, so a file `wh backup` writes must spell it `custom_value_mm`. Asserted on the
/// file's own text, not through a parse: the alias that keeps old backups loading would let a
/// parsed round trip pass while `wh backup` still wrote the old name.
#[test]
fn backup_to_writes_the_custom_value_field_under_its_new_name() {
    let path = write_script("backup-custom-value", &build_script());
    let config_home = scratch_config_dir("backup-custom-value");
    let out_path = std::env::temp_dir().join(format!(
        "wh-backup-custom-value-{}.json",
        std::process::id()
    ));

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
    assert!(
        text.contains("custom_value_mm") && !text.contains("travel_mm"),
        "backup --to must write custom_value_mm, not the old travel_mm: {text}"
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

/// The `origin` recorded in the single backup file the store holds, read through the real parser
/// rather than off the raw text. Insists on exactly one file, so a test can never read a leftover
/// from an earlier run and call it this run's label.
fn only_backup_origin(config_home: &std::path::Path) -> String {
    let dir = config_home.join("wh").join("backups");
    let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(paths.len(), 1, "expected exactly one backup: {paths:?}");
    let path = paths.pop().unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let snap = wh_config::snapshot::Snapshot::from_file_text(&path, &text).unwrap();
    snap.origin.expect("an auto-backup must record an origin")
}

/// The label a real `wh set ap` run writes into the backup file it takes, read back off disk.
/// Every other test of the origin builds a snapshot by hand, so none of them can tell whether a
/// command reaches its own label: an operator choosing between backups by origin restores the
/// wrong board state if it does not.
#[test]
fn set_ap_end_to_end_records_its_own_command_as_the_backup_origin() {
    let path = write_script("set-ap-origin", &set_ap_script(1200));
    let config_home = scratch_config_dir("set-ap-origin");

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
    assert_eq!(only_backup_origin(&config_home), "auto: set ap");

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

/// The reads `wh set rt --off` issues before it plans anything, against the two-key board:
/// `resolve_keys`' matrix read, `keyset::read_membership`'s own matrix read and `0xFE` sweep, then
/// `keyset::global_rt_excluding`'s press/release pair for every key that sweep found outside a
/// keyset *and* outside the selection. `rt_keysets` is each key's `0xFE` value in matrix order
/// ('w' then 'a'), and `sensitivities` the pair each read-back key reports, in the same order.
///
/// `selection` is what `--keys` resolved to, and a key in it contributes no sensitivity read at
/// all, exactly like a key already in a keyset. That exclusion is the point: the keys being reset
/// are usually the ones holding their own sensitivity, so a script that expected them to be read
/// would be scripting the defect this command had in fix round 1.
fn rt_off_pre_plan_lines(
    rt_keysets: [u16; 2],
    sensitivities: [(u16, u16); 2],
    selection: &[u8],
) -> Vec<String> {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // keyset::read_membership's own matrix read
    for (usage, ks) in [0x1Au8, 0x04].into_iter().zip(rt_keysets) {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, ks));
    }
    for ((usage, ks), (press, release)) in [0x1Au8, 0x04]
        .into_iter()
        .zip(rt_keysets)
        .zip(sensitivities)
    {
        if ks != 0 || selection.contains(&usage) {
            continue;
        }
        lines.extend(layout_read_lines(usage, layout::RT_PRESS, press));
        lines.extend(layout_read_lines(usage, layout::RT_RELEASE, release));
    }
    lines
}

/// The auto-backup phase for the boards the `set rt --off` tests below use, taking both keys'
/// full state rather than assuming either: a board with a rapid trigger keyset on it is exactly
/// the case these tests exist for, and `auto_backup_lines_with_modes` hardcodes 'w' free at
/// 500/500 and 'a' at 0/0, neither of which such a board holds. Each key is
/// `(ap, mode, press, release, ap keyset, rt keyset)`, `WasdKeyState`'s own shape.
fn auto_backup_lines_rt_off(w: WasdKeyState, a: WasdKeyState) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(sync_lines("SNRTOFFTEST00001", "V1.0.0.001"));
    lines.extend(profile_lines(0));
    lines.extend(global_travel_lines(500, 200, 200));
    lines.extend(matrix_lines());
    for (usage, (ap, mode, press, release, apks, rtks)) in [(0x1Au8, w), (0x04, a)] {
        lines.extend(key_settings_lines(
            usage, ap, mode, press, release, apks, rtks,
        ));
    }
    lines
}

/// The write frames a plan sends, as `[out, in]` lines: the value batch first, then one frame per
/// membership record, which is `WritePlan::frames`' own ordering and the vendor's (`rt-off-w`,
/// frame 70, sends `0xFE` last).
fn write_lines(value_records: &[KeyRecord], membership_records: &[KeyRecord]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut frames = Vec::new();
    if !value_records.is_empty() {
        frames.extend(cmds::write_key_records(value_records));
    }
    frames.extend(cmds::write_key_records_singly(membership_records));
    for f in &frames {
        lines.push(out_line(f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    lines
}

/// The four value records `Change::rt_off` writes for 'w': MODE with the touch nibble forced to
/// Single, the actuation point echoed back untouched, and both sensitivities reset.
fn rt_off_value_records(mode: u16, ap: u16, press: u16, release: u16) -> Vec<KeyRecord> {
    vec![
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: mode,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: ap,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_PRESS,
            value: press,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::RT_RELEASE,
            value: release,
        },
    ]
}

/// The one membership record a rapid trigger off sends for 'w': `0xFE = 0`, clearing whatever
/// keyset it held.
fn rt_off_membership_records() -> Vec<KeyRecord> {
    vec![KeyRecord {
        key: 0x1A,
        layout: layout::KEYSET_RT,
        value: 0,
    }]
}

/// `wh set rt --keys w --off` previewed against a board where 'w' is the only member of rapid
/// trigger keyset 1, holding its own 0.50/0.50mm while the board's free key sits at 0.10/0.10mm.
/// Pins the whole frame sequence: the vendor's per-key rapid trigger off resets the sensitivities
/// to the global and then clears `0xFE` as the last thing it sends (`captures/rt-off-w.jsonl`,
/// frame 70), so a preview missing either the reset or the membership frame fails here.
///
/// The announcement is asserted as a whole line, not by the "ceases to exist" clause alone: a
/// clause pinned by its own wording can be welded onto its neighbour with no separator and every
/// substring assertion still passes.
#[test]
fn set_rt_off_dry_run_resets_sensitivities_and_clears_membership() {
    let mut lines = rt_off_pre_plan_lines([1, 0], [(500, 500), (100, 100)], &[0x1A]);
    // `plan`'s own six-layout read of 'w': rapid trigger on (touch nibble 3) with advanced nibble
    // 1 and high byte 0x02, its own 0.50/0.50mm, and rapid trigger keyset 1.
    lines.extend(key_settings_lines(0x1A, 1000, 0x0231, 500, 500, 0, 1));

    let path = write_script("set-rt-off-membership-dry", &lines);
    let config_home = scratch_config_dir("set-rt-off-membership-dry");

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
    assert!(
        stdout.lines().any(|l| l
            == "rt: removing w from keyset 1, 0.50/0.50mm to 0.10/0.10mm, mode Rt to Single, \
                keyset 1 ceases to exist"),
        "unexpected stdout: {stdout}"
    );

    let mut expected: Vec<String> =
        cmds::write_key_records(&rt_off_value_records(0x0211, 1000, 100, 100))
            .iter()
            .map(|f| hex(f))
            .collect();
    expected.extend(
        cmds::write_key_records_singly(&rt_off_membership_records())
            .iter()
            .map(|f| hex(f)),
    );
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "a rapid trigger off must reset both sensitivities and then clear 0xFE, in that order: \
         {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The write-path sibling of the preview above, on the same board: the announcement reaching the
/// operator through the real command, the frames actually sent (`ReplayTransport` matches every
/// one byte for byte), and the readback verification's own label. Proving the string is built
/// right is a different claim from proving it reaches the operator, which is why both exist.
#[test]
fn set_rt_off_end_to_end_clears_membership_and_verifies() {
    let mut lines = rt_off_pre_plan_lines([1, 0], [(500, 500), (100, 100)], &[0x1A]);
    lines.extend(key_settings_lines(0x1A, 1000, 0x0231, 500, 500, 0, 1));
    lines.extend(auto_backup_lines_rt_off(
        (1000, 0x0231, 500, 500, 0, 1),
        (1500, 0x10, 100, 100, 0, 0),
    ));
    lines.extend(write_lines(
        &rt_off_value_records(0x0211, 1000, 100, 100),
        &rt_off_membership_records(),
    ));
    // The readback: rapid trigger off, both sensitivities at the global, and `0xFE` back to 0.
    lines.extend(key_settings_lines(0x1A, 1000, 0x0211, 100, 100, 0, 0));

    let path = write_script("set-rt-off-membership-write", &lines);
    let config_home = scratch_config_dir("set-rt-off-membership-write");

    let out = run_wh(&["set", "rt", "--keys", "w", "--off"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.lines().any(|l| l
            == "rt: removing w from keyset 1, 0.50/0.50mm to 0.10/0.10mm, mode Rt to Single, \
                keyset 1 ceases to exist"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.lines().any(|l| l == "rt off: 1 key verified"),
        "unexpected stdout: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// A key already off (touch nibble 1) and already at the board's global sensitivity still gets its
/// `0xFE` cleared, and gets no value records at all: `plan` suppresses the value bundle when
/// nothing differs, but emits the membership record unconditionally, which is the whole point of
/// routing this command through it. The script carries the membership frame and no value frame,
/// so a spurious value bundle hits an unscripted send and `ReplayTransport` rejects it.
#[test]
fn set_rt_off_on_an_already_off_key_clears_membership_and_writes_no_value_records() {
    let mut lines = rt_off_pre_plan_lines([0, 0], [(100, 100), (100, 100)], &[0x1A]);
    // 'w' at MODE 0x10: touch Single, the commonest real value on this board, rapid trigger
    // already off, and already holding the global 0.10/0.10mm that 'a', the one key left outside
    // the selection, is what the base is read from.
    lines.extend(key_settings_lines(0x1A, 1000, 0x10, 100, 100, 0, 0));
    lines.extend(auto_backup_lines_rt_off(
        (1000, 0x10, 100, 100, 0, 0),
        (1500, 0x10, 100, 100, 0, 0),
    ));
    lines.extend(write_lines(&[], &rt_off_membership_records()));
    lines.extend(key_settings_lines(0x1A, 1000, 0x10, 100, 100, 0, 0));

    let path = write_script("set-rt-off-already-off", &lines);
    let config_home = scratch_config_dir("set-rt-off-already-off");

    let out = run_wh(&["set", "rt", "--keys", "w", "--off"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.lines().any(|l| l
            == "rt: w already at 0.10/0.10mm in no rt keyset, membership rewritten, value \
                unchanged"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.lines().any(|l| l == "rt off: 1 key verified"),
        "unexpected stdout: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The four-key board both disagreement tests below share. Every key is free of a rapid trigger
/// keyset; 'w', the one being reset, holds its own 0.50/0.50mm, and the three keys the selection
/// leaves behind do not agree with each other either: 'a' at 0.50/0.50mm, 's' and 'd' at
/// 0.10/0.10mm. So the disagreement is among keys *outside* the selection, which is the only case
/// where refusing is right. 'w' contributes no reading at all, being excluded, which is what stops
/// the ordinary "turn rapid trigger off on the one key that has it" run from refusing.
///
/// `read_sensitivities` is whether the sweep happens at all: `--press`/`--release` skips it
/// entirely, and a script carrying reads that never happen fails as loudly as one missing reads
/// that do.
///
/// The counts are 2 and 1, and the first value read is the one held by a single key, so the
/// refusal's descending order is observable rather than a tie or an accident of read order.
fn rt_off_split_board_lines(read_sensitivities: bool) -> Vec<String> {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, 0));
    }
    if read_sensitivities {
        // 'w' (0x1A) is absent: it is the selection, and the base read excludes it.
        for (usage, (press, release)) in
            [0x04u8, 0x16, 0x07]
                .into_iter()
                .zip([(500u16, 500u16), (100, 100), (100, 100)])
        {
            lines.extend(layout_read_lines(usage, layout::RT_PRESS, press));
            lines.extend(layout_read_lines(usage, layout::RT_RELEASE, release));
        }
    }
    lines
}

/// `--off` on a board where the keys left outside the selection disagree refuses rather than
/// picking a winner, names each distinct value with how many keys hold it in descending order of
/// count, and names both ways past: the override flags, and widening the selection. Asserts the
/// whole sentence, because a bare `!success` would also pass on a run that merely reached the end
/// of a short script, which is what a mutant that stops refusing actually produces here.
#[test]
fn set_rt_off_refuses_when_the_free_keys_disagree_and_names_the_override_flags() {
    let path = write_script("set-rt-off-split", &rt_off_split_board_lines(true));
    let config_home = scratch_config_dir("set-rt-off-split");

    let out = run_wh(&["set", "rt", "--keys", "w", "--off"], &path, &config_home);
    assert!(
        !out.status.success(),
        "expected a non-zero exit, got success with stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(
            "the keys left outside this selection and outside every rapid trigger keyset \
             disagree on the global sensitivity (2 key(s) at 0.10/0.10mm, 1 key(s) at \
             0.50/0.50mm), so there is no one value to reset to; pass --press and --release to \
             say which, or include those keys in the selection so they are reset too"
        ),
        "unexpected stderr: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The other half of the exclusion rule, and the regression fix round 1 caught: a free key with
/// rapid trigger on and its own sensitivity, every other key at the board's base. That is the
/// commonest way anyone runs this command, and reading the base without excluding the selection
/// makes 'w' its own disagreement and refuses it. Here the base read sees only 'a', 's' and 'd',
/// all agreeing at 0.10/0.10mm, so it succeeds with no override at all.
#[test]
fn set_rt_off_on_the_one_key_holding_its_own_sensitivity_does_not_refuse() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, 0));
    }
    // The base read, 'w' excluded: the three keys left behind all agree at 0.10/0.10mm.
    for usage in [0x04u8, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::RT_PRESS, 100));
        lines.extend(layout_read_lines(usage, layout::RT_RELEASE, 100));
    }
    lines.extend(key_settings_lines(0x1A, 1000, 0x0231, 500, 500, 0, 0));

    let path = write_script("set-rt-off-own-sensitivity", &lines);
    let config_home = scratch_config_dir("set-rt-off-own-sensitivity");

    let out = run_wh(
        &["set", "rt", "--keys", "w", "--off", "--dry-run"],
        &path,
        &config_home,
    );
    assert!(
        out.status.success(),
        "the key being reset must not count as its own disagreement: stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // No source parenthetical: this value really was read from the board's own free keys, unlike
    // the override test below, and the sentence must not claim otherwise in either direction.
    assert!(
        stdout
            .lines()
            .any(|l| l
                == "rt: returning w to 0.10/0.10mm, mode Rt to Single, already in no rt keyset"),
        "unexpected stdout: {stdout}"
    );

    let mut expected: Vec<String> =
        cmds::write_key_records(&rt_off_value_records(0x0211, 1000, 100, 100))
            .iter()
            .map(|f| hex(f))
            .collect();
    expected.extend(
        cmds::write_key_records_singly(&rt_off_membership_records())
            .iter()
            .map(|f| hex(f)),
    );
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "unexpected frames: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The escape hatch the refusal above names, actually working: the same disagreeing board, with
/// `--press`/`--release` supplied, writes rather than refusing. The script carries no
/// press/release sweep at all, so an implementation that read the global anyway before preferring
/// the flags would hit an unscripted send and `ReplayTransport` would reject it.
#[test]
fn set_rt_off_with_press_and_release_writes_on_a_board_whose_free_keys_disagree() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, 0));
    }
    lines.extend(key_settings_lines(0x1A, 1000, 0x0231, 500, 500, 0, 0));
    lines.extend(auto_backup_lines_wasd(
        0,
        (1000, 0x0231, 500, 500, 0, 0),
        (1500, 0x10, 100, 100, 0, 0),
        (1500, 0x10, 100, 100, 0, 0),
        (1500, 0x10, 100, 100, 0, 0),
    ));
    lines.extend(write_lines(
        &rt_off_value_records(0x0211, 1000, 300, 400),
        &rt_off_membership_records(),
    ));
    lines.extend(key_settings_lines(0x1A, 1000, 0x0211, 300, 400, 0, 0));

    let path = write_script("set-rt-off-override", &lines);
    let config_home = scratch_config_dir("set-rt-off-override");

    let out = run_wh(
        &[
            "set",
            "rt",
            "--keys",
            "w",
            "--off",
            "--press",
            "0.3",
            "--release",
            "0.4",
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
    // "returning to" reads as a destination the board defines, which 0.30/0.40mm is not: nothing
    // was read to reach it. The parenthetical is asserted inside the whole line, so removing it
    // fails here and so does welding it onto the wrong clause.
    assert!(
        stdout.lines().any(|l| l
            == "rt: returning w to 0.30/0.40mm (from --press/--release, not the board's base), \
                mode Rt to Single, already in no rt keyset"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.lines().any(|l| l == "rt off: 1 key verified"),
        "unexpected stdout: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `--off` on a board where every key sits in a rapid trigger keyset: nothing is left to read a
/// global sensitivity from, so it refuses and names the override flags. The board shape this exact
/// sentence is true of is that one and only that one, every key holding a non-zero `0xFE`.
/// `rt_off_base` excludes the selection, so `NoneOutsideAKeyset` carries a second board shape too,
/// free keys existing while every one of them is selected, and that one has its own sentence and
/// its own test below. Asserting either here without saying which board it holds for would be an
/// assertion true only of its own fixture, which cannot catch a wrong-cause defect.
#[test]
fn set_rt_off_refuses_when_no_key_sits_outside_a_rapid_trigger_keyset() {
    let lines = rt_off_pre_plan_lines([1, 1], [(500, 500), (500, 500)], &[0x1A]);

    let path = write_script("set-rt-off-no-free-key", &lines);
    let config_home = scratch_config_dir("set-rt-off-no-free-key");

    let out = run_wh(&["set", "rt", "--keys", "w", "--off"], &path, &config_home);
    assert!(
        !out.status.success(),
        "expected a non-zero exit, got success with stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(
            "no key is outside a rapid trigger keyset, so there is no global sensitivity to \
             reset these to; pass --press and --release to say which value to use"
        ),
        "unexpected stderr: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The two-key board both whole-board tests below share: 'w' and 'a' are the only members of
/// rapid trigger keyset 1, so `--keys all` empties it and it ceases to exist. 'w' has rapid
/// trigger on with its own 0.50/0.50mm, 'a' is already off at MODE 0x10 and already at
/// 0.10/0.10mm, so exactly one touch nibble moves and exactly one key gets a value bundle. No
/// sensitivity sweep is scripted: a whole-board selection excludes every free key from the base
/// read, and these tests pass `--press`/`--release` instead.
fn rt_off_whole_board_reads() -> Vec<String> {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // keyset::read_membership's own matrix read
    lines.extend(layout_read_lines(0x1A, layout::KEYSET_RT, 1));
    lines.extend(layout_read_lines(0x04, layout::KEYSET_RT, 1));
    lines.extend(key_settings_lines(0x1A, 1000, 0x0231, 500, 500, 0, 1));
    lines.extend(key_settings_lines(0x04, 1500, 0x10, 100, 100, 0, 1));
    lines
}

/// The prompt both whole-board tests assert, in full. Held in one place because the two of them
/// assert it for opposite reasons, one that it was answered and one that it was refused, and a
/// wording change must not be able to satisfy one while the other quietly stops matching the
/// prompt at all.
const RT_OFF_WHOLE_BOARD_PROMPT: &str = "this selects every key on the board: every key moves to \
     0.10/0.10mm (from --press/--release, not the board's base), and rt keyset(s) 1 will cease to \
     exist, 1 key(s) have rapid trigger switched off";

/// `wh set rt --keys all --off` empties every rapid trigger keyset on the board, which before this
/// command wrote membership at all it could not do, so it takes the same typed `yes` the other
/// three whole-board routes take. The prompt goes to stderr and must not appear on stdout, the
/// hazard 2.25 measured: a redirected stdout would otherwise trap it in the file with nothing on
/// screen while the run blocks on stdin.
///
/// The whole prompt is asserted, not one clause: half of it could be deleted with a substring
/// assertion still green, which is a defect this project has already found by mutation once.
#[test]
fn set_rt_off_over_the_whole_board_requires_a_typed_yes() {
    let mut lines = rt_off_whole_board_reads();
    lines.extend(auto_backup_lines_rt_off(
        (1000, 0x0231, 500, 500, 0, 1),
        (1500, 0x10, 100, 100, 0, 1),
    ));
    // Only 'w' gets a value bundle: 'a' is already off and already at 0.10/0.10mm. Both get the
    // membership clear regardless, which is what empties keyset 1.
    lines.extend(write_lines(
        &rt_off_value_records(0x0211, 1000, 100, 100),
        &[
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
        ],
    ));
    lines.extend(key_settings_lines(0x1A, 1000, 0x0211, 100, 100, 0, 0));
    lines.extend(key_settings_lines(0x04, 1500, 0x10, 100, 100, 0, 0));

    let path = write_script("set-rt-off-whole-board-yes", &lines);
    let config_home = scratch_config_dir("set-rt-off-whole-board-yes");

    let out = run_wh_stdin(
        &[
            "set",
            "rt",
            "--keys",
            "all",
            "--off",
            "--press",
            "0.1",
            "--release",
            "0.1",
        ],
        &path,
        &config_home,
        "yes\n",
    );
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(RT_OFF_WHOLE_BOARD_PROMPT),
        "unexpected stderr: {stderr}"
    );
    // Both halves, matching every sibling route: the warning and the "type yes to continue: "
    // line are written separately, so asserting one leaves the other free to move to the wrong
    // stream or duplicate onto both with the suite green. Measured: a `println!` of the second
    // half onto stdout was caught by five tests across the other three routes and by none here.
    assert!(
        stderr.contains("type yes to continue"),
        "the prompt's second half must reach stderr: {stderr}"
    );
    assert!(
        !stdout.contains("this selects every key on the board")
            && !stdout.contains("type yes to continue"),
        "neither half of the prompt belongs on stdout: {stdout}"
    );
    assert!(
        stdout.lines().any(|l| {
            l
            == "rt: removing w from keyset 1, 0.50/0.50mm to 0.10/0.10mm (from --press/--release, \
                not the board's base), mode Rt to Single, keyset 1 ceases to exist"
        }),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.lines().any(|l| {
            l
            == "rt: removing a from keyset 1, 0.10/0.10mm to 0.10/0.10mm (from --press/--release, \
                not the board's base), keyset 1 ceases to exist"
        }),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.lines().any(|l| l == "rt off: 2 keys verified"),
        "unexpected stdout: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The negative half: anything but `yes` stops the run before the auto-backup, so the script below
/// carries no write frames at all. A guard that prompted and then wrote anyway would send an
/// unscripted frame and `ReplayTransport` would reject it, which is a second, independent proof
/// that nothing was written beyond the refusal sentence itself.
#[test]
fn set_rt_off_over_the_whole_board_refuses_anything_but_yes() {
    let path = write_script("set-rt-off-whole-board-no", &rt_off_whole_board_reads());
    let config_home = scratch_config_dir("set-rt-off-whole-board-no");

    let out = run_wh_stdin(
        &[
            "set",
            "rt",
            "--keys",
            "all",
            "--off",
            "--press",
            "0.1",
            "--release",
            "0.1",
        ],
        &path,
        &config_home,
        "no\n",
    );
    assert!(
        !out.status.success(),
        "expected a non-zero exit, got success with stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(RT_OFF_WHOLE_BOARD_PROMPT),
        "unexpected stderr: {stderr}"
    );
    assert!(
        stderr.contains("type yes to continue"),
        "the prompt's second half must reach stderr: {stderr}"
    );
    assert!(
        !stdout.contains("this selects every key on the board")
            && !stdout.contains("type yes to continue"),
        "neither half of the prompt belongs on stdout: {stdout}"
    );
    // The full sentence, subject included: "was not confirmed" alone is emitted by three other
    // whole-board guards, so it cannot tell this command's refusal from theirs.
    assert!(
        stderr.contains("rapid trigger off over the whole board was not confirmed"),
        "unexpected stderr: {stderr}"
    );
    // A refusal announces nothing: the per-key lines describe a write that is not happening.
    assert!(
        !stdout.contains("removing w from keyset 1"),
        "a refused run must announce nothing: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The second board shape behind `NoneOutsideAKeyset`, and the one `wh set rt --keys all --off`
/// always produces: free keys do exist, and every one of them is in this selection, so the base
/// read has nothing left to look at. Distinct from the "no key is outside a rapid trigger keyset"
/// board above, which has no free key at all, and it must say so rather than sending an operator
/// looking for keysets that are not there. Both keys here hold `0xFE = 0`.
///
/// This is also why the whole-board confirmation above needs `--press`/`--release` to be reachable:
/// without them the run refuses here, before any plan exists to confirm.
#[test]
fn set_rt_off_over_the_whole_board_says_every_free_key_is_in_the_selection() {
    let mut lines = matrix_lines(); // resolve_keys
    lines.extend(matrix_lines()); // keyset::read_membership's own matrix read
    lines.extend(layout_read_lines(0x1A, layout::KEYSET_RT, 0));
    lines.extend(layout_read_lines(0x04, layout::KEYSET_RT, 0));

    let path = write_script("set-rt-off-all-free-selected", &lines);
    let config_home = scratch_config_dir("set-rt-off-all-free-selected");

    let out = run_wh(
        &["set", "rt", "--keys", "all", "--off"],
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
        stderr.contains(
            "every key outside a rapid trigger keyset is also in this selection, so there is no \
             global sensitivity left to reset these to; pass --press and --release to say which \
             value to use"
        ),
        "unexpected stderr: {stderr}"
    );
    // The other cause's sentence must not be the one that came out: this board plainly has free
    // keys, and telling its operator there are none is the exact wrong-cause defect.
    assert!(
        !stderr.contains("no key is outside a rapid trigger keyset"),
        "wrong cause named: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `--off` resets both sensitivities together, so half an override is refused rather than silently
/// discarded: discarding it would read the board's own global instead and reset the keys to a value
/// the operator did not ask for, having just watched them type part of one. Refused before a
/// session opens, so the script is empty and no transport is touched.
///
/// Both directions, because they are separate match arms: a mutant that keeps one and drops the
/// other is exactly the shape a single-direction test cannot see.
#[test]
fn set_rt_off_refuses_half_an_override_in_either_direction() {
    for args in [
        ["set", "rt", "--keys", "w", "--off", "--press", "0.3"],
        ["set", "rt", "--keys", "w", "--off", "--release", "0.4"],
    ] {
        let tag = format!(
            "set-rt-off-half-override-{}",
            args[5].trim_start_matches('-')
        );
        let path = write_script(&tag, &[]);
        let config_home = scratch_config_dir(&tag);

        let out = run_wh(&args, &path, &config_home);
        assert!(
            !out.status.success(),
            "expected a non-zero exit for {args:?}, got success with stdout: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(
                "--off resets both sensitivities, so pass --press and --release together or \
                 neither; with neither, both come from the keys outside every rapid trigger \
                 keyset that this selection leaves behind"
            ),
            "unexpected stderr for {args:?}: {stderr}"
        );
        // Refused before anything opens a transport, so the run never even names one.
        assert!(
            !stderr.contains("transport:"),
            "a malformed invocation must be refused before a session opens: {stderr}"
        );

        std::fs::remove_file(path).unwrap();
        let _ = std::fs::remove_dir_all(&config_home);
    }
}

/// The corrupted-advanced-nibble sibling of the `set rt --set` test above, over the rapid trigger
/// off path: the scripted readback drops the advanced nibble (MODE 0x10 instead of the written
/// 0x11), which a verification checking only `!rt_enabled()` would wrongly pass. Asserts the
/// mismatch sentence `wh`'s own `report_verification` emits, not the bare word "mismatch", which
/// `ReplayTransport`'s own "send mismatch" would also satisfy on a broken fixture.
#[test]
fn set_rt_off_end_to_end_detects_a_corrupted_advanced_nibble_on_readback() {
    let mut lines = rt_off_pre_plan_lines([0, 0], [(100, 100), (100, 100)], &[0x1A]);
    // `plan`'s read of 'w': MODE 0x31 (touch Rt, advanced nibble 1), already at the global
    // sensitivity, so only the MODE nibble moves.
    lines.extend(key_settings_lines(0x1A, 1000, 0x31, 100, 100, 0, 0));
    lines.extend(auto_backup_lines_rt_off(
        (1000, 0x31, 100, 100, 0, 0),
        (1500, 0x10, 100, 100, 0, 0),
    ));
    lines.extend(write_lines(
        &rt_off_value_records(0x11, 1000, 100, 100),
        &rt_off_membership_records(),
    ));

    // The readback: MODE comes back 0x10, not the 0x11 that was written.
    lines.extend(key_settings_lines(0x1A, 1000, 0x10, 100, 100, 0, 0));

    let path = write_script("set-rt-off-nibble-mismatch", &lines);
    let config_home = scratch_config_dir("set-rt-off-nibble-mismatch");

    let out = run_wh(&["set", "rt", "--keys", "w", "--off"], &path, &config_home);
    assert!(
        !out.status.success(),
        "expected a non-zero exit, got success with stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(
            "readback mismatch on 1 key(s), backup retained, use `wh restore --last` to roll back"
        ),
        "unexpected stderr: {stderr}"
    );
    assert!(
        stderr.contains("w: board reports mode 0x0010 (rt off), wanted mode 0x0011 (rt off)"),
        "unexpected stderr: {stderr}"
    );

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
    // The confirmation is built after `plan`, matching `remove`, so its reads precede the
    // prompt even on the declined half: only the write that follows a `yes` is what the decline
    // never reaches.
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        decline_lines.extend(key_settings_lines(usage, 1200, 0x18, 100, 150, ks, 0));
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
    let decline_stdout = String::from_utf8_lossy(&decline_out.stdout);
    assert!(
        decline_err.contains("ap: this selection moves every key into one new keyset, keyset 3"),
        "got: {decline_err}"
    );
    assert!(
        decline_err.contains("ap keyset(s) 1, 2 will cease to exist, their members absorbed"),
        "got: {decline_err}"
    );
    assert!(
        decline_err.contains("wh set ap --base 1.50"),
        "got: {decline_err}"
    );
    // The full sentence, subject included: `wh keyset remove`, `wh keyset create` and
    // `wh set rt --off` all end their own refusals with "was not confirmed", so the tail alone
    // cannot tell this command's refusal from any of theirs.
    assert!(
        decline_err.contains("ap set over the whole board was not confirmed"),
        "got: {decline_err}"
    );
    // All four keys sit at MODE 0x18 here, none on touch nibble 0, so `moved_modes` is 0 and the
    // mode clause must be absent: an over-counting regression would otherwise tell the operator
    // keys are about to move off global travel when none are, a fabricated claim in the prompt.
    assert!(
        !decline_err.contains("move off global travel"),
        "got: {decline_err}"
    );
    // The negative half is what actually guards the split: asserting the prompt is present on
    // stderr does not stop a future change sending it to both streams, only this does. Checks
    // both the "type yes" line `confirm` itself prints and the warning text above it, since a
    // stray print of just the warning (not the "type yes" line) would slip past a check for
    // only one of the two.
    assert!(
        !decline_stdout.contains("type yes to continue")
            && !decline_stdout.contains("this selection moves every key into one new keyset"),
        "the prompt must not also reach stdout: got stdout: {decline_stdout}"
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
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 2), (0x16, 3), (0x07, 0)] {
        lines.extend(key_settings_lines(usage, 1200, 0x18, 100, 150, ks, 0));
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
        stderr.contains("ap: this selection moves every key into one new keyset, keyset 4"),
        "got: {stderr}"
    );
    assert!(
        stderr.contains("ap keyset(s) 1, 2, 3 will cease to exist, their members absorbed"),
        "got: {stderr}"
    );
    // Without this, a mutation that ignores `confirm`'s result and writes anyway still exits
    // non-zero here, since the unconfirmed write then runs into the exhausted decline script:
    // a status and prompt text that fire either way cannot tell a refusal from that accident.
    // The subject is asserted with it, since three other commands end a refusal the same way.
    assert!(
        stderr.contains("ap set over the whole board was not confirmed"),
        "got: {stderr}"
    );

    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Finding: the value clause and the keyset clause ("no ap keysets exist to lose") can both read
/// as a no-op on a board with no ap keysets at all, while `Change::ap`'s own promotion still
/// takes every free key off touch nibble 0 ("follow global travel") permanently. `plan` is the
/// only thing that knows how many; this pins the mode count actually reaching the operator, not
/// only that a prompt fires.
#[test]
fn set_ap_over_the_whole_board_names_the_mode_count_when_promoting_off_global_travel() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        // Touch nibble 0 (Global): `Change::ap` promotes every one of these to Single.
        lines.extend(key_settings_lines(usage, 2000, 0x00, 100, 150, 0, 0));
    }
    let path = write_script("set-ap-whole-board-mode-count", &lines);
    let config_home = scratch_config_dir("set-ap-whole-board-mode-count");
    let out = run_wh_stdin(
        &["set", "ap", "--keys", "all", "--set", "2.00"],
        &path,
        &config_home,
        "no\n",
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no ap keysets exist to lose"),
        "got: {stderr}"
    );
    assert!(
        stderr.contains("4 key(s) move off global travel onto their own actuation point"),
        "got: {stderr}"
    );
    assert!(stderr.contains("wh set ap --base 2.00"), "got: {stderr}");
    // Same reasoning as the test above: a status and prompt text that fire whatever the answer
    // cannot tell a refusal from the unconfirmed write hitting the exhausted decline script, and
    // the tail alone cannot tell this command's refusal from the three others that share it.
    assert!(
        stderr.contains("ap set over the whole board was not confirmed"),
        "got: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
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

/// The negative half is what actually guards the split: asserting the prompt is present on
/// stderr does not stop a future change sending it to both streams, only
/// `!stdout.contains(..)` does. `wh keyset remove`'s own sibling test
/// (`keyset_remove_prompt_goes_to_stderr_not_stdout`) exists for exactly this reason; this pins
/// the same fact for `wh set ap`.
#[test]
fn set_ap_over_the_whole_board_prompt_goes_to_stderr_not_stdout() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        lines.extend(key_settings_lines(usage, 1200, 0x18, 100, 150, ks, 0));
    }
    let path = write_script("set-ap-whole-board-prompt-stream", &lines);
    let config_home = scratch_config_dir("set-ap-whole-board-prompt-stream");
    let out = run_wh_stdin(
        &["set", "ap", "--keys", "all", "--set", "1.50"],
        &path,
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
    // Checks both the "type yes" line and the warning text above it: a stray print of just the
    // warning would slip past a check for only one of the two.
    assert!(
        !stdout.contains("type yes to continue")
            && !stdout.contains("this selection moves every key into one new keyset"),
        "the prompt must not also reach stdout: got stdout: {stdout}"
    );
    // Same reasoning as the other whole-board decline tests: a status that fires whatever the
    // answer cannot tell a refusal from the unconfirmed write hitting the exhausted decline
    // script, and the shared tail cannot tell this command's refusal from another's.
    assert!(
        stderr.contains("ap set over the whole board was not confirmed"),
        "got: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Finding: `ApMembership::Keep` is real behaviour with a real hazard shape (a whole-board
/// selection that is already exactly one keyset), and nothing pinned it. Measured: every key in
/// keyset 1, `--keys all --set 1.50`, `Stdio::null()` stdin, must still exit 0 and rewrite every
/// key's actuation point with no prompt on either stream, since nothing ceases to exist and no
/// new keyset is created. `ap_membership_for`'s `whole && taken.len() == usages.len()` is what
/// decides this, in a different module from the guard; this end-to-end test is what would notice
/// if a rewrite there, or in `confirm_whole_board_ap_set`'s own `Keep` check, started prompting
/// (or hanging) here instead.
#[test]
fn set_ap_over_the_whole_board_keep_does_not_prompt() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 1));
    }
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(key_settings_lines(usage, 1200, 0x18, 100, 150, 1, 0)); // plan's reads
    }
    lines.extend(auto_backup_lines_wasd(
        0,
        (1200, 0x18, 100, 150, 1, 0),
        (1200, 0x18, 100, 150, 1, 0),
        (1200, 0x18, 100, 150, 1, 0),
        (1200, 0x18, 100, 150, 1, 0),
    ));
    // `Keep` writes membership to no key at all (`index` is `None`), only the value records
    // every key's actuation point moving.
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
    for f in cmds::write_key_records(&value_records[..12]) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    for f in cmds::write_key_records(&value_records[12..]) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(key_settings_lines(usage, 1500, 0x18, 100, 150, 1, 0));
    }

    let path = write_script("set-ap-whole-board-keep", &lines);
    let config_home = scratch_config_dir("set-ap-whole-board-keep");
    let out = run_wh(
        &["set", "ap", "--keys", "all", "--set", "1.50"],
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
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("ap 1.50mm: 4 keys verified"),
        "got: {stdout}"
    );
    assert!(
        !stdout.contains("type yes to continue") && !stderr.contains("type yes to continue"),
        "Keep must never prompt: stdout {stdout}\nstderr {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}
/// `wh keyset create <kind> --keys all` moves every key on the board into one freshly allocated
/// index, so every existing keyset of that kind loses all of its members and ceases to exist.
/// `wh set ap --keys all` and `wh keyset remove --keys all` already ask for a typed `yes` before
/// the same destruction; this pins the third route to it.
///
/// Board: two keysets partition the whole four-key board, `w,a` in keyset 1 and `s,d` in keyset
/// 2, so both cease to exist and every member moves into a freshly allocated index (3, one past
/// the higher of the two).
#[test]
fn keyset_create_over_the_whole_board_requires_a_typed_yes() {
    let mut decline_lines = matrix_lines_wasd(); // resolve_keys
    decline_lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        decline_lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    // The confirmation is built after `plan`, matching `remove` and `set ap`, so its reads
    // precede the prompt even on the declined half: only the write that follows a `yes` is what
    // the decline never reaches.
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        decline_lines.extend(key_settings_lines(usage, 1200, 0x18, 100, 150, ks, 0));
    }
    let decline_script = write_script("keyset-create-whole-board-no", &decline_lines);
    let decline_config_home = scratch_config_dir("keyset-create-whole-board-no");
    let decline_out = run_wh_stdin(
        &["keyset", "create", "ap", "--keys", "all", "--value", "1.50"],
        &decline_script,
        &decline_config_home,
        "no\n",
    );
    assert!(!decline_out.status.success());
    let decline_err = String::from_utf8_lossy(&decline_out.stderr);
    let decline_stdout = String::from_utf8_lossy(&decline_out.stdout);
    assert!(
        decline_err
            .contains("ap: this selects every key on the board: every key moves into the new keyset 3 at 1.50mm"),
        "got: {decline_err}"
    );
    assert!(
        decline_err.contains("ap keyset(s) 1, 2 will cease to exist, their members absorbed"),
        "got: {decline_err}"
    );
    // The refusal sentence itself, not merely a non-zero status: an unconfirmed write that ran
    // anyway would hit the exhausted decline script and exit non-zero too, so a status check
    // alone cannot tell a real refusal from that accident.
    assert!(
        decline_err.contains("ap keyset creation over the whole board was not confirmed"),
        "got: {decline_err}"
    );
    // All four keys sit at MODE 0x18 (touch Single) here, none on touch nibble 0, so no key
    // moves off global travel and the mode clause must be absent: an over-counting regression
    // would otherwise fabricate movement in the prompt.
    assert!(
        !decline_err.contains("move off global travel"),
        "got: {decline_err}"
    );
    // A refusal writes nothing and announces nothing: `announce_steal` sits after the guard, so
    // stdout carries neither the announcement nor any part of the prompt.
    assert!(
        !decline_stdout.contains("type yes to continue")
            && !decline_stdout.contains("this selects every key on the board"),
        "the prompt must not also reach stdout: got stdout: {decline_stdout}"
    );
    assert!(
        !decline_stdout.contains("ap keyset 3: creating at"),
        "a refusal must announce nothing: got stdout: {decline_stdout}"
    );

    std::fs::remove_file(decline_script).unwrap();
    let _ = std::fs::remove_dir_all(&decline_config_home);

    // `yes` proceeds: the same board, but the whole pipeline this time, the auto-backup
    // snapshot, the actual write frames, and the readback verification.
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

    let accept_script = write_script("keyset-create-whole-board-yes", &accept_lines);
    let accept_config_home = scratch_config_dir("keyset-create-whole-board-yes");
    let accept_out = run_wh_stdin(
        &["keyset", "create", "ap", "--keys", "all", "--value", "1.50"],
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
        accept_stdout.contains("ap keyset create: 4 keys verified"),
        "got: {accept_stdout}"
    );

    std::fs::remove_file(accept_script).unwrap();
    let _ = std::fs::remove_dir_all(&accept_config_home);
}

/// The negative half is what actually guards the split: asserting the prompt is present on
/// stderr does not stop a future change sending it to both streams, only `!stdout.contains(..)`
/// does. Both halves of the prompt are checked on stdout, the warning text and the "type yes"
/// line `confirm` itself writes, since a stray `println!` of the warning alone would slip past a
/// check for only one of the two.
#[test]
fn keyset_create_over_the_whole_board_prompt_goes_to_stderr_not_stdout() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        lines.extend(key_settings_lines(usage, 1200, 0x18, 100, 150, ks, 0));
    }
    let path = write_script("keyset-create-whole-board-prompt-stream", &lines);
    let config_home = scratch_config_dir("keyset-create-whole-board-prompt-stream");
    let out = run_wh_stdin(
        &["keyset", "create", "ap", "--keys", "all", "--value", "1.50"],
        &path,
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
        stderr.contains("this selects every key on the board"),
        "the warning must reach stderr: got stderr: {stderr}"
    );
    assert!(
        !stdout.contains("type yes to continue")
            && !stdout.contains("this selects every key on the board"),
        "the prompt must not also reach stdout: got stdout: {stdout}"
    );
    // A status that fires whatever the answer cannot tell a refusal from the unconfirmed write
    // hitting the exhausted decline script, so the refusal sentence is pinned too.
    assert!(
        stderr.contains("ap keyset creation over the whole board was not confirmed"),
        "got: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `create rt --keys all` destroys every rapid trigger keyset by exactly the same mechanism, so
/// it asks too. It also switches rapid trigger on for every key that had it off, which the
/// prompt must count off `plan`: the keyset clause cannot say it, and on a board with no rt
/// keysets at all it would be the only thing moving.
///
/// The board mixes the three origins that matter, so the count is constrained rather than
/// coincidentally equal to the selection's size: `w` and `d` are already at touch nibble 3 (their
/// own rapid trigger) and do not move at all, `a` is at nibble 0 and `s` at nibble 1, both
/// measured as rapid trigger off. Two of four move, so a count read off the selection instead of
/// off `plan` reports 4 here and fails. MODE `0x10` is the commonest real value on this board and
/// the least covered by these fixtures, which is why `s` carries it.
#[test]
fn keyset_create_rt_over_the_whole_board_requires_a_typed_yes() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, ks));
    }
    // Touch nibble 3 on `w`/`d` (already their own rapid trigger, so no mode record at all),
    // nibble 0 on `a` and nibble 1 on `s`, both measured as rapid trigger off.
    for (usage, ks, mode) in [
        (0x1Au8, 1u16, 0x30u16),
        (0x04, 1, 0x00),
        (0x16, 2, 0x10),
        (0x07, 2, 0x30),
    ] {
        lines.extend(key_settings_lines(usage, 1200, mode, 100, 150, 0, ks));
    }
    let path = write_script("keyset-create-rt-whole-board-no", &lines);
    let config_home = scratch_config_dir("keyset-create-rt-whole-board-no");
    let out = run_wh_stdin(
        &[
            "keyset",
            "create",
            "rt",
            "--keys",
            "all",
            "--press",
            "0.30",
            "--release",
            "0.40",
        ],
        &path,
        &config_home,
        "no\n",
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stderr.contains(
            "rt: this selects every key on the board: every key moves into the new keyset 3 at \
             0.30/0.40mm"
        ),
        "got: {stderr}"
    );
    assert!(
        stderr.contains("rt keyset(s) 1, 2 will cease to exist, their members absorbed"),
        "got: {stderr}"
    );
    // The leading separator and the tail of the clause before it are part of the assertion: a
    // clause welded straight onto the keyset clause with no `, ` would otherwise pass.
    assert!(
        stderr.contains("their members absorbed, 2 key(s) have rapid trigger switched on"),
        "got: {stderr}"
    );
    // No key here came from nibble 2, so the sensitivity-source clause has nothing to count and
    // must be absent entirely: a count of `0`, or one read off the selection's size, fails here.
    assert!(
        !stderr.contains("move onto their own rapid trigger sensitivity"),
        "got: {stderr}"
    );
    assert!(
        stderr.contains("rt keyset creation over the whole board was not confirmed"),
        "got: {stderr}"
    );
    assert!(
        !stdout.contains("type yes to continue")
            && !stdout.contains("this selects every key on the board"),
        "the prompt must not also reach stdout: got stdout: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The other half of the rt mode split, on the board where no key was off to begin with. `a` is
/// at touch nibble 2, measured as rapid trigger already on but following the board's global
/// sensitivity, and `s` is at nibble 5, which nothing has measured at all. Both move onto the new
/// keyset's own sensitivity, and neither may be described as having rapid trigger switched on:
/// for `a` that is measurably false, and for `s` it would be an inference stated as a
/// measurement. `w` and `d` sit at nibble 3 and do not move.
///
/// Two of four move, so the count cannot come from the selection, and the switched-on clause has
/// nothing to count and must be absent entirely rather than reading `0`.
#[test]
fn keyset_create_rt_over_the_whole_board_does_not_claim_an_already_on_key_was_off() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, 0));
    }
    for (usage, mode) in [
        (0x1Au8, 0x30u16),
        (0x04, 0x20), // RtGlobal: rapid trigger already on, following the global sensitivity
        (0x16, 0x50), // Unknown(5): unmeasured, so no origin may be claimed for it
        (0x07, 0x30),
    ] {
        lines.extend(key_settings_lines(usage, 1200, mode, 100, 150, 0, 0));
    }
    let path = write_script("keyset-create-rt-whole-board-already-on", &lines);
    let config_home = scratch_config_dir("keyset-create-rt-whole-board-already-on");
    let out = run_wh_stdin(
        &[
            "keyset",
            "create",
            "rt",
            "--keys",
            "all",
            "--press",
            "0.30",
            "--release",
            "0.40",
        ],
        &path,
        &config_home,
        "no\n",
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(
            "rt: this selects every key on the board: every key moves into the new keyset 1 at \
             0.30/0.40mm"
        ),
        "got: {stderr}"
    );
    assert!(
        stderr.contains("no rt keysets exist to lose"),
        "got: {stderr}"
    );
    // Separator and preceding clause included, so the join is pinned and not just the wording.
    assert!(
        stderr.contains(
            "no rt keysets exist to lose, 2 key(s) move onto their own rapid trigger sensitivity"
        ),
        "got: {stderr}"
    );
    // The claim this test exists to forbid: neither key was off, and one of the two is a nibble
    // nothing has measured, so nothing may say rapid trigger is being switched on for either.
    assert!(
        !stderr.contains("rapid trigger switched on"),
        "got: {stderr}"
    );
    assert!(
        stderr.contains("rt keyset creation over the whole board was not confirmed"),
        "got: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The board the split exists for: both rapid trigger clauses non-zero at once, which neither of
/// the two tests above reaches. `w` at nibble 0 and `a` at nibble 1 are measured as rapid trigger
/// off and are being switched on; `s` at nibble 2 had it on already, following the board's global
/// sensitivity, and is only moving onto its own; `d` at nibble 3 does not move at all.
///
/// Asserted as one string covering both clauses and the separator that joins them, since three
/// separate `contains` checks would let the concatenation itself break: a second clause emitted
/// only when the first is empty prints the two switched-on keys and says nothing at all about
/// the third key moving, with every other assertion here still passing. The counts differ from
/// each other and from the selection's size (2, 1 and 4), so no clause can be reading the wrong
/// one of the three.
#[test]
fn keyset_create_rt_over_the_whole_board_names_both_mode_clauses_when_both_apply() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_RT, 1));
    }
    for (usage, mode) in [
        (0x1Au8, 0x00u16), // Global: rapid trigger off
        (0x04, 0x10),      // Single: rapid trigger off
        (0x16, 0x20),      // RtGlobal: on already, following the global sensitivity
        (0x07, 0x30),      // Rt: already its own, so no mode record at all
    ] {
        lines.extend(key_settings_lines(usage, 1200, mode, 100, 150, 0, 1));
    }
    let path = write_script("keyset-create-rt-whole-board-both-clauses", &lines);
    let config_home = scratch_config_dir("keyset-create-rt-whole-board-both-clauses");
    let out = run_wh_stdin(
        &[
            "keyset",
            "create",
            "rt",
            "--keys",
            "all",
            "--press",
            "0.30",
            "--release",
            "0.40",
        ],
        &path,
        &config_home,
        "no\n",
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(
            "rt: this selects every key on the board: every key moves into the new keyset 2 at \
             0.30/0.40mm"
        ),
        "got: {stderr}"
    );
    assert!(
        stderr.contains(
            "rt keyset(s) 1 will cease to exist, their members absorbed, 2 key(s) have rapid \
             trigger switched on, 1 key(s) move onto their own rapid trigger sensitivity"
        ),
        "got: {stderr}"
    );
    assert!(
        stderr.contains("rt keyset creation over the whole board was not confirmed"),
        "got: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// A board with no ap keysets at all: nothing ceases to exist, so the keyset clause reads as a
/// no-op, and every key already holds the requested 2.00mm, so no actuation point moves either.
/// What does move is every key's touch nibble, off 0 ("follow global travel") onto its own
/// pinned actuation point, permanently. Only `plan` knows that, which is why the mode count is
/// read off it; this pins both the "none to lose" wording and the count actually reaching the
/// operator on the board where they are the whole warning.
#[test]
fn keyset_create_over_the_whole_board_with_no_keysets_still_names_the_mode_count() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        // Touch nibble 0 (Global): `Change::ap` promotes every one of these to Single.
        lines.extend(key_settings_lines(usage, 2000, 0x00, 100, 150, 0, 0));
    }
    let path = write_script("keyset-create-whole-board-no-keysets", &lines);
    let config_home = scratch_config_dir("keyset-create-whole-board-no-keysets");
    let out = run_wh_stdin(
        &["keyset", "create", "ap", "--keys", "all", "--value", "2.00"],
        &path,
        &config_home,
        "no\n",
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr
            .contains("ap: this selects every key on the board: every key moves into the new keyset 1 at 2.00mm"),
        "got: {stderr}"
    );
    // One assertion over the join, not two over the halves: the separator between the keyset
    // clause and the mode clause is part of what the operator reads, and two independent
    // `contains` checks leave it free to disappear.
    assert!(
        stderr.contains(
            "no ap keysets exist to lose, 4 key(s) move off global travel onto their own \
             actuation point"
        ),
        "got: {stderr}"
    );
    assert!(
        stderr.contains("ap keyset creation over the whole board was not confirmed"),
        "got: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The regression guard for not over-triggering: a selection short of the whole matrix must
/// never prompt, even over a real write with no `yes` waiting on stdin (`run_wh`'s default,
/// `Stdio::null()`). A guard that mistakenly fired here would either hang reading an exhausted
/// stdin or refuse with "was not confirmed"; a clean, successful write rules out both.
#[test]
fn keyset_create_over_a_partial_selection_does_not_prompt() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for usage in [0x1Au8, 0x04, 0x16, 0x07] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    lines.extend(key_settings_lines(0x1A, 1200, 0x18, 100, 150, 0, 0)); // plan's read of w
    lines.extend(key_settings_lines(0x04, 1200, 0x18, 100, 150, 0, 0)); // plan's read of a
    lines.extend(auto_backup_lines_wasd(
        0,
        (1200, 0x18, 100, 150, 0, 0),
        (1200, 0x18, 100, 150, 0, 0),
        (1200, 0x18, 100, 150, 0, 0),
        (1200, 0x18, 100, 150, 0, 0),
    ));
    let value_records: Vec<KeyRecord> = [0x1Au8, 0x04]
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
    for f in cmds::write_key_records(&value_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    let membership_records: Vec<KeyRecord> = [0x1Au8, 0x04]
        .iter()
        .map(|&usage| KeyRecord {
            key: usage,
            layout: layout::KEYSET_AP,
            value: 1,
        })
        .collect();
    for f in cmds::write_key_records_singly(&membership_records) {
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    for usage in [0x1Au8, 0x04] {
        lines.extend(key_settings_lines(usage, 1500, 0x18, 100, 150, 1, 0));
    }
    let path = write_script("keyset-create-partial-no-prompt", &lines);
    let config_home = scratch_config_dir("keyset-create-partial-no-prompt");
    let out = run_wh(
        &["keyset", "create", "ap", "--keys", "w,a", "--value", "1.50"],
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
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("ap keyset create: 2 keys verified"),
        "got: {stdout}"
    );
    assert!(
        !stdout.contains("type yes to continue") && !stderr.contains("type yes to continue"),
        "a partial selection must never prompt: stdout {stdout}\nstderr {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `--dry-run` never prompts, on a board where the selection really does cover the matrix: it
/// writes nothing, so there is nothing to confirm yet. Empty stdin (`run_wh`'s default) would
/// hang the process if the guard fired here regardless of `--dry-run`, so a clean, successful
/// exit is itself the proof it did not.
#[test]
fn keyset_create_over_the_whole_board_does_not_prompt_on_dry_run() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 2)] {
        lines.extend(key_settings_lines(usage, 1200, 0x18, 100, 150, ks, 0));
    }
    let path = write_script("keyset-create-whole-board-dry-run", &lines);
    let config_home = scratch_config_dir("keyset-create-whole-board-dry-run");
    let out = run_wh(
        &[
            "keyset",
            "create",
            "ap",
            "--keys",
            "all",
            "--value",
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
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stdout.contains("dry run, no writes sent"), "got: {stdout}");
    assert!(
        !frame_lines(&stdout).is_empty(),
        "dry run must still print frames: {stdout}"
    );
    assert!(
        !stdout.contains("type yes to continue") && !stderr.contains("type yes to continue"),
        "dry run must never prompt: stdout {stdout}\nstderr {stderr}"
    );
    assert!(
        !stdout.contains("this selects every key on the board")
            && !stderr.contains("this selects every key on the board"),
        "dry run must never warn either: stdout {stdout}\nstderr {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The trigger is the resolved selection covering the board's matrix, never the literal string
/// `all`: spelling the same four keys out one by one must prompt identically. This is the
/// fixture that catches a rewrite checking `--keys` for the word `all` instead of comparing the
/// resolved usages against the membership read the command already performs.
///
/// Three keysets this time, each with a single member, plus one free key, so all three must be
/// named: the board this guards is 68 keys wide, and a prompt that dropped one from the list
/// would understate what is about to be lost.
#[test]
fn keyset_create_over_the_whole_board_spelled_out_key_by_key_still_prompts() {
    let mut lines = matrix_lines_wasd(); // resolve_keys
    lines.extend(matrix_lines_wasd()); // keyset::read_membership's own matrix read
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 2), (0x16, 3), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 2), (0x16, 3), (0x07, 0)] {
        lines.extend(key_settings_lines(usage, 1200, 0x18, 100, 150, ks, 0));
    }
    let path = write_script("keyset-create-whole-board-spelled-out", &lines);
    let config_home = scratch_config_dir("keyset-create-whole-board-spelled-out");
    let out = run_wh_stdin(
        &[
            "keyset", "create", "ap", "--keys", "w,a,s,d", "--value", "1.50",
        ],
        &path,
        &config_home,
        "no\n",
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr
            .contains("ap: this selects every key on the board: every key moves into the new keyset 4 at 1.50mm"),
        "got: {stderr}"
    );
    assert!(
        stderr.contains("ap keyset(s) 1, 2, 3 will cease to exist, their members absorbed"),
        "got: {stderr}"
    );
    assert!(
        stderr.contains("ap keyset creation over the whole board was not confirmed"),
        "got: {stderr}"
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
/// once (`--keys all`): the membership sweep, `plan`'s own six-layout read of each key, and
/// nothing else. A whole-board selection excludes every free key from the base read, so there is
/// no sensitivity sweep at all here and `--press`/`--release` is required; the script carries no
/// sweep, so an implementation that read one anyway would be rejected. A stray write or SAVE would
/// hit the exhausted script and `ReplayTransport` would reject it too.
///
/// Also pins that `--dry-run` never prompts, even over a whole-board selection that would
/// otherwise destroy every rapid trigger keyset: it writes nothing, so there is nothing to
/// confirm, and the run must not block on a stdin the harness closed.
#[test]
fn set_rt_off_dry_run_reads_matrix_and_mode_but_sends_no_write_or_save() {
    let mut lines = rt_off_pre_plan_lines([0, 0], [(100, 100), (100, 100)], &[0x1A, 0x04]);
    lines.extend(key_settings_lines(0x1A, 1000, 0x0231, 100, 100, 0, 0));
    lines.extend(key_settings_lines(0x04, 1500, 0x0037, 100, 100, 0, 0));
    let path = write_script("set-rt-off-dry-run", &lines);
    let config_home = scratch_config_dir("set-rt-off-dry-run");

    let out = run_wh(
        &[
            "set",
            "rt",
            "--keys",
            "all",
            "--off",
            "--press",
            "0.1",
            "--release",
            "0.1",
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
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stdout.contains("dry run"), "unexpected stdout: {stdout}");
    assert!(
        !stderr.contains("this selects every key on the board")
            && !stderr.contains("type yes to continue")
            && !stdout.contains("type yes to continue"),
        "a dry run writes nothing and must not prompt on either stream: {stderr}"
    );

    // The exact frame set, not just that each expected frame appears somewhere: pins that each
    // key's advanced nibble and high byte survive independently, and that each gets its own
    // `0xFE = 0` record after the value batch, one per frame.
    let mut expected: Vec<String> = cmds::write_key_records(&[
        KeyRecord {
            key: 0x1A,
            layout: layout::MODE,
            value: 0x0211,
        },
        KeyRecord {
            key: 0x1A,
            layout: layout::AP,
            value: 1000,
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
        KeyRecord {
            key: 0x04,
            layout: layout::MODE,
            value: 0x0017,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::AP,
            value: 1500,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x04,
            layout: layout::RT_RELEASE,
            value: 100,
        },
    ])
    .iter()
    .map(|f| hex(f))
    .collect();
    expected.extend(
        cmds::write_key_records_singly(&[
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
        ])
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

/// A key following the board's global rapid trigger (nibble 2) must still get a MODE frame
/// turning it off: before nibble 2 was modelled as `RtGlobal`, `ops::rt_off_records` had no match arm
/// for it, folded it into `Unknown`, and sent nothing at all for `wh set rt --off`.
/// `Change::rt_off`'s own `TouchChange::Off` carries the same nibble mapping.
#[test]
fn set_rt_off_turns_off_a_key_following_the_global_rapid_trigger() {
    let mut lines = rt_off_pre_plan_lines([0, 0], [(100, 100), (100, 100)], &[0x1A]);
    lines.extend(key_settings_lines(0x1A, 1000, 0x0220, 100, 100, 0, 0));
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
    // This is the only route from `wh set rt --off` into `announce_remove`'s mode-only branch,
    // the free key whose sensitivity does not move while its touch nibble does, so the sentence
    // is asserted here or nowhere. Whole line, including the ", already in no rt keyset" the mode
    // clause is joined to: a clause pinned by its own wording can be welded onto its neighbour.
    assert!(
        stdout
            .lines()
            .any(|l| l
                == "rt: w keeps 0.10/0.10mm, mode RtGlobal to Single, already in no rt keyset"),
        "unexpected stdout: {stdout}"
    );

    let mut expected: Vec<String> =
        cmds::write_key_records(&rt_off_value_records(0x0210, 1000, 100, 100))
            .iter()
            .map(|f| hex(f))
            .collect();
    expected.extend(
        cmds::write_key_records_singly(&rt_off_membership_records())
            .iter()
            .map(|f| hex(f)),
    );
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

/// A bare `wh set ap`, with neither `--keys`/`--pick` nor `--set`/`--base`, is refused by clap
/// itself before any session opens, naming both missing arguments in one error rather than
/// making the operator retry twice. `set` moved from a bare `f64` (clap-required, unconditionally)
/// to `Option<f64>` with `required_unless_present = "base"` so `--base` could make it optional;
/// this pins that the ordinary case (no `--base` at all) still gets clap's own required-argument
/// error, exit status 2, not a hand-rolled one from inside `run.rs`.
#[test]
fn set_ap_bare_invocation_names_both_missing_arguments() {
    let path = write_script("set-ap-bare", &[]);
    let config_home = scratch_config_dir("set-ap-bare");

    let out = run_wh(&["set", "ap"], &path, &config_home);
    assert!(!out.status.success());
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected clap's own exit status"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--keys") && stderr.contains("--set"),
        "expected both missing arguments named, got: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

// -- set ap --base: sets the board's base actuation point, keysets untouched --

/// The three DEFKEY roundtrips for a six-key board: 'w' (0x1A) and 'a' (0x04) in the first row
/// pair, a two-key `ap` keyset; 's' (0x16) and 'd' (0x07) in the second, free; 'e' (0x08) and 'b'
/// (0x05) in the third, also free. Shared by every `set ap --base` test below.
fn matrix_lines_base_board() -> Vec<String> {
    let mut lines = Vec::new();
    let row_pairs = [(0u8, 1u8), (2u8, 3u8), (4u8, 5u8)];
    for (i, &(a, b)) in row_pairs.iter().enumerate() {
        lines.push(out_line(&cmds::read_defkey_rows(a, b)));
        let payload = match i {
            0 => defkey_payload(a, b, Some(0x1A), Some(0x04)), // w, a: the keyset
            1 => defkey_payload(a, b, Some(0x16), Some(0x07)), // s, d: free
            _ => defkey_payload(a, b, Some(0x08), Some(0x05)), // e, b: free
        };
        lines.push(in_line(&reply(cmds::cmd::DEFKEY, &payload)));
    }
    lines
}

/// `keyset::read_membership`'s own matrix read plus its per-key `0xFF` sweep over the six-key
/// board above: 'w' and 'a' hold keyset 1, 's', 'd', 'e' and 'b' hold none.
fn base_board_membership_lines() -> Vec<String> {
    let mut lines = matrix_lines_base_board();
    lines.extend(layout_read_lines(0x1A, layout::KEYSET_AP, 1));
    lines.extend(layout_read_lines(0x04, layout::KEYSET_AP, 1));
    lines.extend(layout_read_lines(0x16, layout::KEYSET_AP, 0));
    lines.extend(layout_read_lines(0x07, layout::KEYSET_AP, 0));
    lines.extend(layout_read_lines(0x08, layout::KEYSET_AP, 0));
    lines.extend(layout_read_lines(0x05, layout::KEYSET_AP, 0));
    lines
}

/// `plan`'s own six-layout read of the four free keys, in matrix order: each at 2.00mm, MODE
/// `mode`, rt press/release 100/150, no keyset membership of either kind.
fn base_board_free_key_reads_at_mode(mode: u16) -> Vec<String> {
    let mut lines = Vec::new();
    for &usage in &[0x16u8, 0x07, 0x08, 0x05] {
        lines.extend(key_settings_lines(usage, 2000, mode, 100, 150, 0, 0));
    }
    lines
}

/// `base_board_free_key_reads_at_mode` at MODE 0x18 (Single, advanced 8), the shape most tests
/// below need: touch already off nibble 0, so `--base`'s promotion has nothing to move.
fn base_board_free_key_reads() -> Vec<String> {
    base_board_free_key_reads_at_mode(0x18)
}

/// The 16 value records `plan` writes for the four free keys moving to 1.95mm (1950um): each
/// key's MODE echoed back unchanged (0x18, since touch is already `Single`, not `Global`), AP at
/// the new base, and both rt sensitivities echoed back unchanged.
fn base_board_value_records(ap_um: u16) -> Vec<KeyRecord> {
    let mut records = Vec::new();
    for &usage in &[0x16u8, 0x07, 0x08, 0x05] {
        records.push(KeyRecord {
            key: usage,
            layout: layout::MODE,
            value: 0x18,
        });
        records.push(KeyRecord {
            key: usage,
            layout: layout::AP,
            value: ap_um,
        });
        records.push(KeyRecord {
            key: usage,
            layout: layout::RT_PRESS,
            value: 100,
        });
        records.push(KeyRecord {
            key: usage,
            layout: layout::RT_RELEASE,
            value: 150,
        });
    }
    records
}

/// `wh set ap --base 1.95 --dry-run` writes `0x04 = 1950` to the four free keys and no `0xFF`
/// record at all. Asserted by exact full-sequence frame equality: a selection that wrongly
/// wrote membership would append a trailing `0xFF` frame, which fails this exact comparison,
/// not just a "contains" check. A selection that wrongly included the two-key keyset fails this
/// test too, but earlier and elsewhere: measured, it fails at `ReplayTransport`'s own send
/// mismatch, reading a key's settings the script never scripted, before any frame is ever
/// printed for the comparison below to see.
#[test]
fn set_ap_base_writes_every_free_key_and_no_membership() {
    let mut lines = base_board_membership_lines();
    lines.extend(base_board_free_key_reads());

    let path = write_script("set-ap-base-dry", &lines);
    let config_home = scratch_config_dir("set-ap-base-dry");
    let out = run_wh(
        &["set", "ap", "--base", "1.95", "--dry-run"],
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

    // `plan.frames()` never splits one key's own group across a report boundary: 4 keys * 4
    // records is 16, over the 14-per-report limit, so it packs whole groups, 12 then 4, not the
    // vendor's own layout-major batching.
    let value_records = base_board_value_records(1950);
    let mut expected: Vec<String> = cmds::write_key_records(&value_records[0..12])
        .iter()
        .map(|f| hex(f))
        .collect();
    expected.extend(
        cmds::write_key_records(&value_records[12..16])
            .iter()
            .map(|f| hex(f)),
    );
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "must write only the four free keys' AP, no membership record: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Round 4's Finding 1: every `--base` fixture above has four free keys, so `key_or_keys(total)`
/// has never actually been asked for the singular. A two-key board, 'w' in a keyset and 'a' the
/// only free key, exercises it: the count must read "1 key", not "1 keys".
#[test]
fn set_ap_base_uses_the_singular_when_only_one_free_key_moves() {
    let mut lines = matrix_lines(); // keyset::read_membership's own matrix read
    lines.extend(layout_read_lines(0x1A, layout::KEYSET_AP, 1)); // w, keyset member
    lines.extend(layout_read_lines(0x04, layout::KEYSET_AP, 0)); // a, the only free key
    lines.extend(key_settings_lines(0x04, 2000, 0x18, 100, 150, 0, 0)); // plan's read of a

    let path = write_script("set-ap-base-singular", &lines);
    let config_home = scratch_config_dir("set-ap-base-singular");
    let out = run_wh(
        &["set", "ap", "--base", "1.95", "--dry-run"],
        &path,
        &config_home,
    );
    assert!(
        out.status.success(),
        "stdout: {}
stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("the actuation point outside every keyset moves to 1.95mm on 1 key,"),
        "got: {stdout}"
    );
    assert!(!stdout.contains("1 keys"), "got: {stdout}");

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// One free key's `(usage, ap, mode)` for building a mixed-state four-free-key fixture: rt
/// press/release fixed at 100/150 and no keyset membership of either kind, matching every other
/// `--base` fixture above. Lets the tests below give each of the four free keys its own starting
/// value and touch mode, rather than the uniform state `base_board_free_key_reads_at_mode` gives
/// all four.
fn base_board_free_key_reads_custom(specs: &[(u8, u16, u16)]) -> Vec<String> {
    let mut lines = Vec::new();
    for &(usage, ap, mode) in specs {
        lines.extend(key_settings_lines(usage, ap, mode, 100, 150, 0, 0));
    }
    lines
}

/// The full auto-backup snapshot read over the six-key base board, with the four free keys at
/// whatever `(usage, ap, mode)` `specs` gives them: sync, profile, global travel, matrix, then
/// six-layout reads for all six keys in matrix order ('w' and 'a' still holding keyset 1 at
/// 1.20mm/`0x18`, unaffected by `--base`).
fn auto_backup_lines_base_board_custom(specs: &[(u8, u16, u16)]) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(sync_lines("SNBASETEST0000001", "V1.0.0.001"));
    lines.extend(profile_lines(0));
    lines.extend(global_travel_lines(500, 200, 200));
    lines.extend(matrix_lines_base_board());
    lines.extend(key_settings_lines(0x1A, 1200, 0x18, 100, 150, 1, 0)); // w, keyset member
    lines.extend(key_settings_lines(0x04, 1200, 0x18, 100, 150, 1, 0)); // a, keyset member
    lines.extend(base_board_free_key_reads_custom(specs));
    lines
}

/// Round 2's Finding 1: the primary clause was built from `free.len()`, not from `plan`, so a
/// board where every free key already holds the target value still reported all four "moving",
/// while `plan` sent nothing at all (`plan.is_empty()`, so `apply` never calls `roundtrip_many`).
/// The fixture below has no write frames scripted at all, which is itself part of the proof: a
/// wrongly-reverted count would still pass this test on wording alone if the code actually did
/// send a frame the script never scripted, since `ReplayTransport` would refuse it outright.
#[test]
fn set_ap_base_does_not_claim_movement_when_every_free_key_already_holds_the_base() {
    let specs = [
        (0x16u8, 1950u16, 0x18u16),
        (0x07, 1950, 0x18),
        (0x08, 1950, 0x18),
        (0x05, 1950, 0x18),
    ];
    let mut lines = base_board_membership_lines();
    lines.extend(base_board_free_key_reads_custom(&specs));
    lines.extend(auto_backup_lines_base_board_custom(&specs));
    // No write frames at all: every free key already matches the target, so `plan` sends
    // nothing, and `apply` no-ops on an empty plan rather than calling `roundtrip_many`.
    // `verify_write_as`'s own readback of the four free keys, unchanged.
    lines.extend(base_board_free_key_reads_custom(&specs));

    let path = write_script("set-ap-base-nothing-moves", &lines);
    let config_home = scratch_config_dir("set-ap-base-nothing-moves");
    let out = run_wh(&["set", "ap", "--base", "1.95"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(
            "the actuation point outside every keyset already matches 1.95mm on 4 keys, \
             nothing written, keysets untouched"
        ),
        "got: {stdout}"
    );
    assert!(
        !stdout.contains("moves to 1.95mm"),
        "must not claim movement that did not happen: {stdout}"
    );
    // `plan.is_empty()` means no key got a record at all, so no touch mode moved either: this
    // is what proves the round-2 comment claiming `mode_clause` is always empty here, since a
    // board where it fired here would still pass on wording alone if this line were absent.
    assert!(!stdout.contains("global travel"), "got: {stdout}");
    assert!(stdout.contains("4 keys verified"), "got: {stdout}");

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Round 3's Finding 1: "nothing moves" is a claim about the whole write, not about `0x04`
/// alone, and it is false here. All four free keys start on touch nibble 0 ("follow global
/// travel") already holding the target value, so `ap_value_moved_count` is 0, but `Change::ap`'s
/// promotion still sends a full record bundle for every one of them, MODE included: two write
/// frames and sixteen records reach the board, permanently pinning every free key off global
/// travel. This fixture is also what Finding 2 needed and did not have: a key that gets an AP
/// record (echoed, unchanged) while its AP value does not move, the only shape that can tell
/// `ap_value_moved_count` apart from a naive count of keys `plan` sent an AP record for.
#[test]
fn set_ap_base_reports_the_mode_promotion_as_a_real_write_when_no_value_moves() {
    let specs = [
        (0x16u8, 2000u16, 0x00u16),
        (0x07, 2000, 0x00),
        (0x08, 2000, 0x00),
        (0x05, 2000, 0x00),
    ];
    let mut lines = base_board_membership_lines();
    lines.extend(base_board_free_key_reads_custom(&specs));
    lines.extend(auto_backup_lines_base_board_custom(&specs));

    // Each of the four: MODE 0x10 (Global promoted to Single, advanced 0), AP echoed 2000
    // (unchanged, already the target), RT press/release echoed 100/150.
    let value_records = [
        KeyRecord {
            key: 0x16,
            layout: layout::MODE,
            value: 0x10,
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
        KeyRecord {
            key: 0x07,
            layout: layout::MODE,
            value: 0x10,
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
        KeyRecord {
            key: 0x08,
            layout: layout::MODE,
            value: 0x10,
        },
        KeyRecord {
            key: 0x08,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x08,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x08,
            layout: layout::RT_RELEASE,
            value: 150,
        },
        KeyRecord {
            key: 0x05,
            layout: layout::MODE,
            value: 0x10,
        },
        KeyRecord {
            key: 0x05,
            layout: layout::AP,
            value: 2000,
        },
        KeyRecord {
            key: 0x05,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x05,
            layout: layout::RT_RELEASE,
            value: 150,
        },
    ];
    for batch in [&value_records[0..12], &value_records[12..16]] {
        for f in &cmds::write_key_records(batch) {
            lines.push(out_line(f));
            lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
        }
    }
    // verify_write_as's readback of the four free keys: MODE now 0x10, AP unchanged at 2000.
    for &usage in &[0x16u8, 0x07, 0x08, 0x05] {
        lines.extend(key_settings_lines(usage, 2000, 0x10, 100, 150, 0, 0));
    }

    let path = write_script("set-ap-base-mode-only-real-write", &lines);
    let config_home = scratch_config_dir("set-ap-base-mode-only-real-write");
    let out = run_wh(&["set", "ap", "--base", "2.00"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("no key outside a keyset changes its actuation point, keysets untouched"),
        "got: {stdout}"
    );
    assert!(
        !stdout.contains("nothing"),
        "a real write went out (two frames, sixteen records); must not claim nothing did: {stdout}"
    );
    assert!(
        stdout.contains("4 key(s) move off global travel onto their own actuation point"),
        "got: {stdout}"
    );
    assert!(stdout.contains("4 keys verified"), "got: {stdout}");

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The mixed case: two free keys already at the target, two not. The count in the headline must
/// be 2, not 4 (`free.len()`) and not 0, and only the two that actually move get a write.
#[test]
fn set_ap_base_names_a_mixed_move_count_when_only_some_free_keys_move() {
    let specs = [
        (0x16u8, 1950u16, 0x18u16), // s: already there, no record at all
        (0x07, 1950, 0x18),         // d: already there, no record at all
        (0x08, 2000, 0x18),         // e: moves
        (0x05, 2000, 0x18),         // b: moves
    ];
    let mut lines = base_board_membership_lines();
    lines.extend(base_board_free_key_reads_custom(&specs));

    let path = write_script("set-ap-base-mixed-move", &lines);
    let config_home = scratch_config_dir("set-ap-base-mixed-move");
    let out = run_wh(
        &["set", "ap", "--base", "1.95", "--dry-run"],
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
        stdout.contains(
            "the actuation point outside every keyset moves to 1.95mm on 2 of 4 keys; the \
             actuation point on the rest already matches 1.95mm"
        ),
        "got: {stdout}"
    );

    let value_records = [
        KeyRecord {
            key: 0x08,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x08,
            layout: layout::AP,
            value: 1950,
        },
        KeyRecord {
            key: 0x08,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x08,
            layout: layout::RT_RELEASE,
            value: 150,
        },
        KeyRecord {
            key: 0x05,
            layout: layout::MODE,
            value: 0x18,
        },
        KeyRecord {
            key: 0x05,
            layout: layout::AP,
            value: 1950,
        },
        KeyRecord {
            key: 0x05,
            layout: layout::RT_PRESS,
            value: 100,
        },
        KeyRecord {
            key: 0x05,
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
        "only the two keys that actually move get a record: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Round 4's Finding 4, checked against the shape it names explicitly: the mixed-move case
/// where a non-mover is also on touch nibble 0. `s` and `d` both keep their actuation point
/// (already at the target), but `s` still promotes off global travel while `d` does not; `e`
/// and `b` both move their actuation point, and `e` also promotes. The two clauses must stay
/// independent: "the rest already there" for the two non-movers, alongside a mode count of 2
/// (`s` and `e`) that includes one mover and one non-mover, neither implying the other skipped.
#[test]
fn set_ap_base_keeps_the_value_and_mode_clauses_independent_in_the_mixed_case() {
    let specs = [
        (0x16u8, 1950u16, 0x00u16), // s: keeps its ap, promotes off global travel
        (0x07, 1950, 0x18),         // d: keeps its ap, already off global travel
        (0x08, 2000, 0x00),         // e: moves, promotes off global travel
        (0x05, 2000, 0x18),         // b: moves, already off global travel
    ];
    let mut lines = base_board_membership_lines();
    lines.extend(base_board_free_key_reads_custom(&specs));

    let path = write_script("set-ap-base-mixed-both", &lines);
    let config_home = scratch_config_dir("set-ap-base-mixed-both");
    let out = run_wh(
        &["set", "ap", "--base", "1.95", "--dry-run"],
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
        stdout.contains(
            "the actuation point outside every keyset moves to 1.95mm on 2 of 4 keys; the \
             actuation point on the rest already matches 1.95mm, keysets untouched, 2 key(s) \
             move off global travel onto their own actuation point"
        ),
        "got: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Finding 5's coverage gap: the mode clause had an all-move and a none-move test but no mixed
/// one, where the sibling `--keys` path has all three
/// (`confirm_whole_board_ap_set_names_the_new_index_the_losing_keysets_the_mode_count_and_the_base_alternative`).
/// Two free keys start at touch nibble 0 and two do not; every key's own value still moves (kept
/// uniform here so this test is only about the mode count, not `--base`'s own value count above).
#[test]
fn set_ap_base_names_a_mixed_mode_count_when_only_some_free_keys_are_at_global_travel() {
    let specs = [
        (0x16u8, 2000u16, 0x00u16), // s: Global, promotes
        (0x07, 2000, 0x00),         // d: Global, promotes
        (0x08, 2000, 0x18),         // e: already Single
        (0x05, 2000, 0x18),         // b: already Single
    ];
    let mut lines = base_board_membership_lines();
    lines.extend(base_board_free_key_reads_custom(&specs));

    let path = write_script("set-ap-base-mixed-mode", &lines);
    let config_home = scratch_config_dir("set-ap-base-mixed-mode");
    let out = run_wh(
        &["set", "ap", "--base", "1.95", "--dry-run"],
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
        stdout.contains(
            "the actuation point outside every keyset moves to 1.95mm on 4 keys, keysets \
             untouched, 2 key(s) move off global travel onto their own actuation point"
        ),
        "got: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Every free key starts on touch nibble 0 ("follow global travel"), so `--base`'s promotion
/// moves all four of them onto their own pinned actuation point, exactly `Change::ap` already
/// does for every other actuation point write. The announcement must say so, the same defect
/// this project has killed three times over on the sibling `--keys` path: a silent mode change
/// with the operator told only that a depth moved.
#[test]
fn set_ap_base_names_the_mode_count_when_promoting_off_global_travel() {
    let mut lines = base_board_membership_lines();
    lines.extend(base_board_free_key_reads_at_mode(0x00));

    let path = write_script("set-ap-base-nibble0", &lines);
    let config_home = scratch_config_dir("set-ap-base-nibble0");
    let out = run_wh(
        &["set", "ap", "--base", "1.95", "--dry-run"],
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
        stdout.contains(", 4 key(s) move off global travel onto their own actuation point"),
        "expected the announcement to name the mode count, got: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The mirror of the test above: every free key already sits off touch nibble 0 (MODE 0x18,
/// `Single`), so `--base`'s promotion has nothing to move and the announcement must not claim it
/// did.
#[test]
fn set_ap_base_omits_the_mode_clause_when_no_free_key_is_at_global_travel() {
    let mut lines = base_board_membership_lines();
    lines.extend(base_board_free_key_reads_at_mode(0x18));

    let path = write_script("set-ap-base-no-nibble0", &lines);
    let config_home = scratch_config_dir("set-ap-base-no-nibble0");
    let out = run_wh(
        &["set", "ap", "--base", "1.95", "--dry-run"],
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
        !stdout.contains("global travel"),
        "did not expect a mode clause, got: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `--base` and `--keys` both name what to write, and disagree, so clap refuses before any
/// session opens: no replay script is ever read.
#[test]
fn set_ap_base_refuses_alongside_keys() {
    let path = write_script("set-ap-base-keys-conflict", &[]);
    let config_home = scratch_config_dir("set-ap-base-keys-conflict");

    let out = run_wh(
        &["set", "ap", "--base", "1.5", "--keys", "w"],
        &path,
        &config_home,
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--base") && stderr.contains("--keys"),
        "expected the refusal to name both flags, got: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `--base` names the board; `--set` names a value for a selection. The two disagree on what is
/// being set, so clap refuses before any session opens.
#[test]
fn set_ap_base_refuses_alongside_set() {
    let path = write_script("set-ap-base-set-conflict", &[]);
    let config_home = scratch_config_dir("set-ap-base-set-conflict");

    let out = run_wh(
        &["set", "ap", "--base", "1.5", "--set", "1.2"],
        &path,
        &config_home,
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--base") && stderr.contains("--set"),
        "expected the refusal to name both flags, got: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// A board with no key outside a keyset: `--base` has nothing to write, so it refuses up front
/// rather than sending nothing and reporting success.
#[test]
fn set_ap_base_refuses_when_every_key_is_in_a_keyset() {
    let mut lines = matrix_lines(); // keyset::read_membership's own matrix read
    lines.extend(layout_read_lines(0x1A, layout::KEYSET_AP, 1));
    lines.extend(layout_read_lines(0x04, layout::KEYSET_AP, 1));

    let path = write_script("set-ap-base-none-free", &lines);
    let config_home = scratch_config_dir("set-ap-base-none-free");
    let out = run_wh(&["set", "ap", "--base", "1.95"], &path, &config_home);
    assert!(
        !out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no key outside a keyset to write"),
        "got: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `keyset::read_membership`'s own matrix read plus its per-key `0xFF` sweep over a six-key
/// board with **no keyset at all**: every key holds membership `0`, so `--base`'s free set is
/// the whole matrix. Used only by the whole-board confirmation test below, where the free set
/// covering the whole matrix is the entire point: `confirm_whole_board_ap_set` only decides
/// whether to prompt after checking `usages.len() == m.entries().len()`, so a fixture with any
/// free key excluded (the two-key-keyset board above) would let a wrongly-wired call return
/// early on that check alone, without ever proving the call itself is absent.
fn no_keysets_board_membership_lines() -> Vec<String> {
    let mut lines = matrix_lines_base_board();
    for &usage in &[0x1Au8, 0x04, 0x16, 0x07, 0x08, 0x05] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, 0));
    }
    lines
}

/// `plan`'s own six-layout read of all six keys on the no-keysets board, matrix order: each at
/// 2.00mm, MODE 0x18 (Single, already off touch nibble 0), rt press/release 100/150, no keyset
/// membership of either kind.
fn no_keysets_board_key_reads() -> Vec<String> {
    let mut lines = Vec::new();
    for &usage in &[0x1Au8, 0x04, 0x16, 0x07, 0x08, 0x05] {
        lines.extend(key_settings_lines(usage, 2000, 0x18, 100, 150, 0, 0));
    }
    lines
}

/// The full auto-backup snapshot read over the no-keysets six-key board: sync, profile, global
/// travel, matrix, then the same six-layout reads as `no_keysets_board_key_reads`.
fn auto_backup_lines_no_keysets_board() -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(sync_lines("SNBASENOKS0000001", "V1.0.0.001"));
    lines.extend(profile_lines(0));
    lines.extend(global_travel_lines(500, 200, 200));
    lines.extend(matrix_lines_base_board());
    lines.extend(no_keysets_board_key_reads());
    lines
}

/// The 24 value records `plan` writes for all six keys on the no-keysets board moving to
/// `ap_um`: each key's MODE echoed back unchanged (0x18), AP at the new base, both rt
/// sensitivities echoed back unchanged.
fn no_keysets_board_value_records(ap_um: u16) -> Vec<KeyRecord> {
    let mut records = Vec::new();
    for &usage in &[0x1Au8, 0x04, 0x16, 0x07, 0x08, 0x05] {
        records.push(KeyRecord {
            key: usage,
            layout: layout::MODE,
            value: 0x18,
        });
        records.push(KeyRecord {
            key: usage,
            layout: layout::AP,
            value: ap_um,
        });
        records.push(KeyRecord {
            key: usage,
            layout: layout::RT_PRESS,
            value: 100,
        });
        records.push(KeyRecord {
            key: usage,
            layout: layout::RT_RELEASE,
            value: 150,
        });
    }
    records
}

/// `--base` never reaches Task 2's whole-board confirmation, even when its own free set covers
/// the entire matrix, the exact condition `confirm_whole_board_ap_set` checks for before it
/// decides whether to prompt at all: it writes no membership at all, so `ap_membership_for` is
/// not even in its path. Run with a null stdin (`run_wh`, not `run_wh_stdin`): if `--base`
/// wrongly called the guard here, EOF would read as a rejection and the run would fail, so
/// success is itself the proof, on a board built so the guard's own short-circuit could not
/// hide a wrongly-wired call.
#[test]
fn set_ap_base_does_not_prompt() {
    let mut lines = no_keysets_board_membership_lines();
    lines.extend(no_keysets_board_key_reads());
    lines.extend(auto_backup_lines_no_keysets_board());

    let value_records = no_keysets_board_value_records(1950);
    for batch in [&value_records[0..12], &value_records[12..24]] {
        for f in &cmds::write_key_records(batch) {
            lines.push(out_line(f));
            lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
        }
    }
    // verify_write_as's readback of all six keys, now at the new base.
    for &usage in &[0x1Au8, 0x04, 0x16, 0x07, 0x08, 0x05] {
        lines.extend(key_settings_lines(usage, 1950, 0x18, 100, 150, 0, 0));
    }

    let path = write_script("set-ap-base-no-prompt", &lines);
    let config_home = scratch_config_dir("set-ap-base-no-prompt");
    let out = run_wh(&["set", "ap", "--base", "1.95"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("verified"), "got: {stdout}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("moves every key into one new keyset"),
        "the whole-board ap-set prompt reached stderr: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

// --- write path: `set mm` -------------------------------------------------------------------

/// The full script for `wh set mm --value <mm>` against the two-key board: the standalone
/// pre-write read the announcement is built from, the auto-backup phase, the single write frame
/// and its ack, then the post-write readback `verify_write_as`'s sibling inside `set mm` sends.
/// `old_um`/`target_um`/`readback_um` let the happy-path and mismatch tests below share this
/// builder and diverge only on those three numbers.
fn set_mm_script(old_um: u16, target_um: u16, readback_um: u16) -> Vec<String> {
    let mut lines = Vec::new();
    // The pre-write read reports 0/0 for the dead zones, matching every measured board, so a
    // mutant that writes the read dead zones back instead of the vendor constants sends a frame
    // this script never scripted and fails on `ReplayTransport`'s own send mismatch.
    lines.extend(global_travel_lines(old_um, 0, 0));
    lines.extend(auto_backup_lines(0));

    let write = cmds::write_global_travel(Um(target_um), Um(200), Um(200));
    lines.push(out_line(&write));
    lines.push(in_line(&reply(cmds::cmd::DB, &[0x01, 0, 0])));

    // The dead zones read back as 0 on every measured board regardless of what was written, so
    // the readback fixture carries 0/0 here rather than echoing 200/200.
    lines.extend(global_travel_lines(readback_um, 0, 0));
    lines
}

/// `set mm --value 1.5` end to end against a board reading 0.90mm: the pre-write read, the
/// auto-backup, the write, and a readback that matches (1500um = 1.50mm). Exit 0, the whole
/// announcement line naming both values, "verified" in stdout, and a real backup file on disk.
#[test]
fn set_mm_end_to_end_backs_up_writes_and_verifies() {
    let path = write_script("set-mm-ok", &set_mm_script(900, 1500, 1500));
    let config_home = scratch_config_dir("set-mm-ok");

    let out = run_wh(&["set", "mm", "--value", "1.5"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout
            .lines()
            .any(|l| l == "mm custom value: 0.90mm -> 1.50mm"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("mm custom value: verified"),
        "unexpected stdout: {stdout}"
    );

    let backups = std::fs::read_dir(config_home.join("wh").join("backups"))
        .unwrap()
        .count();
    assert_eq!(backups, 1, "expected exactly one auto-backup file on disk");

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The mismatch twin of the test above: the board reads back 1400um (1.40mm) where 1500um
/// (1.50mm) was written. Non-zero exit, and the failure names both values, not just the word
/// "mismatch": `ReplayTransport`'s own violation wording also contains "mismatch", so a bare
/// `contains("mismatch")` cannot tell a real readback mismatch from a broken fixture.
#[test]
fn set_mm_end_to_end_reports_mismatch_on_readback() {
    let path = write_script("set-mm-mismatch", &set_mm_script(900, 1500, 1400));
    let config_home = scratch_config_dir("set-mm-mismatch");

    let out = run_wh(&["set", "mm", "--value", "1.5"], &path, &config_home);
    assert!(
        !out.status.success(),
        "expected a non-zero exit, got success with stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("1.40mm") && stderr.contains("1.50mm"),
        "the failure must name both mm values, got: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `wh set mm --value 1.5 --dry-run` prints the exact write frame and sends nothing: the script
/// carries only the standalone pre-write read, and any attempt to send the write itself or the
/// auto-backup's own board sweep would hit `ReplayTransport`'s own send mismatch.
#[test]
fn set_mm_dry_run_prints_the_frame_and_sends_no_write() {
    let lines = global_travel_lines(900, 0, 0);
    let path = write_script("set-mm-dry", &lines);
    let config_home = scratch_config_dir("set-mm-dry");

    let out = run_wh(
        &["set", "mm", "--value", "1.5", "--dry-run"],
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
        stdout
            .lines()
            .any(|l| l == "mm custom value: 0.90mm -> 1.50mm"),
        "unexpected stdout: {stdout}"
    );
    let expected = vec![hex(&cmds::write_global_travel(Um(1500), Um(200), Um(200)))];
    assert_eq!(
        frame_lines(&stdout),
        expected,
        "unexpected frame sequence: {stdout}"
    );
    assert!(
        stdout.contains("dry run, no writes sent"),
        "unexpected stdout: {stdout}"
    );

    let backups_dir = config_home.join("wh").join("backups");
    assert!(
        !backups_dir.exists() || std::fs::read_dir(&backups_dir).unwrap().count() == 0,
        "dry run must not create a backup"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// An out-of-range value (the record tops out at 4.00mm, this one asks for 4.5) must be refused
/// before a session ever opens, against a genuinely empty replay script: if the command sent
/// anything at all before finishing validation, `ReplayTransport` would reject the unexpected
/// send. Asserts the exact text `wh_proto::value::Um::from_mm`'s error produces, not a bare
/// "out of range", which a different refusal elsewhere in the codebase could also satisfy.
#[test]
fn set_mm_refuses_an_out_of_range_value_before_any_session_opens() {
    let path = write_script("set-mm-out-of-range", &[]);
    let config_home = scratch_config_dir("set-mm-out-of-range");

    let out = run_wh(&["set", "mm", "--value", "4.5"], &path, &config_home);
    assert!(
        !out.status.success(),
        "expected a non-zero exit, got success with stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("4.5mm is out of range (0mm to 4mm)"),
        "unexpected stderr: {stderr}"
    );
    // Refused before anything opens a transport, so the run never even names one.
    assert!(
        !stderr.contains("transport:"),
        "a malformed invocation must be refused before a session opens: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The label a real `wh set mm` run writes into the backup file it takes, read back off disk.
/// `every_backup_reason_renders_its_persisted_origin_string` (`run.rs`) only proves the string is
/// built correctly, not that this command's own call site reaches it: `BackupReason::SetMm` is
/// born with this tie, unlike the six variants `docs/tasks.md`'s closed 2.30 entry still lists
/// as untied.
#[test]
fn set_mm_end_to_end_records_its_own_command_as_the_backup_origin() {
    let path = write_script("set-mm-origin", &set_mm_script(900, 1500, 1500));
    let config_home = scratch_config_dir("set-mm-origin");

    let out = run_wh(&["set", "mm", "--value", "1.5"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(only_backup_origin(&config_home), "auto: set mm");

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// A no-op `set mm`, the board already at the target value, skips the write entirely: no backup,
/// no write frame, no readback, only the pre-write read the announcement needs. The script carries
/// nothing past that read, so a build that still takes a backup or sends the write would hit
/// `ReplayTransport`'s own send mismatch, not merely a wrong message.
#[test]
fn set_mm_skips_the_write_when_the_board_already_holds_the_target() {
    let path = write_script("set-mm-noop", &global_travel_lines(1500, 0, 0));
    let config_home = scratch_config_dir("set-mm-noop");

    let out = run_wh(&["set", "mm", "--value", "1.5"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout
            .lines()
            .any(|l| l == "mm custom value already matches 1.50mm, nothing written"),
        "unexpected stdout: {stdout}"
    );

    let backups_dir = config_home.join("wh").join("backups");
    assert!(
        !backups_dir.exists() || std::fs::read_dir(&backups_dir).unwrap().count() == 0,
        "a no-op set mm must not take a backup"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `restore`'s own preamble inside the session, before `auto_backup`: its independent profile
/// read, then the live matrix read whose usages the snapshot's keys are checked against. Every
/// restore fixture that gets past both refusals starts with these, so the two reads cannot drift
/// apart across the dozen scripts below.
fn restore_preamble_lines(profile_idx: u8) -> Vec<String> {
    let mut lines = profile_lines(profile_idx);
    lines.extend(matrix_lines());
    lines
}

/// A snapshot's JSON text for one key, 'w', with a caller-chosen `ap_mm` and `profile` (one-based,
/// or `None` for a snapshot with no recorded profile at all), so the out-of-range, happy-path,
/// and profile-safety restore tests below can all share it and diverge only on those two values.
fn restore_snapshot_json(ap_mm: f64, profile: Option<u8>) -> String {
    restore_snapshot_json_with_globals(ap_mm, profile, 2.0, 0.2, 0.1)
}

/// `restore_snapshot_json` with the whole global record chosen too: the custom value, for the test
/// that proves it reaches the wire from the file, and the dead zones, for the two that prove they
/// do not.
fn restore_snapshot_json_with_globals(
    ap_mm: f64,
    profile: Option<u8>,
    custom_value_mm: f64,
    press_dead_mm: f64,
    release_dead_mm: f64,
) -> String {
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
            custom_value_mm,
            press_dead_mm,
            release_dead_mm,
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
/// a matching readback. Shared by the six restore tests below that run all the way through, so
/// each restores identical snapshot content and diverges only on the fixture around it.
/// `membership` is `true` for a snapshot that recorded 'w' at keyset `0` (an explicit "no
/// keyset", `Some(0)`), which sends the two membership frames below, and `false` for one that
/// predates keyset recording at all (`None`), which must send neither: the fields differ only in
/// what `restore` knows about 'w's membership, not in what it reads back, so both call sites can
/// still share the same value batch and readback.
fn restore_write_and_verify_lines(membership: bool) -> Vec<String> {
    restore_write_and_verify_lines_at(membership, 2000)
}

/// `restore_write_and_verify_lines` with the custom value on the wire chosen too, in micrometres,
/// for the test that restores a snapshot recording something other than the 2.00mm every other
/// snapshot fixture in this file holds.
fn restore_write_and_verify_lines_at(membership: bool, custom_value_um: u16) -> Vec<String> {
    let mut lines = Vec::new();
    // The custom value is the snapshot's own; the dead zones are not, they are the 200um each
    // every measured vendor write carries, not the 0.2/0.1 the shared snapshot records.
    let db_write = cmds::write_global_travel(
        wh_proto::value::Um(custom_value_um),
        wh_proto::value::Um(200),
        wh_proto::value::Um(200),
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
///
/// Also the accept side of the matrix refusal for a snapshot covering fewer keys than the board:
/// it holds only 'w', the board has 'w' and 'a', and a check demanding the snapshot cover the
/// whole matrix would refuse here. 'a' is left alone, which `ReplayTransport` enforces rather
/// than an assertion: no frame addressing 'a' appears after the auto-backup's own reads.
#[test]
fn restore_happy_path_backs_up_and_verifies() {
    let config_home = scratch_config_dir("restore-happy");
    let snap_path = write_snapshot("restore-happy", 1.2, Some(1));

    // `restore` reads the board's profile as its own, independent roundtrip before ever calling
    // `auto_backup`, whose own `snapshot_from_device` pipeline reads it again internally; both
    // replies report the same board profile index 0 (UI profile 1), matching the snapshot.
    let mut lines = restore_preamble_lines(0);
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

/// `wh restore` must put the dead zones the vendor writes on the wire, 200um each, whatever the
/// snapshot recorded. Measured 2026-09-05 across every `cmd 0x29` frame in `captures/`: all three
/// vendor writes carry `press_dead=200` and `release_dead=200`, while all 14 reads report `0` for
/// both, so a restore built from what was read writes a pair the vendor never writes.
/// `restore_write_and_verify_lines` scripts the 200/200 frame and `ReplayTransport` matches byte
/// for byte, so sending the snapshot's own values fails on the send, not on an assertion here.
fn assert_restore_sends_the_vendor_dead_zones(tag: &str, press_dead_mm: f64, release_dead_mm: f64) {
    let config_home = scratch_config_dir(tag);
    let snap_path = std::env::temp_dir().join(format!("wh-{tag}-{}.json", std::process::id()));
    std::fs::write(
        &snap_path,
        restore_snapshot_json_with_globals(1.2, Some(1), 2.0, press_dead_mm, release_dead_mm),
    )
    .unwrap();

    let mut lines = restore_preamble_lines(0);
    lines.extend(auto_backup_lines(0));
    lines.extend(restore_write_and_verify_lines(true));
    let path = write_script(tag, &lines);

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

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The dead zones a snapshot taken off a real board actually holds: every measured read reports
/// `0` for both, so this is the case that made `wh restore` write `0, 0`.
#[test]
fn restore_sends_the_vendor_dead_zones_from_a_snapshot_recording_zeros() {
    assert_restore_sends_the_vendor_dead_zones("restore-dead-zones-zero", 0.0, 0.0);
}

/// Dead zones that are neither zero nor 200, so the values on the wire cannot be passing here by
/// happening to agree with the file: they do not come from the snapshot at all.
#[test]
fn restore_sends_the_vendor_dead_zones_from_a_snapshot_recording_other_values() {
    assert_restore_sends_the_vendor_dead_zones("restore-dead-zones-other", 0.35, 0.45);
}

/// The custom value on the wire is the one the snapshot recorded. Every other snapshot fixture in
/// this file records 2.00mm for it, so nothing else here can tell "the value flows from the file"
/// apart from "the value happens to be 2.00mm": measured, replacing the field read in
/// `snap_to_global` with `snap.global.custom_value_mm.max(2.0)` leaves the whole workspace green.
/// This snapshot records 0.10mm, which is what 13 of the 15 read replies in `captures/` report, so
/// the write must carry 100um. The defect it guards against is real and silent: a board on 0.10mm
/// backed up and restored as 2000um, with `wh` still printing a verified restore, since
/// `verify_restore` re-reads keys and never the `0x29` record.
#[test]
fn restore_sends_the_custom_value_the_snapshot_recorded() {
    let config_home = scratch_config_dir("restore-custom-value");
    let snap_path = std::env::temp_dir().join(format!(
        "wh-restore-custom-value-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &snap_path,
        restore_snapshot_json_with_globals(1.2, Some(1), 0.1, 0.0, 0.0),
    )
    .unwrap();

    let mut lines = restore_preamble_lines(0);
    lines.extend(auto_backup_lines(0));
    lines.extend(restore_write_and_verify_lines_at(true, 100));
    let path = write_script("restore-custom-value", &lines);

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

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// A stored JSON snapshot spelling the global field `travel_mm`, the name it had before it was
/// corrected to `custom_value_mm`. Real backups on the operator's disk are written that way, one of
/// which proved a destroy-and-restore hardware test, so the serde alias has to keep them restoring.
/// Hand-written rather than serialized, since no serializer can produce the old name any more, and
/// it restores the same values `restore_write_and_verify_lines` scripts, so the custom value has to
/// arrive intact for the global write to match rather than merely be tolerated by the parser.
#[test]
fn restore_from_a_snapshot_spelling_the_old_travel_mm_still_works() {
    let config_home = scratch_config_dir("restore-old-travel-mm");
    let snap_path = std::env::temp_dir().join(format!(
        "wh-restore-old-travel-mm-{}.json",
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
    { "name": "w", "usage": 26, "ap_mm": 1.2, "rt": true, "rt_press_mm": 0.5,
      "rt_release_mm": 0.6, "mode_raw": 544, "ap_keyset": 0, "rt_keyset": 0 }
  ]
}"#,
    )
    .unwrap();

    let mut lines = restore_preamble_lines(0);
    lines.extend(auto_backup_lines(0));
    lines.extend(restore_write_and_verify_lines(true));
    let path = write_script("restore-old-travel-mm", &lines);

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

    let mut lines = restore_preamble_lines(0);
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

    let mut lines = restore_preamble_lines(0);
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
    // The mismatch refusal's own full sentence, not a tail it shares with the no-recorded-profile
    // refusal below: matching only the profile numbers cannot tell the two refusals apart, and a
    // wrong-refusal defect would then pass.
    assert!(
        stderr.contains("snapshot was taken on profile 1 but the board is on profile 2"),
        "unexpected stderr: {stderr}"
    );

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The other refusal case: no recorded profile at all. Refused unconditionally, before
/// `auto_backup` or `ops::restore_all` ever run; same "script ends right after restore's own
/// direct profile read" reasoning as the mismatch test above.
#[test]
fn restore_refuses_a_snapshot_with_no_recorded_profile() {
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
    // This refusal's own full sentence, for the same reason the mismatch test asserts its own:
    // the two must stay distinguishable, and a bare "profile 1" matches both.
    assert!(
        stderr.contains(
            "snapshot has no recorded profile, so whether it belongs to the board's current \
             profile (profile 1) cannot be verified"
        ),
        "unexpected stderr: {stderr}"
    );
    assert!(
        !stderr.contains("was taken on profile"),
        "the mismatch refusal's wording must not reach the no-recorded-profile case: {stderr}"
    );
    // Neither dead cause survives: `--force` no longer exists, and no released `wh` ever wrote a
    // snapshot from before the profile field existed (it landed before the first release).
    assert!(
        !stderr.contains("--force") && !stderr.contains("predates"),
        "the refusal must name neither the removed flag nor the dead cause: {stderr}"
    );

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `--force` is gone, so `wh restore --force` must fail to parse rather than reach any of `wh`'s
/// own code. Asserts clap's unknown-argument wording, which only that path emits, and that none
/// of `restore`'s refusals ran: a bare non-zero exit would also be produced by the empty replay
/// script, and "--force" alone appears in a clap usage dump for a flag that still exists.
#[test]
fn restore_rejects_the_removed_force_flag_at_parse_time() {
    let config_home = scratch_config_dir("restore-force-removed");
    let snap_path = write_snapshot("restore-force-removed", 1.2, Some(1));
    let empty_replay = write_script("restore-force-removed", &[]);

    let out = run_wh(
        &["restore", snap_path.to_str().unwrap(), "--force"],
        &empty_replay,
        &config_home,
    );
    assert!(
        !out.status.success(),
        "expected a non-zero exit, got success with stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unexpected argument '--force' found"),
        "expected clap's own unknown-argument error: {stderr}"
    );
    assert!(
        !stderr.contains("snapshot has no recorded profile")
            && !stderr.contains("snapshot was taken on profile"),
        "parsing must fail before any of restore's own refusals run: {stderr}"
    );

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(empty_replay).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `restore`'s own direct profile read is a hard refusal on a wire index the board could never
/// report under the four measured profiles. `dump`, `backup` and `set`'s auto-backup now stop on
/// it too, through `snapshot_from_device`, but this is the separate read `restore` makes for its
/// own comparison, and it is asserted separately so neither can lose the stop on its own.
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

/// A snapshot taken on a different key matrix must be refused, not partly applied. The board here
/// has 'w' and 'a'; the snapshot holds 'w' and 's', so exactly one usage is absent, which pins
/// that the check is per usage rather than "the two matrices differ". The refusal names 's' and
/// counts it.
///
/// Nothing is written, and the backups directory being empty afterwards is the assertion that
/// pins it: the check sits before `auto_backup`, the last point at which nothing has happened.
/// The script deliberately carries the whole auto-backup phase past the matrix read, unused on a
/// correct run, so that a check moved after `auto_backup` still reaches its refusal and is caught
/// by the empty-directory assertion rather than by an exhausted script, which would be the same
/// failure a dozen unrelated defects produce. Everything after the auto-backup is still
/// unscripted, so any write frame fails on the send.
#[test]
fn restore_refuses_a_snapshot_carrying_a_usage_the_board_does_not_have() {
    let config_home = scratch_config_dir("restore-foreign-matrix");
    let snap_path =
        std::env::temp_dir().join(format!("wh-restore-foreign-{}.json", std::process::id()));
    std::fs::write(
        &snap_path,
        r#"{
  "firmware": "V1.0.0.001",
  "serial": "SNRESTORETEST001",
  "taken_at": "2026-08-28T12:00:00Z",
  "profile": 1,
  "global": { "custom_value_mm": 2.0, "press_dead_mm": 0.2, "release_dead_mm": 0.1 },
  "keys": [
    { "name": "w", "usage": 26, "ap_mm": 1.2, "rt": false, "rt_press_mm": 0.5,
      "rt_release_mm": 0.6, "mode_raw": 24, "ap_keyset": 0, "rt_keyset": 0 },
    { "name": "s", "usage": 22, "ap_mm": 1.2, "rt": false, "rt_press_mm": 0.5,
      "rt_release_mm": 0.6, "mode_raw": 24, "ap_keyset": 0, "rt_keyset": 0 }
  ]
}"#,
    )
    .unwrap();

    let mut lines = restore_preamble_lines(0);
    lines.extend(auto_backup_lines(0));
    let path = write_script("restore-foreign-matrix", &lines);
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
        stderr.contains("snapshot has 1 key this board does not have (s)"),
        "the refusal must name and count the absent usage: {stderr}"
    );
    assert!(
        stderr.contains("Take a fresh snapshot on this board"),
        "the refusal must say what the operator can do: {stderr}"
    );

    let backups = config_home.join("wh").join("backups");
    let count = std::fs::read_dir(&backups).map(|d| d.count()).unwrap_or(0);
    assert_eq!(
        count, 0,
        "the refusal sits before auto_backup, so no backup may exist: {backups:?}"
    );

    std::fs::remove_file(snap_path).unwrap();
    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The `snapshot_from_device` sibling of the restore profile-index refusal above: the same
/// out-of-range wire index (0xFE), reached through `wh backup`, must stop the command rather
/// than degrade to
/// `profile = None`. The profile is a value in `0..=3` on the wire and nothing else, so a board
/// reporting anything else is an error and `wh` goes no further: no snapshot file is written.
#[test]
fn backup_fails_and_writes_nothing_on_an_out_of_range_profile_index() {
    let mut lines = sync_lines("SNOUTOFRANGE0001", "V1.0.0.001");
    lines.extend(profile_lines(0xFE));
    // Nothing follows the profile read: the global travel, matrix and per-key reads a completed
    // backup would send have no script entry, so one reaching the wire fails on the send.

    let path = write_script("backup-out-of-range", &lines);
    let config_home = scratch_config_dir("backup-out-of-range");

    let out = run_wh(&["backup"], &path, &config_home);
    assert!(
        !out.status.success(),
        "an out-of-range profile index must stop the backup: stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("reading the board's active profile")
            && stderr.contains("board reported profile index 254"),
        "the failure must name the profile read and the offending index: {stderr}"
    );

    // `wh backup` with no `--to` writes into the store, so an empty (or absent) backups
    // directory is the proof nothing was written, not just the absence of a success message.
    let backups = config_home.join("wh").join("backups");
    let count = std::fs::read_dir(&backups).map(|d| d.count()).unwrap_or(0);
    assert_eq!(
        count, 0,
        "backup must write no file when it stops: {backups:?}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `wh dump` reaches `snapshot_from_device` too, and must stop on the same out-of-range index
/// rather than printing a warning and carrying on with an unknown profile. Its own test rather
/// than a rider on `backup`'s: `dump` writes nothing to disk, so "no file appeared" cannot stand
/// in for it, and it is the command whose degrading was visible to the operator.
///
/// Driven through `--table`, the only form that prints a profile line at all, against a script
/// carrying the whole dump and not just the profile read. Both are what make the stdout
/// assertion mean something: a dump that carried on would complete and print its table, so the
/// assertion fails on the defect it names rather than on an exhausted script. On a correct run
/// the frames after the profile read go unused.
#[test]
fn dump_fails_on_an_out_of_range_profile_index() {
    let mut lines = sync_lines("SNOUTOFRANGE0002", "V1.0.0.001");
    lines.extend(profile_lines(0xFE));
    lines.extend(global_travel_lines(500, 200, 200));
    lines.extend(matrix_lines());
    lines.extend(key_settings_lines(0x1A, 1200, 0x0230, 500, 500, 0, 0));
    lines.extend(key_settings_lines(0x04, 1500, 0x00, 0, 0, 0, 0));

    let path = write_script("dump-out-of-range", &lines);
    let config_home = scratch_config_dir("dump-out-of-range");

    let out = run_wh(&["dump", "--table"], &path, &config_home);
    assert!(
        !out.status.success(),
        "an out-of-range profile index must stop the dump: stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("reading the board's active profile")
            && stderr.contains("board reported profile index 254"),
        "the failure must name the profile read and the offending index: {stderr}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // The exact string `dump`'s `None` arm prints, so this cannot pass by naming a phrase the
    // code no longer emits. Nothing of the table may appear either: the run stops before the
    // first `writeln!`.
    assert!(
        !stdout.contains("profile unrecorded") && !stdout.contains("SNOUTOFRANGE0002"),
        "dump must stop rather than print an unrecorded profile and continue: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The distinction that justifies `DeviceError::ProfileOutOfRange` existing as its own variant,
/// separate from `DeviceError::Decode`: a profile reply that fails to decode for a reason other
/// than an out-of-range index (here, a payload too short to hold the index at all) fails
/// `backup` with its own distinct message, not the out-of-range one the test above covers.
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
        "a garbled profile reply must fail backup with its own decode message: \
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

/// `selftest` against the dead zones a real board reports. All 15 read replies in `captures/`
/// report `press_dead = 0` and `release_dead = 0`, a board state no other fixture in this file
/// scripts, and `selftest` rewrites the record with exactly what it read, so this is the one place
/// `wh` still puts a zero dead zone on the wire. Pinned by exact frame equality. The printed line
/// must name the custom value rather than calling it travel, and must not claim the write changes
/// nothing: whether the board holds a dead zone it never reports is unestablished, see
/// `docs/backlog.md`.
#[test]
fn selftest_on_a_board_reporting_zero_dead_zones_rewrites_them_as_read() {
    let mut lines = Vec::new();
    lines.extend(sync_lines("SNSELFTEST0000002", "V1.0.0.001"));
    lines.extend(global_travel_lines(100, 0, 0));
    let rewrite = cmds::write_global_travel(
        wh_proto::value::Um(100),
        wh_proto::value::Um(0),
        wh_proto::value::Um(0),
    );
    lines.push(out_line(&rewrite));
    lines.push(in_line(&reply(cmds::cmd::DB, &[0x01, 0, 0])));
    lines.extend(global_travel_lines(100, 0, 0));

    let path = write_script("selftest-zero-dead", &lines);
    let config_home = scratch_config_dir("selftest-zero-dead");
    let out = run_wh(&["selftest"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(
            "global custom value: 0.10mm, rewriting the record with the values just read"
        ),
        "the selftest line must name the custom value and what it rewrites: {stdout}"
    );
    assert!(
        stdout.contains("selftest OK: write path verified by rewriting the values just read"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        !stdout.contains("no-op"),
        "selftest must not claim the write changes nothing while the dead zones are unestablished: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// WSL only forwards an environment variable across the WSL/Windows boundary `bin/wh` execs
/// through when it is named in `WSLENV`; a `bin/wh` that forgot to set it once let a `wh restore`
/// silently fall back to a real device while the operator believed `WH_REPLAY` made it
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
            custom_value_mm: 2.0,
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
            custom_value_mm: 2.0,
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

    let mut lines = restore_preamble_lines(0);
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
            custom_value_mm: 2.0,
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
    let mut lines = restore_preamble_lines(0);
    lines.extend(auto_backup_lines(0));

    // Dead zones: the 200 each the vendor writes, not the 0.2/0.1 `snapshot_json_with_keysets`
    // records.
    let db_write = cmds::write_global_travel(
        wh_proto::value::Um(2000),
        wh_proto::value::Um(200),
        wh_proto::value::Um(200),
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
///
/// Also the accept side of the matrix refusal for a snapshot whose usages are exactly the
/// board's: both 'w' and 'a' are on the two-key board, so nothing is missing and the restore
/// runs end to end.
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
/// on the unscripted send rather than merely on an assertion below. Its global field spells
/// `travel_mm`, as a backup of that age does; the current name is `custom_value_mm`.
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

    let mut lines = restore_preamble_lines(0);
    // The auto-backup's own live read: 'w' holds ap keyset 4 and rt keyset 2, 'a' holds neither.
    lines.extend(sync_lines("SNWRITETEST00001", "V1.0.0.001"));
    lines.extend(profile_lines(0));
    lines.extend(global_travel_lines(500, 200, 200));
    lines.extend(matrix_lines());
    lines.extend(key_settings_lines(0x1A, 1000, 0x0220, 500, 500, 4, 2));
    lines.extend(key_settings_lines(0x04, 1500, 0x00, 0, 0, 0, 0));

    // `restore`'s own writes: global travel, then 'w's value batch. No membership frame at all.
    // The dead zones are the 200 each the vendor writes, not this snapshot's 0.2/0.1.
    let db_write = cmds::write_global_travel(
        wh_proto::value::Um(2000),
        wh_proto::value::Um(200),
        wh_proto::value::Um(200),
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
        stderr.contains("snapshot was taken on profile 1 but the board is on profile 2"),
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

/// The board's `0xbe` adjust-mode edge, as an `in_line`, built through the real frame encoder
/// like `reply` above rather than a hand-typed checksum.
fn adjust_edge_in_line(entering: bool) -> String {
    let sub = if entering { 0x00 } else { 0x01 };
    in_line(&reply(0x00, &[0x00, 0xbe, sub]))
}

/// A `0xbe` frame with an unmeasured third byte: still certainly unsolicited (`be_event`),
/// but neither measured edge, so it queues as `Unknown` rather than either note.
fn unmeasured_be_edge_in_line() -> String {
    in_line(&reply(0x00, &[0x00, 0xbe, 0x02]))
}

/// The exact stderr lines `with_session` prints for each edge kind, verbatim: the tests below
/// compare a whole line, not a substring, so a note wrapped in a prefix or suffix cannot pass.
const ADJUST_ENTERED_NOTE: &str = "note: the board entered its own adjust mode during this \
    command; settings may have changed underneath it";
const ADJUST_LEFT_NOTE: &str = "note: the board left its own adjust mode during this command; \
    settings may have changed underneath it";

/// How many lines of `text` equal `line` exactly, not merely contain it.
fn count_exact_lines(text: &str, line: &str) -> usize {
    text.lines().filter(|l| *l == line).count()
}

/// An 0xbe edge arriving mid-command surfaces as exactly one stderr note, the command's own
/// work is untouched, and stdout carries no trace of it.
#[test]
fn a_mid_command_adjust_edge_prints_one_stderr_note_and_changes_nothing_else() {
    let mut lines = matrix_lines();
    let mut key_lines = key_settings_lines(0x1A, 1200, 0x30, 400, 600, 0, 0);
    // Spliced between the AP read's own request and its scripted reply, an `in` line the
    // command never asked for, matching how the real board interleaves it mid-roundtrip.
    key_lines.insert(1, adjust_edge_in_line(true));
    lines.extend(key_lines);
    let path = write_script("adjust-note-mid-command", &lines);
    let config_home = scratch_config_dir("adjust-note-mid-command");

    let out = run_wh(&["get", "ap", "--keys", "w"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        count_exact_lines(&stderr, ADJUST_ENTERED_NOTE),
        1,
        "got: {stderr}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("w: ap 1.20mm keyset none"),
        "the command's own work must be untouched: {stdout}"
    );
    assert!(
        !stdout.contains("adjust mode"),
        "the note must stay off stdout: {stdout}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// Two edges of the same kind still print one note; both kinds print one each.
#[test]
fn repeated_edges_of_one_kind_print_once_and_both_kinds_print_one_each() {
    let mut lines = matrix_lines();
    let mut key_lines = key_settings_lines(0x1A, 1200, 0x30, 400, 600, 0, 0);
    // Inserted highest index first so earlier indices stay valid; because each insert shifts
    // everything after it, this lands as `be 00` before AP's reply, `be 00` before MODE's reply,
    // then `be 01` before RT_PRESS's reply, spliced across the six roundtrips
    // `read_key_settings` sends for 'w'.
    key_lines.insert(5, adjust_edge_in_line(false));
    key_lines.insert(3, adjust_edge_in_line(true));
    key_lines.insert(1, adjust_edge_in_line(true));
    lines.extend(key_lines);
    let path = write_script("adjust-note-repeated", &lines);
    let config_home = scratch_config_dir("adjust-note-repeated");

    let out = run_wh(&["get", "ap", "--keys", "w"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        count_exact_lines(&stderr, ADJUST_ENTERED_NOTE),
        1,
        "got: {stderr}"
    );
    assert_eq!(
        count_exact_lines(&stderr, ADJUST_LEFT_NOTE),
        1,
        "got: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The position of the one stderr line equal to `note`, so a test can compare which of two
/// notes printed first without caring how many other lines sit around them.
fn note_line_index(stderr: &str, note: &str) -> usize {
    stderr
        .lines()
        .position(|l| l == note)
        .unwrap_or_else(|| panic!("no line equal to {note:?} in: {stderr}"))
}

/// The ordering hazard: printing entered-then-left unconditionally would, on wire order `be 01`
/// then `be 00`, put "entered" last, claiming the board is still adjusting when it just left.
/// Ordered by each kind's own latest arrival instead, so the final line always matches the
/// board's most recent known edge.
#[test]
fn wire_order_left_then_entered_prints_the_left_note_first_and_entered_note_last() {
    let mut lines = matrix_lines();
    let mut key_lines = key_settings_lines(0x1A, 1200, 0x30, 400, 600, 0, 0);
    // Highest index first: `be 00` (entering) spliced before RT_RELEASE's reply, `be 01`
    // (leaving) spliced before AP's reply, so leaving arrives chronologically first.
    key_lines.insert(7, adjust_edge_in_line(true));
    key_lines.insert(1, adjust_edge_in_line(false));
    lines.extend(key_lines);
    let path = write_script("adjust-note-wire-order-left-first", &lines);
    let config_home = scratch_config_dir("adjust-note-wire-order-left-first");

    let out = run_wh(&["get", "ap", "--keys", "w"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let left_at = note_line_index(&stderr, ADJUST_LEFT_NOTE);
    let entered_at = note_line_index(&stderr, ADJUST_ENTERED_NOTE);
    assert!(
        left_at < entered_at,
        "left arrived first on the wire and must print first: got left at line {left_at}, \
         entered at line {entered_at}, stderr: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The mirror of the test above: wire order `be 00` then `be 01` prints entered first, left last.
#[test]
fn wire_order_entered_then_left_prints_the_entered_note_first_and_left_note_last() {
    let mut lines = matrix_lines();
    let mut key_lines = key_settings_lines(0x1A, 1200, 0x30, 400, 600, 0, 0);
    // Highest index first: `be 01` (leaving) spliced before RT_RELEASE's reply, `be 00`
    // (entering) spliced before AP's reply, so entering arrives chronologically first.
    key_lines.insert(7, adjust_edge_in_line(false));
    key_lines.insert(1, adjust_edge_in_line(true));
    lines.extend(key_lines);
    let path = write_script("adjust-note-wire-order-entered-first", &lines);
    let config_home = scratch_config_dir("adjust-note-wire-order-entered-first");

    let out = run_wh(&["get", "ap", "--keys", "w"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let entered_at = note_line_index(&stderr, ADJUST_ENTERED_NOTE);
    let left_at = note_line_index(&stderr, ADJUST_LEFT_NOTE);
    assert!(
        entered_at < left_at,
        "entered arrived first on the wire and must print first: got entered at line \
         {entered_at}, left at line {left_at}, stderr: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// `wh get ap` sends only cmd `0x22`/`0x23`, never cmd `0x00`, so a `be 02` frame there was always
/// skipped by the pre-existing mismatch arm; that fixture could not have pinned this. `wh profile`
/// with no argument sends the one bare cmd `0x00` roundtrip: this script used to die with an
/// opaque decode error, now it queues as `Unknown`, the command succeeds, and no note prints for it.
#[test]
fn a_cmd_zero_command_with_an_unmeasured_be_edge_succeeds_and_prints_no_note_for_it() {
    let mut lines = profile_lines(0); // board reports wire index 0, UI "profile 1"
    lines.insert(1, unmeasured_be_edge_in_line());
    let path = write_script("profile-unmeasured-be", &lines);
    let config_home = scratch_config_dir("profile-unmeasured-be");

    let out = run_wh(&["profile"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("profile 1"), "unexpected stdout: {stdout}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stdout.contains("adjust mode") && !stderr.contains("adjust mode"),
        "no note for an unmeasured edge on either stream: stdout: {stdout}\nstderr: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The negative half: a command whose script carries no edge prints no note.
#[test]
fn a_command_with_no_edges_prints_no_adjust_note() {
    let mut lines = matrix_lines();
    lines.extend(key_settings_lines(0x1A, 1200, 0x30, 400, 600, 0, 0));
    let path = write_script("adjust-note-absent", &lines);
    let config_home = scratch_config_dir("adjust-note-absent");

    let out = run_wh(&["get", "ap", "--keys", "w"], &path, &config_home);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("adjust mode"),
        "unexpected stderr: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}

/// The hazard a reviewer flagged: a command whose script carries an edge and then goes on to
/// fail (here, a readback mismatch) must still print the note. `with_session` computes the
/// result, drains the queue, prints, and only then returns the result, so the note must survive
/// an error path exactly as it does the happy path above.
#[test]
fn a_failing_command_still_prints_the_adjust_note() {
    let mut lines = set_ap_script(1100); // 1100um readback where 1200um was written: mismatch
                                         // Spliced before the script's very last reply, the readback of 'w's KEYSET_RT layout, an
                                         // `in` line the command never asked for.
    let last = lines.len() - 1;
    lines.insert(last, adjust_edge_in_line(true));
    let path = write_script("adjust-note-on-failure", &lines);
    let config_home = scratch_config_dir("adjust-note-on-failure");

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
        stderr.contains("1.10mm") && stderr.contains("1.20mm"),
        "the failure itself must still be reported: {stderr}"
    );
    assert_eq!(
        count_exact_lines(&stderr, ADJUST_ENTERED_NOTE),
        1,
        "the note must survive the error path: got: {stderr}"
    );

    std::fs::remove_file(path).unwrap();
    let _ = std::fs::remove_dir_all(&config_home);
}
