//! The key matrix widget: a keyboard-shaped grid of bordered caps, laid out from the board's own
//! `DefKeyRow`s so its shape always follows what the device actually reports rather than an
//! assumed layout.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders};
use std::collections::HashSet;
use wh_proto::cmds::DefKeyRow;
use wh_proto::keys::{label, name_for_usage};

/// The vendor's own display label for a cap, used only by `render_matrix`: never fed back into
/// `wh_proto::keys`, whose selector vocabulary is the CLI's own contract and stays as it is.
/// Everything not listed here falls back to `label(usage)` uppercased, the CLI selector name.
/// Read off the vendor's own rendering (`research/vendor-bundle/2026-09-05/screenshots/`), not a
/// standard to import: `- = [ ] \ ; ' , . /` for punctuation, `CAPS`, `SHIFT` and `CTRL` on both
/// sides, `WIN`, `ALT` on both sides, and glyph arrows for the arrow cluster.
pub fn cap_label(usage: u8) -> String {
    match name_for_usage(usage) {
        Some("minus") => "-".to_string(),
        Some("equals") => "=".to_string(),
        Some("lbracket") => "[".to_string(),
        Some("rbracket") => "]".to_string(),
        Some("backslash") => "\\".to_string(),
        Some("semicolon") => ";".to_string(),
        Some("quote") => "'".to_string(),
        Some("comma") => ",".to_string(),
        Some("period") => ".".to_string(),
        Some("slash") => "/".to_string(),
        Some("capslock") => "CAPS".to_string(),
        Some("lshift") | Some("rshift") => "SHIFT".to_string(),
        Some("lctrl") | Some("rctrl") => "CTRL".to_string(),
        Some("lgui") | Some("rgui") => "WIN".to_string(),
        Some("lalt") | Some("ralt") => "ALT".to_string(),
        Some("left") => "\u{2190}".to_string(),
        Some("down") => "\u{2193}".to_string(),
        Some("right") => "\u{2192}".to_string(),
        Some("up") => "\u{2191}".to_string(),
        // Usage 0x01 is deliberately unnamed in `wh_proto::keys` (see keys.rs: confirming it
        // means remapping FN away, and FN is the layer that would undo that). The vendor's own
        // rendering labels it FN regardless, so this is the vendor's label, not a measured
        // protocol name.
        None if usage == 0x01 => "FN".to_string(),
        _ => label(usage).to_uppercase(),
    }
}

/// Width of one key cap in units of a standard cap, from the key's name. Everything not
/// listed is 1.0. These are the ANSI-DK proportions read off the vendor's own rendering
/// (research/vendor-bundle/2026-09-05/screenshots/), not a standard to import.
pub fn cap_units(usage: u8) -> f32 {
    match name_for_usage(usage) {
        Some("backspace") => 2.0,
        Some("tab") => 1.5,
        Some("backslash") => 1.5,
        Some("capslock") => 1.75,
        Some("enter") => 2.25,
        Some("lshift") => 2.25,
        Some("rshift") => 1.75,
        Some("space") => 6.25,
        // Measured at ~1.257 against the baseline 1u cap on the vendor's own bottom row
        // (research/vendor-bundle/2026-09-05/screenshots/01-actuation-point.png): the only
        // value that also makes that row's units sum to 16.0, the same total as every other row.
        Some("lctrl") | Some("lgui") | Some("lalt") => 1.25,
        _ => 1.0,
    }
}

/// One cap's value line. `show: false` renders it blank, for a key a tab has nothing to say
/// about rather than stale or wrong text.
pub struct CapValue {
    pub show: bool,
    pub text: String,
}

/// Each cap is this many rows tall: top border, label, value, bottom border.
const CAP_HEIGHT: u16 = 4;
/// Columns in one standard (1.0 unit) cap.
const CAP_UNIT_COLS: f32 = 7.0;

/// A row's per-key column widths: each key's width is the difference between two roundings of
/// the row's own running unit total, not `cap_units(usage) * CAP_UNIT_COLS` rounded per key in
/// isolation, which drifts by up to a column whenever a row's fractional units do not clear to a
/// whole number until a later key. A plain 1.0-unit key still renders at `CAP_UNIT_COLS` always.
fn row_widths(usages: &[u8]) -> Vec<u16> {
    let mut widths = Vec::with_capacity(usages.len());
    let mut prev_cols = 0u16;
    let mut cum_units = 0.0f32;
    for &usage in usages {
        cum_units += cap_units(usage);
        let cols = (cum_units * CAP_UNIT_COLS).round() as u16;
        widths.push(cols - prev_cols);
        prev_cols = cols;
    }
    widths
}

/// Columns the widest of `rows` needs, before `render_matrix` draws anything. A full ANSI-DK
/// 68-key board's widest row needs 112 (every row sums to the same 16.0 `cap_units` total,
/// measured against `research/vendor-bundle/2026-09-05/screenshots/`), so with the 64-column
/// left pane beside it the whole frame wants 176; the refusal states the figure for the rows read.
pub fn needed_width(rows: &[DefKeyRow]) -> u16 {
    rows.iter()
        .map(|row| {
            let units: f32 = row.keys.iter().map(|&(_, u)| cap_units(u)).sum();
            (units * CAP_UNIT_COLS).round() as u16
        })
        .max()
        .unwrap_or(0)
}

/// The refusal shown in the matrix's place, naming the frame width the operator has to reach.
/// `frame_cols` is the whole frame, left pane included, not the matrix pane alone: resizing a
/// terminal is the only thing the operator can do about it.
pub fn too_narrow_text(frame_cols: u16) -> String {
    format!("TERMINAL TOO NARROW FOR THE KEY MATRIX: IT NEEDS {frame_cols} COLUMNS")
}

/// Renders `rows` as a grid of bordered caps into `area`, left to right per row in column order.
/// An empty `DefKeyRow` (see the filter below) consumes no vertical space; populated rows share
/// horizontal borders, the vendor's own cadence (see the `y +=` comment below for the arithmetic).
/// If the widest row does not fit `area`'s width, draws nothing and leaves `rects` empty: a
/// clipped matrix would be worse than none.
pub fn render_matrix(
    area: Rect,
    buf: &mut Buffer,
    rows: &[DefKeyRow],
    value_of: impl Fn(u8) -> CapValue,
    selected: &HashSet<u8>,
    rects: &mut Vec<(Rect, u8)>,
) {
    rects.clear();

    let row_caps: Vec<Vec<(u8, u16)>> = rows
        .iter()
        .map(|row| {
            let mut keys = row.keys.clone();
            keys.sort_by_key(|&(col, _)| col);
            let usages: Vec<u8> = keys.iter().map(|&(_, usage)| usage).collect();
            usages.iter().copied().zip(row_widths(&usages)).collect()
        })
        // A real board's own DEFKEY read leaves at least one of its six logical rows empty
        // (measured 2026-09-06); dropped here, before any row is positioned, not drawn as a
        // blank `CAP_HEIGHT` gap.
        .filter(|caps: &Vec<(u8, u16)>| !caps.is_empty())
        .collect();

    if area.width == 0 || area.height == 0 || needed_width(rows) > area.width {
        return;
    }

    let mut y = area.y;
    for caps in &row_caps {
        if y.saturating_add(CAP_HEIGHT) > area.y + area.height {
            break;
        }
        let mut x = area.x;
        for &(usage, width) in caps {
            if width == 0 || x + width > area.x + area.width {
                break;
            }
            let rect = Rect::new(x, y, width, CAP_HEIGHT);
            let block = Block::default().borders(Borders::ALL);
            let inner = block.inner(rect);
            block.render(rect, buf);

            let label_line = center(&cap_label(usage), inner.width as usize);
            buf.set_string(inner.x, inner.y, &label_line, Style::default());

            let value = value_of(usage);
            let value_line = if value.show {
                center(&value.text, inner.width as usize)
            } else {
                " ".repeat(inner.width as usize)
            };
            buf.set_string(inner.x, inner.y + 1, &value_line, Style::default());

            if selected.contains(&usage) {
                // Top border, label, value: never the bottom border, which the row below draws
                // over on the very next iteration regardless (default style), silently erasing
                // anything set here. See the report for the full reasoning.
                let styled = Rect::new(rect.x, rect.y, rect.width, CAP_HEIGHT - 1);
                buf.set_style(styled, Style::default().add_modifier(Modifier::REVERSED));
            }

            // The full drawn rect, unshrunk: rows are pushed top to bottom, so `key_at`'s
            // first-match search resolves a shared row to the cap above with no shrinking
            // needed, the vendor's own hit-test convention.
            rects.push((rect, usage));
            x += width;
        }
        // Always 3, not 4: the next row's own top border reuses this row's own bottom border
        // row, so only 3 new rows of vertical space are actually spent on it.
        y += CAP_HEIGHT - 1;
    }
}

/// Centres `text` within `width` columns, truncating to fit and padding with spaces so the
/// returned string is always exactly `width` columns wide: callers write it whole, with no
/// separate blank-fill step of their own.
fn center(text: &str, width: usize) -> String {
    let truncated: String = text.chars().take(width).collect();
    let pad = width.saturating_sub(truncated.chars().count());
    let left = pad / 2;
    let right = pad - left;
    format!("{}{}{}", " ".repeat(left), truncated, " ".repeat(right))
}

/// Which key's cap, if any, contains `(col, row)`. A point in a border, in the space past the
/// last cap of a row, or outside the grid entirely is a miss (`None`), the same as a click that
/// landed nowhere the matrix drew.
pub fn key_at(rects: &[(Rect, u8)], col: u16, row: u16) -> Option<u8> {
    rects
        .iter()
        .find(|(rect, _)| {
            col >= rect.x
                && col < rect.x + rect.width
                && row >= rect.y
                && row < rect.y + rect.height
        })
        .map(|&(_, usage)| usage)
}
