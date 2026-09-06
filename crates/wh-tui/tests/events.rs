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

/// The same open-read sequence as `build_script`, but with the firmware and 'w''s AP each
/// changed from `build_script`'s own values (`V1.0.0.001`/1200 to `V1.0.0.002`/1300). Used as the
/// re-read block in the crown test below: `build_script()` called twice would be byte-identical
/// on both reads, so a mutation that discards the re-read's result entirely (keeping the first
/// model) could still pass every assertion that only checks the wire, never the model. A second
/// block differing in a value the test actually reads closes that gap.
fn second_read_script() -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(sync_lines("SNTUITEST0000001", "V1.0.0.002"));
    lines.extend(profile_lines(0));
    lines.extend(global_travel_lines(500, 200, 200));
    lines.extend(matrix_lines());
    lines.extend(key_settings_lines(0x1A, 1300, 0x0230, 500, 500, 1, 0));
    lines.extend(key_settings_lines(0x04, 1500, 0x00, 0, 0, 0, 0));
    lines
}

#[test]
fn a_be00_edge_raises_the_locked_banner_and_be01_rereads_and_lowers_it() {
    // The open read, then the board's own two edges (each preceded by an empty poll), then a
    // second full read block, deliberately different from the first (see `second_read_script`):
    // the vendor re-reads everything on `be 01`, so does `wh`, and the model it ends up holding
    // must actually be the second read's, not the first's kept around unchanged.
    let mut lines = build_script();
    lines.push("{\"dir\":\"wait\"}".to_string());
    lines.push(adjust_edge_line(true));
    lines.push("{\"dir\":\"wait\"}".to_string());
    lines.push(adjust_edge_line(false));
    lines.extend(second_read_script());

    let t = ReplayTransport::from_jsonl(&lines.join("\n")).unwrap();
    let mut s = Session::new(t);
    let board = BoardModel::read(&mut s).unwrap();
    let mut app = App::new(board, "0.5.0-alpha");
    assert_eq!(
        app.board.firmware, "V1.0.0.001",
        "the first read's own firmware"
    );
    assert_eq!(
        app.board.key(0x1A).unwrap().ap.0,
        1200,
        "the first read's own AP for 'w'"
    );

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

    // The re-read's own, different values must actually have landed in `app.board`, not just
    // been sent and read off the wire: a mutation that discards the read's result and keeps the
    // first model would still consume the script fully and must still be caught here.
    assert_eq!(
        app.board.firmware, "V1.0.0.002",
        "app.board must carry the re-read's own firmware, not the first read's"
    );
    assert_eq!(
        app.board.key(0x1A).unwrap().ap.0,
        1300,
        "app.board must carry the re-read's own AP for 'w', not the first read's"
    );

    assert!(
        s.into_inner().finished(),
        "the re-read must consume the script's second read block"
    );
}

#[test]
fn a_successful_reread_clears_a_status_note_left_over_from_before_the_edge() {
    // A note from earlier in the session (however it got set) must not survive a re-read that
    // succeeds: it would tell the operator the board's values might be stale, or point at some
    // now-old event, when the screen in front of them is in fact freshly confirmed.
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
    app.status = Some("NOTE: A READ TIMED OUT; VALUES MAY BE STALE".to_string());

    app.tick(&mut s).unwrap(); // tick1: wait
    app.tick(&mut s).unwrap(); // tick2: entering edge, locks
    assert_eq!(
        app.status.as_deref(),
        Some("NOTE: A READ TIMED OUT; VALUES MAY BE STALE"),
        "the entering edge must not touch a note left over from before it"
    );
    app.tick(&mut s).unwrap(); // tick3: wait
    let changed4 = app.tick(&mut s).unwrap(); // tick4: leaving edge, a successful re-read
    assert!(changed4, "the leaving edge must report a change");
    assert_eq!(
        app.status, None,
        "a successful re-read must clear a note left over from before the edge"
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
