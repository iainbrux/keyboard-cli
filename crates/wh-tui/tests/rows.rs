mod support;

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::Terminal;
use support::two_key_board;
use wh_device::ops::KeySettings;
use wh_proto::cmds::{GlobalTravel, Mode, ProfileNumber};
use wh_proto::value::Um;
use wh_tui::app::{draw, App, Tab};
use wh_tui::board::BoardModel;
use wh_tui::rows::{render_row, Control, SettingRow};

/// Every rendered line of the buffer, right-trimmed, so tests assert whole lines. Copied from
/// `app::tests::buffer_lines` and its other copies in `chrome.rs`/`matrix.rs`, kept in sync
/// deliberately rather than shared, for the same reason those are not shared with each other.
fn buffer_lines(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let buf = terminal.backend().buffer().clone();
    let area = buf.area;
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

#[test]
fn a_stepper_row_renders_label_leaders_and_value_as_one_exact_line() {
    // Built once by hand: label (23 chars incl. no trailing space) + dot leaders + "< 2.00 MM >"
    // (11 chars), exactly filling a 62-wide area with no padding either side.
    let expected = "GLOBAL ACTUATION POINT.............................< 2.00 MM >";
    let area = Rect::new(0, 0, expected.chars().count() as u16, 1);
    let mut buf = Buffer::empty(area);
    let row = SettingRow {
        label: "GLOBAL ACTUATION POINT".to_string(),
        control: Control::Stepper {
            value: "2.00 MM".to_string(),
        },
        disabled: false,
        indent: 0,
    };
    render_row(area, &mut buf, &row);

    let line: String = (0..area.width)
        .map(|x| buf[(x, 0)].symbol().to_string())
        .collect();
    assert_eq!(line, expected, "the whole rendered line must match exactly");
}

#[test]
fn a_disabled_row_is_dim_across_its_whole_width() {
    let area = Rect::new(0, 0, 30, 1);
    let mut buf = Buffer::empty(area);
    let row = SettingRow {
        label: "RT SENSITIVITY".to_string(),
        control: Control::Stepper {
            value: "0.10 MM".to_string(),
        },
        disabled: true,
        indent: 0,
    };
    render_row(area, &mut buf, &row);

    for x in 0..area.width {
        assert!(
            buf[(x, 0)].modifier.contains(Modifier::DIM),
            "column {x} of a disabled row must be dim"
        );
    }
}

/// A board with one key outside any AP keyset (sets the global row) and two sharing AP keyset 1
/// (the keyset row), built directly rather than through `support`: none of that module's
/// fixtures carry a non-zero `ap_keyset`.
fn ap_keyset_board() -> BoardModel {
    let key = |usage: u8, ap: u16, ap_keyset: u16| KeySettings {
        usage,
        ap: Um(ap),
        mode: Mode::from_value(0x0010),
        rt_press: Um(0),
        rt_release: Um(0),
        ap_keyset,
        rt_keyset: 0,
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
        rows: Vec::new(),
        keys: vec![
            key(0x16, 2000, 0), // 's', outside any ap keyset: sets the global row to 2.00mm
            key(0x1A, 1200, 1), // 'w', in ap keyset 1
            key(0x04, 1500, 1), // 'a', in ap keyset 1
        ],
    }
}

#[test]
fn the_ap_tab_body_renders_global_custom_value_and_keyset_rows() {
    let mut app = App::new(ap_keyset_board(), "0.5.0-alpha");
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let lines = buffer_lines(&terminal);

    let left_width = 56usize; // area.width.min(56) in app::draw
    let dots = |label: &str, control: &str| ".".repeat(left_width - label.len() - control.len());

    let global_line = format!(
        "GLOBAL ACTUATION POINT{}< 2.00 MM >",
        dots("GLOBAL ACTUATION POINT", "< 2.00 MM >")
    );
    let custom_value_line = format!(
        "\"MM\" CUSTOM VALUE{}< 0.50 MM >",
        dots("\"MM\" CUSTOM VALUE", "< 0.50 MM >")
    );
    let keyset_line = format!("[X] W,A{}[^]", dots("[X] W,A", "[^]"));

    assert!(
        lines.iter().any(|l| l == &global_line),
        "global AP row missing or wrong, whole line: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == &custom_value_line),
        "\"MM\" CUSTOM VALUE row missing or wrong, whole line: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == &keyset_line),
        "AP keyset row missing or wrong, whole line: {lines:?}"
    );

    let status = "> CLICK ON THE KEYS TO MAKE A KEYSET";
    let action = "[RESET KEYSETS]";
    let prompt_line = format!(
        "{status}{}{action}",
        " ".repeat(left_width - status.len() - action.len())
    );
    assert!(
        lines.iter().any(|l| l == &prompt_line),
        "prompt line missing or wrong, whole line: {lines:?}"
    );
}

#[test]
fn rt_sub_rows_render_dim_while_global_rt_is_off() {
    // `two_key_board`'s keys are both `Mode::from_value(0x0010)` (touch nibble `Single`), so
    // neither is rapid-trigger-enabled: global rapid trigger reads off.
    let mut app = App::new(two_key_board(), "0.5.0-alpha");
    app.tab = Tab::RapidTrigger;
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();

    let buf = terminal.backend().buffer().clone();
    let lines = buffer_lines(&terminal);
    let left_last_col = 55u16; // left pane is 56 columns wide (area.width.min(56)), 0-indexed

    let global_y = lines
        .iter()
        .position(|l| l.starts_with("GLOBAL RAPID TRIGGER"))
        .expect("the global rapid trigger row must render") as u16;
    assert!(
        !buf[(0, global_y)].modifier.contains(Modifier::DIM),
        "the global row itself must not be dim while it is the thing reading OFF: {lines:?}"
    );

    for label in [
        "SEPARATE PRESS AND RELEASE",
        "RT SENSITIVITY",
        "CONTINUOUS RAPID TRIGGER",
    ] {
        let y = lines
            .iter()
            .position(|l| l.starts_with(label))
            .unwrap_or_else(|| panic!("the {label} row must render: {lines:?}"))
            as u16;
        assert!(
            buf[(0, y)].modifier.contains(Modifier::DIM),
            "{label}'s leftmost cell must be dim while global RT is off: {lines:?}"
        );
        assert!(
            buf[(left_last_col, y)].modifier.contains(Modifier::DIM),
            "{label}'s rightmost cell (within the left pane) must be dim too: {lines:?}"
        );
    }
}

/// Locates `"[RESET KEYSETS]"` on whichever rendered line contains it and returns its row and
/// its own column span, read back from the rendered text rather than assumed from a hard-coded
/// column: a layout change that moved the prompt line would move this test's target with it.
fn reset_keysets_span(lines: &[String]) -> (u16, std::ops::Range<u16>) {
    let action = "[RESET KEYSETS]";
    let y = lines
        .iter()
        .position(|l| l.contains(action))
        .expect("the prompt line's RESET KEYSETS action must render") as u16;
    let start = lines[y as usize].find(action).unwrap() as u16;
    (y, start..start + action.chars().count() as u16)
}

#[test]
fn reset_keysets_is_dim_when_the_tabs_keyset_list_is_empty() {
    // `two_key_board`'s keys both carry `ap_keyset: 0`: no AP keyset exists.
    let mut app = App::new(two_key_board(), "0.5.0-alpha");
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();

    let buf = terminal.backend().buffer().clone();
    let lines = buffer_lines(&terminal);
    let (y, span) = reset_keysets_span(&lines);

    for x in span {
        assert!(
            buf[(x, y)].modifier.contains(Modifier::DIM),
            "column {x} of RESET KEYSETS must be dim when the AP tab has no keyset: {lines:?}"
        );
    }
}

#[test]
fn reset_keysets_is_not_dim_when_the_tab_has_a_keyset() {
    // `ap_keyset_board` puts 'w' and 'a' in AP keyset 1: one keyset exists.
    let mut app = App::new(ap_keyset_board(), "0.5.0-alpha");
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();

    let buf = terminal.backend().buffer().clone();
    let lines = buffer_lines(&terminal);
    let (y, span) = reset_keysets_span(&lines);

    for x in span {
        assert!(
            !buf[(x, y)].modifier.contains(Modifier::DIM),
            "column {x} of RESET KEYSETS must not be dim when the AP tab has a keyset: {lines:?}"
        );
    }
}
