pub mod app;
pub mod board;

use anyhow::{Context, Result};
use board::BoardModel;
use crossterm::event::{self, Event, KeyEventKind, KeyModifiers};
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
    let result = event_loop(&mut terminal, board, wh_version);
    // Always restore, whether the loop returned an error or not.
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    board: BoardModel,
    wh_version: &str,
) -> Result<()> {
    let mut app = app::App::new(board, wh_version);
    while !app.quit {
        terminal.draw(|f| app::draw(f, &mut app))?;
        if event::poll(Duration::from_millis(15))? {
            if let Event::Key(k) = event::read()? {
                if matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    if k.modifiers.contains(KeyModifiers::CONTROL)
                        && k.code == event::KeyCode::Char('c')
                    {
                        app.quit = true;
                    } else {
                        app.handle_key(k.code);
                    }
                }
            }
        }
    }
    Ok(())
}
