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

pub const MODE_APPLICATION_KEYPAD: u8 = 0b0000_0001;
pub const MODE_APPLICATION_CURSOR: u8 = 0b0000_0010;
pub const MODE_HIDE_CURSOR: u8 = 0b0000_0100;
pub const MODE_ALTERNATE_SCREEN: u8 = 0b0000_1000;
pub const MODE_BRACKETED_PASTE: u8 = 0b0001_0000;
/// Insert mode (IRM, `CSI 4 h`): text pushes the rest of the row right.
pub const MODE_INSERT: u8 = 0b0010_0000;
/// Autowrap turned off (DECAWM, `CSI ? 7 l`): text stops at the last
/// column instead of going on to the next row.
pub const MODE_NO_WRAP: u8 = 0b0100_0000;

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
    pub(crate) modes: u8,
    pub(crate) mouse_protocol_mode: MouseProtocolMode,
    pub(crate) mouse_protocol_encoding: MouseProtocolEncoding,
    pub(crate) cursor_style: CursorStyle,
    /// The last character written, for REP.
    pub(crate) last_char: Option<char>,
    /// A flag a column: a tab stop there. One set for both screens, as
    /// tmux keeps them.
    tabs: Vec<bool>,
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
            mouse_protocol_encoding: MouseProtocolEncoding::Default,
            cursor_style: CursorStyle::Default,
            last_char: None,
            tabs: default_tabs(size.cols.get()),
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
        if size.cols != self.alternate_grid.size().cols {
            self.tabs = default_tabs(size.cols.get());
        }
        if !(self.reflow && self.mode(MODE_ALTERNATE_SCREEN)) {
            self.resize_normal(size, &mut on_normal);
        }
        self.alternate_grid.set_size(size, on_alternate);
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

    #[must_use]
    pub const fn mouse_protocol_encoding(&self) -> MouseProtocolEncoding {
        self.mouse_protocol_encoding
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

    pub(crate) fn enter_alternate_grid(&mut self) {
        self.grid_mut().set_history_offset(0);
        self.set_mode(MODE_ALTERNATE_SCREEN);
        self.alternate_grid.allocate_rows();
    }

    pub(crate) fn exit_alternate_grid(&mut self) {
        self.clear_mode(MODE_ALTERNATE_SCREEN);
        // Resized while the alternate screen showed: the normal screen takes
        // the size now, as tmux's `screen_alternate_off`.
        let size = self.alternate_grid.size();
        if self.reflow && self.grid.size() != size {
            self.resize_normal(size, |_| {});
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

    pub(crate) const fn set_mode(&mut self, mode: u8) {
        self.modes |= mode;
    }

    pub(crate) const fn clear_mode(&mut self, mode: u8) {
        self.modes &= !mode;
    }

    pub(crate) const fn mode(&self, mode: u8) -> bool {
        self.modes & mode != 0
    }

    pub(crate) fn clear_mouse_mode(&mut self, mode: MouseProtocolMode) {
        if self.mouse_protocol_mode == mode {
            self.mouse_protocol_mode = MouseProtocolMode::None;
        }
    }

    pub(crate) fn clear_mouse_encoding(&mut self, encoding: MouseProtocolEncoding) {
        if self.mouse_protocol_encoding == encoding {
            self.mouse_protocol_encoding = MouseProtocolEncoding::Default;
        }
    }
}
