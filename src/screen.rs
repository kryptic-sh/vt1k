//! The terminal's state: its grids, the pen, modes and hyperlinks.

use alloc::{borrow::Cow, string::String, vec::Vec};

use crate::attrs::Attrs;
use crate::grid::{Grid, Pos, Reflow, Size};
use crate::{Cell, Color};

/// The xterm mouse reporting a program asked for.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum MouseProtocolMode {
    /// No reports.
    #[default]
    None,
    /// Presses only (mode 9, X10).
    Press,
    /// Presses and releases (mode 1000).
    PressRelease,
    /// Those, and motion with a button held (mode 1002).
    ButtonMotion,
    /// Those, and all motion (mode 1003).
    AnyMotion,
}

/// How mouse reports are encoded.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum MouseProtocolEncoding {
    /// One byte a coordinate.
    #[default]
    Default,
    /// UTF-8 coordinates (mode 1005).
    Utf8,
    /// SGR (mode 1006).
    Sgr,
}

/// The cursor's shape, as DECSCUSR (`CSI n SP q`) sets it.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum CursorStyle {
    /// The terminal's own.
    #[default]
    Default,
    BlinkBlock,
    SteadyBlock,
    BlinkUnderline,
    SteadyUnderline,
    BlinkVerticalBar,
    SteadyVerticalBar,
}

impl CursorStyle {
    /// The style DECSCUSR's parameter names, if any.
    pub(crate) const fn from_decscusr(n: u16) -> Option<Self> {
        Some(match n {
            0 => Self::Default,
            1 => Self::BlinkBlock,
            2 => Self::SteadyBlock,
            3 => Self::BlinkUnderline,
            4 => Self::SteadyUnderline,
            5 => Self::BlinkVerticalBar,
            6 => Self::SteadyVerticalBar,
            _ => return None,
        })
    }

    /// DECSCUSR's parameter for this style: 0 for the terminal's own.
    #[must_use]
    pub const fn decscusr(self) -> u8 {
        match self {
            Self::Default => 0,
            Self::BlinkBlock => 1,
            Self::SteadyBlock => 2,
            Self::BlinkUnderline => 3,
            Self::SteadyUnderline => 4,
            Self::BlinkVerticalBar => 5,
            Self::SteadyVerticalBar => 6,
        }
    }
}

/// An OSC 8 hyperlink: where it points, and the `id` that joins the
/// pieces of one link broken up.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Hyperlink {
    /// Where the link points.
    pub uri: String,
    /// The `id` parameter it was given, if any.
    pub id: Option<String>,
}

pub const MODE_APPLICATION_KEYPAD: u16 = 0b0000_0001;
pub const MODE_APPLICATION_CURSOR: u16 = 0b0000_0010;
pub const MODE_HIDE_CURSOR: u16 = 0b0000_0100;
pub const MODE_ALTERNATE_SCREEN: u16 = 0b0000_1000;
pub const MODE_BRACKETED_PASTE: u16 = 0b0001_0000;
/// Insert mode (IRM, `CSI 4 h`): text pushes the rest of the row right.
pub const MODE_INSERT: u16 = 0b0010_0000;
/// Autowrap turned off (DECAWM, `CSI ? 7 l`): text stops at the last
/// column instead of going on to the next row.
pub const MODE_NO_WRAP: u16 = 0b0100_0000;
/// Focus reports asked for (`CSI ? 1004 h`).
pub const MODE_FOCUS: u16 = 1 << 7;
/// The cursor blinks: an odd DECSCUSR, or `CSI ? 12 h`.
pub const MODE_CURSOR_BLINK: u16 = 1 << 8;
/// The cursor made very visible (`CSI 34 l`), as tmux's
/// `MODE_CURSOR_VERY_VISIBLE`.
pub const MODE_CURSOR_VERY_VISIBLE: u16 = 1 << 9;
/// The UTF-8 mouse encoding set (1005), kept apart from SGR's as tmux
/// keeps them.
pub const MODE_MOUSE_UTF8: u16 = 1 << 10;
/// The SGR mouse encoding set (1006).
pub const MODE_MOUSE_SGR: u16 = 1 << 11;

/// The terminal's state: the normal screen with its history, the
/// alternate screen, the pen, modes and hyperlinks.
#[derive(Clone, Debug)]
pub struct Screen {
    pub(crate) grid: Grid,
    pub(crate) alternate_grid: Grid,
    pub(crate) attrs: Attrs,
    pub(crate) saved_attrs: Attrs,
    /// The OSC 8 hyperlink text is written under, as a cell keeps it (an
    /// index into `hyperlinks` plus one; 0 for none).
    pub(crate) link: u16,
    /// Each link by its number; one cleared stays as `None`, so a cell
    /// still holding its number links to nothing rather than a later link.
    hyperlinks: Vec<Option<Hyperlink>>,
    /// Rewrap the normal screen to a new width ([`Screen::set_reflow`]).
    reflow: bool,
    /// What the last reflow did, until [`Screen::take_reflow`] takes it.
    reflowed: Option<Reflow>,
    pub(crate) modes: u16,
    pub(crate) mouse_protocol_mode: MouseProtocolMode,
    pub(crate) cursor_style: CursorStyle,
    /// The last character written, for REP.
    pub(crate) last_char: Option<char>,
    /// A flag a column: a tab stop there. One set for both screens, as
    /// tmux keeps them.
    tabs: Vec<bool>,
    /// [`Screen::alternate_saved`].
    pub(crate) alternate_saved: Option<(u16, u16)>,
    /// [`Screen::set_alternate_allowed`].
    alternate_allowed: bool,
}

/// The columns between the default tab stops.
const TAB_WIDTH: u16 = 8;

/// Tab stops every [`TAB_WIDTH`] columns, as tmux's screen_reset_tabs.
fn default_tabs(cols: u16) -> Vec<bool> {
    (0..cols).map(|c| c > 0 && c % TAB_WIDTH == 0).collect()
}

impl Screen {
    pub(crate) fn new(size: Size, history: usize) -> Self {
        let mut grid = Grid::new(size, Some(history));
        grid.allocate_rows();
        Self {
            grid,
            alternate_grid: Grid::new(size, None),
            attrs: Attrs::new(),
            saved_attrs: Attrs::new(),
            link: 0,
            hyperlinks: Vec::new(),
            reflow: false,
            reflowed: None,
            modes: 0,
            mouse_protocol_mode: MouseProtocolMode::None,
            cursor_style: CursorStyle::Default,
            last_char: None,
            tabs: default_tabs(size.cols.get()),
            alternate_saved: None,
            alternate_allowed: true,
        }
    }

    /// Resize. Rows pushed off the top go to the history; none of them is
    /// reported (use [`Parser::set_size`](crate::Parser::set_size) for that).
    pub fn set_size(&mut self, rows: core::num::NonZeroU16, cols: core::num::NonZeroU16) {
        self.resize(Size { rows, cols }, |_| {}, |_| {});
    }

    /// The columns with a tab stop, counted from 0.
    pub fn tab_stops(&self) -> impl Iterator<Item = u16> + '_ {
        (0..)
            .zip(&self.tabs)
            .filter_map(|(col, &stop)| stop.then_some(col))
    }

    /// The default tab stops back, as tmux's screen_reset_tabs, for a host
    /// resetting the screen its own way.
    pub fn reset_tab_stops(&mut self) {
        self.tabs = default_tabs(self.grid().size().cols.get());
    }

    /// HTS (`ESC H`) or TBC (`CSI g`): the tab stop at the cursor's column
    /// set or cleared.
    pub(crate) fn set_tab(&mut self, on: bool) {
        let col = usize::from(self.grid().pos().col);
        if let Some(stop) = self.tabs.get_mut(col) {
            *stop = on;
        }
    }

    /// TBC 3: every tab stop cleared.
    pub(crate) fn clear_tabs(&mut self) {
        self.tabs.fill(false);
    }

    /// HT as tmux's input_c0_dispatch: the cursor on to the next tab stop,
    /// or the last column if there is none; from the last column, nowhere.
    pub(crate) fn tab(&mut self) {
        let cols = self.grid().size().cols.get();
        let col = self.grid().pos().col;
        if col >= cols - 1 {
            return;
        }
        let stop = (col + 1..cols - 1)
            .find(|&c| self.tabs[usize::from(c)])
            .unwrap_or(cols - 1);
        self.grid_mut().col_set(stop);
    }

    /// CBT as tmux's input_csi_dispatch: the cursor back `count` tab stops.
    pub(crate) fn back_tab(&mut self, count: u16) {
        let cols = self.grid().size().cols.get();
        let mut col = self.grid().pos().col.min(cols - 1);
        for _ in 0..count {
            if col == 0 {
                break;
            }
            col -= 1;
            while col > 0 && !self.tabs[usize::from(col)] {
                col -= 1;
            }
        }
        self.grid_mut().col_set(col);
    }

    /// Rewrap the normal screen and its history to a new width on a
    /// resize, as tmux's `grid_reflow`, where it is otherwise cut or
    /// padded; while the alternate screen shows, the normal screen keeps its
    /// size until the alternate screen goes. Off by default.
    pub fn set_reflow(&mut self, reflow: bool) {
        self.reflow = reflow;
    }

    pub(crate) const fn reflow(&self) -> bool {
        self.reflow
    }

    pub(crate) const fn alternate_allowed(&self) -> bool {
        self.alternate_allowed
    }

    /// What the last reflow did to the lines, once.
    pub fn take_reflow(&mut self) -> Option<Reflow> {
        self.reflowed.take()
    }

    pub(crate) fn resize(
        &mut self,
        size: Size,
        mut on_normal: impl FnMut(crate::capture::RowContents<'_>),
        on_alternate: impl FnMut(crate::capture::RowContents<'_>),
    ) {
        // A new width takes the default tab stops, as tmux's screen_resize.
        let new_width = size.cols != self.alternate_grid.size().cols;
        if !(self.reflow && self.mode(MODE_ALTERNATE_SCREEN)) {
            self.resize_normal(size, &mut on_normal);
        }
        self.alternate_grid.set_size(size, on_alternate);
        if new_width {
            self.reset_tab_stops();
        }
    }

    fn resize_normal(&mut self, size: Size, on_row: impl FnMut(crate::capture::RowContents<'_>)) {
        let cols = self.grid.size().cols;
        if self.reflow && size.cols != cols {
            self.grid.set_size(
                Size {
                    rows: size.rows,
                    cols,
                },
                on_row,
            );
            self.reflowed = Some(self.grid.reflow(size.cols.get()));
        } else {
            self.grid.set_size(size, on_row);
        }
    }

    /// Rows and columns.
    #[must_use]
    pub fn size(&self) -> (core::num::NonZeroU16, core::num::NonZeroU16) {
        let size = self.grid().size();
        (size.rows, size.cols)
    }

    /// Rows of history above the screen (none on the alternate screen).
    #[must_use]
    pub fn scrollback_len(&self) -> usize {
        self.grid().history_len()
    }

    /// The most rows of history kept.
    #[must_use]
    pub fn scrollback_capacity(&self) -> usize {
        self.grid().history_capacity()
    }

    /// Scroll the view back `rows` rows into the history (0 for the
    /// bottom); [`Screen::cell`] and the rest read the view.
    pub fn set_scrollback(&mut self, rows: usize) {
        self.grid_mut().set_history_offset(rows);
    }

    /// How far the view is scrolled back.
    #[must_use]
    pub fn scrollback(&self) -> usize {
        self.grid().history_offset()
    }

    /// Drop the history, and scroll the view to the bottom.
    pub fn clear_scrollback(&mut self) {
        self.grid_mut().erase_all_history(|_| {});
    }

    /// The view's text, rows joined by newlines (none for a wrapped row),
    /// blanks at the ends of rows and trailing empty rows left out.
    #[must_use]
    pub fn contents(&self) -> String {
        let mut out = String::new();
        self.grid().write_contents(&mut out);
        out
    }

    /// Each row of the view's text, `width` columns from column `start`.
    pub fn rows(&self, start: u16, width: u16) -> impl Iterator<Item = String> + '_ {
        self.grid().visible_rows().map(move |row| {
            let mut out = String::new();
            row.write_contents(&mut out, start, width, false);
            out
        })
    }

    /// The text from `(start_row, start_col)` up to `(end_row, end_col)`
    /// (exclusive), rows joined as [`Screen::contents`] joins them.
    #[must_use]
    pub fn contents_between(
        &self,
        start_row: u16,
        start_col: u16,
        end_row: u16,
        end_col: u16,
    ) -> String {
        match start_row.cmp(&end_row) {
            core::cmp::Ordering::Less => {
                let cols = self.size().1.get();
                let mut out = String::new();
                for (i, row) in self
                    .grid()
                    .visible_rows()
                    .enumerate()
                    .skip(usize::from(start_row))
                    .take(usize::from(end_row - start_row) + 1)
                {
                    if i == usize::from(start_row) {
                        row.write_contents(&mut out, start_col, cols - start_col, false);
                        if !row.wrapped() {
                            out.push('\n');
                        }
                    } else if i == usize::from(end_row) {
                        row.write_contents(&mut out, 0, end_col, false);
                    } else {
                        row.write_contents(&mut out, 0, cols, false);
                        if !row.wrapped() {
                            out.push('\n');
                        }
                    }
                }
                out
            }
            core::cmp::Ordering::Equal if start_col < end_col => self
                .rows(start_col, end_col - start_col)
                .nth(usize::from(start_row))
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// The cursor's row and column; the column is the screen's width when a
    /// character was just written into the last one (the next wraps).
    #[must_use]
    pub fn cursor_position(&self) -> (u16, u16) {
        let pos = self.grid().pos();
        (pos.row, pos.col)
    }

    /// The cell at `(row, col)` of the view.
    #[must_use]
    pub fn cell(&self, row: u16, col: u16) -> Option<&Cell> {
        self.grid().visible_cell(Pos { row, col })
    }

    /// The cells of line `line` of the history and the screen taken
    /// together: line 0 is the oldest row of history, the screen's rows
    /// follow it, wherever the view is scrolled to. A row of history holds
    /// only its cells up to its last written one, and can be wider than the
    /// screen after a narrowing resize without reflow.
    #[must_use]
    pub fn line_cells(&self, line: usize) -> Option<Cow<'_, [Cell]>> {
        self.grid().line(line).map(|row| match row {
            Cow::Borrowed(row) => Cow::Borrowed(row.as_slice()),
            Cow::Owned(row) => Cow::Owned(row.into_cells()),
        })
    }

    /// Line `line`'s text goes on into the next line.
    #[must_use]
    pub fn line_wrapped(&self, line: usize) -> bool {
        self.grid().line_wrapped(line)
    }

    /// While the alternate screen shows, row `row` of the normal screen
    /// under it.
    #[must_use]
    pub fn normal_screen_row_cells(&self, row: u16) -> Option<&[Cell]> {
        if !self.alternate_screen() {
            return None;
        }
        self.grid.visible_row(row).map(crate::row::Row::as_slice)
    }

    /// While the alternate screen shows, whether row `row` of the normal
    /// screen under it wraps.
    #[must_use]
    pub fn normal_screen_row_wrapped(&self, row: u16) -> bool {
        self.alternate_screen()
            && self
                .grid
                .visible_row(row)
                .is_some_and(crate::row::Row::wrapped)
    }

    /// The hyperlink a cell's [`Cell::hyperlink_id`] names.
    #[must_use]
    pub fn hyperlink(&self, id: core::num::NonZeroU16) -> Option<&Hyperlink> {
        self.hyperlinks.get(usize::from(id.get()) - 1)?.as_ref()
    }

    /// Forget every hyperlink: cells keep their ids, which then name none,
    /// and text written from here on is under none.
    pub fn clear_hyperlinks(&mut self) {
        self.hyperlinks.fill(None);
        self.link = 0;
    }

    /// Write text from here on under the OSC 8 link with `params` and
    /// `uri`, none for an empty `uri`.
    pub(crate) fn set_hyperlink(&mut self, params: &[u8], uri: &[u8]) {
        if uri.is_empty() {
            self.link = 0;
            return;
        }
        let id = params
            .split(|&b| b == b':')
            .find_map(|p| p.strip_prefix(b"id="))
            .map(|id| String::from_utf8_lossy(id).into_owned());
        let link = Hyperlink {
            uri: String::from_utf8_lossy(uri).into_owned(),
            id,
        };
        let index = self
            .hyperlinks
            .iter()
            .position(|h| h.as_ref() == Some(&link))
            .unwrap_or_else(|| {
                self.hyperlinks.push(Some(link));
                self.hyperlinks.len() - 1
            });
        self.link = u16::try_from(index + 1).unwrap_or(0);
    }

    /// Row `row` of the view goes on into the next.
    #[must_use]
    pub fn row_wrapped(&self, row: u16) -> bool {
        self.grid()
            .visible_row(row)
            .is_some_and(crate::row::Row::wrapped)
    }

    #[must_use]
    pub const fn alternate_screen(&self) -> bool {
        self.mode(MODE_ALTERNATE_SCREEN)
    }

    #[must_use]
    pub const fn application_keypad(&self) -> bool {
        self.mode(MODE_APPLICATION_KEYPAD)
    }

    #[must_use]
    pub const fn application_cursor(&self) -> bool {
        self.mode(MODE_APPLICATION_CURSOR)
    }

    #[must_use]
    pub const fn hide_cursor(&self) -> bool {
        self.mode(MODE_HIDE_CURSOR)
    }

    /// Insert mode (IRM, `CSI 4 h`) is on.
    #[must_use]
    pub const fn insert_mode(&self) -> bool {
        self.mode(MODE_INSERT)
    }

    /// Autowrap (DECAWM, `CSI ? 7 h`) is on, as it is to begin with.
    #[must_use]
    pub const fn autowrap(&self) -> bool {
        !self.mode(MODE_NO_WRAP)
    }

    #[must_use]
    pub const fn bracketed_paste(&self) -> bool {
        self.mode(MODE_BRACKETED_PASTE)
    }

    #[must_use]
    pub const fn mouse_protocol_mode(&self) -> MouseProtocolMode {
        self.mouse_protocol_mode
    }

    /// The encoding mouse reports take: SGR's before UTF-8's when both are
    /// set, as tmux's input_key_get_mouse picks.
    #[must_use]
    pub const fn mouse_protocol_encoding(&self) -> MouseProtocolEncoding {
        if self.mode(MODE_MOUSE_SGR) {
            MouseProtocolEncoding::Sgr
        } else if self.mode(MODE_MOUSE_UTF8) {
            MouseProtocolEncoding::Utf8
        } else {
            MouseProtocolEncoding::Default
        }
    }

    /// The UTF-8 mouse encoding (1005) is set, beside SGR's or not, as
    /// tmux's `MODE_MOUSE_UTF8`.
    #[must_use]
    pub const fn mouse_utf8(&self) -> bool {
        self.mode(MODE_MOUSE_UTF8)
    }

    /// Focus reports were asked for (mode 1004).
    #[must_use]
    pub const fn focus_reports(&self) -> bool {
        self.mode(MODE_FOCUS)
    }

    /// The cursor blinks, as the program last set it: an odd DECSCUSR, or
    /// mode 12.
    #[must_use]
    pub const fn cursor_blinking(&self) -> bool {
        self.mode(MODE_CURSOR_BLINK)
    }

    /// The cursor was made very visible (`CSI 34 l`; `CSI 34 h` ends it).
    #[must_use]
    pub const fn cursor_very_visible(&self) -> bool {
        self.mode(MODE_CURSOR_VERY_VISIBLE)
    }

    /// Origin mode (DECOM, `CSI ? 6 h`) is on.
    #[must_use]
    pub const fn origin_mode(&self) -> bool {
        self.grid().origin_mode()
    }

    /// The scroll region's top and bottom rows, counted from 0.
    #[must_use]
    pub const fn scroll_region(&self) -> (u16, u16) {
        self.grid().scroll_region()
    }

    /// Where the cursor was (row, column) when `CSI ? 1049 h` last switched
    /// to the alternate screen, as tmux's `saved_cx` / `saved_cy`: kept after
    /// the switch back, and not set by 47 or 1047.
    #[must_use]
    pub const fn alternate_saved(&self) -> Option<(u16, u16)> {
        self.alternate_saved
    }

    /// Whether the program may switch to the alternate screen (tmux's
    /// `alternate-screen` option); when not, its switches are ignored.
    pub const fn set_alternate_allowed(&mut self, allowed: bool) {
        self.alternate_allowed = allowed;
    }

    #[must_use]
    pub const fn cursor_style(&self) -> CursorStyle {
        self.cursor_style
    }

    /// The pen's foreground.
    #[must_use]
    pub const fn fgcolor(&self) -> Color {
        self.attrs.fg
    }

    /// The pen's background.
    #[must_use]
    pub const fn bgcolor(&self) -> Color {
        self.attrs.bg
    }

    pub(crate) const fn grid(&self) -> &Grid {
        if self.mode(MODE_ALTERNATE_SCREEN) {
            &self.alternate_grid
        } else {
            &self.grid
        }
    }

    pub(crate) const fn grid_mut(&mut self) -> &mut Grid {
        if self.mode(MODE_ALTERNATE_SCREEN) {
            &mut self.alternate_grid
        } else {
            &mut self.grid
        }
    }

    /// To the alternate screen (47, 1047 or 1049 set) as tmux's
    /// screen_alternate_on: cleared, the cursor where it was, the scroll
    /// region and origin mode the same (tmux keeps one for the terminal);
    /// 1049 saves the cursor for its way back. Nothing when already there
    /// or when switches are not allowed.
    pub(crate) fn alternate_on(&mut self, mode: u16) {
        if !self.alternate_allowed || self.mode(MODE_ALTERNATE_SCREEN) {
            return;
        }
        let pos = self.grid.pos();
        if mode == 1049 {
            self.alternate_saved = Some((pos.row, pos.col));
            self.save_cursor();
        }
        self.grid.set_history_offset(0);
        self.alternate_grid.clear();
        self.alternate_grid.allocate_rows();
        self.alternate_grid.take_region_from(&self.grid);
        self.alternate_grid.put_pos(pos);
        self.set_mode(MODE_ALTERNATE_SCREEN);
    }

    /// From the alternate screen (47, 1047 or 1049 reset) as tmux's
    /// screen_alternate_off: 1049 puts back the cursor it saved (even
    /// when not on the alternate screen), 47 and 1047 keep it where it is;
    /// the scroll region and origin mode come along.
    pub(crate) fn alternate_off(&mut self, mode: u16) {
        if !self.alternate_allowed {
            return;
        }
        let was = self.mode(MODE_ALTERNATE_SCREEN);
        if !was && mode != 1049 {
            return;
        }
        let pos = self.alternate_grid.pos();
        if was {
            self.clear_mode(MODE_ALTERNATE_SCREEN);
            // Resized while the alternate screen showed: the normal screen
            // takes the size now, as tmux's `screen_alternate_off`.
            let size = self.alternate_grid.size();
            if self.reflow && self.grid.size() != size {
                self.resize_normal(size, |_| {});
            }
            self.grid.take_region_from(&self.alternate_grid);
        }
        if mode == 1049 {
            self.restore_cursor();
        } else {
            self.grid.put_pos(pos);
        }
    }

    pub(crate) const fn save_cursor(&mut self) {
        self.grid_mut().save_cursor();
        self.saved_attrs = self.attrs;
    }

    pub(crate) const fn restore_cursor(&mut self) {
        self.grid_mut().restore_cursor();
        self.attrs = self.saved_attrs;
    }

    pub(crate) const fn set_mode(&mut self, mode: u16) {
        self.modes |= mode;
    }

    pub(crate) const fn clear_mode(&mut self, mode: u16) {
        self.modes &= !mode;
    }

    pub(crate) const fn mode(&self, mode: u16) -> bool {
        self.modes & mode != 0
    }
}
