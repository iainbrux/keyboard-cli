//! The key matrix widget: a keyboard-shaped grid of bordered caps, laid out from the board's own
//! `DefKeyRow`s so its shape always follows what the device actually reports rather than an
//! assumed layout.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders};
use std::collections::HashSet;
use wh_proto::cmds::DefKeyRow;
use wh_proto::keys::{label, name_for_usage};

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
const TOO_NARROW: &str = "TERMINAL TOO NARROW FOR THE KEY MATRIX";

/// Renders `rows` as a grid of bordered caps into `area`, left to right per row in column order,
/// rows stacked with no gap. If the widest row does not fit `area`'s width, renders a single
/// centred warning line instead and leaves `rects` empty: a clipped or wrapped matrix would be
/// worse than an honest refusal. `selected` caps render with `Modifier::REVERSED` over the whole
/// cell (border included).
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
            keys.into_iter()
                .map(|(_, usage)| (usage, (cap_units(usage) * CAP_UNIT_COLS).round() as u16))
                .collect()
        })
        .collect();

    let widest = row_caps
        .iter()
        .map(|caps| caps.iter().map(|&(_, w)| w).sum::<u16>())
        .max()
        .unwrap_or(0);

    if area.width == 0 || area.height == 0 || widest > area.width {
        if area.width > 0 && area.height > 0 {
            let text_width = TOO_NARROW.chars().count() as u16;
            let x = area.x + area.width.saturating_sub(text_width) / 2;
            let y = area.y + area.height / 2;
            buf.set_string(x, y, TOO_NARROW, Style::default());
        }
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

            let label_line = center(&label(usage).to_uppercase(), inner.width as usize);
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
