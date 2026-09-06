use crossterm::event::KeyCode;
use ratatui::prelude::*;

/// The project's own mark, not the vendor's: Wallhack's logo is Wallhack's.
pub const LOGO: &[&str] = &[
    "00     00  00   00",
    "00  0  00  00   00",
    "00 000 00  0000000",
    "0000 0000  00   00",
    " 000 000   00   00",
    "  00 00    00   00",
];

pub struct App {
    pub wh_version: String,
    pub quit: bool,
}

impl App {
    pub fn new(wh_version: &str) -> Self {
        Self {
            wh_version: wh_version.to_string(),
            quit: false,
        }
    }

    pub fn banner(&self) -> String {
        format!("WALLHACK TERMINAL BY \"@BRUX\" - V{}", self.wh_version)
    }

    pub fn handle_key(&mut self, code: KeyCode) {
        if matches!(code, KeyCode::Char('q') | KeyCode::Esc) {
            self.quit = true;
        }
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let mut y = 0u16;
    for line in LOGO {
        f.render_widget(Line::raw(*line), Rect::new(0, y, area.width, 1));
        y += 1;
    }
    y += 1;
    f.render_widget(Line::raw(app.banner()), Rect::new(0, y, area.width, 1));
    y += 2;
    f.render_widget(
        Line::raw("NAVIGATE WITH MOUSE OR ARROW & ENTER KEYS"),
        Rect::new(0, y, area.width, 1),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Every rendered line of the buffer, right-trimmed, so tests assert whole lines.
    pub(crate) fn buffer_lines(terminal: &Terminal<TestBackend>) -> Vec<String> {
        let buf = terminal.backend().buffer().clone();
        let area = buf.area;
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn the_banner_line_renders_whole_and_exact() {
        let mut app = App::new("0.5.0-alpha");
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let lines = buffer_lines(&terminal);
        assert!(
            lines
                .iter()
                .any(|l| l == "WALLHACK TERMINAL BY \"@BRUX\" - V0.5.0-alpha"),
            "banner missing or wrong: {lines:?}"
        );
    }

    #[test]
    fn q_and_esc_quit() {
        let mut app = App::new("x");
        app.handle_key(crossterm::event::KeyCode::Char('q'));
        assert!(app.quit);
        let mut app = App::new("x");
        app.handle_key(crossterm::event::KeyCode::Esc);
        assert!(app.quit);
    }
}
