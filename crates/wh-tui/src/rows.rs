//! The settings row widget and the row lists it takes for the AP and RT tabs.
//!
//! Every setting in the vendor UI is one line: a label, dot leaders filling the gap, then a
//! control at the right, either a stepper (`< VALUE >`) or a button (`[LABEL]`). This module
//! owns that one widget (`render_row`) and the pure functions that turn a board's keys into the
//! rows each tab shows. Nothing here writes to the device: every control renders a value only,
//! arrow and click interaction on it is a later plan.

use crate::board::{ap_keysets, global_ap, global_rt, rt_keysets, GlobalValue, KeysetView};
use ratatui::prelude::*;
use wh_device::ops::KeySettings;
use wh_proto::cmds::TouchMode;
use wh_proto::keys::label;
use wh_proto::value::Um;

/// One row's right-hand control. Both variants render a value only in this phase; the stepper's
/// `< >` arrows and the button are inert until a later plan wires clicks to them.
pub enum Control {
    Stepper { value: String },
    Button { label: String },
}

/// One settings row: a label, then dot leaders, then its control, the vendor's own row shape.
/// `disabled` dims the whole row (its control stays inert either way in this phase). `indent` is
/// columns of leading space before the label, for a row nested under another, a keyset's own
/// sub-steppers in a later plan.
pub struct SettingRow {
    pub label: String,
    pub control: Control,
    pub disabled: bool,
    pub indent: u16,
}

/// The vendor's own button shape, `[LABEL]`. Shared by `render_row`'s `Control::Button` arm and
/// `render_prompt`'s right-side action, so the two never drift into different bracket styles.
fn button_text(label: &str) -> String {
    format!("[{label}]")
}

/// Renders `row` into `area` (one line tall): `LABEL` then dot leaders then the control,
/// `< VALUE >` for a stepper or `[LABEL]` for a button, filling `area.width` exactly. A disabled
/// row renders `Modifier::DIM` over the whole width, not just the control, since a dimmed value
/// next to a fully legible label would be a different sentence than "this setting is inert".
pub fn render_row(area: Rect, buf: &mut Buffer, row: &SettingRow) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let control_text = match &row.control {
        Control::Stepper { value } => format!("< {value} >"),
        Control::Button { label } => button_text(label),
    };
    let label_text = format!("{}{}", " ".repeat(row.indent as usize), row.label);
    let width = area.width as usize;
    let leader_len =
        width.saturating_sub(label_text.chars().count() + control_text.chars().count());

    let mut chars: Vec<char> = label_text
        .chars()
        .chain(std::iter::repeat_n('.', leader_len))
        .chain(control_text.chars())
        .collect();
    chars.truncate(width);
    while chars.len() < width {
        chars.push(' ');
    }
    let line: String = chars.into_iter().collect();
    buf.set_string(area.x, area.y, &line, Style::default());

    if row.disabled {
        buf.set_style(area, Style::default().add_modifier(Modifier::DIM));
    }
}

/// Renders the prompt line under the settings rows: `status` at the left, one right-aligned
/// action button (the same `[LABEL]` shape `render_row` gives a `Control::Button`), dimmed when
/// `disabled`. Returns the button's own rect, recorded by the caller for click handling in a
/// later plan; the button is inert either way in this phase.
pub fn render_prompt(
    area: Rect,
    buf: &mut Buffer,
    status: &str,
    action: &str,
    disabled: bool,
) -> Rect {
    let action_text = button_text(action);
    let width = area.width as usize;
    let action_len = action_text.chars().count();
    let gap_len = width.saturating_sub(status.chars().count() + action_len);

    let mut chars: Vec<char> = status
        .chars()
        .chain(std::iter::repeat_n(' ', gap_len))
        .chain(action_text.chars())
        .collect();
    chars.truncate(width);
    while chars.len() < width {
        chars.push(' ');
    }
    let line: String = chars.into_iter().collect();
    buf.set_string(area.x, area.y, &line, Style::default());

    let clipped_action_len = action_len.min(width);
    let action_rect = Rect::new(
        area.x + (width - clipped_action_len) as u16,
        area.y,
        clipped_action_len as u16,
        1,
    );
    if disabled {
        buf.set_style(action_rect, Style::default().add_modifier(Modifier::DIM));
    }
    action_rect
}

fn global_ap_text(v: GlobalValue<Um>) -> String {
    match v {
        GlobalValue::Agreed(ap) => format!("{:.2} MM", ap.to_mm()),
        GlobalValue::Mixed => "MIXED".to_string(),
        GlobalValue::NoneOutside => "-".to_string(),
    }
}

/// A keyset's own row, shared by AP and RT: `[X] {members}`, the vendor's own literal checkbox
/// text (not the keyset's index, see `research/vendor-bundle/2026-09-05/screenshots/index.md`),
/// with a `^` collapse-marker button. Collapsing itself is a later plan.
fn keyset_row(group: &KeysetView) -> SettingRow {
    let members = group
        .members
        .iter()
        .map(|&u| label(u).to_uppercase())
        .collect::<Vec<_>>()
        .join(",");
    SettingRow {
        label: format!("[X] {members}"),
        control: Control::Button {
            label: "^".to_string(),
        },
        disabled: false,
        indent: 0,
    }
}

/// The AP tab's rows: the global actuation point, the configurator's own `"MM" CUSTOM VALUE`
/// stepper step size (`travel`, not the actuation point itself, see `docs/keysets.md`), then one
/// row per AP keyset.
pub fn ap_rows(keys: &[KeySettings], travel: Um) -> Vec<SettingRow> {
    let mut rows = vec![
        SettingRow {
            label: "GLOBAL ACTUATION POINT".to_string(),
            control: Control::Stepper {
                value: global_ap_text(global_ap(keys)),
            },
            disabled: false,
            indent: 0,
        },
        SettingRow {
            label: "\"MM\" CUSTOM VALUE".to_string(),
            control: Control::Stepper {
                value: format!("{:.2} MM", travel.to_mm()),
            },
            disabled: false,
            indent: 0,
        },
    ];
    for group in ap_keysets(keys) {
        rows.push(keyset_row(&group));
    }
    rows
}

/// The RT tab's rows: the global rapid trigger toggle, its three dependent sub-settings
/// (disabled while the global toggle is off), then one row per RT keyset. `SEPARATE PRESS AND
/// RELEASE` and `RT SENSITIVITY` are derived from the outside-keyset press/release pair
/// (`docs/keysets.md`'s "not a stored bit": separate is `press != release`); `CONTINUOUS RAPID
/// TRIGGER` is read directly off any outside-keyset key's own `RtContinuous` mode.
pub fn rt_rows(keys: &[KeySettings]) -> Vec<SettingRow> {
    let (off, global_value, separate_value, sensitivity_value) = match global_rt(keys) {
        GlobalValue::Agreed((p, r)) => (
            false,
            format!("{:.2} MM", p.to_mm()),
            if p != r { "ON" } else { "OFF" }.to_string(),
            if p == r {
                format!("{:.2} MM", p.to_mm())
            } else {
                format!("{:.2}/{:.2} MM", p.to_mm(), r.to_mm())
            },
        ),
        GlobalValue::Mixed => (
            false,
            "MIXED".to_string(),
            "MIXED".to_string(),
            "MIXED".to_string(),
        ),
        GlobalValue::NoneOutside => (true, "OFF".to_string(), "OFF".to_string(), "-".to_string()),
    };
    let continuous_on = keys
        .iter()
        .any(|k| k.rt_keyset == 0 && k.mode.touch == TouchMode::RtContinuous);

    let mut rows = vec![
        SettingRow {
            label: "GLOBAL RAPID TRIGGER".to_string(),
            control: Control::Stepper {
                value: global_value,
            },
            disabled: false,
            indent: 0,
        },
        SettingRow {
            label: "SEPARATE PRESS AND RELEASE".to_string(),
            control: Control::Stepper {
                value: separate_value,
            },
            disabled: off,
            indent: 0,
        },
        SettingRow {
            label: "RT SENSITIVITY".to_string(),
            control: Control::Stepper {
                value: sensitivity_value,
            },
            disabled: off,
            indent: 0,
        },
        SettingRow {
            label: "CONTINUOUS RAPID TRIGGER".to_string(),
            control: Control::Stepper {
                value: if continuous_on { "ON" } else { "OFF" }.to_string(),
            },
            disabled: off,
            indent: 0,
        },
    ];
    for group in rt_keysets(keys) {
        rows.push(keyset_row(&group));
    }
    rows
}
