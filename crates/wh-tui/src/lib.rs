pub mod app;
pub mod board;
pub mod matrix;
pub mod rows;

use anyhow::{Context, Result};
use board::BoardModel;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, KeyModifiers,
    MouseEventKind,
};
use std::time::Duration;
use wh_device::session::Session;
use wh_device::transport::Transport;

pub fn run<T: Transport>(session: &mut Session<T>, wh_version: &str) -> Result<()> {
    let board = BoardModel::read(session).context("could not read the board")?;
    let mut terminal = match ratatui::try_init() {
        Ok(t) => t,
        Err(e) => {
            ratatui::restore();
            return Err(e).context("could not enter the alternate screen");
        }
    };
    if let Err(e) = crossterm::execute!(std::io::stdout(), EnableMouseCapture) {
        ratatui::restore();
        return Err(e).context("could not enable mouse capture");
    }
    install_panic_hook();
    let result = event_loop(&mut terminal, session, board, wh_version);
    // Always disable mouse capture and restore, whether the loop returned an error or not.
    let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

/// Restores mouse capture on the way out of a panic. ratatui's own hook puts back raw mode and
/// the alternate screen and knows nothing about `EnableMouseCapture`, so without this a panic
/// leaves the operator's shell reporting every mouse movement as escape sequences.
fn install_panic_hook() {
    install_panic_hook_with(|| {
        let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
    });
}

/// `install_panic_hook` with the terminal write injected, so a test can prove both steps run and
/// in which order without writing escape sequences at whatever is running it.
fn install_panic_hook_with(disable: impl Fn() + Send + Sync + 'static) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        disable();
        previous(info);
    }));
}

/// Applies one terminal event to `app`, returning whether the display has to be redrawn.
///
/// Mouse motion is deliberately not a change: `EnableMouseCapture` turns on any-event tracking,
/// so every movement arrives here, and repainting the whole frame for a pointer crossing the
/// window is how the loop falls behind. A resize is a change even though no state moved: the
/// `key_rects` and `tab_rects` recorded by the last draw describe the pre-resize layout, and the
/// next click is routed against them before any redraw would otherwise happen.
fn apply_event(app: &mut app::App, ev: Event) -> bool {
    match ev {
        Event::Key(k) if matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == event::KeyCode::Char('c') {
                app.quit = true;
            } else {
                app.handle_key(k.code);
            }
            true
        }
        Event::Mouse(m) if !matches!(m.kind, MouseEventKind::Moved) => {
            app.handle_mouse(m.kind, m.column, m.row);
            true
        }
        Event::Resize(_, _) => true,
        _ => false,
    }
}

/// Applies every event of `events` in one pass, returning whether any of them changed the
/// display. Taking one event per loop iteration cannot keep up: crossterm delivers one event per
/// `read`, and any-event mouse tracking queues one per movement, so at roughly 30ms an iteration
/// the queue grows faster than it drains and the frame lags behind the pointer that filled it.
fn drain_input(app: &mut app::App, events: impl Iterator<Item = Event>) -> bool {
    let mut changed = false;
    for ev in events {
        changed |= apply_event(app, ev);
    }
    changed
}

/// The events already queued, and no waiting: `poll(ZERO)` answers false the moment the queue is
/// empty, which ends the iteration.
struct Pending;

impl Iterator for Pending {
    type Item = Event;

    fn next(&mut self) -> Option<Event> {
        match event::poll(Duration::ZERO) {
            Ok(true) => event::read().ok(),
            _ => None,
        }
    }
}

fn event_loop<T: Transport>(
    terminal: &mut ratatui::DefaultTerminal,
    session: &mut Session<T>,
    board: BoardModel,
    wh_version: &str,
) -> Result<()> {
    let mut app = app::App::new(board, wh_version);
    terminal.draw(|f| app::draw(f, &mut app))?;
    while !app.quit {
        // Wait briefly for the first event, then take everything else already queued behind it.
        let input_changed = if event::poll(Duration::from_millis(15))? {
            drain_input(&mut app, Pending)
        } else {
            false
        };
        let tick_changed = app.tick(session)?;
        if input_changed || tick_changed {
            terminal.draw(|f| app::draw(f, &mut app))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use app::{App, Tab};
    use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent};
    use std::sync::{Arc, Mutex};

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn a_resize_forces_a_redraw() {
        let mut app = App::new(board::test_fixture(), "x");
        assert!(
            apply_event(&mut app, Event::Resize(80, 24)),
            "a resize must be reported as a change: the recorded rects describe the old layout"
        );
        assert_eq!(app.tab, Tab::ActuationPoint, "a resize must move nothing");
        assert!(!app.quit);
    }

    #[test]
    fn mouse_motion_is_not_a_change_but_a_click_is() {
        let mut app = App::new(board::test_fixture(), "x");
        assert!(
            !apply_event(&mut app, mouse(MouseEventKind::Moved, 5, 14)),
            "mouse motion must not force a redraw"
        );
        assert!(
            apply_event(
                &mut app,
                mouse(MouseEventKind::Down(MouseButton::Left), 5, 14)
            ),
            "a click must force a redraw"
        );
    }

    #[test]
    fn every_queued_event_is_applied_in_one_drain_not_just_the_first() {
        let mut app = App::new(board::test_fixture(), "x");
        let queued = vec![
            key(KeyCode::Right),
            mouse(MouseEventKind::Moved, 1, 1),
            key(KeyCode::Right),
            key(KeyCode::Right),
        ];
        assert!(drain_input(&mut app, queued.into_iter()));
        assert_eq!(
            app.tab,
            Tab::Switches,
            "all three Right presses must land in one drain, motion between them ignored"
        );
    }

    #[test]
    fn a_drain_of_motion_alone_reports_no_change() {
        let mut app = App::new(board::test_fixture(), "x");
        let queued = vec![
            mouse(MouseEventKind::Moved, 1, 1),
            mouse(MouseEventKind::Moved, 2, 1),
        ];
        assert!(
            !drain_input(&mut app, queued.into_iter()),
            "a pointer crossing the window must not repaint the frame"
        );
    }

    #[test]
    fn ctrl_c_quits() {
        let mut app = App::new(board::test_fixture(), "x");
        let ev = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(apply_event(&mut app, ev));
        assert!(
            app.quit,
            "ctrl-c must quit: raw mode means no signal arrives"
        );
    }

    /// The hook itself, through a real panic. The escape-sequence write is injected, so this
    /// proves the ordering and the delegation, not that crossterm reaches a terminal.
    #[test]
    fn the_panic_hook_disables_mouse_capture_before_delegating() {
        let order = Arc::new(Mutex::new(Vec::new()));

        // The hook in place before ours, standing in for ratatui's: records, then does whatever
        // the default hook does, so a panic elsewhere in this binary still prints while ours is
        // installed.
        let default = std::panic::take_hook();
        let recorder = Arc::clone(&order);
        std::panic::set_hook(Box::new(move |info| {
            recorder.lock().unwrap().push("previous");
            default(info);
        }));

        let recorder = Arc::clone(&order);
        install_panic_hook_with(move || recorder.lock().unwrap().push("disable"));

        let _ = std::panic::catch_unwind(|| panic!("deliberate panic, this test expects it"));
        let _ = std::panic::take_hook();

        assert_eq!(
            *order.lock().unwrap(),
            vec!["disable", "previous"],
            "mouse capture must be disabled before the previous hook restores the terminal"
        );
    }
}
