use crate::board::{ap_keysets, rt_keysets, BoardModel};
use crate::matrix::{render_matrix, CapValue};
use crate::rows::{self, render_prompt, render_row};
use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use ratatui::prelude::*;
use std::collections::HashSet;

/// The AP and RT tabs' prompt line, identical between them: both build keysets the same way
/// (click keys, Enter or ADD KEYSET commits), so the vendor gives them the same status text.
const KEYSET_PROMPT: &str = "> CLICK ON THE KEYS TO MAKE A KEYSET";
const RESET_KEYSETS_ACTION: &str = "RESET KEYSETS";

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
    /// Each rendered key cap's on-screen rect and usage, recorded during the last `draw`.
    /// Rendered as `Modifier::REVERSED` when the usage is in `selection`; nothing drives
    /// `selection` yet, that arrives with click-to-select in a later plan.
    pub key_rects: Vec<(Rect, u8)>,
    pub selection: HashSet<u8>,
    /// The prompt line's right-side action button rect, recorded during the last `draw`. `None`
    /// on a tab with no prompt line. The button is inert until a later plan wires it up.
    pub prompt_action_rect: Option<Rect>,
}

impl App {
    pub fn new(board: BoardModel, wh_version: &str) -> Self {
        Self {
            board,
            wh_version: wh_version.to_string(),
            quit: false,
            tab: Tab::ActuationPoint,
            tab_rects: Vec::new(),
            key_rects: Vec::new(),
            selection: HashSet::new(),
            prompt_action_rect: None,
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

    // The body: everything between the tab row and the footer, split horizontally. The left
    // pane holds the settings rows below; the matrix takes whatever is left.
    let body_y = y + 1;
    let footer_y = area.height.saturating_sub(1);
    let body_height = footer_y.saturating_sub(body_y);
    // 46 (task 6's placeholder, explicitly ledgered as unpinned) is too narrow for this task's
    // own mandated prompt line: "> CLICK ON THE KEYS TO MAKE A KEYSET" (36) plus
    // "[RESET KEYSETS]" (15) is 51 columns with no gap between them at all. 56 leaves that gap
    // a visible 5 columns.
    let left_width = area.width.min(56);
    let matrix_area = Rect::new(
        left_width,
        body_y,
        area.width.saturating_sub(left_width),
        body_height,
    );

    // The left pane: settings rows for whichever tab is active, then one prompt line. Only AP
    // and RT build rows in this phase; the other tabs are an honest gap left for later plans.
    let left_area = Rect::new(0, body_y, left_width, body_height);
    let settings_rows = match app.tab {
        Tab::ActuationPoint => rows::ap_rows(&app.board.keys, app.board.global.travel),
        Tab::RapidTrigger => rows::rt_rows(&app.board.keys),
        _ => Vec::new(),
    };
    let mut ry = left_area.y;
    for row in &settings_rows {
        if ry >= left_area.y + left_area.height {
            break;
        }
        render_row(
            Rect::new(left_area.x, ry, left_area.width, 1),
            f.buffer_mut(),
            row,
        );
        ry += 1;
    }
    app.prompt_action_rect = None;
    if matches!(app.tab, Tab::ActuationPoint | Tab::RapidTrigger)
        && ry < left_area.y + left_area.height
    {
        let keysets_empty = match app.tab {
            Tab::ActuationPoint => ap_keysets(&app.board.keys).is_empty(),
            Tab::RapidTrigger => rt_keysets(&app.board.keys).is_empty(),
            _ => true,
        };
        let prompt_area = Rect::new(left_area.x, ry, left_area.width, 1);
        let action_rect = render_prompt(
            prompt_area,
            f.buffer_mut(),
            KEYSET_PROMPT,
            RESET_KEYSETS_ACTION,
            keysets_empty,
        );
        app.prompt_action_rect = Some(action_rect);
    }

    let value_of = |usage: u8| -> CapValue {
        match app.tab {
            Tab::ActuationPoint => match app.board.key(usage) {
                Some(k) => CapValue {
                    show: true,
                    text: format!("{:.2}", k.ap.to_mm()),
                },
                None => CapValue {
                    show: false,
                    text: String::new(),
                },
            },
            Tab::RapidTrigger => match app.board.key(usage) {
                Some(k) if k.rt_keyset != 0 => CapValue {
                    show: true,
                    text: format!("{:.2}", k.rt_press.to_mm()),
                },
                _ => CapValue {
                    show: false,
                    text: String::new(),
                },
            },
            _ => CapValue {
                show: false,
                text: String::new(),
            },
        }
    };
    let mut key_rects = Vec::new();
    render_matrix(
        matrix_area,
        f.buffer_mut(),
        &app.board.rows,
        value_of,
        &app.selection,
        &mut key_rects,
    );
    app.key_rects = key_rects;

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

    /// One rendered cap's line at `rect`'s column span, `row_offset` rows below `rect.y`: the
    /// whole cap-cell content (border included), not a substring search. Mirrors
    /// `tests/matrix.rs`'s own `cap_line`, kept separate deliberately: that one reads a raw
    /// `Buffer` from `render_matrix` directly, this one reads a `Terminal` through the full
    /// `draw`, and the two must never drift together silently.
    fn cap_line(lines: &[String], rect: Rect, row_offset: u16) -> String {
        let line = &lines[(rect.y + row_offset) as usize];
        let chars: Vec<char> = line.chars().collect();
        let start = rect.x as usize;
        let end = (rect.x + rect.width) as usize;
        (start..end)
            .map(|i| chars.get(i).copied().unwrap_or(' '))
            .collect()
    }

    #[test]
    fn the_matrix_shows_each_keys_actuation_point_on_the_actuation_point_tab() {
        let mut app = App::new(crate::board::test_fixture(), "0.5.0-alpha");
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();

        let w_rect = app
            .key_rects
            .iter()
            .find(|&&(_, u)| u == 0x1A)
            .expect("w's cap must be recorded")
            .0;
        let lines = buffer_lines(&terminal);
        assert_eq!(
            cap_line(&lines, w_rect, 2),
            "│1.20 │",
            "w's AP value line, whole cap-cell: {lines:?}"
        );
    }

    #[test]
    fn the_matrix_shows_rt_press_only_for_keys_in_an_rt_keyset_on_the_rapid_trigger_tab() {
        let mut app = App::new(crate::board::test_fixture(), "0.5.0-alpha");
        app.tab = Tab::RapidTrigger;
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();

        let lines = buffer_lines(&terminal);
        let a_rect = app
            .key_rects
            .iter()
            .find(|&&(_, u)| u == 0x04)
            .expect("a's cap must be recorded")
            .0;
        let w_rect = app
            .key_rects
            .iter()
            .find(|&&(_, u)| u == 0x1A)
            .expect("w's cap must be recorded")
            .0;

        assert_eq!(
            cap_line(&lines, a_rect, 2),
            "│0.30 │",
            "a is in an rt keyset, must show rt_press: {lines:?}"
        );
        assert_eq!(
            cap_line(&lines, w_rect, 2),
            "│     │",
            "w is outside any rt keyset, must show a blank value line: {lines:?}"
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
