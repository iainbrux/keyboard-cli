//! End-to-end tests of `wh keyset list` over replay scripts.
//!
//! `ReplayTransport` matches each outgoing frame against the script byte for byte and rejects
//! anything else, on purpose: an unscripted, reordered, or otherwise-different send must fail
//! loudly. Loosening that match to make a test pass would defeat the harness.

use std::process::Command;
use wh_device::replay::hex;
use wh_proto::cmds::{self, layout};

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
/// `ops::read_layout_value` parses, matching what `keyset::read_membership` actually sends.
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

/// `wh keyset list ap` groups the board's 0xFF values into keysets and prints each one's members
/// by name. The script gives four keys, two of them at index 1 and one at index 2, so a
/// implementation that printed every non-zero key as its own keyset would fail here.
#[test]
fn keyset_list_ap_groups_members_by_index() {
    let mut lines = matrix_lines();
    for (usage, ks) in [(0x1Au8, 1u16), (0x04, 1), (0x16, 2), (0x07, 0)] {
        lines.extend(layout_read_lines(usage, layout::KEYSET_AP, ks));
    }
    // one read_key_settings per keyset, for the value column
    lines.extend(key_settings_lines(0x1A, 2000, 0x0018, 100, 100, 1, 0));
    lines.extend(key_settings_lines(0x16, 1200, 0x0018, 100, 100, 2, 0));
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
        !text.contains("d"),
        "key d holds 0 and is in no keyset: {text}"
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
