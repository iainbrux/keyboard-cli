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
/// Columns per unit for the lattice's own interior-plus-one-shared-border, not 7 (the old,
/// pre-lattice width of an isolated 1.0-unit cap, both its own borders counted): a join with a
/// neighbour now reuses one border column instead of drawing a second one, so it costs one column
/// less per unit than that. Chosen close to that previous density (7) rather than re-measured,
/// since it is a rendering choice (how many columns represent one keycap unit), not a proportion
/// read off the vendor's own screenshot the way `cap_units` itself is.
const LATTICE_COLS_PER_UNIT: f32 = 6.0;

/// A row's own column boundaries, one more entry than its own key count: `boundaries[i]` is where
/// the cumulative unit total after `i` keys lands, `round(cumulative_units * LATTICE_COLS_PER_UNIT)`,
/// relative to the row's own start (`boundaries[0] == 0`, always). Because this maps a cumulative
/// unit total directly to a column through one fixed scale, not a per-row telescoping sum
/// renormalised to that row's own total, two different rows whose cumulative unit totals agree
/// land on the very same column, regardless of how many keys or which fractional key (1.75u
/// `capslock`, 1.75u `rshift`, each in a different row) got them there: the drift a per-row-local
/// sum could introduce depending on where in its own row a fractional key happened to sit cannot
/// occur, since no row-local sum is ever computed.
fn unit_boundaries(usages: &[u8]) -> Vec<u16> {
    let mut boundaries = Vec::with_capacity(usages.len() + 1);
    boundaries.push(0);
    let mut cum_units = 0.0f32;
    for &usage in usages {
        cum_units += cap_units(usage);
        boundaries.push((cum_units * LATTICE_COLS_PER_UNIT).round() as u16);
    }
    boundaries
}

/// A row's per-key column widths, from `unit_boundaries`: key `i`'s own rect spans
/// `boundaries[i]` to `boundaries[i + 1]` *inclusive*, both its own borders, so its width is that
/// difference plus one. Consecutive keys therefore overlap by exactly one column, the shared
/// lattice seam `render_matrix`'s own horizontal advance (`x += width - 1`) consumes.
fn row_widths(usages: &[u8]) -> Vec<u16> {
    unit_boundaries(usages)
        .windows(2)
        .map(|w| w[1] - w[0] + 1)
        .collect()
}

/// Columns the widest of `rows` needs, before `render_matrix` draws anything: one row's own total
/// unit count mapped straight through `unit_boundaries`' own scale, `round(units *
/// LATTICE_COLS_PER_UNIT) + 1` (the `+ 1` for the row's own two outermost borders, only one of
/// which is ever shared with anything). Because this is a pure function of a row's own unit total,
/// **every row whose own total agrees lands on exactly the same width**, regardless of its own key
/// count: the full ANSI-DK board's five rows all share the same 16.0 `cap_units` total (measured
/// against `research/vendor-bundle/2026-09-05/screenshots/`), so they are all 97 columns wide,
/// tied, not merely close. With the 64-column left pane and its own 2-column gutter beside it the
/// whole frame wants 163; the refusal states the figure for the rows read.
pub fn needed_width(rows: &[DefKeyRow]) -> u16 {
    rows.iter()
        .filter(|row| !row.keys.is_empty())
        .map(|row| {
            let units: f32 = row.keys.iter().map(|&(_, u)| cap_units(u)).sum();
            ((units * LATTICE_COLS_PER_UNIT).round() as u16).saturating_add(1)
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
    let mut prev_row_rects: Option<Vec<Rect>> = None;
    for caps in &row_caps {
        if y.saturating_add(CAP_HEIGHT) > area.y + area.height {
            break;
        }
        let mut x = area.x;
        let mut row_rects: Vec<Rect> = Vec::with_capacity(caps.len());
        for (i, &(usage, width)) in caps.iter().enumerate() {
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
                //
                // The right border column, too, whenever a cap follows in this row: that column
                // is the shared lattice seam (see the `x +=` comment below), and the next cap's
                // own `block.render` redraws it, unstyled, on the very next loop iteration, the
                // same fate as the bottom border. Only the last cap of a row keeps its own right
                // border in the styled region, since nothing draws over it afterward.
                let has_next = i + 1 < caps.len();
                let styled_width = if has_next { rect.width - 1 } else { rect.width };
                let styled = Rect::new(rect.x, rect.y, styled_width, CAP_HEIGHT - 1);
                buf.set_style(styled, Style::default().add_modifier(Modifier::REVERSED));
            }

            // The full drawn rect, unshrunk: rows are pushed top to bottom, so `key_at`'s
            // first-match search resolves a shared row to the cap above with no shrinking
            // needed, the vendor's own hit-test convention.
            rects.push((rect, usage));
            row_rects.push(rect);
            // Not `width`: the next cap's own left border column reuses this cap's own right
            // border column, the horizontal analogue of the row cadence below (`CAP_HEIGHT - 1`),
            // so only `width - 1` columns are actually spent advancing past this cap. `key_at`'s
            // first-match search (see its own doc comment) resolves the shared column to this
            // cap, not the one that follows, since this cap's rect is pushed to `rects` first.
            x += width - 1;
        }
        // The row below's own top border just overwrote the row above's own bottom border,
        // wholesale, on the very same physical line: `merge_shared_border` reconciles the two
        // rows' own boundaries there so the join is one continuous rule, not whichever row
        // happened to be drawn last.
        if let Some(prev) = &prev_row_rects {
            merge_shared_border(buf, prev, &row_rects, y);
        }
        prev_row_rects = Some(row_rects);
        // Always 3, not 4: the next row's own top border reuses this row's own bottom border
        // row, so only 3 new rows of vertical space are actually spent on it.
        y += CAP_HEIGHT - 1;
    }
}

/// The columns where `row`'s own caps have a vertical border, the two edge columns of every cap
/// in it (`x` and `x + width - 1`; identical when a cap is exactly one column wide). Two adjacent
/// caps within a row share their join column (see `render_matrix`'s own rect maths: the right edge
/// of cap n is the left edge of cap n+1), so it is inserted into this `HashSet` twice but counted
/// once: a join contributes one boundary column here, not two.
fn row_boundaries(row: &[Rect]) -> HashSet<u16> {
    let mut set = HashSet::new();
    for r in row {
        set.insert(r.x);
        set.insert(r.x + r.width - 1);
    }
    set
}

/// Rewrites the one physical row shared between `upper` (the row above, its own bottom border)
/// and `lower` (the row below, its own top border) at `y`, replacing whichever row's Block
/// widget happened to draw last with a single continuous rule spanning the union of both rows'
/// horizontal extents. Only the symbol changes, never the style: a cap's own `Modifier::REVERSED`
/// selection highlight (applied over its own top border row, see the caller) must survive this
/// rewrite untouched.
///
/// A column covered by only one of the two rows is left exactly as that row already drew it: its
/// own natural corner or rule, since nothing here ever wrote over it in the first place. Only a
/// column both rows cover needs reconciling, and the glyph there follows the brief's own table:
/// `┼` where both rows have a boundary, `┴`/`┬` where only the upper/lower row does, `─`
/// otherwise, `├`/`┤` at the shared line's own left/right end (where the rule continues on one
/// side only), never a corner there since the walls above and below both still connect.
fn merge_shared_border(buf: &mut Buffer, upper: &[Rect], lower: &[Rect], y: u16) {
    if upper.is_empty() || lower.is_empty() {
        return;
    }
    let upper_min = upper[0].x;
    let upper_max = upper[upper.len() - 1].x + upper[upper.len() - 1].width;
    let lower_min = lower[0].x;
    let lower_max = lower[lower.len() - 1].x + lower[lower.len() - 1].width;
    let lo = upper_min.min(lower_min);
    let hi = upper_max.max(lower_max);
    if lo >= hi {
        return;
    }
    let upper_bounds = row_boundaries(upper);
    let lower_bounds = row_boundaries(lower);

    for x in lo..hi {
        let up_rule = x >= upper_min && x < upper_max;
        let down_rule = x >= lower_min && x < lower_max;
        if !(up_rule && down_rule) {
            // Only one row's own rendering reaches this column: it is already correct, left by
            // whichever row drew it, untouched by the other.
            continue;
        }
        let up_stem = upper_bounds.contains(&x);
        let down_stem = lower_bounds.contains(&x);
        let at_left = x == lo;
        let at_right = x == hi - 1;
        let ch = if at_left && at_right {
            '\u{2502}' // │, a one-column union: both walls connect, neither side continues.
        } else if at_left {
            '\u{251c}' // ├
        } else if at_right {
            '\u{2524}' // ┤
        } else if up_stem && down_stem {
            '\u{253c}' // ┼
        } else if up_stem {
            '\u{2534}' // ┴
        } else if down_stem {
            '\u{252c}' // ┬
        } else {
            '\u{2500}' // ─
        };
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_symbol(ch.encode_utf8(&mut [0u8; 4]));
        }
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
