mod support;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use support::*;
use wh_device::replay::ReplayTransport;
use wh_device::session::Session;
use wh_tui::app::{draw, App, LOCKED_BANNER, STATUS_UNKNOWN_EVENT};
use wh_tui::board::BoardModel;

/// Every rendered line of the buffer, right-trimmed, so tests assert whole lines. Copied from
/// `app::tests::buffer_lines` and `chrome.rs`'s own copy, kept in sync deliberately rather than
/// shared: this suite exercises the crate's public surface, not its internals.
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
fn a_be00_edge_raises_the_locked_banner_and_be01_rereads_and_lowers_it() {
    // The open read, then the board's own two edges (each preceded by an empty poll), then a
    // second full read block: the vendor re-reads everything on `be 01`, so does `wh`.
    let mut lines = build_script();
    lines.push("{\"dir\":\"wait\"}".to_string());
    lines.push(adjust_edge_line(true));
    lines.push("{\"dir\":\"wait\"}".to_string());
    lines.push(adjust_edge_line(false));
    lines.extend(build_script());

    let t = ReplayTransport::from_jsonl(&lines.join("\n")).unwrap();
    let mut s = Session::new(t);
    let board = BoardModel::read(&mut s).unwrap();
    let mut app = App::new(board, "0.5.0-alpha");

    // tick1: the scripted wait, nothing happened.
    let changed1 = app.tick(&mut s).unwrap();
    assert!(!changed1, "an empty poll must not report a change");
    assert!(!app.locked, "an empty poll must not lock the board");

    // tick2: the entering edge, the board locks.
    let changed2 = app.tick(&mut s).unwrap();
    assert!(changed2, "the entering edge must report a change");
    assert!(app.locked, "the entering edge must lock the board");

    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let rendered = buffer_lines(&terminal);
    assert!(
        rendered.iter().any(|l| l == LOCKED_BANNER),
        "the locked banner must render as a whole line: {rendered:?}"
    );

    // tick3: the scripted wait, still locked.
    let changed3 = app.tick(&mut s).unwrap();
    assert!(!changed3, "an empty poll must not report a change");
    assert!(app.locked, "the board must stay locked between edges");

    // tick4: the leaving edge, the board re-reads and unlocks.
    let changed4 = app.tick(&mut s).unwrap();
    assert!(changed4, "the leaving edge must report a change");
    assert!(!app.locked, "the leaving edge must unlock the board");

    assert!(
        s.into_inner().finished(),
        "the re-read must consume the script's second read block"
    );
}

#[test]
fn an_unknown_event_lands_in_the_status_line_and_does_not_lock() {
    let mut lines = build_script();
    lines.push(in_line(&reply(0x00, &[0x00, 0xbe, 0x02])));

    let t = ReplayTransport::from_jsonl(&lines.join("\n")).unwrap();
    let mut s = Session::new(t);
    let board = BoardModel::read(&mut s).unwrap();
    let mut app = App::new(board, "0.5.0-alpha");

    let changed = app.tick(&mut s).unwrap();
    assert!(changed, "an unknown event must report a change");
    assert!(!app.locked, "an unknown event must not lock the board");
    assert_eq!(app.status.as_deref(), Some(STATUS_UNKNOWN_EVENT));

    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let rendered = buffer_lines(&terminal);
    assert!(
        rendered.iter().any(|l| l == STATUS_UNKNOWN_EVENT),
        "the unknown-event status must render as a whole line: {rendered:?}"
    );
}
