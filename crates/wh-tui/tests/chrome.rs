mod support;
use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use support::two_key_board;
use wh_tui::app::{draw, App, Tab};

/// Every rendered line of the buffer, right-trimmed, so tests assert whole lines. Copied from
/// `app::tests::buffer_lines`, kept in sync deliberately rather than shared: this suite exercises
/// the crate's public surface, not its internals.
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

#[test]
fn the_logo_and_nav_line_render_whole_and_exact() {
    let mut app = new_app();
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let lines = buffer_lines(&terminal);
    assert_eq!(
        lines[0], "00000000000000000000000",
        "logo first line: {lines:?}"
    );
    assert_eq!(
        lines[20], "NAVIGATE WITH MOUSE OR ARROW & ENTER KEYS",
        "nav line: {lines:?}"
    );
}

#[test]
fn the_tab_row_renders_all_five_titles_and_marks_the_selected_one() {
    let mut app = new_app();
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let lines = buffer_lines(&terminal);
    assert_eq!(
        lines[25], "[ACTUATION POINT]  RAPID TRIGGER  MAPPING  SWITCHES  ADVANCED",
        "tab row: {lines:?}"
    );
}

#[test]
fn right_and_left_arrows_cycle_the_tabs_without_wrapping_past_the_ends() {
    let mut app = new_app();
    assert_eq!(app.tab, Tab::ActuationPoint);

    app.handle_key(KeyCode::Right);
    assert_eq!(app.tab, Tab::RapidTrigger);
    app.handle_key(KeyCode::Right);
    assert_eq!(app.tab, Tab::Mapping);
    app.handle_key(KeyCode::Right);
    assert_eq!(app.tab, Tab::Switches);
    app.handle_key(KeyCode::Right);
    assert_eq!(app.tab, Tab::Advanced);
    app.handle_key(KeyCode::Right);
    assert_eq!(
        app.tab,
        Tab::Advanced,
        "Right at the last tab must not wrap"
    );

    app.handle_key(KeyCode::Left);
    assert_eq!(app.tab, Tab::Switches);
    app.handle_key(KeyCode::Left);
    assert_eq!(app.tab, Tab::Mapping);
    app.handle_key(KeyCode::Left);
    assert_eq!(app.tab, Tab::RapidTrigger);
    app.handle_key(KeyCode::Left);
    assert_eq!(app.tab, Tab::ActuationPoint);
    app.handle_key(KeyCode::Left);
    assert_eq!(
        app.tab,
        Tab::ActuationPoint,
        "Left at the first tab must not wrap"
    );
}

#[test]
fn clicking_a_tab_title_selects_it() {
    let mut app = new_app();
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();

    // The tab-row line, pinned independently in the row-render test above: find MAPPING's
    // column in that literal rather than reading it back from app.tab_rects, so a rect moved
    // out from under the visible text is still caught.
    let tab_row = "[ACTUATION POINT]  RAPID TRIGGER  MAPPING  SWITCHES  ADVANCED";
    let col = tab_row.find("MAPPING").unwrap() as u16 + 1;
    let row = 25u16;

    assert_eq!(app.tab, Tab::ActuationPoint);
    app.handle_mouse(MouseEventKind::Down(MouseButton::Left), col, row);
    assert_eq!(app.tab, Tab::Mapping);
}

#[test]
fn the_footer_renders_help_language_and_support() {
    let mut app = new_app();
    let mut terminal = new_terminal();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let lines = buffer_lines(&terminal);
    assert_eq!(
        lines[49], "HELP  EN JA CH  SUPPORT@WALLHACK.COM",
        "footer line: {lines:?}"
    );
}
