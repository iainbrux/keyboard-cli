mod support;

use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;
use ratatui::Terminal;
use support::{two_key_board, wasd_board};
use wh_tui::app::{
    draw, AdvancedTab, App, Tab, ADVANCED_GAMEPAD_STUB, ADVANCED_GENERAL_STUB, ADVANCED_SHARE_STUB,
    MAPPING_LABELS_ROW_1, MAPPING_LABELS_ROW_2, MAPPING_STUB, SWITCHES_STUB, TABS,
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
    Terminal::new(TestBackend::new(160, 50)).unwrap()
}

/// `left_width` is `area.width.min(56)` in `app::draw`, 56 here since the terminal is 160 wide.
/// Mirrors `tests/rows.rs`'s own `dots` helper, kept separate deliberately: the two suites must
/// never drift together silently.
fn dots(label: &str, control: &str) -> String {
    ".".repeat(56 - label.chars().count() - control.chars().count())
}

/// Asserts the row whose rendered line starts with `label` is dim across its whole width (both
/// the leftmost and, within the 56-column left pane, the rightmost cell), the same shape
/// `tests/rows.rs`'s `rt_sub_rows_render_dim_while_global_rt_is_off` checks for RT's sub-rows.
/// Reads the row's `y` back from the rendered text rather than a hardcoded index, so a row moved
/// out from under its own label is still caught.
fn assert_row_dim(buf: &Buffer, lines: &[String], label: &str) {
    let left_last_col = 55u16; // left pane is 56 columns wide (area.width.min(56)), 0-indexed
    let y = lines
        .iter()
        .position(|l| l.starts_with(label))
        .unwrap_or_else(|| panic!("the {label} row must render: {lines:?}")) as u16;
    assert!(
        buf[(0, y)].modifier.contains(Modifier::DIM),
        "{label}'s leftmost cell must be dim, its control must be disabled: {lines:?}"
    );
    assert!(
        buf[(left_last_col, y)].modifier.contains(Modifier::DIM),
        "{label}'s rightmost cell (within the left pane) must be dim too: {lines:?}"
    );
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

/// The same shape as `dots`, for the three ADVANCED sub-tabs that drop the keyboard pane: their
/// left pane is the whole 160-column frame, not 56 columns, so their rows carry that many more
/// leaders.
fn wide_dots(label: &str, control: &str) -> String {
    ".".repeat(160 - label.chars().count() - control.chars().count())
}

/// ADVANCED's GAMEPAD, DEVICE and SHARE sub-tabs drop the keyboard pane (the design spec's own
/// layout): no cap may be drawn, none recorded for click-to-select, and the left pane takes the
/// full width. Every other tab, GENERAL included, keeps the matrix.
#[test]
fn gamepad_device_and_share_drop_the_keyboard_pane() {
    for sub in [
        AdvancedTab::Gamepad,
        AdvancedTab::Device,
        AdvancedTab::Share,
    ] {
        let mut app = App::new(wasd_board(), "0.5.0-alpha");
        app.tab = Tab::Advanced;
        app.advanced_tab = sub;
        let mut terminal = new_terminal();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let lines = buffer_lines(&terminal);

        assert!(
            app.key_rects.is_empty(),
            "{sub:?} must record no key rects: {lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains('┌')),
            "{sub:?} must draw no cap at all: {lines:?}"
        );
    }

    // The left pane takes the width the matrix gave up: a DEVICE row's leaders fill all 160
    // columns, not 56.
    let mut app = App::new(wasd_board(), "0.5.0-alpha");
    app.tab = Tab::Advanced;
    app.advanced_tab = AdvancedTab::Device;
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let lines = buffer_lines(&terminal);
    let full_width_name = format!("NAME{}WALLHACK K-001", wide_dots("NAME", "WALLHACK K-001"));
    assert!(
        lines.iter().any(|l| l == &full_width_name),
        "DEVICE's rows must fill the full frame width: {lines:?}"
    );

    // Every top-level tab keeps the matrix, and so does ADVANCED > GENERAL.
    for tab in TABS {
        let mut app = App::new(wasd_board(), "0.5.0-alpha");
        app.tab = tab;
        app.advanced_tab = AdvancedTab::General;
        let mut terminal = new_terminal();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let lines = buffer_lines(&terminal);
        assert_eq!(
            app.key_rects.len(),
            4,
            "{tab:?} must render the whole four-key matrix: {lines:?}"
        );
    }
}

#[test]
fn advanced_device_shows_live_name_serial_firmware() {
    let mut app = new_app();
    app.tab = Tab::Advanced;
    app.advanced_tab = AdvancedTab::Device;
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let lines = buffer_lines(&terminal);

    // DEVICE drops the keyboard pane, so its left pane is the full 160 columns.
    let name_line = format!("NAME{}WALLHACK K-001", wide_dots("NAME", "WALLHACK K-001"));
    let serial_line = format!(
        "SERIAL NUMBER{}SNTUITEST0000001",
        wide_dots("SERIAL NUMBER", "SNTUITEST0000001")
    );
    let firmware_line = format!(
        "FIRMWARE VERSION{}V1.0.0.001",
        wide_dots("FIRMWARE VERSION", "V1.0.0.001")
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

    // The sub-tab row sits one line below the main tab row (y=25, see chrome.rs), at y=26.
    // Find DEVICE's column in that literal rather than reading it back from app.advanced_rects,
    // so a rect moved out from under the visible text is still caught.
    let sub_tab_row_y = 26u16;
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

#[test]
fn up_and_down_cycle_the_advanced_subtab_without_wrapping_past_the_ends() {
    let mut app = new_app();
    app.tab = Tab::Advanced;
    assert_eq!(app.advanced_tab, AdvancedTab::General);

    app.handle_key(KeyCode::Down);
    assert_eq!(app.advanced_tab, AdvancedTab::Gamepad);
    app.handle_key(KeyCode::Down);
    assert_eq!(app.advanced_tab, AdvancedTab::Device);
    app.handle_key(KeyCode::Down);
    assert_eq!(app.advanced_tab, AdvancedTab::Share);
    app.handle_key(KeyCode::Down);
    assert_eq!(
        app.advanced_tab,
        AdvancedTab::Share,
        "Down at the last sub-tab must not wrap"
    );

    app.handle_key(KeyCode::Up);
    assert_eq!(app.advanced_tab, AdvancedTab::Device);
    app.handle_key(KeyCode::Up);
    assert_eq!(app.advanced_tab, AdvancedTab::Gamepad);
    app.handle_key(KeyCode::Up);
    assert_eq!(app.advanced_tab, AdvancedTab::General);
    app.handle_key(KeyCode::Up);
    assert_eq!(
        app.advanced_tab,
        AdvancedTab::General,
        "Up at the first sub-tab must not wrap"
    );
}

#[test]
fn up_and_down_do_nothing_when_a_non_advanced_tab_is_selected() {
    let mut app = new_app();
    assert_eq!(app.tab, Tab::ActuationPoint);
    assert_eq!(app.advanced_tab, AdvancedTab::General);

    app.handle_key(KeyCode::Down);
    assert_eq!(
        app.advanced_tab,
        AdvancedTab::General,
        "Down must not touch advanced_tab while a non-ADVANCED tab is selected"
    );
    app.handle_key(KeyCode::Up);
    assert_eq!(
        app.advanced_tab,
        AdvancedTab::General,
        "Up must not touch advanced_tab while a non-ADVANCED tab is selected"
    );

    // Also confirm it while parked on a sub-tab other than the default, so the guard is checked
    // against `app.tab`, not merely against `advanced_tab` already being at its ends.
    app.tab = Tab::Advanced;
    app.advanced_tab = AdvancedTab::Gamepad;
    app.tab = Tab::Mapping;
    app.handle_key(KeyCode::Down);
    assert_eq!(
        app.advanced_tab,
        AdvancedTab::Gamepad,
        "Down must not touch advanced_tab from a different tab even mid-cycle"
    );
    app.handle_key(KeyCode::Up);
    assert_eq!(
        app.advanced_tab,
        AdvancedTab::Gamepad,
        "Up must not touch advanced_tab from a different tab even mid-cycle"
    );
}

/// While the board is locked, every guard in `handle_key` and `handle_mouse` holds: the ADVANCED
/// sub-tab neither cycles nor answers a click, while quit and top-level tab navigation, keyboard
/// and mouse both, still work. Without these the two `!self.locked` guards could be deleted with
/// the suite green.
#[test]
fn a_locked_board_freezes_the_advanced_sub_tabs_but_not_quit_or_tab_navigation() {
    let mut app = new_app();
    app.tab = Tab::Advanced;
    app.advanced_tab = AdvancedTab::Gamepad;
    app.locked = true;
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let lines = buffer_lines(&terminal);

    app.handle_key(KeyCode::Down);
    assert_eq!(
        app.advanced_tab,
        AdvancedTab::Gamepad,
        "Down must not cycle the sub-tab while locked: {lines:?}"
    );
    app.handle_key(KeyCode::Up);
    assert_eq!(
        app.advanced_tab,
        AdvancedTab::Gamepad,
        "Up must not cycle the sub-tab while locked: {lines:?}"
    );

    // The sub-tab row still renders while locked, and its rects are still recorded; the click is
    // refused by the guard, not by there being nothing to hit.
    let sub_tab_row_y = 26u16;
    let sub_tab_line = &lines[sub_tab_row_y as usize];
    assert_eq!(
        sub_tab_line, "GENERAL  [GAMEPAD]  DEVICE  SHARE",
        "the sub-tab row: {lines:?}"
    );
    let col = sub_tab_line.find("DEVICE").unwrap() as u16;
    app.handle_mouse(MouseEventKind::Down(MouseButton::Left), col, sub_tab_row_y);
    assert_eq!(
        app.advanced_tab,
        AdvancedTab::Gamepad,
        "a sub-tab click must be ignored while locked: {lines:?}"
    );

    // Top-level navigation is the exception the banner's own wording relies on.
    app.handle_key(KeyCode::Left);
    assert_eq!(
        app.tab,
        Tab::Switches,
        "Left must still move the top-level tab while locked"
    );
    let tab_row = &lines[25];
    let mapping_col = tab_row.find("MAPPING").unwrap() as u16;
    app.handle_mouse(MouseEventKind::Down(MouseButton::Left), mapping_col, 25);
    assert_eq!(
        app.tab,
        Tab::Mapping,
        "a top-level tab click must still work while locked"
    );

    app.handle_key(KeyCode::Char('q'));
    assert!(app.quit, "q must still quit while locked");
}

/// The dim state of that inert row, both ways.
#[test]
fn the_advanced_sub_tab_row_dims_while_locked() {
    let row_y = 26u16;
    let row_width = "GENERAL  [GAMEPAD]  DEVICE  SHARE".chars().count() as u16;

    let mut app = new_app();
    app.tab = Tab::Advanced;
    app.advanced_tab = AdvancedTab::Gamepad;
    app.locked = true;
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let lines = buffer_lines(&terminal);
    for x in 0..row_width {
        assert!(
            buf[(x, row_y)].modifier.contains(Modifier::DIM),
            "column {x} of the sub-tab row must be dim while locked: {lines:?}"
        );
    }

    let mut app = new_app();
    app.tab = Tab::Advanced;
    app.advanced_tab = AdvancedTab::Gamepad;
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let lines = buffer_lines(&terminal);
    for x in 0..row_width {
        assert!(
            !buf[(x, row_y)].modifier.contains(Modifier::DIM),
            "column {x} of the sub-tab row must not be dim while unlocked: {lines:?}"
        );
    }
}

#[test]
fn general_gamepad_share_and_switches_rows_render_dim_disabled() {
    // GENERAL: every row from disabled_button/disabled_stepper, including SOCD.
    let mut app = new_app();
    app.tab = Tab::Advanced;
    app.advanced_tab = AdvancedTab::General;
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let lines = buffer_lines(&terminal);
    for label in [
        "RESET PROFILE",
        "FACTORY RESET",
        "POLLING RATE",
        "LED SLEEP TIMER",
        "LED BRIGHTNESS",
        "SYSTEM TYPE",
        "SHOW ANALOG OUTPUT",
        "SAFETY ZONE",
        "SHOW MAPPED KEY LABELS",
        "LOCALIZED KEY LABELS",
        "SOCD",
        "DYNAMIC KEYSTROKE (DKS)",
        "MOD TAP",
        "WALKTHROUGH",
    ] {
        assert_row_dim(&buf, &lines, label);
    }

    // GAMEPAD
    let mut app = new_app();
    app.tab = Tab::Advanced;
    app.advanced_tab = AdvancedTab::Gamepad;
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let lines = buffer_lines(&terminal);
    for label in [
        "GAMEPAD MODE",
        "ENABLE MAPPED KEYBOARD KEYS",
        "DISABLE MAPPED KEY INPUT",
        "SQUARE JOYSTICK OUTPUT",
        "DEPTH-BASED JOYSTICK",
    ] {
        assert_row_dim(&buf, &lines, label);
    }

    // SHARE
    let mut app = new_app();
    app.tab = Tab::Advanced;
    app.advanced_tab = AdvancedTab::Share;
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let lines = buffer_lines(&terminal);
    for label in ["EXPORT PROFILE SETTINGS", "IMPORT PROFILE SETTINGS"] {
        assert_row_dim(&buf, &lines, label);
    }

    // SWITCHES
    let mut app = new_app();
    app.tab = Tab::Switches;
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let lines = buffer_lines(&terminal);
    for label in ["CALIBRATE SWITCHES", "CURRENT SWITCHES"] {
        assert_row_dim(&buf, &lines, label);
    }
}
