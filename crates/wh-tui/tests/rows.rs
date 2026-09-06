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
    let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
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
    let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
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

/// The row line `app::draw` renders in the 56-column left pane: label, dot leaders, control.
/// Mirrors `tests/stubs.rs`'s own `dots` helper, kept separate for the same reason.
fn row_line(label: &str, control: &str) -> String {
    let dots = ".".repeat(56 - label.chars().count() - control.chars().count());
    format!("{label}{dots}{control}")
}

/// A board holding exactly `keys`, everything else fixed: the RT rows read nothing else.
fn rt_board(keys: Vec<KeySettings>) -> BoardModel {
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
        keys,
    }
}

/// One key outside any RT keyset, at the given raw MODE value: `0x0010` is touch nibble 1
/// (`Single`, rapid trigger off), `0x0030` nibble 3 (`Rt`, on), `0x0040` nibble 4
/// (`RtContinuous`, on with continuous).
fn outside_key(usage: u8, mode: u16, press: u16) -> KeySettings {
    KeySettings {
        usage,
        ap: Um(2000),
        mode: Mode::from_value(mode),
        rt_press: Um(press),
        rt_release: Um(press),
        ap_keyset: 0,
        rt_keyset: 0,
    }
}

/// The RT tab's left-pane lines for a board of `keys`.
fn rt_lines(keys: Vec<KeySettings>) -> Vec<String> {
    let mut app = App::new(rt_board(keys), "0.5.0-alpha");
    app.tab = Tab::RapidTrigger;
    let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    buffer_lines(&terminal)
}

/// GLOBAL RAPID TRIGGER is a toggle, not a measurement: the vendor renders
/// `GLOBAL RAPID TRIGGER < OFF >` and puts the millimetres on RT SENSITIVITY below it. All three
/// states, and RT SENSITIVITY keeping its millimetres alongside the ON one.
#[test]
fn global_rapid_trigger_reads_on_off_or_mixed_and_never_a_measurement() {
    let on = rt_lines(vec![
        outside_key(0x1A, 0x0030, 300),
        outside_key(0x04, 0x0030, 300),
    ]);
    assert!(
        on.iter()
            .any(|l| l == &row_line("GLOBAL RAPID TRIGGER", "< ON >")),
        "every outside key has rapid trigger on: {on:?}"
    );
    assert!(
        on.iter()
            .any(|l| l == &row_line("RT SENSITIVITY", "< 0.30 MM >")),
        "RT SENSITIVITY keeps the millimetres: {on:?}"
    );

    let off = rt_lines(vec![
        outside_key(0x1A, 0x0010, 300),
        outside_key(0x04, 0x0010, 300),
    ]);
    assert!(
        off.iter()
            .any(|l| l == &row_line("GLOBAL RAPID TRIGGER", "< OFF >")),
        "no outside key has rapid trigger on: {off:?}"
    );

    let mixed = rt_lines(vec![
        outside_key(0x1A, 0x0030, 300),
        outside_key(0x04, 0x0010, 300),
    ]);
    assert!(
        mixed
            .iter()
            .any(|l| l == &row_line("GLOBAL RAPID TRIGGER", "< MIXED >")),
        "one outside key on and one off is MIXED, not either end: {mixed:?}"
    );
}

/// CONTINUOUS RAPID TRIGGER folds the outside keys the way every sibling row does. One key of
/// sixty-eight in continuous mode is not a board in continuous mode, which is what the `.any(..)`
/// this replaces claimed.
#[test]
fn continuous_rapid_trigger_reads_mixed_when_the_outside_keys_disagree() {
    let all_on = rt_lines(vec![
        outside_key(0x1A, 0x0040, 300),
        outside_key(0x04, 0x0040, 300),
    ]);
    assert!(
        all_on
            .iter()
            .any(|l| l == &row_line("CONTINUOUS RAPID TRIGGER", "< ON >")),
        "every outside key is in continuous mode: {all_on:?}"
    );

    let all_off = rt_lines(vec![
        outside_key(0x1A, 0x0030, 300),
        outside_key(0x04, 0x0030, 300),
    ]);
    assert!(
        all_off
            .iter()
            .any(|l| l == &row_line("CONTINUOUS RAPID TRIGGER", "< OFF >")),
        "rapid trigger on but continuous off on every outside key: {all_off:?}"
    );

    let mixed = rt_lines(vec![
        outside_key(0x1A, 0x0040, 300),
        outside_key(0x04, 0x0030, 300),
    ]);
    assert!(
        mixed
            .iter()
            .any(|l| l == &row_line("CONTINUOUS RAPID TRIGGER", "< MIXED >")),
        "one outside key continuous and one not is MIXED: {mixed:?}"
    );

    // The population, not just the flag: a key with rapid trigger off has no continuous state to
    // disagree with, so it must not drag the row to MIXED. Drop `rt_enabled()` from the fold's
    // filter and this reads MIXED instead of ON.
    let one_off = rt_lines(vec![
        outside_key(0x1A, 0x0040, 300),
        outside_key(0x04, 0x0010, 300),
    ]);
    assert!(
        one_off
            .iter()
            .any(|l| l == &row_line("CONTINUOUS RAPID TRIGGER", "< ON >")),
        "an rt-off key outside the keysets must not count towards continuous: {one_off:?}"
    );
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
    let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
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
    let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
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
