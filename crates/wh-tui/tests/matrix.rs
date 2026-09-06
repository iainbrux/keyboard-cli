mod support;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::Terminal;
use std::collections::HashSet;
use support::{ansi_dk_board, wasd_board};
use wh_proto::cmds::DefKeyRow;
use wh_tui::app::{draw, App, Tab, LOCKED_BANNER};
use wh_tui::matrix::{cap_units, key_at, needed_width, render_matrix, CapValue};

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

/// A one-key-per-row fixture: each row is independent, so a cap's rendered width is exactly
/// `round(cap_units(usage) * 7)` with no cumulative interaction from a neighbour, letting these
/// label tests pin an exact cell width by hand.
fn one_key_rows(usages: &[u8]) -> Vec<DefKeyRow> {
    usages
        .iter()
        .enumerate()
        .map(|(i, &usage)| DefKeyRow {
            row: i as u8,
            keys: vec![(0, usage)],
        })
        .collect()
}

#[test]
fn caps_show_the_vendors_display_labels_for_punctuation_modifiers_and_fn() {
    // semicolon (0x33), left shift (0xE1), right shift (0xE5), the unnamed FN usage (0x01).
    let rows = one_key_rows(&[0x33, 0xE1, 0xE5, 0x01]);
    let area = Rect::new(0, 0, 40, 20);
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

    let lines = buffer_lines(&buf);
    let rect_of = |usage: u8| {
        rects
            .iter()
            .find(|&&(_, u)| u == usage)
            .unwrap_or_else(|| panic!("{usage:#04x}'s cap must be recorded"))
            .0
    };

    assert_eq!(
        cap_line(&lines, rect_of(0x33), 1),
        "│  ;  │",
        "semicolon must show the vendor's own glyph, not the selector name SEMIC: {lines:?}"
    );
    assert_eq!(
        cap_line(&lines, rect_of(0xE1), 1),
        "│    SHIFT     │",
        "left shift must show SHIFT: {lines:?}"
    );
    assert_eq!(
        cap_line(&lines, rect_of(0xE5), 1),
        "│  SHIFT   │",
        "right shift must show SHIFT too: {lines:?}"
    );
    assert_eq!(
        cap_line(&lines, rect_of(0x01), 1),
        "│ FN  │",
        "the unnamed FN usage (0x01) must show FN: {lines:?}"
    );
}

#[test]
fn arrow_caps_render_a_single_column_wide_glyph_and_still_abut() {
    // left (0x50) then right (0x4F) in one row: both plain 1.0-unit caps, so if the glyph were
    // double-width (or otherwise miscounted) the second cap's rect would not start exactly where
    // the first one's ends.
    let rows = vec![DefKeyRow {
        row: 0,
        keys: vec![(0, 0x50), (1, 0x4F)],
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

    let lines = buffer_lines(&buf);
    let left_rect = rects.iter().find(|&&(_, u)| u == 0x50).unwrap().0;
    let right_rect = rects.iter().find(|&&(_, u)| u == 0x4F).unwrap().0;

    assert_eq!(left_rect.width, 7, "left's cap is a plain 1.0-unit cap");
    assert_eq!(
        right_rect.x,
        left_rect.x + left_rect.width,
        "right's cap must abut left's, no gap column between them"
    );
    assert_eq!(
        cap_line(&lines, left_rect, 1),
        "│  \u{2190}  │",
        "left's glyph must occupy exactly one cell: {lines:?}"
    );
    assert_eq!(
        cap_line(&lines, right_rect, 1),
        "│  \u{2192}  │",
        "right's glyph must occupy exactly one cell: {lines:?}"
    );
}

#[test]
fn every_1u_cap_is_the_same_width_and_adjacent_caps_abut_exactly() {
    let board = ansi_dk_board();
    let area = Rect::new(0, 0, 120, 30);
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

    // Two adjacent plain 1.0-unit caps in the QWERTY row: q (0x14) then w (0x1A).
    let q_rect = rects.iter().find(|&&(_, u)| u == 0x14).unwrap().0;
    let w_rect = rects.iter().find(|&&(_, u)| u == 0x1A).unwrap().0;
    assert_eq!(q_rect.width, 7, "a plain 1.0-unit cap is 7 columns");
    assert_eq!(w_rect.width, q_rect.width, "every 1u cap is the same width");
    assert_eq!(
        w_rect.x,
        q_rect.x + q_rect.width,
        "adjacent caps abut exactly, x2 == x1 + width"
    );
}

/// The vendor's rows all share one right edge (measured against
/// `research/vendor-bundle/2026-09-05/screenshots/01-actuation-point.png`: every row's border
/// starts and ends at the same pixel column). All five `ansi_dk_board` rows sum to the same 16.0
/// `cap_units` total (`lctrl`/`lgui`/`lalt` at 1.25 make the bottom row's total match too), so
/// once widths stop drifting from per-key rounding all five rows' right edges land on one column.
#[test]
fn rows_with_equal_cap_units_totals_end_at_the_same_right_edge() {
    let board = ansi_dk_board();
    let area = Rect::new(0, 0, 120, 30);
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

    let row_right_edge = |usage_in_row: u8| {
        rects
            .iter()
            .find(|&&(_, u)| u == usage_in_row)
            .map(|&(r, _)| r.x + r.width)
            .unwrap_or_else(|| panic!("{usage_in_row:#04x}'s cap must be recorded"))
    };

    // One key from each row, each the last key `render_matrix` draws for its row.
    let number_row_end = row_right_edge(0x4C); // delete
    let qwerty_row_end = row_right_edge(0x4A); // home
    let home_row_end = row_right_edge(0x4B); // pageup
    let bottom_letter_row_end = row_right_edge(0x4E); // pagedown
    let space_row_end = row_right_edge(0x4F); // right

    assert_eq!(
        [
            qwerty_row_end,
            home_row_end,
            bottom_letter_row_end,
            space_row_end
        ],
        [number_row_end; 4],
        "rows summing to the same cap_units total must end at the same column"
    );
}

/// The row every `cap_units` proportion edit must keep flush: five rows of a real board sum to
/// the same total, a fact this crate leans on for right-edge alignment (see the test above), so
/// a future edit to any one entry (say widening `enter` without touching anything else) must fail
/// this test before it ships a row that no longer lines up.
#[test]
fn every_rows_cap_units_sum_to_the_same_total() {
    let board = ansi_dk_board();
    assert_eq!(board.rows.len(), 5, "the fixture must be five rows");
    let totals: Vec<f32> = board
        .rows
        .iter()
        .map(|row| row.keys.iter().map(|&(_, u)| cap_units(u)).sum())
        .collect();
    for &total in &totals {
        assert_eq!(
            total, totals[0],
            "every row must sum to the same cap_units total: {totals:?}"
        );
    }
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

/// The whole refusal, as one literal. Deliberately not `too_narrow_text(176)`: a test that
/// compares rendered text against the generator that produced it passes whatever the generator
/// says, including a generator that stopped naming the width at all.
const REFUSAL: &str = "TERMINAL TOO NARROW FOR THE KEY MATRIX: IT NEEDS 176 COLUMNS";

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
/// columns per unit, summing to the same 16.0 units as every other row (measured against the
/// vendor's own rendering, `research/vendor-bundle/2026-09-05/screenshots/`, whose rows all share
/// one right edge). The whole frame therefore wants this plus the 64-column left pane, and the
/// refusal in the width sweep below states exactly that total.
#[test]
fn a_full_ansi_dk_board_needs_112_columns_for_its_widest_row() {
    let board = ansi_dk_board();
    assert_eq!(
        board.rows.iter().map(|r| r.keys.len()).sum::<usize>(),
        68,
        "the fixture must be a full 68-key board"
    );
    assert_eq!(needed_width(&board.rows), 112);
}

/// The refusal at real terminal widths. 176 is the first width that fits the matrix (112 plus
/// the 64-column left pane); every width below it must show the whole sentence, wherever it has
/// to go: the matrix pane at 65 columns is one column wide, and at 64 or less it does not exist.
#[test]
fn every_width_shows_either_the_matrix_or_the_whole_refusal() {
    for width in [50u16, 64, 65, 72, 79, 80, 88, 101, 102, 175, 176] {
        let mut app = App::new(ansi_dk_board(), "0.5.0-alpha");
        let mut terminal = Terminal::new(TestBackend::new(width, 50)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let lines = body_lines(&terminal);

        if width >= 176 {
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

/// The vendor's two top-aligned panes, pixel-scanned against
/// `research/vendor-bundle/2026-09-05/screenshots/01-actuation-point.png` and `05-mapping.png`:
/// the right pane's own prompt shares the TAB ROW itself (y=25, pinned independently by
/// `chrome.rs`'s own row-render test), past the 64-column left pane; the matrix's first cap row
/// starts at `body_y` (26), level with the left pane's first settings row, unchanged from before
/// this round.
#[test]
fn the_prompt_shares_the_tab_row_and_the_matrix_starts_at_body_y() {
    let tab_row_y = 25u16;
    let body_y = 26u16;
    let mut app = App::new(ansi_dk_board(), "0.5.0-alpha");
    let mut terminal = Terminal::new(TestBackend::new(200, 50)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();

    let prompt_rect = app
        .prompt_action_rect
        .expect("the AP tab must record a prompt action rect");
    assert_eq!(
        prompt_rect.y, tab_row_y,
        "the prompt must render on the tab row, not a row of its own below it"
    );

    // The prompt's own status text starts exactly at column 64, right after the left pane: not
    // just that the prompt renders somewhere on the tab row, but that it starts where the left
    // pane ends.
    let buf = terminal.backend().buffer().clone();
    let tab_row_line: String = (0..buf.area.width)
        .map(|x| buf[(x, tab_row_y)].symbol().to_string())
        .collect();
    assert!(
        tab_row_line[64..].starts_with("> CLICK ON THE KEYS TO MAKE A KEYSET"),
        "the prompt must start at column 64: {tab_row_line:?}"
    );

    let matrix_top = app
        .key_rects
        .iter()
        .map(|(rect, _)| rect.y)
        .min()
        .expect("the matrix must render at this width");
    assert_eq!(
        matrix_top, body_y,
        "the matrix's first cap row must start at body_y, level with the settings, not below the prompt"
    );
}

/// Reproduces an operator-reported defect (2026-09-06): with the matrix back at `body_y`, a
/// status note's `note_row` can land mid-matrix (here, on `w`'s own value line), and the message
/// block used to paint it at the frame's full width, blanking whatever cap cell sat there. Uses
/// `wasd_board`'s `w` (first cap, first row) rather than a real board's ESC: no fixture combines
/// the full ANSI-DK rows with populated per-key AP values, and `w` sits at the same relative
/// position (first settings row's own y) that the operator's ESC row did.
#[test]
fn a_status_note_renders_left_of_the_matrix_and_never_overwrites_a_cap() {
    let mut app = App::new(wasd_board(), "0.5.0-alpha");
    app.status = Some(wh_tui::app::STATUS_UNKNOWN_EVENT.to_string());
    let mut terminal = Terminal::new(TestBackend::new(200, 50)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let lines = terminal_lines(&terminal);

    let w_rect = app
        .key_rects
        .iter()
        .find(|&&(_, usage)| usage == 0x1A)
        .expect("w's cap must be recorded")
        .0;
    assert_eq!(
        cap_line(&lines, w_rect, 1),
        "│  W  │",
        "w's label line must survive the note: {lines:?}"
    );
    assert_eq!(
        cap_line(&lines, w_rect, 2),
        "│2.00 │",
        "w's value line must survive the note, not be blanked by it: {lines:?}"
    );

    // The note itself still renders, within the first 64 columns of its own row: not a separate
    // row from w's cap (both are row 28, w's value line), just the left pane's own share of it.
    let note_y = lines
        .iter()
        .position(|l| l.contains("UNRECOGNISED BOARD EVENT"))
        .expect("the status note must render somewhere");
    let left_slice: String = lines[note_y].chars().take(64).collect();
    assert!(
        left_slice.starts_with(wh_tui::app::STATUS_UNKNOWN_EVENT),
        "the note must render within the left pane's own 64 columns: {:?}",
        lines[note_y]
    );
}

/// Asserts none of `key_rects` was overwritten: the perimeter only, not the interior (a cap's own
/// value line is legitimately blank when the board carries no per-key settings, but its border is
/// a box-drawing glyph on every cell, always, so any blank there proves something else painted
/// over it), plus the label row, which is never blank since every key has a non-empty `cap_label`.
fn assert_no_cap_overwritten(buf: &Buffer, lines: &[String], key_rects: &[(Rect, u8)]) {
    for &(rect, usage) in key_rects {
        for x in rect.x..rect.x + rect.width {
            assert_ne!(
                buf[(x, rect.y)].symbol(),
                " ",
                "usage {usage:#04x}'s top border ({x},{}) must not be blanked: {lines:?}",
                rect.y
            );
            assert_ne!(
                buf[(x, rect.y + rect.height - 1)].symbol(),
                " ",
                "usage {usage:#04x}'s bottom border ({x},{}) must not be blanked: {lines:?}",
                rect.y + rect.height - 1
            );
        }
        for y in rect.y..rect.y + rect.height {
            assert_ne!(
                buf[(rect.x, y)].symbol(),
                " ",
                "usage {usage:#04x}'s left border ({},{y}) must not be blanked: {lines:?}",
                rect.x
            );
            assert_ne!(
                buf[(rect.x + rect.width - 1, y)].symbol(),
                " ",
                "usage {usage:#04x}'s right border ({},{y}) must not be blanked: {lines:?}",
                rect.x + rect.width - 1
            );
        }
        let label_line = cap_line(lines, rect, 1);
        assert!(
            label_line.chars().any(|c| c != '│' && c != ' '),
            "usage {usage:#04x}'s label row must not be blank: {label_line:?}"
        );
    }
}

/// The locked banner is 96 columns, wider than the 64-column left pane, so on a frame wide enough
/// for the matrix to render it must wrap there rather than paint across it: this is the same
/// defect as the status-note test above, mutated to the multi-line case, and it must not overwrite
/// any of the 68 recorded cap rects.
#[test]
fn a_locked_banner_on_a_wide_frame_never_overwrites_a_cap() {
    let mut app = App::new(ansi_dk_board(), "0.5.0-alpha");
    app.locked = true;
    let mut terminal = Terminal::new(TestBackend::new(200, 50)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let lines = terminal_lines(&terminal);

    assert_eq!(
        app.key_rects.len(),
        68,
        "the matrix must render whole at this width: {lines:?}"
    );
    assert_no_cap_overwritten(&buf, &lines, &app.key_rects);
}

/// `render_message_block`'s push-up (see its own doc comment) still has to hold once the message
/// is bounded to the left pane's own width: at these heights the two-line banner does not fit
/// after the settings rows and gets pushed up over them, while the matrix (wide enough to render
/// its first row, RAPID TRIGGER's own four fixed settings rows put `note_row` past the body's end
/// at these exact heights) sits untouched to the right throughout.
#[test]
fn the_locked_banner_pushes_up_over_settings_without_touching_the_matrix() {
    for height in [31u16, 32, 33] {
        let mut app = App::new(ansi_dk_board(), "0.5.0-alpha");
        app.tab = Tab::RapidTrigger;
        app.locked = true;
        let mut terminal = Terminal::new(TestBackend::new(200, height)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let lines = terminal_lines(&terminal);

        assert!(
            !app.key_rects.is_empty(),
            "at height {height} the matrix's first row must still render: {lines:?}"
        );
        let left_slices: Vec<String> = lines.iter().map(|l| l.chars().take(64).collect()).collect();
        assert!(
            left_slices
                .windows(2)
                .any(|w| format!("{} {}", w[0].trim(), w[1].trim()) == LOCKED_BANNER),
            "at height {height} the whole banner must appear, wrapped, in the left pane: {lines:?}"
        );
        assert_no_cap_overwritten(&buf, &lines, &app.key_rects);
    }
}

/// The same rule on the height axis, and with the locked banner competing for the same rows.
/// The chrome above the body is 26 rows, so 50x31 leaves the body four rows for two settings
/// rows, the banner and a two-line refusal; 64x35 fits the refusal on one line but only just;
/// 64x51 has room to spare. At all three the banner must be whole (it is a sentence too, and
/// half of one is worse than none) and the refusal must be whole, neither run into the footer
/// nor overwritten by the other.
#[test]
fn a_locked_board_shows_both_the_banner_and_the_whole_refusal_at_every_height() {
    for (width, height) in [(50u16, 31u16), (64, 35), (64, 51)] {
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
