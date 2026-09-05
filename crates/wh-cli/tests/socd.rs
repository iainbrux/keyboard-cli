//! End-to-end tests of `wh socd list`, `pair`, and `unpair` over replay scripts.
//!
//! `ReplayTransport` matches each outgoing frame against the script byte for byte and rejects
//! anything else, on purpose: a query sent for a key that should never have been queried, or a
//! MODE write that clobbers a touch nibble, fails here as a send mismatch rather than as a wrong
//! message. Loosening that match to make a test pass would defeat the harness.

use std::process::Command;
use wh_device::replay::hex;
use wh_proto::cmds::{self, layout};

const W: u8 = 0x1A;
const A: u8 = 0x04;
const S: u8 = 0x16;
const D: u8 = 0x07;
const Q: u8 = 0x14;
const E: u8 = 0x08;

fn out_line(bytes: &[u8; 64]) -> String {
    format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(bytes))
}

fn in_line(bytes: &[u8; 64]) -> String {
    format!("{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(bytes))
}

/// Builds a reply frame the way the real device sends it, with the high bit set on the command
/// byte (see `wh_proto::frame::REPLY_BIT`), so fixtures built through this helper are faithful.
fn reply(cmd: u8, payload: &[u8]) -> [u8; 64] {
    wh_proto::frame::frame(cmd | wh_proto::frame::REPLY_BIT, payload).unwrap()
}

fn scratch_config_dir(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("wh-socd-it-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn write_script(tag: &str, lines: &[String]) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("wh-socd-{tag}-{}.jsonl", std::process::id()));
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

/// A DEFKEY reply payload for one row pair: `[rw, row_a, 21 usages, row_b, 21 usages]`.
fn defkey_payload(row_a: u8, row_b: u8, keys_a: &[u8], keys_b: &[u8]) -> Vec<u8> {
    let mut payload = vec![0u8; 45];
    payload[1] = row_a;
    payload[2..2 + keys_a.len()].copy_from_slice(keys_a);
    payload[23] = row_b;
    payload[24..24 + keys_b.len()].copy_from_slice(keys_b);
    payload
}

/// The three DEFKEY roundtrips `ops::read_matrix` sends, with `row0` and `row1` in the first row
/// pair and the remaining four rows empty, so the matrix reads back in exactly that order.
fn matrix_lines_for(row0: &[u8], row1: &[u8]) -> Vec<String> {
    let mut lines = Vec::new();
    for (i, &(a, b)) in [(0u8, 1u8), (2u8, 3u8), (4u8, 5u8)].iter().enumerate() {
        lines.push(out_line(&cmds::read_defkey_rows(a, b)));
        let payload = match i {
            0 => defkey_payload(a, b, row0, row1),
            _ => defkey_payload(a, b, &[], &[]),
        };
        lines.push(in_line(&reply(cmds::cmd::DEFKEY, &payload)));
    }
    lines
}

/// The six-key board every other script here runs against: w, a, s in the first row and d, q, e
/// in the second, so every sweep below runs in that order, w a s d q e.
fn matrix_lines() -> Vec<String> {
    matrix_lines_for(&[W, A, S], &[D, Q, E])
}

/// One `ops::read_layout_value` roundtrip.
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

/// One `cmd 0x2c` query roundtrip: the request for `usage`, and the board's reply naming
/// `partner` with the raw priority byte `prio`, in the board's own per-key spelling.
fn socd_query_lines(usage: u8, partner: u8, prio: u8) -> Vec<String> {
    vec![
        out_line(&wh_proto::socd::read_pairing(usage)),
        in_line(&reply(
            cmds::cmd::SOCD,
            &[0x00, usage, partner, 0, partner, usage, 0, 0, prio, 0],
        )),
    ]
}

/// One `cmd 0x2c` query roundtrip whose reply payload is given verbatim, for the fixtures that
/// need the board to answer with something the normal helper could not build.
fn socd_query_lines_raw(usage: u8, reply_payload: &[u8]) -> Vec<String> {
    vec![
        out_line(&wh_proto::socd::read_pairing(usage)),
        in_line(&reply(cmds::cmd::SOCD, reply_payload)),
    ]
}

/// One key on the fixture board: its MODE value, and, when its advanced nibble is `8`, the
/// partner and raw priority byte its own `cmd 0x2c` row answers with.
#[derive(Clone, Copy)]
struct Key {
    usage: u8,
    mode: u16,
    row: Option<(u8, u8)>,
}

fn key(usage: u8, mode: u16) -> Key {
    Key {
        usage,
        mode,
        row: None,
    }
}

fn paired(usage: u8, mode: u16, partner: u8, prio: u8) -> Key {
    Key {
        usage,
        mode,
        row: Some((partner, prio)),
    }
}

/// The whole `socd::read_socd` script: the matrix, one MODE read per key in matrix order, then
/// one `cmd 0x2c` query per key the fixture gives a row to.
///
/// A key with a `row` but no `8` nibble, or an `8` nibble and no `row`, would be a fixture bug;
/// the queries are driven by `row` alone so that a build which decided differently from the
/// fixture fails as a send mismatch rather than silently agreeing.
fn read_socd_lines(board: &[Key]) -> Vec<String> {
    let mut lines = matrix_lines();
    for k in board {
        lines.extend(layout_read_lines(k.usage, layout::MODE, k.mode));
    }
    for k in board {
        if let Some((partner, prio)) = k.row {
            lines.extend(socd_query_lines(k.usage, partner, prio));
        }
    }
    lines
}

fn sync_lines() -> Vec<String> {
    let mut payload = vec![0u8; 60];
    let s = b"SNSOCDTEST000001";
    payload[8] = s.len() as u8;
    payload[9..9 + s.len()].copy_from_slice(s);
    let f = b"V1.0.0.001";
    let fw_len_pos = 9 + s.len();
    payload[fw_len_pos] = f.len() as u8;
    payload[fw_len_pos + 1..fw_len_pos + 1 + f.len()].copy_from_slice(f);
    vec![
        out_line(&cmds::sync()),
        in_line(&reply(cmds::cmd::SYNC, &payload)),
    ]
}

fn profile_lines(idx: u8) -> Vec<String> {
    vec![
        out_line(&cmds::read_profile()),
        in_line(&reply(cmds::cmd::CMD, &[0x00, 0x70, idx, 0xFF])),
    ]
}

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

/// One key's six-layout `read_key_settings` script, in the order it issues the reads.
fn key_settings_lines(k: Key) -> Vec<String> {
    let mut lines = Vec::new();
    for (lid, val) in [
        (layout::AP, 1500u16),
        (layout::MODE, k.mode),
        (layout::RT_PRESS, 100),
        (layout::RT_RELEASE, 150),
        (layout::KEYSET_AP, 0),
        (layout::KEYSET_RT, 0),
    ] {
        lines.extend(layout_read_lines(k.usage, lid, val));
    }
    lines
}

/// The full `snapshot_from_device` script `auto_backup` sends against this six-key board.
fn auto_backup_lines(board: &[Key]) -> Vec<String> {
    let mut lines = sync_lines();
    lines.extend(profile_lines(3));
    lines.extend(global_travel_lines(1500, 0, 0));
    lines.extend(matrix_lines());
    for k in board {
        lines.extend(key_settings_lines(*k));
    }
    lines
}

/// True if `s` contains a run of at least `n` consecutive hex-digit characters anywhere in it.
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

fn frame_lines(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|l| contains_hex_run(l, 128))
        .map(str::to_string)
        .collect()
}

/// The origin recorded in the one backup a write command took, read back off disk.
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

fn assert_ok(out: &std::process::Output) {
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn cleanup(script: std::path::PathBuf, config_home: &std::path::Path) {
    std::fs::remove_file(script).unwrap();
    let _ = std::fs::remove_dir_all(config_home);
}

/// The board `socd-reload-read` measured, on this six-key matrix: W+S with S winning, Q+E on
/// last-input, each member answering with its own spelling (W says priority `2`, S says `1`),
/// plus `d` at advanced nibble `9`.
///
/// Nibble `9` is RS in the vendored docs (`research/kbdocs/keyboard/api/performance.md`) and has
/// never been observed on this board; the fixture exists to pin that a bit test would be wrong.
/// It shares bit 3 with SOCD, so a build testing `advanced & 8` instead of `== 8` queries `d` and
/// fails here as a send mismatch, on the wire and with stdout still empty. Measured: it never
/// reaches the point of naming `d` in the output, so weakening this to a string assertion about
/// `d` would test nothing.
fn two_pairs_board() -> [Key; 6] {
    [
        paired(W, 0x0008, S, 2),
        key(A, 0x0000),
        paired(S, 0x0018, W, 1),
        key(D, 0x0009),
        paired(Q, 0x0008, E, 0),
        paired(E, 0x0008, Q, 0),
    ]
}

/// `list` from the MODE sweep to the printed lines: two pairings, each discovered once even
/// though both members are queried, each printed with the winner rather than a priority byte,
/// and the RS-nibble key neither queried nor named.
#[test]
fn socd_list_reports_each_pairing_once_with_its_winner() {
    let board = two_pairs_board();
    let script = write_script("list-two", &read_socd_lines(&board));
    let config_home = scratch_config_dir("list-two");
    let out = run_wh(&["socd", "list"], &script, &config_home);
    assert_ok(&out);

    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        text, "socd pairs:\n  w + s, priority: s\n  q + e, priority: last-input\n2 pairs\n",
        "whole `wh socd list` output"
    );
    cleanup(script, &config_home);
}

/// A board with nothing paired says so in one line, and sends no `cmd 0x2c` at all: the script
/// ends after the MODE sweep, so a build that queried anyway would hit a send mismatch.
#[test]
fn socd_list_says_none_on_a_board_with_no_pairs() {
    let board = [
        key(W, 0x0000),
        key(A, 0x0010),
        key(S, 0x0000),
        key(D, 0x0009),
        key(Q, 0x0000),
        key(E, 0x0000),
    ];
    let script = write_script("list-none", &read_socd_lines(&board));
    let config_home = scratch_config_dir("list-none");
    let out = run_wh(&["socd", "list"], &script, &config_home);
    assert_ok(&out);

    assert_eq!(String::from_utf8_lossy(&out.stdout), "socd pairs: none\n");
    cleanup(script, &config_home);
}

/// A key flagged for SOCD whose partner is not flagged is an error naming both keys, not a
/// pairing quietly listed or quietly dropped. `w` claims `a` as its partner while `a`'s MODE
/// reads `0x0000`, so `a` is never queried and the two rows can never be reconciled.
#[test]
fn socd_list_refuses_a_pairing_whose_partner_is_not_flagged() {
    let board = [
        paired(W, 0x0008, A, 2),
        key(A, 0x0000),
        key(S, 0x0000),
        key(D, 0x0000),
        key(Q, 0x0000),
        key(E, 0x0000),
    ];
    let script = write_script("list-orphan", &read_socd_lines(&board));
    let config_home = scratch_config_dir("list-orphan");
    let out = run_wh(&["socd", "list"], &script, &config_home);
    assert!(!out.status.success(), "expected a refusal");

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "error: the board's SOCD state is inconsistent: w is paired with a, but a's mode \
             does not have SOCD set"
        ),
        "stderr: {err}"
    );
    cleanup(script, &config_home);
}

/// Both members flagged, but answering with different partners: `w` says `s` and `s` says `q`,
/// while `q` says `s` back. The two rows for the `w`/`s` claim disagree, which must stop the
/// command rather than let whichever row was read first win.
#[test]
fn socd_list_refuses_when_two_members_answer_differently() {
    let board = [
        paired(W, 0x0008, S, 2),
        key(A, 0x0000),
        paired(S, 0x0008, Q, 1),
        key(D, 0x0000),
        paired(Q, 0x0008, S, 2),
        key(E, 0x0000),
    ];
    let script = write_script("list-disagree", &read_socd_lines(&board));
    let config_home = scratch_config_dir("list-disagree");
    let out = run_wh(&["socd", "list"], &script, &config_home);
    assert!(!out.status.success(), "expected a refusal");

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "error: the board's SOCD state is inconsistent: w and s answer with different pairings"
        ),
        "stderr: {err}"
    );
    cleanup(script, &config_home);
}

/// An unpaired board, for the `pair` scripts.
fn free_board() -> [Key; 6] {
    [
        key(W, 0x0000),
        key(A, 0x0010),
        key(S, 0x0000),
        key(D, 0x0000),
        key(Q, 0x0000),
        key(E, 0x0000),
    ]
}

/// `wh socd pair a d --priority d` end to end: the pre-write read, the announcement, the backup,
/// the one `cmd 0x2c` frame, and the verification that re-reads both keys' MODE and re-queries
/// both rows. `a` and `d` are used rather than the captured W+S so the frame is an independent
/// encode, and the priority is not the default.
fn pair_a_d_lines() -> Vec<String> {
    let board = free_board();
    let mut lines = read_socd_lines(&board);
    lines.extend(auto_backup_lines(&board));
    let write = wh_proto::socd::write_pair(
        wh_proto::socd::Pairing::new(A, D, wh_proto::socd::Priority::Wins(D)).unwrap(),
    );
    lines.push(out_line(&write));
    lines.push(in_line(&reply(
        cmds::cmd::SOCD,
        &[0x00, A, D, 0, D, A, 0, 0, 2, 0],
    )));
    // The board sets the flag itself: `a` keeps its touch nibble 1 and gains advanced 8, `d`
    // goes from 0x0000 to 0x0008. Then each key's own row, `d`'s re-based to put `d` first.
    lines.extend(layout_read_lines(A, layout::MODE, 0x0018));
    lines.extend(socd_query_lines(A, D, 2));
    lines.extend(layout_read_lines(D, layout::MODE, 0x0008));
    lines.extend(socd_query_lines(D, A, 1));
    lines
}

#[test]
fn socd_pair_writes_one_frame_and_verifies_both_keys() {
    let script = write_script("pair-ad", &pair_a_d_lines());
    let config_home = scratch_config_dir("pair-ad");
    let out = run_wh(
        &["socd", "pair", "a", "d", "--priority", "d"],
        &script,
        &config_home,
    );
    assert_ok(&out);

    let text = String::from_utf8_lossy(&out.stdout);
    for line in [
        "socd: pairing a + d, priority: d\n",
        "socd: the board sets the SOCD mode flag on both keys itself, so no mode record is sent\n",
        "socd: a + d, priority: d verified, both keys report the SOCD mode flag\n",
    ] {
        assert!(text.contains(line), "missing {line:?} in: {text}");
    }
    cleanup(script, &config_home);
}

/// The backup a real `wh socd pair` run takes carries its own label, tying the new
/// `BackupReason` variant to the command path from the start.
#[test]
fn socd_pair_end_to_end_records_its_own_command_as_the_backup_origin() {
    let script = write_script("pair-origin", &pair_a_d_lines());
    let config_home = scratch_config_dir("pair-origin");
    let out = run_wh(
        &["socd", "pair", "a", "d", "--priority", "d"],
        &script,
        &config_home,
    );
    assert_ok(&out);
    assert_eq!(only_backup_origin(&config_home), "auto: socd pair");
    cleanup(script, &config_home);
}

/// `--dry-run` prints the exact frame and sends nothing: the script ends after the pre-write
/// read, so a build that took a backup or wrote would hit a send mismatch, not a wrong message.
#[test]
fn socd_pair_dry_run_sends_no_write() {
    let script = write_script("pair-dry", &read_socd_lines(&free_board()));
    let config_home = scratch_config_dir("pair-dry");
    let out = run_wh(
        &["socd", "pair", "a", "d", "--priority", "d", "--dry-run"],
        &script,
        &config_home,
    );
    assert_ok(&out);

    let text = String::from_utf8_lossy(&out.stdout);
    let want = hex(&wh_proto::socd::write_pair(
        wh_proto::socd::Pairing::new(A, D, wh_proto::socd::Priority::Wins(D)).unwrap(),
    ));
    assert_eq!(frame_lines(&text), vec![want]);
    assert!(text.contains("dry run, no writes sent\n"), "got: {text}");
    assert!(
        !config_home.join("wh").join("backups").exists(),
        "a dry run must take no backup"
    );
    cleanup(script, &config_home);
}

/// The winner is rendered from the key order given on the command line, so the same setting
/// spelled two ways produces the two different priority bytes the board expects. Both runs
/// announce `priority: s`; printing the raw byte instead would print `1` in one and `2` in the
/// other, and neither would be a key name.
#[test]
fn socd_pair_encodes_the_winner_from_both_orderings_of_the_command_line() {
    for (tag, first, second, want_payload) in [
        (
            "ws",
            "w",
            "s",
            [0x01u8, W, S, 0x00, S, W, 0x00, 0x00, 0x02, 0x00],
        ),
        (
            "sw",
            "s",
            "w",
            [0x01u8, S, W, 0x00, W, S, 0x00, 0x00, 0x01, 0x00],
        ),
    ] {
        let script = write_script(
            &format!("pair-order-{tag}"),
            &read_socd_lines(&free_board()),
        );
        let config_home = scratch_config_dir(&format!("pair-order-{tag}"));
        let out = run_wh(
            &[
                "socd",
                "pair",
                first,
                second,
                "--priority",
                "s",
                "--dry-run",
            ],
            &script,
            &config_home,
        );
        assert_ok(&out);

        let text = String::from_utf8_lossy(&out.stdout);
        let want = hex(&wh_proto::frame::frame(cmds::cmd::SOCD, &want_payload).unwrap());
        assert_eq!(frame_lines(&text), vec![want], "{tag}: frame");
        assert!(
            text.contains(&format!("socd: pairing {first} + {second}, priority: s\n")),
            "{tag}: announcement in {text}"
        );
        cleanup(script, &config_home);
    }
}

/// A key may sit in one pair only, so pairing an already-paired key is refused before any write,
/// naming the pairing that already holds it and the command that undoes it.
#[test]
fn socd_pair_refuses_a_key_that_is_already_paired() {
    let script = write_script("pair-taken", &read_socd_lines(&two_pairs_board()));
    let config_home = scratch_config_dir("pair-taken");
    let out = run_wh(&["socd", "pair", "s", "a"], &script, &config_home);
    assert!(!out.status.success(), "expected a refusal");

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "error: s is already in the SOCD pair w + s, priority: s; a key may sit in one pair \
             only, so run `wh socd unpair s` first"
        ),
        "stderr: {err}"
    );
    cleanup(script, &config_home);
}

/// `--priority` naming a key outside the pair is refused before anything is written. The script
/// stops after the board read, so a build that reached the write would fail as a send mismatch.
#[test]
fn socd_pair_refuses_a_priority_key_outside_the_pair() {
    let script = write_script("pair-badprio", &read_socd_lines(&free_board()));
    let config_home = scratch_config_dir("pair-badprio");
    let out = run_wh(
        &["socd", "pair", "w", "s", "--priority", "q"],
        &script,
        &config_home,
    );
    assert!(!out.status.success(), "expected a refusal");

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "error: --priority must name one of the two keys (w or s) or last-input, and 'q' is \
             neither"
        ),
        "stderr: {err}"
    );
    cleanup(script, &config_home);
}

/// The same key twice is refused in `wh`'s own words rather than clap's. The script stops after
/// the board read, so a build that reached the write would fail as a send mismatch.
#[test]
fn socd_pair_refuses_the_same_key_twice() {
    let script = write_script("pair-same", &read_socd_lines(&free_board()));
    let config_home = scratch_config_dir("pair-same");
    let out = run_wh(&["socd", "pair", "w", "w"], &script, &config_home);
    assert!(!out.status.success(), "expected a refusal");

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "error: wh socd pair needs two different keys, and 'w' names the same key twice"
        ),
        "stderr: {err}"
    );
    cleanup(script, &config_home);
}

/// `wh socd unpair q` on the Q+E pair, both members at touch nibble 0: two MODE writes clearing
/// the advanced nibble, no `cmd 0x2c` at all, then a re-read of each key.
fn unpair_qe_lines(dry_run: bool) -> Vec<String> {
    let board = two_pairs_board();
    let mut lines = read_socd_lines(&board);
    // plan_remove reads both keys' MODE again, in the pairing's own key order.
    lines.extend(layout_read_lines(Q, layout::MODE, 0x0008));
    lines.extend(layout_read_lines(E, layout::MODE, 0x0008));
    if dry_run {
        return lines;
    }
    lines.extend(auto_backup_lines(&board));
    for u in [Q, E] {
        let f = cmds::write_key_records_singly(&[cmds::KeyRecord {
            key: u,
            layout: layout::MODE,
            value: 0x0000,
        }])[0];
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    lines.extend(layout_read_lines(Q, layout::MODE, 0x0000));
    lines.extend(layout_read_lines(E, layout::MODE, 0x0000));
    lines
}

#[test]
fn socd_unpair_clears_the_flag_on_both_members() {
    let script = write_script("unpair-qe", &unpair_qe_lines(false));
    let config_home = scratch_config_dir("unpair-qe");
    let out = run_wh(&["socd", "unpair", "q"], &script, &config_home);
    assert_ok(&out);

    let text = String::from_utf8_lossy(&out.stdout);
    for line in [
        "socd: unpairing q + e, priority was last-input\n",
        "socd: clearing the SOCD mode flag on q and e, each key keeps its own touch mode\n",
        "socd: q + e unpaired, the SOCD mode flag is clear on both keys\n",
    ] {
        assert!(text.contains(line), "missing {line:?} in: {text}");
    }
    cleanup(script, &config_home);
}

#[test]
fn socd_unpair_end_to_end_records_its_own_command_as_the_backup_origin() {
    let script = write_script("unpair-origin", &unpair_qe_lines(false));
    let config_home = scratch_config_dir("unpair-origin");
    let out = run_wh(&["socd", "unpair", "q"], &script, &config_home);
    assert_ok(&out);
    assert_eq!(only_backup_origin(&config_home), "auto: socd unpair");
    cleanup(script, &config_home);
}

#[test]
fn socd_unpair_dry_run_sends_no_write() {
    let script = write_script("unpair-dry", &unpair_qe_lines(true));
    let config_home = scratch_config_dir("unpair-dry");
    let out = run_wh(&["socd", "unpair", "q", "--dry-run"], &script, &config_home);
    assert_ok(&out);

    let text = String::from_utf8_lossy(&out.stdout);
    let want: Vec<String> = [Q, E]
        .iter()
        .map(|&u| {
            hex(&cmds::write_key_records_singly(&[cmds::KeyRecord {
                key: u,
                layout: layout::MODE,
                value: 0x0000,
            }])[0])
        })
        .collect();
    assert_eq!(frame_lines(&text), want);
    assert!(
        !config_home.join("wh").join("backups").exists(),
        "a dry run must take no backup"
    );
    cleanup(script, &config_home);
}

/// The case the vendor never measured and `wh` decides: `w` sits at MODE `0x0018`, touch nibble
/// 1 (its own actuation point) over the SOCD nibble. The unpair must write `0x0010`, keeping the
/// touch nibble, not `0x0000`. Clobbering it would detach `w` from its own actuation point, and
/// the write frame here is byte-exact, so that build fails as a send mismatch.
///
/// The pair is named through `s`, its other member, so the plan is built from the pairing rather
/// than from the key the operator typed.
#[test]
fn socd_unpair_preserves_a_non_zero_touch_nibble() {
    let board = two_pairs_board();
    let mut lines = read_socd_lines(&board);
    lines.extend(layout_read_lines(W, layout::MODE, 0x0018));
    lines.extend(layout_read_lines(S, layout::MODE, 0x0008));
    lines.extend(auto_backup_lines(&board));
    for (u, value) in [(W, 0x0010u16), (S, 0x0000)] {
        let f = cmds::write_key_records_singly(&[cmds::KeyRecord {
            key: u,
            layout: layout::MODE,
            value,
        }])[0];
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    lines.extend(layout_read_lines(W, layout::MODE, 0x0010));
    lines.extend(layout_read_lines(S, layout::MODE, 0x0000));

    let script = write_script("unpair-touch1", &lines);
    let config_home = scratch_config_dir("unpair-touch1");
    let out = run_wh(&["socd", "unpair", "s"], &script, &config_home);
    assert_ok(&out);

    let text = String::from_utf8_lossy(&out.stdout);
    for line in [
        "socd: unpairing w + s, priority was s\n",
        "socd: clearing the SOCD mode flag on w and s, keeping w on its own actuation point \
         and s on the board's global travel\n",
        "socd: w + s unpaired, the SOCD mode flag is clear on both keys\n",
    ] {
        assert!(text.contains(line), "missing {line:?} in: {text}");
    }
    cleanup(script, &config_home);
}

/// The high finding of fix round 1, measured on a real build: `kp1` and `kp2` are in `wh-proto`'s
/// key table but not on this six-key board, and pairing them used to write the pairing, report it
/// verified, and leave a state `list` could not show and `unpair` could not undo. Both keys are
/// now refused against the board's own matrix, before any write, with the selector grammar's own
/// wording. The script stops after the board read.
#[test]
fn socd_pair_refuses_a_key_that_is_not_on_this_board() {
    let script = write_script("pair-offboard", &read_socd_lines(&free_board()));
    let config_home = scratch_config_dir("pair-offboard");
    let out = run_wh(&["socd", "pair", "kp1", "kp2"], &script, &config_home);
    assert!(!out.status.success(), "expected a refusal");

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("error: 'kp1' is not a key on this device"),
        "stderr: {err}"
    );
    cleanup(script, &config_home);
}

/// The worse half of the same finding: one real key and one absent one. Pairing `w` with `kp1`
/// used to leave every `wh socd` subcommand refusing with "kp1's mode does not have SOCD set", a
/// claim about a MODE that was never read, recoverable only through the vendor UI. The absent key
/// is refused instead, and the real one is never written.
#[test]
fn socd_pair_refuses_when_only_one_key_is_on_this_board() {
    let script = write_script("pair-halfoffboard", &read_socd_lines(&free_board()));
    let config_home = scratch_config_dir("pair-halfoffboard");
    let out = run_wh(&["socd", "pair", "w", "kp1"], &script, &config_home);
    assert!(!out.status.success(), "expected a refusal");

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("error: 'kp1' is not a key on this device"),
        "stderr: {err}"
    );
    cleanup(script, &config_home);
}

/// `wh socd list` prints an unnamed key as `0xA0`, and the README promises any printed hex can be
/// typed back into any selector. Going through `Selector` is what makes that true here: `unpair`
/// accepts the same spelling `list` printed. The board carries two usages with no name in
/// `wh-proto`'s table, paired with each other.
#[test]
fn socd_accepts_the_hex_form_list_itself_prints() {
    const U1: u8 = 0xA0;
    const U2: u8 = 0xA1;
    let board = [
        paired(U1, 0x0008, U2, 0),
        key(A, 0x0000),
        paired(U2, 0x0008, U1, 0),
        key(D, 0x0000),
        key(Q, 0x0000),
        key(E, 0x0000),
    ];
    let mut lines = matrix_lines_for(&[U1, A, U2], &[D, Q, E]);
    for k in board {
        lines.extend(layout_read_lines(k.usage, layout::MODE, k.mode));
    }
    for k in board {
        if let Some((partner, prio)) = k.row {
            lines.extend(socd_query_lines(k.usage, partner, prio));
        }
    }

    let list_script = write_script("hex-list", &lines);
    let config_home = scratch_config_dir("hex-list");
    let out = run_wh(&["socd", "list"], &list_script, &config_home);
    assert_ok(&out);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "socd pairs:\n  0xA0 + 0xA1, priority: last-input\n1 pair\n"
    );
    cleanup(list_script, &config_home);

    // The same spelling, straight back into `unpair`, as far as the plan's own MODE reads.
    let mut unpair_lines = lines.clone();
    unpair_lines.extend(layout_read_lines(U1, layout::MODE, 0x0008));
    unpair_lines.extend(layout_read_lines(U2, layout::MODE, 0x0008));
    let unpair_script = write_script("hex-unpair", &unpair_lines);
    let config_home = scratch_config_dir("hex-unpair");
    let out = run_wh(
        &["socd", "unpair", "0xA0", "--dry-run"],
        &unpair_script,
        &config_home,
    );
    assert_ok(&out);
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .contains("socd: unpairing 0xA0 + 0xA1, priority was last-input\n"),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    cleanup(unpair_script, &config_home);
}

/// A selector that resolves to several keys is refused for arity rather than silently taking the
/// first, which is also what keeps `all` from reaching a write: there is no whole-board form here
/// to need a typed-`yes` guard.
#[test]
fn socd_pair_refuses_a_selector_naming_more_than_one_key() {
    let script = write_script("pair-multi", &read_socd_lines(&free_board()));
    let config_home = scratch_config_dir("pair-multi");
    let out = run_wh(&["socd", "pair", "wasd", "q"], &script, &config_home);
    assert!(!out.status.success(), "expected a refusal");

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "error: wh socd pair's first key names exactly one key, but 'wasd' resolves to 4 \
             keys (w, a, s, d)"
        ),
        "stderr: {err}"
    );
    cleanup(script, &config_home);
}

/// The grammar's own "did you mean" hint reaches `wh socd`, rather than a second copy of it
/// living in this command tree.
#[test]
fn socd_pair_suggests_a_near_miss_key_name() {
    let script = write_script("pair-typo", &read_socd_lines(&free_board()));
    let config_home = scratch_config_dir("pair-typo");
    let out = run_wh(&["socd", "pair", "ss", "q"], &script, &config_home);
    assert!(!out.status.success(), "expected a refusal");

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("error: unknown key or group 'ss' (did you mean 's'?)"),
        "stderr: {err}"
    );
    cleanup(script, &config_home);
}

/// A key in no pair is refused, naming it, before anything is written. The script ends after the
/// board read, so a build that planned or wrote anyway would hit a send mismatch.
#[test]
fn socd_unpair_refuses_a_key_that_is_not_paired() {
    let script = write_script("unpair-free", &read_socd_lines(&two_pairs_board()));
    let config_home = scratch_config_dir("unpair-free");
    let out = run_wh(&["socd", "unpair", "a"], &script, &config_home);
    assert!(!out.status.success(), "expected a refusal");

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "error: a is not in any SOCD pair; run `wh socd list` to see the pairs on the board"
        ),
        "stderr: {err}"
    );
    cleanup(script, &config_home);
}

/// Naming both members of one pair unpairs it once, not twice: the script carries a single
/// pair's worth of writes, so a build that queued the pairing twice would run past its end.
#[test]
fn socd_unpair_names_one_pair_once_however_many_of_its_members_are_given() {
    let script = write_script("unpair-both", &unpair_qe_lines(false));
    let config_home = scratch_config_dir("unpair-both");
    let out = run_wh(&["socd", "unpair", "q", "e"], &script, &config_home);
    assert_ok(&out);

    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        text.matches("socd: q + e unpaired, the SOCD mode flag is clear on both keys\n")
            .count(),
        1,
        "got: {text}"
    );
    cleanup(script, &config_home);
}

/// The `pair` write path up to a point of the caller's choosing: board read, backup, the write
/// frame, and the echo the board sends back. `echo_prio` is the raw priority byte in that echo.
fn pair_a_d_prefix(echo_prio: u8) -> Vec<String> {
    let board = free_board();
    let mut lines = read_socd_lines(&board);
    lines.extend(auto_backup_lines(&board));
    lines.push(out_line(&wh_proto::socd::write_pair(
        wh_proto::socd::Pairing::new(A, D, wh_proto::socd::Priority::Wins(D)).unwrap(),
    )));
    lines.push(in_line(&reply(
        cmds::cmd::SOCD,
        &[0x00, A, D, 0, D, A, 0, 0, echo_prio, 0],
    )));
    lines
}

/// Guard 1 of 5. The five readback guards in `wh-device`'s SOCD path were all decorative in fix
/// round 0: prefixing every predicate with `false &&` left the whole workspace green, so nothing
/// would have noticed them being deleted, even though the extra roundtrips they cost were the
/// stated reason for verifying at all. One fixture each, from here down.
///
/// The board echoes the pair back with a different priority than the one written.
#[test]
fn socd_pair_refuses_an_echo_that_differs_from_what_was_written() {
    let script = write_script("pair-badecho", &pair_a_d_prefix(0));
    let config_home = scratch_config_dir("pair-badecho");
    let out = run_wh(
        &["socd", "pair", "a", "d", "--priority", "d"],
        &script,
        &config_home,
    );
    assert!(!out.status.success(), "expected a refusal");

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "error: the board's SOCD state is inconsistent: the board echoed a different SOCD \
             pairing than the one written: a + d, priority: last-input"
        ),
        "stderr: {err}"
    );
    cleanup(script, &config_home);
}

/// Guard 2 of 5. The echo is right, but the board did not set the SOCD flag it is measured to set
/// itself, so `pair`'s central claim ("the board sets the flag") is false on this board.
#[test]
fn socd_pair_refuses_when_the_board_did_not_set_the_mode_flag() {
    let mut lines = pair_a_d_prefix(2);
    lines.extend(layout_read_lines(A, layout::MODE, 0x0010));
    let script = write_script("pair-noflag", &lines);
    let config_home = scratch_config_dir("pair-noflag");
    let out = run_wh(
        &["socd", "pair", "a", "d", "--priority", "d"],
        &script,
        &config_home,
    );
    assert!(!out.status.success(), "expected a refusal");

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "error: the board's SOCD state is inconsistent: a reports mode 0x0010 after the pair \
             write, whose advanced nibble is 0 and not 8 (SOCD)"
        ),
        "stderr: {err}"
    );
    cleanup(script, &config_home);
}

/// Guard 3 of 5. The flag is set but the row the board now holds names a different partner, which
/// is the case the re-query exists for: the echo and the MODE read both look fine.
#[test]
fn socd_pair_refuses_when_the_row_read_back_names_another_pairing() {
    let mut lines = pair_a_d_prefix(2);
    lines.extend(layout_read_lines(A, layout::MODE, 0x0018));
    lines.extend(socd_query_lines(A, Q, 0));
    let script = write_script("pair-badrow", &lines);
    let config_home = scratch_config_dir("pair-badrow");
    let out = run_wh(
        &["socd", "pair", "a", "d", "--priority", "d"],
        &script,
        &config_home,
    );
    assert!(!out.status.success(), "expected a refusal");

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "error: the board's SOCD state is inconsistent: a reports the SOCD pairing a + q, \
             priority: last-input after writing a + d, priority: d"
        ),
        "stderr: {err}"
    );
    cleanup(script, &config_home);
}

/// Guard 4 of 5. The unpair's MODE write is acked but the key still reads the SOCD nibble, so the
/// flag the command says it cleared is still there.
#[test]
fn socd_unpair_refuses_when_the_flag_is_still_set_on_readback() {
    let mut lines = unpair_qe_lines(false);
    // Drop the two post-write MODE reads and put back a q that never changed.
    lines.truncate(lines.len() - 4);
    lines.extend(layout_read_lines(Q, layout::MODE, 0x0008));
    let script = write_script("unpair-stuck", &lines);
    let config_home = scratch_config_dir("unpair-stuck");
    let out = run_wh(&["socd", "unpair", "q"], &script, &config_home);
    assert!(!out.status.success(), "expected a refusal");

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "error: the board's SOCD state is inconsistent: q reports mode 0x0008 after the \
             unpair, wanted 0x0000 (SOCD nibble cleared, touch mode and high byte preserved)"
        ),
        "stderr: {err}"
    );
    cleanup(script, &config_home);
}

/// Guard 5 of 5, the one on the query itself. `Session::roundtrip` matches on the command byte
/// alone, so a stale `cmd 0x2c` reply would otherwise hand one key's pairing back for another's
/// query. Here the board answers `w`'s query with a well-formed row that starts at `s`.
#[test]
fn socd_refuses_a_query_reply_for_the_wrong_key() {
    let mut lines = matrix_lines();
    for k in two_pairs_board() {
        lines.extend(layout_read_lines(k.usage, layout::MODE, k.mode));
    }
    lines.extend(socd_query_lines_raw(
        W,
        &[0x00, S, W, 0, W, S, 0, 0, 0x01, 0],
    ));
    let script = write_script("query-wrongkey", &lines);
    let config_home = scratch_config_dir("query-wrongkey");
    let out = run_wh(&["socd", "list"], &script, &config_home);
    assert!(!out.status.success(), "expected a refusal");

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "error: the board's SOCD state is inconsistent: expected the SOCD pairing for w, got \
             a row starting at s"
        ),
        "stderr: {err}"
    );
    cleanup(script, &config_home);
}

/// An unmeasured touch nibble must not reach the operator as Rust tuple-variant syntax. `w` sits
/// at MODE `0x0058`: touch nibble 5, which no capture has ever shown, over the SOCD nibble. The
/// announcement has to say so in words, and the write still preserves the nibble, giving `0x0050`.
#[test]
fn socd_unpair_names_an_unmeasured_touch_mode_in_words() {
    let board = [
        paired(W, 0x0058, S, 2),
        key(A, 0x0000),
        paired(S, 0x0008, W, 1),
        key(D, 0x0009),
        key(Q, 0x0000),
        key(E, 0x0000),
    ];
    let mut lines = read_socd_lines(&board);
    lines.extend(layout_read_lines(W, layout::MODE, 0x0058));
    lines.extend(layout_read_lines(S, layout::MODE, 0x0008));
    lines.extend(auto_backup_lines(&board));
    for (u, value) in [(W, 0x0050u16), (S, 0x0000)] {
        let f = cmds::write_key_records_singly(&[cmds::KeyRecord {
            key: u,
            layout: layout::MODE,
            value,
        }])[0];
        lines.push(out_line(&f));
        lines.push(in_line(&reply(cmds::cmd::KEY, &[0x01])));
    }
    lines.extend(layout_read_lines(W, layout::MODE, 0x0050));
    lines.extend(layout_read_lines(S, layout::MODE, 0x0000));

    let script = write_script("unpair-nibble5", &lines);
    let config_home = scratch_config_dir("unpair-nibble5");
    let out = run_wh(&["socd", "unpair", "w"], &script, &config_home);
    assert_ok(&out);

    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains(
            "socd: clearing the SOCD mode flag on w and s, keeping w on an unmeasured mode (5) \
             and s on the board's global travel\n"
        ),
        "got: {text}"
    );
    // No Rust syntax anywhere in what the operator reads.
    assert!(!text.contains("Unknown("), "got: {text}");
    cleanup(script, &config_home);
}
