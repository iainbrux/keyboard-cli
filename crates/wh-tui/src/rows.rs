//! The settings row widget and the row lists it takes for every tab.
//!
//! Every setting in the vendor UI is one line: a label, dot leaders filling the gap, then a
//! control at the right, a stepper (`< VALUE >`), a button (`[LABEL]`), or plain read-only text
//! (ADVANCED > DEVICE's rows, which have no arrows or brackets because there is nothing to click).
//! This module owns that one widget (`render_row`) and the pure functions that turn a board's
//! keys into the rows each tab shows. Nothing here writes to the device: every stepper and button
//! renders a value only, arrow and click interaction on it is a later plan.

use crate::board::{
    ap_keysets, global_ap, global_rt, rt_keysets, BoardModel, GlobalValue, KeysetView, DEVICE_NAME,
};
use ratatui::prelude::*;
use wh_device::ops::KeySettings;
use wh_proto::cmds::TouchMode;
use wh_proto::keys::label;
use wh_proto::value::Um;

/// One row's right-hand control. `Stepper` and `Button` render a value only in this phase; the
/// stepper's `< >` arrows and the button are inert until a later plan wires clicks to them. `Text`
/// is different in kind, not just in phase: it has no arrows or brackets because there is nothing
/// to click, ever, the ADVANCED > DEVICE rows it renders are read-only device identity.
pub enum Control {
    Stepper { value: String },
    Button { label: String },
    Text { text: String },
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
        Control::Text { text } => text.clone(),
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

fn disabled_stepper(label: &str) -> SettingRow {
    SettingRow {
        label: label.to_string(),
        control: Control::Stepper {
            value: "-".to_string(),
        },
        disabled: true,
        indent: 0,
    }
}

fn disabled_button(label: &str, action: &str) -> SettingRow {
    SettingRow {
        label: label.to_string(),
        control: Control::Button {
            label: action.to_string(),
        },
        disabled: true,
        indent: 0,
    }
}

/// The SWITCHES tab's two rows. Neither reads the device: there is no task yet for switch
/// calibration or reporting (see `docs/tasks.md`), so `CALIBRATE SWITCHES` stays a disabled
/// `[START]` and `CURRENT SWITCHES` always reads `-`.
pub fn switches_rows() -> Vec<SettingRow> {
    vec![
        disabled_button("CALIBRATE SWITCHES", "START"),
        disabled_stepper("CURRENT SWITCHES"),
    ]
}

/// ADVANCED > GENERAL's row list, the vendor's own order (measured from
/// `research/vendor-bundle/2026-09-05/screenshots/08-advanced-general.png`). None of these are
/// read from the device yet, so every stepper reads `-` and every control is disabled.
pub fn advanced_general_rows() -> Vec<SettingRow> {
    vec![
        disabled_button("RESET PROFILE", "SELECT"),
        disabled_button("FACTORY RESET", "SELECT"),
        disabled_stepper("POLLING RATE"),
        disabled_stepper("LED SLEEP TIMER"),
        disabled_stepper("LED BRIGHTNESS"),
        disabled_stepper("SYSTEM TYPE"),
        disabled_stepper("SHOW ANALOG OUTPUT"),
        disabled_stepper("SAFETY ZONE"),
        disabled_stepper("SHOW MAPPED KEY LABELS"),
        disabled_stepper("LOCALIZED KEY LABELS"),
        disabled_button("SOCD", "SELECT"),
        disabled_button("DYNAMIC KEYSTROKE (DKS)", "SELECT"),
        disabled_button("MOD TAP", "SELECT"),
        disabled_button("WALKTHROUGH", "START"),
    ]
}

/// ADVANCED > GAMEPAD's row list (measured from `research/vendor-bundle/2026-09-05/screenshots/
/// 09-advanced-gamepad.png`), minus the joystick curve graph: that is a plotted widget, not a
/// settings row, and has no place in this row list. Every row is unread and disabled, the same
/// honest-stub shape as `advanced_general_rows`.
pub fn advanced_gamepad_rows() -> Vec<SettingRow> {
    vec![
        disabled_stepper("GAMEPAD MODE"),
        disabled_stepper("ENABLE MAPPED KEYBOARD KEYS"),
        disabled_stepper("DISABLE MAPPED KEY INPUT"),
        disabled_stepper("SQUARE JOYSTICK OUTPUT"),
        disabled_stepper("DEPTH-BASED JOYSTICK"),
    ]
}

/// ADVANCED > SHARE's row list. The vendor names each button with the active profile number
/// (measured from `research/vendor-bundle/2026-09-05/screenshots/11-advanced-share.png`); this
/// stub leaves the number out rather than assert one, since neither export nor import is wired to
/// any profile yet.
pub fn advanced_share_rows() -> Vec<SettingRow> {
    vec![
        disabled_button("EXPORT PROFILE SETTINGS", "COPY"),
        disabled_button("IMPORT PROFILE SETTINGS", "IMPORT"),
    ]
}

/// ADVANCED > DEVICE's row list: read-only device identity, live from `board`. `NAME` is the
/// product name (`DEVICE_NAME`), not per-board data, matching the fixed string the device line
/// above the tabs already renders; `SERIAL NUMBER` and `FIRMWARE VERSION` come straight off
/// `board`. None of these three rows is disabled: unlike every other row this task adds, they are
/// not stubs, they are the one sub-tab that is actually built.
pub fn advanced_device_rows(board: &BoardModel) -> Vec<SettingRow> {
    let text_row = |label: &str, text: String| SettingRow {
        label: label.to_string(),
        control: Control::Text { text },
        disabled: false,
        indent: 0,
    };
    vec![
        text_row("NAME", DEVICE_NAME.to_string()),
        text_row("SERIAL NUMBER", board.serial.clone()),
        text_row("FIRMWARE VERSION", board.firmware.clone()),
    ]
}
