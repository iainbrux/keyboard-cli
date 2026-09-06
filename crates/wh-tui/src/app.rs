use crate::board::BoardModel;
use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
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

/// The footer's fixed text, last row of the frame: help, language, then support contact, each
/// separated by two spaces, matching the tab row's own separator.
const FOOTER: &str = "HELP  EN JA CH  SUPPORT@WALLHACK.COM";

/// The five top-level tabs, in the vendor's own order. `TABS` is that order made iterable, for
/// cycling and for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    ActuationPoint,
    RapidTrigger,
    Mapping,
    Switches,
    Advanced,
}

pub const TABS: [Tab; 5] = [
    Tab::ActuationPoint,
    Tab::RapidTrigger,
    Tab::Mapping,
    Tab::Switches,
    Tab::Advanced,
];

impl Tab {
    /// The exact uppercase vendor title for this tab.
    pub fn title(self) -> &'static str {
        match self {
            Tab::ActuationPoint => "ACTUATION POINT",
            Tab::RapidTrigger => "RAPID TRIGGER",
            Tab::Mapping => "MAPPING",
            Tab::Switches => "SWITCHES",
            Tab::Advanced => "ADVANCED",
        }
    }

    fn index(self) -> usize {
        TABS.iter()
            .position(|t| *t == self)
            .expect("every Tab is in TABS")
    }
}

pub struct App {
    pub board: BoardModel,
    pub wh_version: String,
    pub quit: bool,
    pub tab: Tab,
    /// Each tab title's on-screen rect, recorded during the last `draw`, for click-to-select.
    /// Later tasks reuse this same rect-recording pattern for keys and buttons.
    pub tab_rects: Vec<(Rect, Tab)>,
}

impl App {
    pub fn new(board: BoardModel, wh_version: &str) -> Self {
        Self {
            board,
            wh_version: wh_version.to_string(),
            quit: false,
            tab: Tab::ActuationPoint,
            tab_rects: Vec::new(),
        }
    }

    pub fn banner(&self) -> String {
        format!("WALLHACK TERMINAL BY \"@BRUX\" - V{}", self.wh_version)
    }

    pub fn handle_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Right => {
                let i = self.tab.index();
                if i + 1 < TABS.len() {
                    self.tab = TABS[i + 1];
                }
            }
            KeyCode::Left => {
                let i = self.tab.index();
                if i > 0 {
                    self.tab = TABS[i - 1];
                }
            }
            _ => {}
        }
    }

    /// Click-to-select over the rects `draw` recorded into `tab_rects`. Only a left button-down
    /// selects; drags, releases and other buttons are ignored.
    pub fn handle_mouse(&mut self, kind: MouseEventKind, col: u16, row: u16) {
        if !matches!(kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        for (rect, tab) in &self.tab_rects {
            let hit = col >= rect.x && col < rect.x + rect.width && row == rect.y;
            if hit {
                self.tab = *tab;
                return;
            }
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
    y += 2;
    f.render_widget(
        Line::raw(format!("[X] WALLHACK K-001 - {}", app.board.firmware)),
        Rect::new(0, y, area.width, 1),
    );
    y += 1;
    f.render_widget(
        Line::raw(format!("PROFILE < {} >", app.board.profile)),
        Rect::new(0, y, area.width, 1),
    );
    y += 2;

    // The tab row: each title separated by two spaces, the selected one wrapped in `[` `]` and
    // reversed, matching the vendor's inverted selected tab while still surviving in plain text.
    let mut spans = Vec::new();
    let mut tab_rects = Vec::with_capacity(TABS.len());
    let mut x = 0u16;
    for (i, tab) in TABS.iter().enumerate() {
        let selected = *tab == app.tab;
        let text = if selected {
            format!("[{}]", tab.title())
        } else {
            tab.title().to_string()
        };
        let width = text.chars().count() as u16;
        tab_rects.push((Rect::new(x, y, width, 1), *tab));
        let style = if selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        spans.push(Span::styled(text, style));
        x += width;
        if i + 1 < TABS.len() {
            spans.push(Span::raw("  "));
            x += 2;
        }
    }
    f.render_widget(Line::from(spans), Rect::new(0, y, area.width, 1));
    app.tab_rects = tab_rects;

    f.render_widget(
        Line::raw(FOOTER),
        Rect::new(0, area.height.saturating_sub(1), area.width, 1),
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
        let mut app = App::new(crate::board::test_fixture(), "0.5.0-alpha");
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
    fn the_device_line_renders_the_firmware_from_the_board_model() {
        let mut app = App::new(crate::board::test_fixture(), "0.5.0-alpha");
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let lines = buffer_lines(&terminal);
        assert!(
            lines.iter().any(|l| l == "[X] WALLHACK K-001 - V1.0.0.001"),
            "device line missing or wrong: {lines:?}"
        );
    }

    #[test]
    fn the_profile_line_renders_the_one_based_profile_number() {
        let mut app = App::new(crate::board::test_fixture(), "0.5.0-alpha");
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let lines = buffer_lines(&terminal);
        assert!(
            lines.iter().any(|l| l == "PROFILE < 1 >"),
            "profile line missing or wrong: {lines:?}"
        );
    }

    #[test]
    fn q_and_esc_quit() {
        let mut app = App::new(crate::board::test_fixture(), "x");
        app.handle_key(crossterm::event::KeyCode::Char('q'));
        assert!(app.quit);
        let mut app = App::new(crate::board::test_fixture(), "x");
        app.handle_key(crossterm::event::KeyCode::Esc);
        assert!(app.quit);
    }
}
