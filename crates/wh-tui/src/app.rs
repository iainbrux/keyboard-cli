use crate::board::{ap_keysets, rt_keysets, BoardModel, DEVICE_NAME};
use crate::matrix::{needed_width, render_matrix, too_narrow_text, CapValue};
use crate::rows::{self, render_prompt, render_row};
use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use ratatui::prelude::*;
use std::collections::HashSet;
use std::time::Duration;
use wh_device::session::Session;
use wh_device::transport::{DeviceError, Transport};
use wh_proto::event::BoardEvent;

/// The AP and RT tabs' prompt line, identical between them: both build keysets the same way
/// (click keys, Enter or ADD KEYSET commits), so the vendor gives them the same status text.
const KEYSET_PROMPT: &str = "> CLICK ON THE KEYS TO MAKE A KEYSET";
const RESET_KEYSETS_ACTION: &str = "RESET KEYSETS";

/// MAPPING's two sub-tab label rows: plain, non-interactive text, not click targets (unlike
/// ADVANCED's own sub-tab row below), see `05-mapping.png`. The character palette they head is
/// omitted entirely rather than half-built.
pub const MAPPING_LABELS_ROW_1: &str = "BASE LAYER  FN LAYER";
pub const MAPPING_LABELS_ROW_2: &str = "BASE CHARACTERS  EXTENDED CHARACTERS  FUNCTIONS  GAMEPAD";

/// The honest-stub body lines. Each says an edit is not built, never that the feature does not
/// exist: `wh` measures features before it claims their absence (see `docs/tasks.md`), and this
/// task builds none of them, it only stops pretending they are. Defined as `pub const`s so tests
/// pin the exact constant and the rendered text cannot drift from it silently.
pub const MAPPING_STUB: &str = "> MAPPING EDITS ARE NOT BUILT IN WH YET (3.6 IN DOCS/TASKS.MD)";
pub const SWITCHES_STUB: &str = "> SWITCH SETTINGS ARE NOT BUILT IN WH YET";
pub const ADVANCED_GENERAL_STUB: &str = "> EDITING THESE ARRIVES WITH THE TUI'S EDITING PHASE";
pub const ADVANCED_GAMEPAD_STUB: &str = "> GAMEPAD SETTINGS ARE NOT BUILT IN WH YET";
pub const ADVANCED_SHARE_STUB: &str = "> PROFILE SHARING IS NOT BUILT IN WH YET";

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

/// The board's own adjust-mode edges (`docs/protocol.md`, "The board announces its own adjust
/// mode") shown to the operator: while locked the board will not type until the key that started
/// the edit is pressed again, measured, not inferred, so the banner says exactly that.
pub const LOCKED_BANNER: &str =
    "BOARD LOCKED: ADJUSTING ON THE KEYBOARD ITSELF. IT WILL NOT TYPE UNTIL THE KEY IS PRESSED AGAIN.";

/// `tick`'s note for a `BoardEvent::Unknown`: an unmeasured `00 be` third byte, or any other
/// device-initiated frame `wh` does not recognise. Named, not inferred: the frame arrived, its
/// meaning did not.
pub const STATUS_UNKNOWN_EVENT: &str = "NOTE: UNRECOGNISED BOARD EVENT RECEIVED";

/// `tick`'s note when the re-read `BoardModel::read` triggers after `AdjustModeLeft` times out:
/// the old model is kept rather than discarded, so the operator is told the screen may be stale
/// rather than shown a mid-read model.
const STATUS_REREAD_TIMED_OUT: &str = "NOTE: A READ TIMED OUT; VALUES MAY BE STALE";

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

/// ADVANCED's own sub-tab, the vendor's own order (`08`-`11-advanced-*.png`). Cycled and clicked
/// exactly like `Tab`: Up and Down cycle it while `Tab::Advanced` is selected (Left/Right stay
/// with the top-level tabs, so the two cycles never compete for the same keys), no wrapping past
/// either end, same as `Tab`'s own Left/Right cycling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedTab {
    General,
    Gamepad,
    Device,
    Share,
}

pub const ADVANCED_TABS: [AdvancedTab; 4] = [
    AdvancedTab::General,
    AdvancedTab::Gamepad,
    AdvancedTab::Device,
    AdvancedTab::Share,
];

impl AdvancedTab {
    pub fn title(self) -> &'static str {
        match self {
            AdvancedTab::General => "GENERAL",
            AdvancedTab::Gamepad => "GAMEPAD",
            AdvancedTab::Device => "DEVICE",
            AdvancedTab::Share => "SHARE",
        }
    }

    fn index(self) -> usize {
        ADVANCED_TABS
            .iter()
            .position(|t| *t == self)
            .expect("every AdvancedTab is in ADVANCED_TABS")
    }
}

pub struct App {
    pub board: BoardModel,
    pub wh_version: String,
    pub quit: bool,
    pub tab: Tab,
    /// ADVANCED's own sub-tab. Meaningless while `tab != Tab::Advanced`, same as `advanced_rects`
    /// below; neither is reset when leaving the tab; the vendor's own configurator remembers your
    /// place too.
    pub advanced_tab: AdvancedTab,
    /// Each tab title's on-screen rect, recorded during the last `draw`, for click-to-select.
    /// Later tasks reuse this same rect-recording pattern for keys and buttons.
    pub tab_rects: Vec<(Rect, Tab)>,
    /// Each ADVANCED sub-tab title's on-screen rect, recorded during the last `draw`, filled only
    /// while `tab == Tab::Advanced` and cleared every draw otherwise, same rect-recording pattern
    /// as `tab_rects`.
    pub advanced_rects: Vec<(Rect, AdvancedTab)>,
    /// Each rendered key cap's on-screen rect and usage, recorded during the last `draw`.
    /// Rendered as `Modifier::REVERSED` when the usage is in `selection`; nothing drives
    /// `selection` yet, that arrives with click-to-select in a later plan.
    pub key_rects: Vec<(Rect, u8)>,
    pub selection: HashSet<u8>,
    /// The prompt line's right-side action button rect, recorded during the last `draw`. `None`
    /// on a tab with no prompt line. The button is inert until a later plan wires it up.
    pub prompt_action_rect: Option<Rect>,
    /// Set by `tick` on `BoardEvent::AdjustModeEntered`, cleared on `AdjustModeLeft`: the board is
    /// mid on-keyboard edit and, measured, will not type until the key that started it is pressed
    /// again. `draw` renders `LOCKED_BANNER` in the prompt line's place while this is set, and
    /// `handle_key`/`handle_mouse` ignore everything except quit and top-level tab navigation.
    pub locked: bool,
    /// A one-line note `tick` sets, most recently for an unrecognised board event or a re-read
    /// that timed out. Cleared by the next successful `AdjustModeLeft` re-read, whatever set it:
    /// a fresh full read is the strongest signal available that the board's display is
    /// trustworthy again, and a note claiming otherwise (or naming a now-old event) has served
    /// its purpose once the operator has seen it.
    pub status: Option<String>,
}

impl App {
    pub fn new(board: BoardModel, wh_version: &str) -> Self {
        Self {
            board,
            wh_version: wh_version.to_string(),
            quit: false,
            tab: Tab::ActuationPoint,
            advanced_tab: AdvancedTab::General,
            tab_rects: Vec::new(),
            advanced_rects: Vec::new(),
            key_rects: Vec::new(),
            selection: HashSet::new(),
            prompt_action_rect: None,
            locked: false,
            status: None,
        }
    }

    /// One event-poll step of the loop: at most one `poll_event`, routing edges. Returns
    /// `Ok(true)` when the display changed (a redraw is due). `AdjustModeLeft` re-reads the whole
    /// board, the same thing the vendor configurator does on that edge; if that read itself times
    /// out, the old model is kept and a status note says so rather than the operator being shown
    /// nothing or a half-read model. On success, any note already showing is cleared: see
    /// `status`'s own doc for why. Queued events beyond the one this call drains stay queued, the
    /// starvation ruling from the spec: this plan sets no cap on that queue.
    pub fn tick<T: Transport>(&mut self, s: &mut Session<T>) -> Result<bool, DeviceError> {
        match s.poll_event(Duration::from_millis(15))? {
            Some(BoardEvent::AdjustModeEntered) => {
                self.locked = true;
                Ok(true)
            }
            Some(BoardEvent::AdjustModeLeft) => {
                self.locked = false;
                match BoardModel::read(s) {
                    Ok(board) => {
                        self.board = board;
                        self.status = None;
                    }
                    Err(DeviceError::Timeout) => {
                        self.status = Some(STATUS_REREAD_TIMED_OUT.to_string());
                    }
                    Err(e) => return Err(e),
                }
                Ok(true)
            }
            Some(BoardEvent::Unknown(_)) => {
                self.status = Some(STATUS_UNKNOWN_EVENT.to_string());
                Ok(true)
            }
            None => Ok(false),
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
            KeyCode::Down if self.tab == Tab::Advanced && !self.locked => {
                let i = self.advanced_tab.index();
                if i + 1 < ADVANCED_TABS.len() {
                    self.advanced_tab = ADVANCED_TABS[i + 1];
                }
            }
            KeyCode::Up if self.tab == Tab::Advanced && !self.locked => {
                let i = self.advanced_tab.index();
                if i > 0 {
                    self.advanced_tab = ADVANCED_TABS[i - 1];
                }
            }
            _ => {}
        }
    }

    /// Click-to-select over the rects `draw` recorded into `tab_rects`. Only a left button-down
    /// selects; drags, releases and other buttons are ignored. While `locked`, only the top-level
    /// tab row still responds: `advanced_rects` (and every later click target) is ignored, the
    /// same "quit and tab navigation only" rule `handle_key` applies.
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
        if self.locked {
            return;
        }
        for (rect, sub) in &self.advanced_rects {
            let hit = col >= rect.x && col < rect.x + rect.width && row == rect.y;
            if hit {
                self.advanced_tab = *sub;
                return;
            }
        }
    }
}

/// Renders one row of clickable, bracket-and-reverse-marked titles at `(x0, y)`: the vendor's own
/// selected-tab shape, wrapped in `[` `]` and reversed, plain otherwise, each separated by two
/// spaces. Shared by the top `Tab` row and ADVANCED's own sub-tab row below it, the same shape
/// both places, so the two can never drift into different click-rect or render logic. Returns
/// each title's own rect paired with its value, for the caller to record for click-to-select.
fn render_tab_like_row<T: Copy>(
    f: &mut Frame,
    y: u16,
    x0: u16,
    row_width: u16,
    items: &[T],
    selected: impl Fn(T) -> bool,
    title: impl Fn(T) -> &'static str,
) -> Vec<(Rect, T)> {
    let mut spans = Vec::new();
    let mut rects = Vec::with_capacity(items.len());
    let mut x = x0;
    for (i, item) in items.iter().enumerate() {
        let is_selected = selected(*item);
        let text = if is_selected {
            format!("[{}]", title(*item))
        } else {
            title(*item).to_string()
        };
        let width = text.chars().count() as u16;
        rects.push((Rect::new(x, y, width, 1), *item));
        let style = if is_selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        spans.push(Span::styled(text, style));
        x += width;
        if i + 1 < items.len() {
            spans.push(Span::raw("  "));
            x += 2;
        }
    }
    f.render_widget(Line::from(spans), Rect::new(x0, y, row_width, 1));
    rects
}

/// The left pane's width. 46 (task 6's placeholder, explicitly ledgered as unpinned) is too
/// narrow for the mandated prompt line: "> CLICK ON THE KEYS TO MAKE A KEYSET" (36) plus
/// "[RESET KEYSETS]" (15) is 51 columns with no gap between them at all. 56 leaves that gap a
/// visible 5 columns.
const LEFT_WIDTH: u16 = 56;

/// Word-wraps `text` to fit `width` columns, greedy, breaking only at spaces. Every stub constant
/// above fits `left_area`'s 56 columns on one line except `MAPPING_STUB`, whose own cited
/// docs/tasks.md task number pushes it past that width; wrapping keeps the exact text intact
/// rather than truncating it or letting it run into the matrix pane on the right.
fn wrap_stub(text: &str, width: u16) -> Vec<String> {
    let width = width as usize;
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split(' ') {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if candidate.chars().count() > width && !current.is_empty() {
            lines.push(current);
            current = word.to_string();
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Renders `text` word-wrapped (see `wrap_stub`) starting at `*ry` within `area`, advancing
/// `*ry` one row per wrapped line and stopping once `area`'s bottom is reached.
fn render_stub(f: &mut Frame, area: Rect, ry: &mut u16, text: &str) {
    for line in wrap_stub(text, area.width) {
        if *ry >= area.y + area.height {
            break;
        }
        f.render_widget(Line::raw(line), Rect::new(area.x, *ry, area.width, 1));
        *ry += 1;
    }
}

/// The longest single word in `text`: the narrowest a pane can be and still hold `text` whole,
/// since `wrap_stub` breaks only at spaces and a word wider than the pane would be clipped.
fn longest_word(text: &str) -> u16 {
    text.split(' ')
        .map(|w| w.chars().count() as u16)
        .max()
        .unwrap_or(0)
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
        Line::raw(format!("[X] {DEVICE_NAME} - {}", app.board.firmware)),
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
    app.tab_rects = render_tab_like_row(f, y, 0, area.width, &TABS, |t| t == app.tab, Tab::title);

    // The body: everything between the tab row and the footer, split horizontally. The left
    // pane holds the settings rows below; the matrix takes whatever is left.
    let body_y = y + 1;
    let footer_y = area.height.saturating_sub(1);
    let body_height = footer_y.saturating_sub(body_y);
    // ADVANCED's GAMEPAD, DEVICE and SHARE sub-tabs drop the keyboard pane, the vendor's own
    // layout (see the design spec): there is no per-key value on any of them, so the left pane
    // takes the whole width and no cap is drawn or recorded.
    let show_matrix = match app.tab {
        Tab::Advanced => app.advanced_tab == AdvancedTab::General,
        _ => true,
    };
    let left_width = if show_matrix {
        area.width.min(LEFT_WIDTH)
    } else {
        area.width
    };
    let matrix_area = Rect::new(
        left_width,
        body_y,
        area.width.saturating_sub(left_width),
        body_height,
    );

    // The left pane: for MAPPING and ADVANCED, sub-tab label rows above the settings rows (the
    // latter interactive, its own rect-recording pattern); then settings rows for whichever tab
    // (and, on ADVANCED, sub-tab) is active; then either the AP/RT keyset prompt or a single
    // honest-stub line naming what is not built yet. ADVANCED > DEVICE is the one sub-tab that
    // gets no stub line: it is live, not a stub.
    let left_area = Rect::new(0, body_y, left_width, body_height);
    let mut ry = left_area.y;
    app.advanced_rects = Vec::new();

    if app.tab == Tab::Mapping {
        f.render_widget(
            Line::raw(MAPPING_LABELS_ROW_1),
            Rect::new(left_area.x, ry, left_area.width, 1),
        );
        ry += 1;
        f.render_widget(
            Line::raw(MAPPING_LABELS_ROW_2),
            Rect::new(left_area.x, ry, left_area.width, 1),
        );
        ry += 1;
    }

    if app.tab == Tab::Advanced {
        app.advanced_rects = render_tab_like_row(
            f,
            ry,
            left_area.x,
            left_area.width,
            &ADVANCED_TABS,
            |t| t == app.advanced_tab,
            AdvancedTab::title,
        );
        ry += 1;
    }

    let settings_rows = match app.tab {
        Tab::ActuationPoint => rows::ap_rows(&app.board.keys, app.board.global.travel),
        Tab::RapidTrigger => rows::rt_rows(&app.board.keys),
        Tab::Mapping => Vec::new(),
        Tab::Switches => rows::switches_rows(),
        Tab::Advanced => match app.advanced_tab {
            AdvancedTab::General => rows::advanced_general_rows(),
            AdvancedTab::Gamepad => rows::advanced_gamepad_rows(),
            AdvancedTab::Device => rows::advanced_device_rows(&app.board),
            AdvancedTab::Share => rows::advanced_share_rows(),
        },
    };
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

    // While locked, this row renders `LOCKED_BANNER` instead (below, after the matrix, since the
    // banner spans the full frame width and would otherwise be cut back by the matrix pane's own
    // render over the same row): the tab's own prompt or stub does not appear at all, matching
    // "ignore everything except quit and tab navigation" (there is nothing left to click).
    app.prompt_action_rect = None;
    if !app.locked && ry < left_area.y + left_area.height {
        match app.tab {
            Tab::ActuationPoint | Tab::RapidTrigger => {
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
                ry += 1;
            }
            Tab::Mapping => render_stub(f, left_area, &mut ry, MAPPING_STUB),
            Tab::Switches => render_stub(f, left_area, &mut ry, SWITCHES_STUB),
            Tab::Advanced => match app.advanced_tab {
                AdvancedTab::General => render_stub(f, left_area, &mut ry, ADVANCED_GENERAL_STUB),
                AdvancedTab::Gamepad => render_stub(f, left_area, &mut ry, ADVANCED_GAMEPAD_STUB),
                AdvancedTab::Device => {}
                AdvancedTab::Share => render_stub(f, left_area, &mut ry, ADVANCED_SHARE_STUB),
            },
        }
    }
    // The row the locked banner or the status note lands on: right where the tab's own prompt or
    // stub would otherwise sit (or, while locked, right where it was skipped above). Rendered
    // after the matrix below, full frame width, so it is never cut back by the matrix pane's own
    // render over the same row.
    let note_row = ry;

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
    if show_matrix {
        // A board with no rows (nothing to draw) is not a refusal: `needed` is 0 and the pane
        // stays empty. Anything else either fits or gets the refusal, never a clipped grid.
        let needed = needed_width(&app.board.rows);
        let fits = needed <= matrix_area.width && matrix_area.height > 0;
        if fits {
            render_matrix(
                matrix_area,
                f.buffer_mut(),
                &app.board.rows,
                value_of,
                &app.selection,
                &mut key_rects,
            );
        } else if needed > 0 {
            // The refusal, whole, wherever it fits whole: the matrix pane when that pane can
            // hold the message's longest word, otherwise the left pane below the note row, which
            // at those widths is the frame's full width. A clipped half-sentence, or a blank
            // right half with no explanation, leaves the operator nothing to act on.
            let text = too_narrow_text(LEFT_WIDTH.saturating_add(needed));
            if matrix_area.height > 0 && matrix_area.width >= longest_word(&text) {
                let mut my = matrix_area.y;
                render_stub(f, matrix_area, &mut my, &text);
            } else {
                let mut my = note_row + 1;
                render_stub(f, left_area, &mut my, &text);
            }
        }
    }
    app.key_rects = key_rects;

    // Rendered after the matrix, and space-padded to the full frame width by hand rather than
    // through `Line::raw`: a widget only writes as many cells as its text holds, so a text
    // shorter than the matrix pane's own column would otherwise leave that pane's characters
    // showing through on the same row.
    if note_row < left_area.y + left_area.height {
        let note_text = if app.locked {
            Some(LOCKED_BANNER)
        } else {
            app.status.as_deref()
        };
        if let Some(text) = note_text {
            let width = area.width as usize;
            let mut chars: Vec<char> = text.chars().collect();
            chars.truncate(width);
            while chars.len() < width {
                chars.push(' ');
            }
            let line: String = chars.into_iter().collect();
            f.buffer_mut()
                .set_string(0, note_row, &line, Style::default());
        }
    }

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
