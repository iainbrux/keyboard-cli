mod support;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::Terminal;
use std::collections::HashSet;
use support::{ansi_dk_board, wasd_board};
use wh_proto::cmds::DefKeyRow;
use wh_tui::app::{draw, App, LOCKED_BANNER};
use wh_tui::matrix::{key_at, needed_width, render_matrix, CapValue};

/// Every rendered line of a raw `Buffer`, right-trimmed. Copied from `app::tests::buffer_lines`
/// and `chrome.rs`'s own copy, kept in sync deliberately rather than shared, for the same reason
/// those two are not shared with each other: `matrix.rs` exercises `render_matrix` directly
/// against a `Buffer`, not a `Terminal`.
fn buffer_lines(buf: &Buffer) -> Vec<String> {
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

/// A row's own slice of one rendered line, at `rect`'s column span: the whole cap-cell content,
/// not a substring search, so a test asserting it cannot pass on a line that merely contains the
/// expected text somewhere else.
fn cap_line(lines: &[String], rect: Rect, row_offset: u16) -> String {
    let line = &lines[(rect.y + row_offset) as usize];
    let chars: Vec<char> = line.chars().collect();
    let start = rect.x as usize;
    let end = (rect.x + rect.width) as usize;
    (start..end)
        .map(|i| chars.get(i).copied().unwrap_or(' '))
        .collect()
}

#[test]
fn caps_render_label_over_value() {
    let board = wasd_board();
    let area = Rect::new(0, 0, 40, 20);
    let mut buf = Buffer::empty(area);
    let mut rects = Vec::new();
    let selected = HashSet::new();

    render_matrix(
        area,
        &mut buf,
        &board.rows,
        |usage| {
            let k = board.key(usage).unwrap();
            CapValue {
                show: true,
                text: format!("{:.2}", k.ap.to_mm()),
            }
        },
        &selected,
        &mut rects,
    );

    let w_rect = rects
        .iter()
        .find(|&&(_, usage)| usage == 0x1A)
        .expect("w's cap must be recorded")
        .0;
    assert_eq!(w_rect.width, 7, "a plain key is 1.0 units, 7 columns");

    let lines = buffer_lines(&buf);
    assert_eq!(
        cap_line(&lines, w_rect, 1),
        "│  W  │",
        "label line, whole cap-cell: {lines:?}"
    );
    assert_eq!(
        cap_line(&lines, w_rect, 2),
        "│2.00 │",
        "value line, whole cap-cell: {lines:?}"
    );
}

#[test]
fn value_hidden_when_value_of_says_so() {
    let board = wasd_board();
    let area = Rect::new(0, 0, 40, 20);
    let mut buf = Buffer::empty(area);
    let mut rects = Vec::new();
    let selected = HashSet::new();

    render_matrix(
        area,
        &mut buf,
        &board.rows,
        |_usage| CapValue {
            show: false,
            text: "9.99".to_string(),
        },
        &selected,
        &mut rects,
    );

    let w_rect = rects
        .iter()
        .find(|&&(_, usage)| usage == 0x1A)
        .expect("w's cap must be recorded")
        .0;
    let lines = buffer_lines(&buf);
    assert_eq!(
        cap_line(&lines, w_rect, 2),
        "│     │",
        "hidden value line must be blank, whole cap-cell: {lines:?}"
    );
}

#[test]
fn key_at_resolves_a_click_inside_a_cap_and_misses_between_caps() {
    let board = wasd_board();
    let area = Rect::new(0, 0, 40, 20);
    let mut buf = Buffer::empty(area);
    let mut rects = Vec::new();
    let selected = HashSet::new();

    render_matrix(
        area,
        &mut buf,
        &board.rows,
        |_usage| CapValue {
            show: false,
            text: String::new(),
        },
        &selected,
        &mut rects,
    );

    let w_rect = rects
        .iter()
        .find(|&&(_, usage)| usage == 0x1A)
        .expect("w's cap must be recorded")
        .0;

    // Inside the cap: hits w.
    assert_eq!(
        key_at(&rects, w_rect.x + 3, w_rect.y + 2),
        Some(0x1A),
        "a point inside w's cap must resolve to w"
    );

    // Past the right edge of w's cap, on the same row: w's row has only one key, so this is
    // empty space between caps, and must miss.
    assert_eq!(
        key_at(&rects, w_rect.x + w_rect.width, w_rect.y + 2),
        None,
        "a point in the gap past the last cap of a row must miss"
    );
}

#[test]
fn a_selected_cap_renders_reversed() {
    let board = wasd_board();
    let area = Rect::new(0, 0, 40, 20);
    let mut buf = Buffer::empty(area);
    let mut rects = Vec::new();
    let mut selected = HashSet::new();
    selected.insert(0x1Au8);

    render_matrix(
        area,
        &mut buf,
        &board.rows,
        |_usage| CapValue {
            show: false,
            text: String::new(),
        },
        &selected,
        &mut rects,
    );

    let w_rect = rects
        .iter()
        .find(|&&(_, usage)| usage == 0x1A)
        .expect("w's cap must be recorded")
        .0;
    let a_rect = rects
        .iter()
        .find(|&&(_, usage)| usage == 0x04)
        .expect("a's cap must be recorded")
        .0;

    assert!(
        buf[(w_rect.x + 1, w_rect.y + 1)]
            .modifier
            .contains(Modifier::REVERSED),
        "a selected cap's cell must render reversed"
    );
    // Sample the top-left border cell too, not just an inner one: `REVERSED` must cover the
    // whole cap (border included), so a fix that only restyles the inner rect would still pass
    // the inner-cell assertion above.
    assert!(
        buf[(w_rect.x, w_rect.y)]
            .modifier
            .contains(Modifier::REVERSED),
        "a selected cap's border cell must render reversed too"
    );
    assert!(
        !buf[(a_rect.x + 1, a_rect.y + 1)]
            .modifier
            .contains(Modifier::REVERSED),
        "an unselected cap's cell must not render reversed"
    );
}

/// Every rendered line of a `Terminal`, right-trimmed. The width sweep below draws through
/// `app::draw`, not `render_matrix` alone: where the refusal lands is `draw`'s decision.
fn terminal_lines(terminal: &Terminal<TestBackend>) -> Vec<String> {
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

/// The whole refusal, as one literal. Deliberately not `too_narrow_text(169)`: a test that
/// compares rendered text against the generator that produced it passes whatever the generator
/// says, including a generator that stopped naming the width at all.
const REFUSAL: &str = "TERMINAL TOO NARROW FOR THE KEY MATRIX: IT NEEDS 169 COLUMNS";

/// Reassembles the body's message block from whatever `draw` rendered: finds the line holding
/// `needle`, takes every following line's slice from that same column until a blank one, and
/// joins the pieces with single spaces. The block is contiguous, so starting at the locked
/// banner returns the banner and the refusal below it together, which is exactly the claim worth
/// asserting. The footer row is excluded by the caller, so a message that ran into it cannot
/// borrow the footer's text to look complete. A clipped message, a message missing its tail, or
/// no message at all all fail to reproduce the expected sentence, which is the point: the
/// operator must never be left with a fragment.
fn message_from(lines: &[String], needle: &str) -> String {
    let (y, x0) = lines
        .iter()
        .enumerate()
        .find_map(|(y, l)| l.find(needle).map(|x| (y, x)))
        .unwrap_or_else(|| panic!("{needle:?} must render somewhere: {lines:?}"));
    let mut parts = Vec::new();
    for line in lines.iter().skip(y) {
        let chars: Vec<char> = line.chars().collect();
        let piece: String = chars.iter().skip(x0).collect::<String>().trim().to_string();
        if piece.is_empty() {
            break;
        }
        parts.push(piece);
    }
    parts.join(" ")
}

/// The body's rendered lines: everything above the footer row. The refusal must be complete
/// within the body, never running into the footer for its own last word.
fn body_lines(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let mut lines = terminal_lines(terminal);
    lines.pop();
    lines
}

/// The 68-key ANSI-DK layout's widest row, computed from `cap_units` through `needed_width`: the
/// QWERTY row, tab (1.5) plus twelve 1.0 caps plus backslash (1.5) plus the row-end key, at 7
/// columns per unit. The whole frame therefore wants this plus the 56-column left pane, and the
/// refusal in the width sweep below states exactly that total.
#[test]
fn a_full_ansi_dk_board_needs_113_columns_for_its_widest_row() {
    let board = ansi_dk_board();
    assert_eq!(
        board.rows.iter().map(|r| r.keys.len()).sum::<usize>(),
        68,
        "the fixture must be a full 68-key board"
    );
    assert_eq!(needed_width(&board.rows), 113);
}

/// The refusal at real terminal widths. 169 is the first width that fits the matrix (113 plus
/// the 56-column left pane); every width below it must show the whole sentence, wherever it has
/// to go: the matrix pane at 57 columns is one column wide, and at 56 or less it does not exist.
#[test]
fn every_width_shows_either_the_matrix_or_the_whole_refusal() {
    for width in [50u16, 56, 57, 64, 71, 72, 80, 93, 94, 168, 169] {
        let mut app = App::new(ansi_dk_board(), "0.5.0-alpha");
        let mut terminal = Terminal::new(TestBackend::new(width, 40)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let lines = body_lines(&terminal);

        if width >= 169 {
            assert_eq!(
                app.key_rects.len(),
                68,
                "at {width} columns the matrix itself must render: {lines:?}"
            );
            assert!(
                !lines.iter().any(|l| l.contains("TOO NARROW")),
                "at {width} columns there must be no refusal: {lines:?}"
            );
        } else {
            assert!(
                app.key_rects.is_empty(),
                "at {width} columns no cap fits, so none may be recorded: {lines:?}"
            );
            assert_eq!(
                message_from(&lines, "TERMINAL TOO NARROW"),
                REFUSAL,
                "at {width} columns the operator must read the whole refusal: {lines:?}"
            );
        }
    }
}

/// The same rule on the height axis, and with the locked banner competing for the same rows.
/// 50x20 leaves the body four rows for two settings rows, the banner and a two-line refusal;
/// 64x24 fits the refusal on one line but only just; 64x40 has room to spare. At all three the
/// banner must be whole (it is a sentence too, and half of one is worse than none) and the
/// refusal must be whole, neither run into the footer nor overwritten by the other.
#[test]
fn a_locked_board_shows_both_the_banner_and_the_whole_refusal_at_every_height() {
    for (width, height) in [(50u16, 20u16), (64, 24), (64, 40)] {
        let mut app = App::new(ansi_dk_board(), "0.5.0-alpha");
        app.locked = true;
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let lines = body_lines(&terminal);

        // One assertion over the whole block: the banner entire, then the refusal entire, in
        // that order, with nothing lost between them and nothing clipped at either end.
        assert_eq!(
            message_from(&lines, "BOARD LOCKED"),
            format!("{LOCKED_BANNER} {REFUSAL}"),
            "at {width}x{height} both sentences must render whole: {lines:?}"
        );
    }
}

#[test]
fn caps_within_a_row_render_in_column_order_not_input_order() {
    // A single row whose keys are deliberately out of column order: col 5 listed before col 0.
    // The row-pair's `Vec<(col, usage)>` isn't guaranteed sorted by whoever built it, so
    // `render_matrix` must sort by column itself rather than trusting input order.
    let rows = vec![DefKeyRow {
        row: 0,
        keys: vec![(5, 0x16), (0, 0x1A)],
    }];
    let area = Rect::new(0, 0, 40, 10);
    let mut buf = Buffer::empty(area);
    let mut rects = Vec::new();
    let selected = HashSet::new();

    render_matrix(
        area,
        &mut buf,
        &rows,
        |_usage| CapValue {
            show: false,
            text: String::new(),
        },
        &selected,
        &mut rects,
    );

    let col0_rect = rects
        .iter()
        .find(|&&(_, usage)| usage == 0x1A)
        .expect("the col-0 key (0x1A) must be recorded")
        .0;
    let col5_rect = rects
        .iter()
        .find(|&&(_, usage)| usage == 0x16)
        .expect("the col-5 key (0x16) must be recorded")
        .0;
    assert!(
        col0_rect.x < col5_rect.x,
        "the col-0 cap must render left of the col-5 cap: col0 {col0_rect:?}, col5 {col5_rect:?}"
    );
}
