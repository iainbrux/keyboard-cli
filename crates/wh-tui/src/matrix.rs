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

/// A row's per-key column widths, in the order given. Each key's width is the difference between
/// two roundings of the row's own running unit total, not `cap_units(usage) * CAP_UNIT_COLS`
/// rounded in isolation: rounding every key independently drifts by up to a column whenever a
/// row's fractional units (`tab`'s 1.5, `capslock`'s 1.75, and so on) do not clear to a whole
/// number until a later key, which is what left one row a column wider than its neighbours despite
/// `cap_units` summing to the same total for every row of a real board. This way the row's own
/// total always lands on `round(total_units * CAP_UNIT_COLS)` exactly, with no drift left for the
/// last key to absorb, and every plain 1.0-unit key still renders at exactly `CAP_UNIT_COLS`
/// regardless of position: adding a whole number never changes what an earlier fraction rounds to.
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

/// Columns the widest of `rows` needs, which is the width the pane must have before
/// `render_matrix` draws anything. Computed from `cap_units`: a full ANSI-DK 68-key board's
/// widest row (measured 2026-09-06 against `research/vendor-bundle/2026-09-05/screenshots/`,
/// tab, twelve 1.0 caps and backslash, all summing with the row-end key to the same 16.0 units
/// as every other row) needs 112 columns, so with the 56-column left pane beside it the whole
/// frame wants 168: more than a 120-column terminal has, which is why the refusal names the
/// number rather than only refusing. The 112 is what the ANSI-DK layout implies, not a DEFKEY
/// read; the refusal always states the figure for the rows the board itself reported.
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

/// Renders `rows` as a grid of bordered caps into `area`, left to right per row in column order,
/// rows stacked with no gap. If the widest row does not fit `area`'s width, draws nothing and
/// leaves `rects` empty: a clipped matrix would be worse than none. Callers check `needed_width`
/// first and render `too_narrow_text` themselves, because at the widths where this refuses the
/// message often does not fit this pane either. `selected` caps render with `Modifier::REVERSED`
/// over the whole cell (border included).
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
                buf.set_style(rect, Style::default().add_modifier(Modifier::REVERSED));
            }

            rects.push((rect, usage));
            x += width;
        }
        y += CAP_HEIGHT;
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
