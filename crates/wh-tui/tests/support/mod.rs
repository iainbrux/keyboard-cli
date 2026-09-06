//! Fixture builders for `wh-tui`'s integration tests, feeding `ReplayTransport::from_jsonl`
//! directly rather than the binary. Copied from `crates/wh-cli/tests/dump.rs`'s pattern
//! (`out_line`, `in_line`, `reply`, `defkey_payload`, `matrix_lines`, `key_settings_lines`,
//! `sync_lines`, `profile_lines`, `global_travel_lines`, `build_script`), deliberately not
//! shared: the two suites must drift apart only loudly, by a fixture mismatch, never silently
//! through a shared helper edit.
//!
//! Each test binary in this crate compiles this whole module but uses only the subset its own
//! suite needs (`board.rs` reads the device, `chrome.rs` only holds a model to draw), so the
//! module is allowed dead code rather than pared down per binary.
#![allow(dead_code)]

use wh_device::ops::KeySettings;
use wh_device::replay::hex;
use wh_proto::cmds::{self, layout, DefKeyRow, GlobalTravel, Mode, ProfileNumber};
use wh_proto::value::Um;
use wh_tui::board::BoardModel;

pub fn out_line(bytes: &[u8; 64]) -> String {
    format!("{{\"dir\":\"out\",\"hex\":\"{}\"}}", hex(bytes))
}

pub fn in_line(bytes: &[u8; 64]) -> String {
    format!("{{\"dir\":\"in\",\"hex\":\"{}\"}}", hex(bytes))
}

/// Builds a reply frame the way the real device sends it: with the high bit set on the command
/// byte (see `wh_proto::frame::REPLY_BIT`), so fixtures built through this helper are faithful
/// to the wire.
pub fn reply(cmd: u8, payload: &[u8]) -> [u8; 64] {
    wh_proto::frame::frame(cmd | wh_proto::frame::REPLY_BIT, payload).unwrap()
}

/// The unsolicited adjust-mode edge frames, exactly as measured in docs/protocol.md.
pub fn adjust_edge_line(entering: bool) -> String {
    let third = if entering { 0x00 } else { 0x01 };
    in_line(&reply(0x00, &[0x00, 0xbe, third]))
}

/// A DEFKEY reply payload for one row pair: `[rw, row_a, 21 usages, row_b, 21 usages]`, with at
/// most the first column of each row populated. `None` leaves a row empty (no keys), which is
/// what the second and third row pairs of this two-key board need.
pub fn defkey_payload(row_a: u8, row_b: u8, a_col0: Option<u8>, b_col0: Option<u8>) -> Vec<u8> {
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

/// The three DEFKEY roundtrips that make up `ops::read_matrix_rows` for a two-key board ('w' at
/// usage 0x1A, 'a' at usage 0x04): only the first row pair carries keys, the other two are
/// empty.
pub fn matrix_lines() -> Vec<String> {
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
pub fn key_settings_lines(
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

/// The profile-read roundtrip `ops::profile` sends, as `[out, in]` lines: `idx` is the
/// zero-based index the board replies with.
pub fn profile_lines(idx: u8) -> Vec<String> {
    vec![
        out_line(&cmds::read_profile()),
        in_line(&reply(cmds::cmd::CMD, &[0x00, 0x70, idx, 0xFF])),
    ]
}

/// The SYNC roundtrip `ops::device_info` sends, as `[out, in]` lines: `serial` and `firmware`
/// are each written with the length prefix `cmds::parse_sync` reads back.
pub fn sync_lines(serial: &str, firmware: &str) -> Vec<String> {
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
/// travel/press-dead/release-dead values in micrometres.
pub fn global_travel_lines(travel_um: u16, press_um: u16, release_um: u16) -> Vec<String> {
    let mut payload = [0u8; 9];
    payload[3..5].copy_from_slice(&travel_um.to_le_bytes());
    payload[5..7].copy_from_slice(&press_um.to_le_bytes());
    payload[7..9].copy_from_slice(&release_um.to_le_bytes());
    vec![
        out_line(&cmds::read_global_travel()),
        in_line(&reply(cmds::cmd::DB, &payload)),
    ]
}

/// A `BoardModel` literal with two keys and no wire, for chrome tests: those exercise `draw` and
/// `App`'s own state, not the read path, so they need a model to hold rather than a `Session` to
/// read one from. Mirrors `app::tests::test_fixture` in shape, not shared: that one is
/// `pub(crate)` to `wh-tui`'s own crate and unreachable from this external test crate.
#[allow(dead_code)]
pub fn two_key_board() -> BoardModel {
    BoardModel {
        serial: "SNTUITEST0000001".to_string(),
        firmware: "V1.0.0.001".to_string(),
        profile: ProfileNumber::from_wire_index(0).unwrap(),
        global: GlobalTravel {
            travel: Um(500),
            press_dead: Um(200),
            release_dead: Um(200),
        },
        rows: Vec::new(),
        keys: vec![
            KeySettings {
                usage: 0x1A,
                ap: Um(1200),
                mode: Mode::from_value(0x0010),
                rt_press: Um(500),
                rt_release: Um(500),
                ap_keyset: 0,
                rt_keyset: 0,
            },
            KeySettings {
                usage: 0x04,
                ap: Um(1500),
                mode: Mode::from_value(0x0010),
                rt_press: Um(0),
                rt_release: Um(0),
                ap_keyset: 0,
                rt_keyset: 0,
            },
        ],
    }
}

/// A `BoardModel` literal with real `rows` (unlike `two_key_board`, whose `rows` is empty),
/// for `matrix.rs`: the wasd shape from `dump.rs`'s `matrix_lines_wasd`, 'w' (0x1A) and 'a'
/// (0x04) in the first row pair, 's' (0x16) and 'd' (0x07) in the second, one key per row, all
/// in column 0. `w`'s AP is 2000 (2.00mm), the value the matrix tests assert against.
#[allow(dead_code)]
pub fn wasd_board() -> BoardModel {
    let key = |usage: u8, ap: u16, rt_keyset: u16| KeySettings {
        usage,
        ap: Um(ap),
        mode: Mode::from_value(0x0010),
        rt_press: Um(500),
        rt_release: Um(500),
        ap_keyset: 0,
        rt_keyset,
    };
    BoardModel {
        serial: "SNTUITEST0000001".to_string(),
        firmware: "V1.0.0.001".to_string(),
        profile: ProfileNumber::from_wire_index(0).unwrap(),
        global: GlobalTravel {
            travel: Um(500),
            press_dead: Um(200),
            release_dead: Um(200),
        },
        rows: vec![
            DefKeyRow {
                row: 0,
                keys: vec![(0, 0x1A)],
            },
            DefKeyRow {
                row: 1,
                keys: vec![(0, 0x04)],
            },
            DefKeyRow {
                row: 2,
                keys: vec![(0, 0x16)],
            },
            DefKeyRow {
                row: 3,
                keys: vec![(0, 0x07)],
            },
        ],
        keys: vec![
            key(0x1A, 2000, 0),
            key(0x04, 1500, 0),
            key(0x16, 1500, 1),
            key(0x07, 1500, 0),
        ],
    }
}

/// A full 68-key ANSI-DK board in the `DefKeyRow` shape `BoardModel::read` returns: five rows of
/// 15, 15, 14, 14 and 10 keys. Assembled here from the ANSI-DK layout the design spec names, not
/// read off a board (`captures/` is the operator's own data and gitignored), so it is what that
/// physical layout implies rather than a measured DEFKEY read. `keys` is empty: the matrix draws
/// its geometry from `rows` alone, and the tests this feeds assert widths, not values.
#[allow(dead_code)]
pub fn ansi_dk_board() -> BoardModel {
    let row = |index: u8, names: &[&str]| DefKeyRow {
        row: index,
        keys: names
            .iter()
            .enumerate()
            .map(|(col, name)| {
                (
                    col as u8,
                    wh_proto::keys::usage_for_name(name)
                        .unwrap_or_else(|| panic!("{name} must be in wh_proto::keys::TABLE")),
                )
            })
            .collect(),
    };
    BoardModel {
        serial: "SNTUITEST0000001".to_string(),
        firmware: "V1.0.0.001".to_string(),
        profile: ProfileNumber::from_wire_index(0).unwrap(),
        global: GlobalTravel {
            travel: Um(500),
            press_dead: Um(200),
            release_dead: Um(200),
        },
        rows: vec![
            row(
                0,
                &[
                    "esc",
                    "1",
                    "2",
                    "3",
                    "4",
                    "5",
                    "6",
                    "7",
                    "8",
                    "9",
                    "0",
                    "minus",
                    "equals",
                    "backspace",
                    "delete",
                ],
            ),
            row(
                1,
                &[
                    "tab",
                    "q",
                    "w",
                    "e",
                    "r",
                    "t",
                    "y",
                    "u",
                    "i",
                    "o",
                    "p",
                    "lbracket",
                    "rbracket",
                    "backslash",
                    "home",
                ],
            ),
            row(
                2,
                &[
                    "capslock",
                    "a",
                    "s",
                    "d",
                    "f",
                    "g",
                    "h",
                    "j",
                    "k",
                    "l",
                    "semicolon",
                    "quote",
                    "enter",
                    "pageup",
                ],
            ),
            row(
                3,
                &[
                    "lshift", "z", "x", "c", "v", "b", "n", "m", "comma", "period", "slash",
                    "rshift", "up", "pagedown",
                ],
            ),
            row(
                4,
                &[
                    "lctrl", "lgui", "lalt", "space", "ralt", "rgui", "rctrl", "left", "down",
                    "right",
                ],
            ),
        ],
        keys: Vec::new(),
    }
}

/// Six `DefKeyRow`s, `ops::read_matrix_rows`' own real wire shape, with a leading empty row and
/// a second between two populated ones: a real board leaves at least one row empty (measured via
/// a controller probe at 187x80, 2026-09-06), and no other fixture here had one. Four populated
/// rows, not `ansi_dk_board`'s five: two empty rows in six slots leaves four for real keys.
#[allow(dead_code)]
pub fn six_row_board_with_empty_rows() -> BoardModel {
    let row = |index: u8, name: Option<&str>| DefKeyRow {
        row: index,
        keys: name
            .map(|n| {
                vec![(
                    0,
                    wh_proto::keys::usage_for_name(n)
                        .unwrap_or_else(|| panic!("{n} must be in wh_proto::keys::TABLE")),
                )]
            })
            .unwrap_or_default(),
    };
    BoardModel {
        serial: "SNTUITEST0000001".to_string(),
        firmware: "V1.0.0.001".to_string(),
        profile: ProfileNumber::from_wire_index(0).unwrap(),
        global: GlobalTravel {
            travel: Um(500),
            press_dead: Um(200),
            release_dead: Um(200),
        },
        rows: vec![
            row(0, None), // leading empty row
            row(1, Some("esc")),
            row(2, None), // empty row between populated rows
            row(3, Some("tab")),
            row(4, Some("capslock")),
            row(5, Some("lshift")),
        ],
        keys: Vec::new(),
    }
}

/// Composes, in order, exactly the frames `BoardModel::read` sends against the two-key board:
/// sync, profile, global travel, matrix, then six KEY reads per key. Built with `wh_proto::cmds`
/// encoders, not hand-written hex, so the test breaks if an encoder changes.
pub fn build_script() -> Vec<String> {
    let mut lines = Vec::new();

    lines.extend(sync_lines("SNTUITEST0000001", "V1.0.0.001"));
    lines.extend(profile_lines(0)); // board reports profile index 0, i.e. UI "profile 1"
    lines.extend(global_travel_lines(500, 200, 200));
    lines.extend(matrix_lines());

    // Per-key reads, in matrix order: 'w' (0x1A) then 'a' (0x04).
    lines.extend(key_settings_lines(0x1A, 1200, 0x0230, 500, 500, 1, 0));
    lines.extend(key_settings_lines(0x04, 1500, 0x00, 0, 0, 0, 0));

    lines
}
