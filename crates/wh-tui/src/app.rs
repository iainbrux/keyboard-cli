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

/// Wallhack's own mark, used with Wallhack's permission (held by the operator, stated 2026-09-06),
/// extracted verbatim from the vendor bundle's own render (`research/vendor-bundle/2026-09-05/
/// assets/index-DWlSnOsR.js`), not redrawn.
pub const LOGO: &[&str] = &[
    "00000000000000000000000",
    "0000000000000000000000000",
    "000000000000000000000000000",
    "00000000000000000000000000000",
    "000000               0000000000",
    "000000   000000000     00000000",
    "000000   00000000000     000000",
    "000000     00000000000     0000",
    "00000000     00000000000     00",
    " 000000000     00000000000",
    "   000000000     00000000000",
    "     000000000     00000000000",
    "00     000000000     0000000000",
    "0000     000000000     00000000",
    "000000     000000000     000000",
    "00000000     000000000     0000",
    " 000000000     00000000      00",
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

/// `tick`'s note while the `AdjustModeLeft` re-read is in flight, drawn before the read starts.
/// The read blocks the single thread and Ctrl-C with it (see `tick`), so the one honest thing to
/// do is say so rather than leave a frozen screen looking crashed.
pub const STATUS_REREADING: &str = "NOTE: RE-READING THE BOARD; INPUT WAITS UNTIL IT ANSWERS";

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
    ///
    /// **That re-read blocks everything.** It runs on the one thread, so while it is in flight no
    /// key is read, and raw mode has cleared ISIG, so Ctrl-C is only another byte nobody is
    /// reading. `redraw` is called once first, with `STATUS_REREADING` showing, so the screen
    /// says what it is doing instead of appearing dead. The exposure, worst case: one roundtrip
    /// gives up after `session::TOTAL_TIMEOUT` (1500ms), and the read is 6 roundtrips plus 6 per
    /// key, 414 on a 68-key board, so a board answering slowly enough to use its whole budget on
    /// every one of them freezes the UI for about ten minutes. A board that simply stops
    /// answering ends it at the first roundtrip's 1.5s. Plan 2 needs a real answer here when
    /// writes join this path.
    pub fn tick<T: Transport>(
        &mut self,
        s: &mut Session<T>,
        redraw: &mut impl FnMut(&mut App),
    ) -> Result<bool, DeviceError> {
        match s.poll_event(Duration::from_millis(15))? {
            Some(BoardEvent::AdjustModeEntered) => {
                self.locked = true;
                Ok(true)
            }
            Some(BoardEvent::AdjustModeLeft) => {
                self.locked = false;
                self.status = Some(STATUS_REREADING.to_string());
                redraw(self);
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
        format!("WALLHACK TERMINAL BY @BRUX - V{}", self.wh_version)
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

/// The left pane's width. The tab row is 61 columns wide at any selection (exactly one title
/// gains its two bracket characters, so the total never changes), and the tab row must fit inside
/// this pane for the right pane's own prompt to share its row without overwriting it (see `draw`):
/// 62 is that minimum with no gap at all. 64 leaves a two-column gap, visible but not wasteful.
const LEFT_WIDTH: u16 = 64;

/// Word-wraps `text` to fit `width` columns, greedy, breaking only at spaces. A stub renders on
/// one row of the tab row now (see `draw`) and never needs this; the locked banner and the
/// too-narrow refusal, both rendered at the frame's own width, still can.
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

/// Renders the body's message lines at `width` (see `draw`: the left pane's own width whenever a
/// matrix is drawn beside it, the frame's full width otherwise), starting at `note_row` and
/// running down from there. When the block will not fit before the body ends it is pushed up over
/// the rows above instead of being cut off at the footer: a settings row hidden on a thirty-one-
/// row terminal costs the operator less than half a sentence, and half a sentence is what a
/// bottom-clipped block leaves.
///
/// Every line is padded to `width` by hand rather than rendered through `Line::raw`: a widget
/// writes only as many cells as its text holds, so a shorter line would leave whatever it covers
/// showing through as a tail.
fn render_message_block(f: &mut Frame, width: u16, body: Rect, note_row: u16, lines: &[String]) {
    if lines.is_empty() || body.height == 0 {
        return;
    }
    let body_end = body.y + body.height;
    let needed = lines.len() as u16;
    let start = if note_row.saturating_add(needed) <= body_end {
        note_row
    } else {
        body_end.saturating_sub(needed).max(body.y)
    };
    for (i, text) in lines.iter().enumerate() {
        let y = start + i as u16;
        if y >= body_end {
            break;
        }
        let mut chars: Vec<char> = text.chars().collect();
        chars.truncate(width as usize);
        while chars.len() < width as usize {
            chars.push(' ');
        }
        let line: String = chars.into_iter().collect();
        f.buffer_mut().set_string(0, y, &line, Style::default());
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
    let tab_row_y = y;

    // Two top-aligned panes (pixel-measured, see the design spec): the right pane's prompt or
    // stub shares the tab row's own y; the matrix's first cap row starts at `body_y`, level with
    // the left pane's first settings row. ADVANCED's GAMEPAD/DEVICE/SHARE drop the matrix pane
    // and keep their stub in the left pane, which has no tab row of its own to share.
    let body_y = y + 1;
    let footer_y = area.height.saturating_sub(1);
    let body_height = footer_y.saturating_sub(body_y);
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
    // (and, on ADVANCED, sub-tab) is active. ADVANCED > DEVICE gets no stub line: it is live, not
    // a stub.
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
        // While locked, `handle_key` and `handle_mouse` both ignore this row, so it is dimmed
        // like any other inert control: a live-looking sub-tab that answers nothing is something
        // the operator would otherwise have to discover by clicking it.
        if app.locked {
            f.buffer_mut().set_style(
                Rect::new(left_area.x, ry, left_area.width, 1),
                Style::default().add_modifier(Modifier::DIM),
            );
        }
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

    // ADVANCED's GAMEPAD, DEVICE and SHARE sub-tabs have no right pane (`show_matrix` is false),
    // so their stub, if any, stays here at the end of the left pane's own flow. Every other tab's
    // prompt or stub renders on the tab row instead, below. Locked renders neither: nothing here
    // is clickable while locked, so there is nothing to name.
    if !app.locked && !show_matrix && ry < left_area.y + left_area.height {
        match app.advanced_tab {
            AdvancedTab::Gamepad => render_stub(f, left_area, &mut ry, ADVANCED_GAMEPAD_STUB),
            AdvancedTab::Device => {}
            AdvancedTab::Share => render_stub(f, left_area, &mut ry, ADVANCED_SHARE_STUB),
            // Unreachable: `show_matrix` is only false on ADVANCED, and only for these three
            // sub-tabs; GENERAL always shows the matrix.
            AdvancedTab::General => {}
        }
    }
    // The row the body's own message block starts on: right where the left pane's own flow (and,
    // on the three sub-tabs above, its stub) ends. Rendered after the matrix below, full frame
    // width, so it is never cut back by the matrix pane's own render.
    let note_row = ry;

    // The right pane's prompt or stub shares the tab row, past the left pane's own width, one
    // row tall: a stub that needed to wrap here would have nowhere to go but the matrix's own
    // first row, so `render_stub` stopping after one line is the safer failure. While locked
    // there is nothing to click, so nothing renders here.
    app.prompt_action_rect = None;
    if show_matrix && !app.locked {
        let right_area = Rect::new(
            left_width,
            tab_row_y,
            area.width.saturating_sub(left_width),
            1,
        );
        match app.tab {
            Tab::ActuationPoint | Tab::RapidTrigger => {
                let keysets_empty = match app.tab {
                    Tab::ActuationPoint => ap_keysets(&app.board.keys).is_empty(),
                    Tab::RapidTrigger => rt_keysets(&app.board.keys).is_empty(),
                    _ => true,
                };
                let action_rect = render_prompt(
                    right_area,
                    f.buffer_mut(),
                    KEYSET_PROMPT,
                    RESET_KEYSETS_ACTION,
                    keysets_empty,
                );
                app.prompt_action_rect = Some(action_rect);
            }
            Tab::Mapping => {
                let mut right_ry = right_area.y;
                render_stub(f, right_area, &mut right_ry, MAPPING_STUB);
            }
            Tab::Switches => {
                let mut right_ry = right_area.y;
                render_stub(f, right_area, &mut right_ry, SWITCHES_STUB);
            }
            // Unreachable in practice: `show_matrix` on ADVANCED is only true for GENERAL.
            Tab::Advanced => {
                let mut right_ry = right_area.y;
                render_stub(f, right_area, &mut right_ry, ADVANCED_GENERAL_STUB);
            }
        }
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
            // A key in an RT keyset whose own rapid trigger is off has no sensitivity to show:
            // `rt_keyset != 0` alone would print one, the same over-wide predicate `global_rt`
            // avoids with `rt_enabled`.
            Tab::RapidTrigger => match app.board.key(usage) {
                Some(k) if k.rt_keyset != 0 && k.rt_enabled() => CapValue {
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
    let mut refusal: Option<String> = None;
    let mut matrix_rendered = false;
    if show_matrix {
        // A board with no rows (nothing to draw) is not a refusal: `needed` is 0 and the pane
        // stays empty. Anything else either fits or gets the refusal, never a clipped grid.
        let needed = needed_width(&app.board.rows);
        let fits = needed <= matrix_area.width && matrix_area.height > 0;
        if fits {
            matrix_rendered = true;
            render_matrix(
                matrix_area,
                f.buffer_mut(),
                &app.board.rows,
                value_of,
                &app.selection,
                &mut key_rects,
            );
        } else if needed > 0 {
            refusal = Some(too_narrow_text(LEFT_WIDTH.saturating_add(needed)));
        }
    }
    app.key_rects = key_rects;

    // The body's message block: the locked banner or the status note, then the too-narrow
    // refusal. Width is the left pane's own whenever the matrix is actually drawn, never the
    // frame's, so a settings-row count that puts `note_row` mid-matrix cannot paint over a cap;
    // it is the frame's full width only when there is no matrix to protect (no right pane at
    // all, or the too-narrow refusal itself, which is the one case with nothing there to hit).
    let message_width = if matrix_rendered {
        left_width
    } else {
        area.width
    };
    let mut message: Vec<String> = Vec::new();
    let note = if app.locked {
        Some(LOCKED_BANNER)
    } else {
        app.status.as_deref()
    };
    if let Some(text) = note {
        message.extend(wrap_stub(text, message_width));
    }
    if let Some(text) = &refusal {
        message.extend(wrap_stub(text, message_width));
    }
    render_message_block(f, message_width, left_area, note_row, &message);

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
        let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let lines = buffer_lines(&terminal);
        assert!(
            lines
                .iter()
                .any(|l| l == "WALLHACK TERMINAL BY @BRUX - V0.5.0-alpha"),
            "banner missing or wrong: {lines:?}"
        );
    }

    #[test]
    fn the_device_line_renders_the_firmware_from_the_board_model() {
        let mut app = App::new(crate::board::test_fixture(), "0.5.0-alpha");
        let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
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
        let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
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
        let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
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
        let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();

        let lines = buffer_lines(&terminal);
        let rect_of = |usage: u8| {
            app.key_rects
                .iter()
                .find(|&&(_, u)| u == usage)
                .unwrap_or_else(|| panic!("{usage:#04X}'s cap must be recorded"))
                .0
        };

        assert_eq!(
            cap_line(&lines, rect_of(0x04), 2),
            "│0.30 │",
            "a is in an rt keyset with rapid trigger on, must show rt_press: {lines:?}"
        );
        assert_eq!(
            cap_line(&lines, rect_of(0x1A), 2),
            "│     │",
            "w is outside any rt keyset, must show a blank value line: {lines:?}"
        );
        assert_eq!(
            cap_line(&lines, rect_of(0x16), 2),
            "│     │",
            "s is in an rt keyset but its rapid trigger is off, so it has no sensitivity to \
             show: {lines:?}"
        );
    }

    /// Pins the vendor mark's own first and last row, whole-line, plus the row count: a partial
    /// paste or a dropped row (top or bottom) fails this before it fails anything downstream.
    #[test]
    fn logo_pins_first_and_last_row_and_row_count() {
        assert_eq!(LOGO.len(), 17, "the vendor mark is 17 rows: {LOGO:?}");
        assert_eq!(
            LOGO[0], "00000000000000000000000",
            "the mark's first row: {LOGO:?}"
        );
        assert_eq!(
            LOGO[16], " 000000000     00000000      00",
            "the mark's last row: {LOGO:?}"
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
