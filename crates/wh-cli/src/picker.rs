//! Interactive key picker for `--pick`.
//!
//! The selection logic (cursor movement, toggling, select-all/clear-all, and building the
//! final usage list) is a pure function of state and a key code, so it lives in
//! [`PickerState`] and is unit tested without a TTY. The terminal shell around it, `pick`
//! below, is kept as thin as possible: it owns the `ratatui`/`crossterm` calls and nothing
//! else.

use anyhow::{bail, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::DefaultTerminal;

/// What a single key code should do to a live picker session.
enum Outcome {
    /// Keep looping; the caller should redraw.
    Continue,
    /// Enter was pressed: stop looping and build the result.
    Accept,
    /// `q` or Esc was pressed: stop looping and report cancellation.
    Cancel,
}

/// The picker's selection state, independent of any terminal or rendering concern.
///
/// `names` and `usages` are parallel to each other and to `selected`, all indexed by
/// position in the board's key universe (the order `ops::read_matrix` returned it in).
struct PickerState {
    usages: Vec<u8>,
    names: Vec<String>,
    selected: Vec<bool>,
    cursor: usize,
}

impl PickerState {
    /// Builds picker state for a non-empty universe. Returns `None` for an empty universe,
    /// which the caller must reject before ever constructing a state or a `ListState`: an
    /// empty list has no valid cursor position, and `Down`'s `(i + 1).min(len - 1)` would
    /// underflow on `len == 0`.
    fn new(universe: &[u8]) -> Option<Self> {
        if universe.is_empty() {
            return None;
        }
        let names: Vec<String> = universe.iter().map(|&u| crate::run::key_label(u)).collect();
        Some(PickerState {
            usages: universe.to_vec(),
            names,
            selected: vec![false; universe.len()],
            cursor: 0,
        })
    }

    /// Applies one key code to the state, mutating it in place, and reports what the caller
    /// should do next. Keys this picker does not use (letters other than `a`, function keys,
    /// etc.) fall through to `Outcome::Continue` with no effect, the same as an unrecognised
    /// key on any modal prompt.
    fn apply(&mut self, code: KeyCode) -> Outcome {
        match code {
            KeyCode::Up => {
                self.cursor = self.cursor.saturating_sub(1);
                Outcome::Continue
            }
            KeyCode::Down => {
                self.cursor = (self.cursor + 1).min(self.usages.len() - 1);
                Outcome::Continue
            }
            KeyCode::Char(' ') => {
                self.selected[self.cursor] = !self.selected[self.cursor];
                Outcome::Continue
            }
            KeyCode::Char('a') => {
                // Asymmetric by design: `a` selects everything unless everything is already
                // selected, in which case it clears everything. A plain toggle-each-bit would
                // instead flip a mixed selection to its exact complement, which is not what a
                // user pressing "select all" on a partial selection wants.
                let all_selected = self.selected.iter().all(|&s| s);
                self.selected.iter_mut().for_each(|s| *s = !all_selected);
                Outcome::Continue
            }
            KeyCode::Enter => Outcome::Accept,
            KeyCode::Char('q') | KeyCode::Esc => Outcome::Cancel,
            _ => Outcome::Continue,
        }
    }

    /// The usages behind every currently selected position, in universe order.
    fn picked(&self) -> Vec<u8> {
        self.usages
            .iter()
            .zip(&self.selected)
            .filter(|(_, &s)| s)
            .map(|(&u, _)| u)
            .collect()
    }
}

/// Prompts the user to pick keys interactively from `universe` and returns their usages.
///
/// `universe` is the board's real key list, read live by the caller immediately before this
/// runs (see `resolve_keys` in `run.rs`), not the static key table: a picker offering a key
/// the attached board does not have would let the user select something no write can reach.
pub fn pick(universe: &[u8]) -> Result<Vec<u8>> {
    let Some(mut state) = PickerState::new(universe) else {
        bail!("this board reports no keys to pick from");
    };

    let mut terminal = ratatui::init();
    // The loop's outcome is captured here and the terminal is restored unconditionally
    // below, on every path, success or failure alike: `terminal.draw(...)?` and
    // `event::read()?` inside the loop both return early on error, which would otherwise
    // skip `ratatui::restore()` and leave the user's shell in raw mode on the alternate
    // screen. `ratatui::init()`'s panic hook covers a panic, but an early `?` return is not
    // a panic and never trips that hook.
    let result = run_loop(&mut terminal, &mut state);
    ratatui::restore();

    match result? {
        Outcome::Accept => {}
        _ => bail!("cancelled"),
    }
    let picked = state.picked();
    if picked.is_empty() {
        bail!("no keys selected");
    }
    Ok(picked)
}

/// The event loop itself, factored out of `pick` only so `ratatui::restore()` can sit at a
/// single call site that every exit from this function, `?` included, is forced through.
fn run_loop(terminal: &mut DefaultTerminal, state: &mut PickerState) -> Result<Outcome> {
    let mut list_state = ListState::default();
    list_state.select(Some(state.cursor));
    loop {
        terminal.draw(|f| draw(f, state, &mut list_state))?;
        if let Event::Key(k) = event::read()? {
            // crossterm reports both Press and Release key events on Windows, the platform
            // this tool actually runs on. Matching on `k.code` alone would handle every
            // physical keypress twice, most visibly as space toggling a key on and straight
            // back off. Only act on Press.
            if k.kind != KeyEventKind::Press {
                continue;
            }
            match state.apply(k.code) {
                Outcome::Continue => {
                    list_state.select(Some(state.cursor));
                }
                outcome @ (Outcome::Accept | Outcome::Cancel) => return Ok(outcome),
            }
        }
    }
}

fn draw(f: &mut Frame, state: &PickerState, list_state: &mut ListState) {
    let items: Vec<ListItem> = state
        .names
        .iter()
        .zip(&state.selected)
        .map(|(n, &sel)| ListItem::new(format!("[{}] {n}", if sel { "x" } else { " " })))
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" pick keys - space: toggle, a: all, enter: accept, q: cancel "),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(list, f.area(), list_state);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_for(universe: &[u8]) -> PickerState {
        PickerState::new(universe).expect("non-empty universe")
    }

    #[test]
    fn empty_universe_is_rejected_before_any_state_exists() {
        assert!(PickerState::new(&[]).is_none());
    }

    #[test]
    fn down_at_the_last_item_does_not_move_past_the_end() {
        let mut s = state_for(&[0x04, 0x05, 0x06]);
        for _ in 0..10 {
            s.apply(KeyCode::Down);
        }
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn up_at_the_first_item_does_not_underflow() {
        let mut s = state_for(&[0x04, 0x05, 0x06]);
        for _ in 0..10 {
            s.apply(KeyCode::Up);
        }
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn space_toggles_exactly_the_item_under_the_cursor() {
        let mut s = state_for(&[0x04, 0x05, 0x06]);
        s.apply(KeyCode::Down); // cursor -> 1
        s.apply(KeyCode::Char(' '));
        assert_eq!(s.selected, vec![false, true, false]);
        // Toggling again clears it, and only it.
        s.apply(KeyCode::Char(' '));
        assert_eq!(s.selected, vec![false, false, false]);
    }

    #[test]
    fn a_selects_all_when_some_are_unselected() {
        let mut s = state_for(&[0x04, 0x05, 0x06]);
        s.apply(KeyCode::Char(' ')); // select only index 0
        s.apply(KeyCode::Char('a'));
        assert_eq!(s.selected, vec![true, true, true]);
    }

    #[test]
    fn a_clears_all_when_every_one_is_already_selected() {
        let mut s = state_for(&[0x04, 0x05, 0x06]);
        s.apply(KeyCode::Char('a')); // none selected -> select all
        assert_eq!(s.selected, vec![true, true, true]);
        s.apply(KeyCode::Char('a')); // all selected -> clear all
        assert_eq!(s.selected, vec![false, false, false]);
    }

    #[test]
    fn picked_matches_selected_positions_in_universe_order_for_a_mixed_selection() {
        // Five keys, a non-contiguous selection (indices 0, 2, 4), so an off-by-one in the
        // zip between `usages` and `selected` cannot pass by accident.
        let universe = [0x04, 0x05, 0x06, 0x07, 0x08];
        let mut s = state_for(&universe);
        s.apply(KeyCode::Char(' ')); // index 0
        s.apply(KeyCode::Down);
        s.apply(KeyCode::Down);
        s.apply(KeyCode::Char(' ')); // index 2
        s.apply(KeyCode::Down);
        s.apply(KeyCode::Down);
        s.apply(KeyCode::Char(' ')); // index 4
        assert_eq!(s.picked(), vec![0x04, 0x06, 0x08]);
    }

    #[test]
    fn accepting_with_nothing_selected_is_refused() {
        let s = state_for(&[0x04, 0x05, 0x06]);
        assert!(s.picked().is_empty());
    }

    #[test]
    fn enter_and_cancel_keys_report_the_right_outcome() {
        let mut s = state_for(&[0x04]);
        assert!(matches!(s.apply(KeyCode::Enter), Outcome::Accept));
        assert!(matches!(s.apply(KeyCode::Char('q')), Outcome::Cancel));
        assert!(matches!(s.apply(KeyCode::Esc), Outcome::Cancel));
    }

    #[test]
    fn an_unrecognised_key_leaves_state_untouched() {
        let mut s = state_for(&[0x04, 0x05]);
        let before = (s.cursor, s.selected.clone());
        assert!(matches!(s.apply(KeyCode::Char('z')), Outcome::Continue));
        assert_eq!((s.cursor, s.selected.clone()), before);
    }
}
