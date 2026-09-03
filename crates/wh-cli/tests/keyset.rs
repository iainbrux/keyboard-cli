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
/// `wh keyset list` never touches the config store, but `run_wh` always sets one, so each test
/// still gets its own rather than racing another test's over the same path.
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
