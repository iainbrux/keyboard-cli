mod support;

use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use support::two_key_board;
use wh_tui::app::{
    draw, AdvancedTab, App, Tab, ADVANCED_GAMEPAD_STUB, ADVANCED_GENERAL_STUB, ADVANCED_SHARE_STUB,
    MAPPING_LABELS_ROW_1, MAPPING_LABELS_ROW_2, MAPPING_STUB, SWITCHES_STUB,
};

/// Every rendered line of the buffer, right-trimmed, so tests assert whole lines. Copied from
/// `app::tests::buffer_lines` and its other copies in `chrome.rs`/`matrix.rs`/`rows.rs`, kept in
/// sync deliberately rather than shared, for the same reason those are not shared with each other.
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

fn new_app() -> App {
    App::new(two_key_board(), "0.5.0-alpha")
}

fn new_terminal() -> Terminal<TestBackend> {
    Terminal::new(TestBackend::new(160, 40)).unwrap()
}

/// `left_width` is `area.width.min(56)` in `app::draw`, 56 here since the terminal is 160 wide.
/// Mirrors `tests/rows.rs`'s own `dots` helper, kept separate deliberately: the two suites must
/// never drift together silently.
fn dots(label: &str, control: &str) -> String {
    ".".repeat(56 - label.chars().count() - control.chars().count())
}

#[test]
fn mapping_renders_its_subtab_labels_and_the_stub_line() {
    let mut app = new_app();
    app.tab = Tab::Mapping;
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let lines = buffer_lines(&terminal);

    assert!(
        lines.iter().any(|l| l == MAPPING_LABELS_ROW_1),
        "the BASE LAYER / FN LAYER label row is missing or wrong: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == MAPPING_LABELS_ROW_2),
        "the character sub-tab label row is missing or wrong: {lines:?}"
    );

    // MAPPING_STUB (62 chars) is wider than the 56-column left pane, so it renders word-wrapped
    // across two lines rather than truncated; each half is asserted whole, and joining them with
    // a space must reproduce MAPPING_STUB exactly, so the wrap can never silently drop a word.
    let wrap_1 = "> MAPPING EDITS ARE NOT BUILT IN WH YET (3.6 IN";
    let wrap_2 = "DOCS/TASKS.MD)";
    assert_eq!(
        format!("{wrap_1} {wrap_2}"),
        MAPPING_STUB,
        "the two wrapped halves must reconstruct the pinned stub constant exactly"
    );
    assert!(
        lines.iter().any(|l| l == wrap_1),
        "the mapping stub line's first half is missing or wrong: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == wrap_2),
        "the mapping stub line's second half is missing or wrong: {lines:?}"
    );
}

#[test]
fn switches_renders_rows_and_the_stub_line() {
    let mut app = new_app();
    app.tab = Tab::Switches;
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let lines = buffer_lines(&terminal);

    let calibrate_line = format!(
        "CALIBRATE SWITCHES{}[START]",
        dots("CALIBRATE SWITCHES", "[START]")
    );
    let current_line = format!("CURRENT SWITCHES{}< - >", dots("CURRENT SWITCHES", "< - >"));

    assert!(
        lines.iter().any(|l| l == &calibrate_line),
        "the CALIBRATE SWITCHES row is missing or wrong: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == &current_line),
        "the CURRENT SWITCHES row is missing or wrong: {lines:?}"
    );
    // Guards against a blanked constant matching a blank padded row by coincidence: a wiped
    // `SWITCHES_STUB` renders no line at all, which `.any(|l| l == "")` would otherwise accept.
    assert!(!SWITCHES_STUB.is_empty(), "SWITCHES_STUB must not be blank");
    assert!(
        lines.iter().any(|l| l == SWITCHES_STUB),
        "the switches stub line is missing or wrong: {lines:?}"
    );
}

#[test]
fn advanced_device_shows_live_name_serial_firmware() {
    let mut app = new_app();
    app.tab = Tab::Advanced;
    app.advanced_tab = AdvancedTab::Device;
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let lines = buffer_lines(&terminal);

    let name_line = format!("NAME{}WALLHACK K-001", dots("NAME", "WALLHACK K-001"));
    let serial_line = format!(
        "SERIAL NUMBER{}SNTUITEST0000001",
        dots("SERIAL NUMBER", "SNTUITEST0000001")
    );
    let firmware_line = format!(
        "FIRMWARE VERSION{}V1.0.0.001",
        dots("FIRMWARE VERSION", "V1.0.0.001")
    );

    assert!(
        lines.iter().any(|l| l == &name_line),
        "the NAME row is missing or wrong: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == &serial_line),
        "the SERIAL NUMBER row is missing or wrong, must be live from BoardModel: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == &firmware_line),
        "the FIRMWARE VERSION row is missing or wrong, must be live from BoardModel: {lines:?}"
    );

    // No stub text on this sub-tab: it is live, not a stub.
    for stub in [
        MAPPING_STUB,
        SWITCHES_STUB,
        ADVANCED_GENERAL_STUB,
        ADVANCED_GAMEPAD_STUB,
        ADVANCED_SHARE_STUB,
    ] {
        assert!(
            !lines.iter().any(|l| l == stub),
            "ADVANCED > DEVICE must render no stub line, found {stub:?}: {lines:?}"
        );
    }
}

#[test]
fn advanced_general_rows_render_with_stub_markers_where_unbuilt() {
    let mut app = new_app();
    app.tab = Tab::Advanced;
    app.advanced_tab = AdvancedTab::General;
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let lines = buffer_lines(&terminal);

    let socd_line = format!("SOCD{}[SELECT]", dots("SOCD", "[SELECT]"));
    let polling_line = format!("POLLING RATE{}< - >", dots("POLLING RATE", "< - >"));
    let led_line = format!("LED SLEEP TIMER{}< - >", dots("LED SLEEP TIMER", "< - >"));

    assert!(
        lines.iter().any(|l| l == &socd_line),
        "the SOCD row must render as a disabled [SELECT] button: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == &polling_line),
        "the POLLING RATE row must show \"-\", it is not read from the device yet: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == &led_line),
        "the LED SLEEP TIMER row must show \"-\", it is not read from the device yet: {lines:?}"
    );
    // Guards against a blanked constant matching a blank padded row by coincidence, same as
    // `switches_renders_rows_and_the_stub_line`.
    assert!(
        !ADVANCED_GENERAL_STUB.is_empty(),
        "ADVANCED_GENERAL_STUB must not be blank"
    );
    assert!(
        lines.iter().any(|l| l == ADVANCED_GENERAL_STUB),
        "the advanced general stub line is missing or wrong: {lines:?}"
    );
}

#[test]
fn advanced_gamepad_and_share_render_their_stub_lines() {
    // Guards against a blanked constant matching a blank padded row by coincidence, same as
    // `switches_renders_rows_and_the_stub_line`.
    assert!(
        !ADVANCED_GAMEPAD_STUB.is_empty(),
        "ADVANCED_GAMEPAD_STUB must not be blank"
    );
    assert!(
        !ADVANCED_SHARE_STUB.is_empty(),
        "ADVANCED_SHARE_STUB must not be blank"
    );

    let mut app = new_app();
    app.tab = Tab::Advanced;
    app.advanced_tab = AdvancedTab::Gamepad;
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let lines = buffer_lines(&terminal);
    assert!(
        lines.iter().any(|l| l == ADVANCED_GAMEPAD_STUB),
        "the gamepad stub line is missing or wrong: {lines:?}"
    );

    let mut app = new_app();
    app.tab = Tab::Advanced;
    app.advanced_tab = AdvancedTab::Share;
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let lines = buffer_lines(&terminal);
    assert!(
        lines.iter().any(|l| l == ADVANCED_SHARE_STUB),
        "the share stub line is missing or wrong: {lines:?}"
    );
}

#[test]
fn clicking_an_advanced_sub_tab_selects_it() {
    let mut app = new_app();
    app.tab = Tab::Advanced;
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let lines = buffer_lines(&terminal);

    // The sub-tab row sits one line below the main tab row (y=14, see chrome.rs), at y=15.
    // Find DEVICE's column in that literal rather than reading it back from app.advanced_rects,
    // so a rect moved out from under the visible text is still caught.
    let sub_tab_row_y = 15u16;
    let sub_tab_line = &lines[sub_tab_row_y as usize];
    assert_eq!(
        sub_tab_line, "[GENERAL]  GAMEPAD  DEVICE  SHARE",
        "the sub-tab row: {lines:?}"
    );
    let col = sub_tab_line.find("DEVICE").unwrap() as u16;

    assert_eq!(app.advanced_tab, AdvancedTab::General);
    app.handle_mouse(MouseEventKind::Down(MouseButton::Left), col, sub_tab_row_y);
    assert_eq!(app.advanced_tab, AdvancedTab::Device);
}
