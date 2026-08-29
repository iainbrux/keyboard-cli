//! Interactive key picker for `--pick`.
//!
//! The selection logic (cursor movement, toggling, select-all/clear-all, finishing an
//! accepted selection) is a pure function of state and a key code, so it lives in
//! [`PickerState`] and is unit tested without a TTY. `pick` below just owns the
//! `ratatui`/`crossterm` calls.

use anyhow::{bail, Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::DefaultTerminal;
use std::io::IsTerminal;

/// How far `PageUp`/`PageDown` move the cursor in one press. A full board's key list is
/// taller than a typical terminal window, so single-step `Up`/`Down` alone would take dozens
/// of presses to cross it.
const PAGE: usize = 10;

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
    /// Builds picker state for a non-empty universe. Returns `None` for an empty one: an
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
    /// should do next. Keys this picker does not use fall through to `Outcome::Continue`
    /// with no effect.
    fn apply(&mut self, code: KeyCode) -> Outcome {
        let last = self.usages.len() - 1;
        match code {
            KeyCode::Up => {
                self.cursor = self.cursor.saturating_sub(1);
                Outcome::Continue
            }
            KeyCode::Down => {
                self.cursor = (self.cursor + 1).min(last);
                Outcome::Continue
            }
            KeyCode::PageUp => {
                self.cursor = self.cursor.saturating_sub(PAGE);
                Outcome::Continue
            }
            KeyCode::PageDown => {
                self.cursor = (self.cursor + PAGE).min(last);
                Outcome::Continue
            }
            KeyCode::Home => {
                self.cursor = 0;
                Outcome::Continue
            }
            KeyCode::End => {
                self.cursor = last;
                Outcome::Continue
            }
            KeyCode::Char(' ') => {
                self.selected[self.cursor] = !self.selected[self.cursor];
                Outcome::Continue
            }
            KeyCode::Char('a') => {
                // Asymmetric by design: `a` selects everything unless everything is already
                // selected, in which case it clears everything, rather than flipping a mixed
                // selection to its exact complement.
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

    /// Finalizes an accepted selection, refusing an empty one.
    ///
    /// Lives here rather than as a bare check in the terminal shell so it is covered by unit
    /// tests, not only by a real terminal session. Without it, pressing Enter with nothing
    /// toggled would let `set ap --pick` iterate zero keys and exit 0 claiming success while
    /// writing nothing. `resolve_keys` enforces the same non-empty contract for `--keys`.
    fn finish(&self) -> Result<Vec<u8>> {
        let picked = self.picked();
        if picked.is_empty() {
            bail!("no keys selected");
        }
        Ok(picked)
    }
}

/// Whether a key event is a live keystroke to act on, not a key-up report. `Press` and
/// `Repeat` (held key, Kitty protocol) count; `Release` must not, since Windows sends it
/// alongside `Press` for every keypress, and matching on `code` alone would handle each
/// press twice, most visibly as space toggling a key on and straight back off.
fn is_actionable_press(kind: KeyEventKind) -> bool {
    matches!(kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

/// Refuses `--pick` when stdout is not an interactive terminal. Takes the terminal check's
/// result rather than calling `is_terminal()` itself, so a test can drive it with a plain
/// `false` instead of running `pick()` end to end, which would enter raw mode and hang in
/// `event::read()` waiting for a keystroke.
///
/// Without this check, `wh get ap --pick > keys.txt` sends the alternate-screen and list
/// output straight into the file while raw mode silently eats the user's keystrokes, with no
/// indication why.
fn refuse_if_not_terminal(is_tty: bool) -> Result<()> {
    if !is_tty {
        bail!(
            "--pick needs an interactive terminal, but stdout here is redirected or piped: \
             pass --keys instead"
        );
    }
    Ok(())
}

/// Prompts the user to pick keys interactively from `universe` and returns their usages.
///
/// `universe` is the board's real key list, read live by the caller (see `resolve_keys` in
/// `run.rs`), not the static key table: offering a key the attached board does not have
/// would let the user select something no write can reach.
pub fn pick(universe: &[u8]) -> Result<Vec<u8>> {
    refuse_if_not_terminal(std::io::stdout().is_terminal())?;
    let Some(mut state) = PickerState::new(universe) else {
        bail!("this board reports no keys to pick from");
    };

    // `try_init`, not the panicking `init`: a console-less launch (a detached exe, a
    // service) can hit this. `try_init` can fail partway through (raw mode enabled but the
    // alternate screen never entered, or the reverse); calling `ratatui::restore()` anyway
    // is safe, since each of its steps is a no-op if never reached.
    let mut terminal = match ratatui::try_init() {
        Ok(t) => t,
        Err(e) => {
            ratatui::restore();
            return Err(e)
                .context("failed to start the interactive picker; is a terminal attached?");
        }
    };
    // The result is captured, not returned directly, so `ratatui::restore()` below always
    // runs even when the loop's `?`s return early. Otherwise the shell would be left in raw
    // mode on the alternate screen.
    let result = run_loop(&mut terminal, &mut state);
    ratatui::restore();

    match result? {
        Outcome::Accept => {}
        _ => bail!("cancelled"),
    }
    state.finish()
}

/// The event loop itself, factored out of `pick` only so `ratatui::restore()` can sit at a
/// single call site that every exit from this function, `?` included, is forced through.
fn run_loop(terminal: &mut DefaultTerminal, state: &mut PickerState) -> Result<Outcome> {
    let mut list_state = ListState::default();
    list_state.select(Some(state.cursor));
    loop {
        terminal.draw(|f| draw(f, state, &mut list_state))?;
        if let Event::Key(k) = event::read()? {
            if !is_actionable_press(k.kind) {
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
    fn up_moves_the_cursor_back_one_position() {
        // Unlike the underflow test above, this fails if `Up` is ever made a no-op: it
        // moves down twice, up once, and pins the cursor at exactly the middle position.
        let mut s = state_for(&[0x04, 0x05, 0x06]);
        s.apply(KeyCode::Down);
        s.apply(KeyCode::Down);
        s.apply(KeyCode::Up);
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn home_moves_the_cursor_to_the_first_item() {
        let mut s = state_for(&[0x04, 0x05, 0x06, 0x07, 0x08]);
        s.apply(KeyCode::End);
        s.apply(KeyCode::Home);
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn end_moves_the_cursor_to_the_last_item() {
        let mut s = state_for(&[0x04, 0x05, 0x06, 0x07, 0x08]);
        s.apply(KeyCode::End);
        assert_eq!(s.cursor, 4);
    }

    #[test]
    fn page_down_does_not_move_past_the_end_on_a_short_universe() {
        let mut s = state_for(&[0x04, 0x05, 0x06]);
        s.apply(KeyCode::PageDown);
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn page_up_does_not_underflow_on_a_short_universe() {
        let mut s = state_for(&[0x04, 0x05, 0x06]);
        s.apply(KeyCode::PageUp);
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn page_down_then_page_up_on_a_long_universe_moves_by_a_full_page() {
        // Long enough that `PAGE` fits twice over, so a page move is distinguishable from a
        // clamp to either end.
        let universe: Vec<u8> = (0..(PAGE as u8 * 3)).collect();
        let mut s = state_for(&universe);
        s.apply(KeyCode::PageDown);
        assert_eq!(s.cursor, PAGE);
        s.apply(KeyCode::PageUp);
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
        assert!(s.finish().is_err());
    }

    #[test]
    fn finish_returns_the_selection_when_something_is_selected() {
        let mut s = state_for(&[0x04, 0x05, 0x06]);
        s.apply(KeyCode::Down);
        s.apply(KeyCode::Char(' '));
        assert_eq!(s.finish().unwrap(), vec![0x05]);
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

    #[test]
    fn press_and_repeat_are_actionable_release_is_not() {
        assert!(is_actionable_press(KeyEventKind::Press));
        assert!(is_actionable_press(KeyEventKind::Repeat));
        assert!(!is_actionable_press(KeyEventKind::Release));
    }

    #[test]
    fn refuse_if_not_terminal_rejects_a_non_terminal_stdout() {
        // A plain `false`, not a real terminal: calling `pick()` itself under a test's own
        // tty would enter raw mode and block in `event::read()` waiting for a keystroke.
        let err = refuse_if_not_terminal(false).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("terminal"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn refuse_if_not_terminal_accepts_a_terminal_stdout() {
        assert!(refuse_if_not_terminal(true).is_ok());
    }
}
