mod support;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use std::collections::HashSet;
use support::wasd_board;
use wh_proto::cmds::DefKeyRow;
use wh_tui::matrix::{key_at, render_matrix, CapValue};

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
