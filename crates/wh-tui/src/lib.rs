pub mod app;
pub mod board;
pub mod matrix;
pub mod rows;

use anyhow::{Context, Result};
use board::BoardModel;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, KeyModifiers,
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
    let result = event_loop(&mut terminal, session, board, wh_version);
    // Always disable mouse capture and restore, whether the loop returned an error or not.
    let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
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
        let mut input_changed = false;
        if event::poll(Duration::from_millis(15))? {
            match event::read()? {
                Event::Key(k) => {
                    if matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                        if k.modifiers.contains(KeyModifiers::CONTROL)
                            && k.code == event::KeyCode::Char('c')
                        {
                            app.quit = true;
                        } else {
                            app.handle_key(k.code);
                        }
                        input_changed = true;
                    }
                }
                Event::Mouse(m) => {
                    app.handle_mouse(m.kind, m.column, m.row);
                    input_changed = true;
                }
                _ => {}
            }
        }
        let tick_changed = app.tick(session)?;
        if input_changed || tick_changed {
            terminal.draw(|f| app::draw(f, &mut app))?;
        }
    }
    Ok(())
}
