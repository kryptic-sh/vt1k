//! Feeding a program's output to a [`Screen`].

use alloc::vec::Vec;

use unicode_width::UnicodeWidthChar as _;

use crate::Screen;
use crate::attrs::Attrs;
use crate::capture::RowContents;
use crate::grid::{Pos, Size};
use crate::screen::{
    MODE_ALTERNATE_SCREEN, MODE_APPLICATION_CURSOR, MODE_APPLICATION_KEYPAD, MODE_BRACKETED_PASTE,
    MODE_HIDE_CURSOR, MODE_INSERT, MODE_NO_WRAP, MouseProtocolEncoding, MouseProtocolMode,
};
use crate::{Callbacks, Color, UnderlineStyle};

const BASE64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=";
/// The selections OSC 52 may name.
const CLIPBOARD_SELECTOR: &[u8] = b"cpqs01234567";

/// A terminal: a [`Screen`] and the escape-sequence parser feeding it,
/// telling `CB` what the screen does not keep.
pub struct Parser<CB: Callbacks = ()> {
    parser: vte::Parser,
    terminal: Terminal<CB>,
}

impl Parser {
    /// A screen of `rows` and `cols`, keeping up to `scrollback` rows of
    /// history.
    #[must_use]
    pub fn new(
        rows: core::num::NonZeroU16,
        cols: core::num::NonZeroU16,
        scrollback: usize,
    ) -> Self {
        Self::new_with_callbacks(rows, cols, scrollback, ())
    }
}

impl Default for Parser {
    /// 24 rows of 80 columns, no history.
    fn default() -> Self {
        const ROWS: core::num::NonZeroU16 = core::num::NonZeroU16::new(24).unwrap();
        const COLS: core::num::NonZeroU16 = core::num::NonZeroU16::new(80).unwrap();
        Self::new(ROWS, COLS, 0)
    }
}

impl<CB: Callbacks> Parser<CB> {
    /// As [`Parser::new`], telling `callbacks`.
    pub fn new_with_callbacks(
        rows: core::num::NonZeroU16,
        cols: core::num::NonZeroU16,
        scrollback: usize,
        callbacks: CB,
    ) -> Self {
        Self {
            parser: vte::Parser::new(),
            terminal: Terminal {
                screen: Screen::new(Size { rows, cols }, scrollback),
                callbacks,
            },
        }
    }

    /// Feed `bytes` of output.
    pub fn process(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.terminal, bytes);
    }

    /// Resize, telling the callbacks of each row the history loses.
    pub fn set_size(&mut self, rows: core::num::NonZeroU16, cols: core::num::NonZeroU16) {
        let callbacks = core::cell::RefCell::new(&mut self.terminal.callbacks);
        self.terminal.screen.resize(
            Size { rows, cols },
            |row| callbacks.borrow_mut().on_scroll(row, false),
            |row| callbacks.borrow_mut().on_scroll(row, true),
        );
    }

    #[must_use]
    pub const fn screen(&self) -> &Screen {
        &self.terminal.screen
    }

    pub const fn screen_mut(&mut self) -> &mut Screen {
        &mut self.terminal.screen
    }

    pub const fn callbacks(&self) -> &CB {
        &self.terminal.callbacks
    }

    pub const fn callbacks_mut(&mut self) -> &mut CB {
        &mut self.terminal.callbacks
    }

    pub const fn screen_and_callbacks_mut(&mut self) -> (&mut Screen, &mut CB) {
        (&mut self.terminal.screen, &mut self.terminal.callbacks)
    }
}

/// The screen and callbacks together, as the parser drives them.
struct Terminal<CB: Callbacks> {
    screen: Screen,
    callbacks: CB,
}

impl<CB: Callbacks> Terminal<CB> {
    /// What a row lost off the top tells the callbacks: the screen's own
    /// rows count as the alternate screen's while it shows.
    fn on_row(alternate: bool, callbacks: &mut CB) -> impl FnMut(RowContents<'_>) + '_ {
        move |row| callbacks.on_scroll(row, alternate)
    }

    fn alternate(&self) -> bool {
        self.screen.mode(MODE_ALTERNATE_SCREEN)
    }

    /// How many columns `c` takes, or `None` where it is not drawn: a
    /// control character, one wider than the screen, or a combining one
    /// with nothing before it.
    fn char_width(&self, c: char) -> Option<u16> {
        let pos = self.screen.grid().pos();
        let cols = self.screen.grid().size().cols();
        let width = match c.width() {
            None if u32::from(c) < 256 => return None,
            width => width.unwrap_or(1),
        };
        // Triple width (U+17D8) is drawn as double.
        let width = u16::try_from(width.min(2)).unwrap_or(2);
        if width > cols || (width == 0 && pos.col == 0) {
            return None;
        }
        Some(width)
    }

    /// Whether a character `width` wide written past the row's end marks
    /// the row as wrapping: only when its last column holds something, as
    /// tmux decides.
    fn should_wrap(&self, width: u16) -> bool {
        let pos = self.screen.grid().pos();
        let cols = self.screen.grid().size().cols();
        pos.col + width > cols
            && self
                .screen
                .grid()
                .drawing_cell(Pos {
                    row: pos.row,
                    col: cols - 1,
                })
                .is_some_and(|c| c.has_contents() || c.is_wide_continuation())
    }

    fn text(&mut self, c: char) {
        let pos = self.screen.grid().pos();
        let attrs = self.screen.attrs;
        let Some(width) = self.char_width(c) else {
            return;
        };
        let mut base;
        let was_wide;
        if width > 0 {
            let cols = self.screen.grid().size().cols();
            let autowrap = self.screen.autowrap();
            // Without autowrap a wide character that does not fit is left
            // out, as tmux's screen_write_cell.
            if !autowrap && width > 1 && (pos.col != cols && pos.col + width > cols) {
                return;
            }
            if self.screen.mode(MODE_INSERT) {
                let col = pos.col;
                if let Some(row) = self.screen.grid_mut().drawing_row_mut(pos.row) {
                    row.insert(col, width);
                }
            }
            if autowrap {
                let wrap = self.should_wrap(width);
                let on_row = Self::on_row(self.alternate(), &mut self.callbacks);
                self.screen.grid_mut().col_wrap(width, wrap, on_row);
            } else if pos.col + width > cols {
                return;
            }
            base = self.screen.grid().pos();
            was_wide = self.cell(base).is_wide();
            let link = self.screen.link;
            if let Some(cell) = self.cell_mut(base) {
                cell.set(c, attrs);
                cell.set_link(link);
            }
            self.screen.last_char = Some(c);
            if autowrap {
                self.screen.grid_mut().col_inc(width);
            } else {
                // The cursor sticks at the last column (`col_set` keeps it
                // on the row), as tmux's.
                self.screen.grid_mut().col_set(base.col + width);
            }
        } else {
            // A character combining with the one before the cursor (the
            // last one, where a widened character left the cursor past it).
            let cols = self.screen.grid().size().cols();
            base = Pos {
                row: pos.row,
                col: (pos.col - 1).min(cols - 1),
            };
            if base.col > 0 && self.cell(base).is_wide_continuation() {
                base.col -= 1;
            }
            was_wide = self.cell(base).is_wide();
            if let Some(cell) = self.cell_mut(base) {
                cell.append(c);
            }
            if !was_wide && self.cell(base).is_wide() {
                // It widened past the row's end: it moves to the next row.
                let on_row = Self::on_row(self.alternate(), &mut self.callbacks);
                self.screen.grid_mut().col_wrap(1, false, on_row);
                let now = self.screen.grid().pos();
                self.wrap_widened_cell(base, now);
                if now.col == 0 {
                    base = now;
                }
            }
        }
        let is_wide = self.cell(base).is_wide();
        self.text_cleanup(base, was_wide, is_wide);
    }

    /// A cell widened by a combining character: moved to the next row's
    /// start if it no longer fits, the cursor past it.
    fn wrap_widened_cell(&mut self, old: Pos, now: Pos) {
        if now.col == 0 {
            let attrs = self.screen.attrs;
            let (contents, link) = {
                let cell = self.cell(old);
                (alloc::string::String::from(cell.contents()), cell.link())
            };
            if let Some(moved) = self.cell_mut(now) {
                moved.set_contents(&contents, attrs);
                moved.set_link(link);
            }
            if let Some(cell) = self.cell_mut(old) {
                cell.clear(attrs);
            }
            self.screen.grid_mut().col_inc(2);
        } else {
            self.screen.grid_mut().col_inc(1);
        }
    }

    /// After a character went into `base`, the wide characters around it it
    /// broke cleared, and a new wide one's second half marked.
    fn text_cleanup(&mut self, base: Pos, was_wide: bool, is_wide: bool) {
        let cols = self.screen.grid().size().cols();
        let attrs = self.screen.attrs;
        let link = self.screen.link;
        if base.col > 0
            && let Some(prev) = self.cell_mut(Pos {
                row: base.row,
                col: base.col - 1,
            })
            && prev.is_wide()
        {
            let a = *prev.attrs();
            prev.clear(a);
        }
        let next = Pos {
            row: base.row,
            col: base.col + 1,
        };
        if was_wide
            && !is_wide
            && let Some(cell) = self.cell_mut(next)
        {
            cell.set(' ', attrs);
        }
        if !was_wide
            && is_wide
            && let Some(cell) = self.cell_mut(next)
        {
            let next_was_wide = cell.is_wide();
            cell.clear(Attrs::new());
            cell.set_wide_continuation(true);
            cell.set_link(link);
            if next_was_wide {
                if let Some(cell) = self.cell_mut(Pos {
                    row: base.row,
                    col: base.col + 2,
                }) {
                    cell.clear(attrs);
                }
                if base.col + 2 == cols - 1
                    && let Some(row) = self.screen.grid_mut().drawing_row_mut(base.row)
                {
                    row.wrap(false);
                }
            }
        }
    }

    /// The cell at `pos`, a blank one off the grid (see [`Self::cell_mut`]).
    fn cell(&self, pos: Pos) -> &crate::Cell {
        self.screen.grid().drawing_cell(pos).unwrap_or(&BLANK)
    }

    /// The cell at `pos`. Off the grid only where a character widened by
    /// what combined with it no longer fits its row (vt100 panics there);
    /// what would have gone there is then left out.
    fn cell_mut(&mut self, pos: Pos) -> Option<&mut crate::Cell> {
        self.screen.grid_mut().drawing_cell_mut(pos)
    }

    fn lf(&mut self) {
        let on_row = Self::on_row(self.alternate(), &mut self.callbacks);
        self.screen.grid_mut().row_inc_scroll(1, on_row);
    }

    fn ris(&mut self) {
        let size = self.screen.grid.size();
        let history = self.screen.grid.history_capacity();
        self.screen = Screen::new(size, history);
    }

    fn rep(&mut self, count: u16) {
        if let Some(c) = self.screen.last_char {
            for _ in 0..count {
                self.text(c);
            }
        }
    }

    fn ed(&mut self, mode: u16) -> bool {
        let attrs = self.screen.attrs;
        match mode {
            0 => self.screen.grid_mut().erase_all_forward(attrs),
            1 => self.screen.grid_mut().erase_all_backward(attrs),
            2 => self.screen.grid_mut().erase_all(attrs),
            3 => {
                let on_row = Self::on_row(self.alternate(), &mut self.callbacks);
                self.screen.grid_mut().erase_all_history(on_row);
            }
            _ => return false,
        }
        true
    }

    fn el(&mut self, mode: u16) -> bool {
        let attrs = self.screen.attrs;
        match mode {
            0 => self.screen.grid_mut().erase_row_forward(attrs),
            1 => self.screen.grid_mut().erase_row_backward(attrs),
            2 => self.screen.grid_mut().erase_row(attrs),
            _ => return false,
        }
        true
    }

    /// `CSI ? … h` / `l`: each parameter set or reset, the callbacks told of
    /// each one not handled (with the whole sequence).
    fn dec_mode(&mut self, params: &vte::Params, set: bool, unhandled: &mut impl FnMut(&mut Self)) {
        for param in params {
            let screen = &mut self.screen;
            match (param, set) {
                ([7], true) => screen.clear_mode(MODE_NO_WRAP),
                ([7], false) => screen.set_mode(MODE_NO_WRAP),
                ([1], true) => screen.set_mode(MODE_APPLICATION_CURSOR),
                ([1], false) => screen.clear_mode(MODE_APPLICATION_CURSOR),
                ([6], on) => screen.grid_mut().set_origin_mode(on),
                ([9], true) => screen.mouse_protocol_mode = MouseProtocolMode::Press,
                ([9], false) => screen.clear_mouse_mode(MouseProtocolMode::Press),
                ([25], true) => screen.clear_mode(MODE_HIDE_CURSOR),
                ([25], false) => screen.set_mode(MODE_HIDE_CURSOR),
                ([47], true) => screen.enter_alternate_grid(),
                ([47], false) => screen.exit_alternate_grid(),
                ([1000], true) => screen.mouse_protocol_mode = MouseProtocolMode::PressRelease,
                ([1000], false) => screen.clear_mouse_mode(MouseProtocolMode::PressRelease),
                ([1002], true) => screen.mouse_protocol_mode = MouseProtocolMode::ButtonMotion,
                ([1002], false) => screen.clear_mouse_mode(MouseProtocolMode::ButtonMotion),
                ([1003], true) => screen.mouse_protocol_mode = MouseProtocolMode::AnyMotion,
                ([1003], false) => screen.clear_mouse_mode(MouseProtocolMode::AnyMotion),
                ([1005], true) => screen.mouse_protocol_encoding = MouseProtocolEncoding::Utf8,
                ([1005], false) => screen.clear_mouse_encoding(MouseProtocolEncoding::Utf8),
                ([1006], true) => screen.mouse_protocol_encoding = MouseProtocolEncoding::Sgr,
                ([1006], false) => screen.clear_mouse_encoding(MouseProtocolEncoding::Sgr),
                ([1049], true) => {
                    screen.save_cursor();
                    screen.alternate_grid.clear();
                    screen.enter_alternate_grid();
                }
                ([1049], false) => {
                    screen.exit_alternate_grid();
                    screen.restore_cursor();
                }
                ([2004], true) => screen.set_mode(MODE_BRACKETED_PASTE),
                ([2004], false) => screen.clear_mode(MODE_BRACKETED_PASTE),
                _ => unhandled(self),
            }
        }
    }

    /// `CSI … m`, as vt100 reads it: a parameter not understood is told
    /// to the callbacks (with the whole sequence) and the rest still apply;
    /// a colour too large for a byte, or one's parameters cut short, ends
    /// the sequence there.
    fn sgr(&mut self, params: &vte::Params, unhandled: &mut impl FnMut(&mut Self)) {
        if params.is_empty() {
            self.screen.attrs = Attrs::new();
            return;
        }
        let byte = |n: u16| u8::try_from(n).ok();
        let mut iter = params.iter();
        while let Some(param) = iter.next() {
            let attrs = &mut self.screen.attrs;
            match param {
                [0] => *attrs = Attrs::new(),
                [1] => attrs.set_bold(),
                [2] => attrs.set_dim(),
                [3] => attrs.set_italic(true),
                [4] => attrs.set_underline_style(UnderlineStyle::Single),
                // 4:0 stops the underline, 4:1 to 4:5 choose its style.
                [4, style] => {
                    if let Some(style) = UnderlineStyle::from_sgr(*style) {
                        attrs.set_underline_style(style);
                    }
                }
                [5 | 6] => attrs.set_blink(true),
                [7] => attrs.set_inverse(true),
                [8] => attrs.set_hidden(true),
                [9] => attrs.set_strikethrough(true),
                [21] => attrs.set_underline_style(UnderlineStyle::Double),
                [22] => attrs.set_normal_intensity(),
                [23] => attrs.set_italic(false),
                [24] => attrs.set_underline_style(UnderlineStyle::None),
                [25] => attrs.set_blink(false),
                [27] => attrs.set_inverse(false),
                [28] => attrs.set_hidden(false),
                [29] => attrs.set_strikethrough(false),
                [53] => attrs.set_overline(true),
                [55] => attrs.set_overline(false),
                [n @ 30..=37] => attrs.fg = Color::Idx((n - 30) as u8),
                [39] => attrs.fg = Color::Default,
                [n @ 40..=47] => attrs.bg = Color::Idx((n - 40) as u8),
                [49] => attrs.bg = Color::Default,
                [59] => attrs.ul = Color::Default,
                [n @ 90..=97] => attrs.fg = Color::Idx((n - 82) as u8),
                [n @ 100..=107] => attrs.bg = Color::Idx((n - 92) as u8),
                [which @ (38 | 48 | 58), rest @ ..] => {
                    let colour = match rest {
                        // The colon forms, with or without the colour space.
                        [2, r, g, b] | [2, _, r, g, b] => match (byte(*r), byte(*g), byte(*b)) {
                            (Some(r), Some(g), Some(b)) => Color::Rgb(r, g, b),
                            _ => return,
                        },
                        [5, i] => match byte(*i) {
                            Some(i) => Color::Idx(i),
                            None => return,
                        },
                        // The semicolon form: the rest in the parameters
                        // after it.
                        [] => {
                            let kind = iter.next();
                            let mut next = || match iter.next() {
                                Some(&[n]) => byte(n),
                                _ => None,
                            };
                            match kind {
                                Some(&[2]) => match (next(), next(), next()) {
                                    (Some(r), Some(g), Some(b)) => Color::Rgb(r, g, b),
                                    _ => return,
                                },
                                Some(&[5]) => match next() {
                                    Some(i) => Color::Idx(i),
                                    None => return,
                                },
                                None => return,
                                Some(_) => {
                                    unhandled(self);
                                    return;
                                }
                            }
                        }
                        _ => {
                            unhandled(self);
                            continue;
                        }
                    };
                    let attrs = &mut self.screen.attrs;
                    match which {
                        38 => attrs.fg = colour,
                        48 => attrs.bg = colour,
                        _ => attrs.ul = colour,
                    }
                }
                _ => unhandled(self),
            }
        }
    }
}

static BLANK: crate::Cell = crate::Cell::new();

impl<CB: Callbacks> vte::Perform for Terminal<CB> {
    fn print(&mut self, c: char) {
        if c == '\u{fffd}' || ('\u{80}'..'\u{a0}').contains(&c) {
            self.callbacks.unhandled_char(&mut self.screen, c);
        } else {
            self.text(c);
        }
    }

    fn execute(&mut self, b: u8) {
        match b {
            7 => self.callbacks.audible_bell(&mut self.screen),
            8 => self.screen.grid_mut().col_dec(1),
            9 => self.screen.tab(),
            10..=12 => self.lf(),
            13 => self.screen.grid_mut().col_set(0),
            // Shift in and out: character sets are not kept, but they are
            // not errors either.
            14 | 15 => {}
            _ => self.callbacks.unhandled_control(&mut self.screen, b),
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, b: u8) {
        if let Some(&i) = intermediates.first() {
            self.callbacks.unhandled_escape(
                &mut self.screen,
                Some(i),
                intermediates.get(1).copied(),
                b,
            );
            return;
        }
        match b {
            b'7' => self.screen.save_cursor(),
            b'8' => self.screen.restore_cursor(),
            b'=' => self.screen.set_mode(MODE_APPLICATION_KEYPAD),
            b'>' => self.screen.clear_mode(MODE_APPLICATION_KEYPAD),
            b'M' => self.screen.grid_mut().row_dec_scroll(1),
            b'c' => self.ris(),
            b'H' => self.screen.set_tab(true),
            b'g' => self.callbacks.visual_bell(&mut self.screen),
            _ => self
                .callbacks
                .unhandled_escape(&mut self.screen, None, None, b),
        }
    }

    fn csi_dispatch(&mut self, params: &vte::Params, intermediates: &[u8], _ignore: bool, c: char) {
        let all: Vec<&[u16]> = params.iter().collect();
        let i1 = intermediates.first().copied();
        let i2 = intermediates.get(1).copied();
        let mut unhandled = |t: &mut Self| {
            t.callbacks.unhandled_csi(&mut t.screen, i1, i2, &all, c);
        };
        let one = |default: u16| first(params, 0).filter(|&n| n != 0).unwrap_or(default);
        match (i1, c) {
            (None, '@') => self.screen.grid_mut().insert_cells(one(1)),
            (None, 'A') => self.screen.grid_mut().row_dec_clamp(one(1)),
            (None, 'B') => self.screen.grid_mut().row_inc_clamp(one(1)),
            (None, 'C') => self.screen.grid_mut().col_inc_clamp(one(1)),
            (None, 'D') => self.screen.grid_mut().col_dec(one(1)),
            (None, 'E') => {
                let grid = self.screen.grid_mut();
                grid.col_set(0);
                grid.row_inc_clamp(one(1));
            }
            (None, 'F') => {
                let grid = self.screen.grid_mut();
                grid.col_set(0);
                grid.row_dec_clamp(one(1));
            }
            (None, 'G' | '`') => self.screen.grid_mut().col_set(one(1) - 1),
            (None, 'Z') => self.screen.back_tab(one(1)),
            // TBC: tmux clears the stop at the cursor (0) or all (3).
            (None, 'g') => match first(params, 0).unwrap_or(0) {
                0 => self.screen.set_tab(false),
                3 => self.screen.clear_tabs(),
                _ => unhandled(self),
            },
            (None, 'H' | 'f') => {
                let row = first(params, 0).filter(|&n| n != 0).unwrap_or(1);
                let col = first(params, 1).filter(|&n| n != 0).unwrap_or(1);
                self.screen.grid_mut().set_pos(Pos {
                    row: row - 1,
                    col: col - 1,
                });
            }
            (None | Some(b'?'), 'J') => {
                if !self.ed(one(0)) {
                    unhandled(self);
                }
            }
            (None | Some(b'?'), 'K') => {
                if !self.el(one(0)) {
                    unhandled(self);
                }
            }
            (None, 'L') => self.screen.grid_mut().insert_lines(one(1)),
            (None, 'M') => self.screen.grid_mut().delete_lines(one(1)),
            (None, 'P') => self.screen.grid_mut().delete_cells(one(1)),
            (None, 'S') => {
                let on_row = Self::on_row(self.alternate(), &mut self.callbacks);
                self.screen.grid_mut().scroll_up(one(1), on_row);
            }
            (None, 'T') => self.screen.grid_mut().scroll_down(one(1)),
            (None, 'X') => {
                let attrs = self.screen.attrs;
                self.screen.grid_mut().erase_cells(one(1), attrs);
            }
            (None, 'b') => self.rep(one(1)),
            (None, 'd') => self.screen.grid_mut().row_set(one(1) - 1),
            (None, 'm') => self.sgr(params, &mut unhandled),
            (None, 'r') => {
                let rows = self.screen.grid().size().rows();
                let top = first(params, 0).filter(|&n| n != 0).unwrap_or(1);
                let bottom = first(params, 1).filter(|&n| n != 0).unwrap_or(rows);
                self.screen
                    .grid_mut()
                    .set_scroll_region(top - 1, bottom - 1);
            }
            (None, 't') if first(params, 0) == Some(8) => {
                let (rows, cols) = self.screen.size();
                let rows = first(params, 1).unwrap_or_else(|| rows.get());
                let cols = first(params, 2).unwrap_or_else(|| cols.get());
                self.callbacks.resize(&mut self.screen, (rows, cols));
            }
            // SM / RM: insert mode the only one kept; the callbacks are told
            // of each other parameter.
            (None, 'h' | 'l') => {
                for param in params {
                    if param == [4] {
                        if c == 'h' {
                            self.screen.set_mode(MODE_INSERT);
                        } else {
                            self.screen.clear_mode(MODE_INSERT);
                        }
                    } else {
                        unhandled(self);
                    }
                }
            }
            (Some(b'?'), 'h') => self.dec_mode(params, true, &mut unhandled),
            (Some(b'?'), 'l') => self.dec_mode(params, false, &mut unhandled),
            (Some(b' '), 'q') => match crate::screen::CursorStyle::from_decscusr(one(0)) {
                Some(style) => self.screen.cursor_style = style,
                None => unhandled(self),
            },
            // Any other `SP` final is told without its intermediates.
            (Some(b' '), _) => {
                self.callbacks
                    .unhandled_csi(&mut self.screen, None, None, &all, c);
            }
            (None, _) => {
                self.callbacks
                    .unhandled_csi(&mut self.screen, None, None, &all, c);
            }
            (Some(b'?'), _) => {
                self.callbacks
                    .unhandled_csi(&mut self.screen, Some(b'?'), i2, &all, c);
            }
            _ => unhandled(self),
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        match params {
            [b"0", s] => {
                self.callbacks.set_window_icon_name(&mut self.screen, s);
                self.callbacks.set_window_title(&mut self.screen, s);
            }
            [b"1", s] => self.callbacks.set_window_icon_name(&mut self.screen, s),
            [b"2", s] => self.callbacks.set_window_title(&mut self.screen, s),
            // The URI may hold `;`, which splits it into parameters here.
            [b"8", link, uri @ ..] if !uri.is_empty() => {
                let uri = uri.join(&b';');
                self.screen.set_hyperlink(link, &uri);
            }
            [b"52", ty, data] if ty.iter().all(|c| CLIPBOARD_SELECTOR.contains(c)) => {
                if *data == b"?" {
                    self.callbacks.paste_from_clipboard(&mut self.screen, ty);
                } else if data.iter().all(|c| BASE64.contains(c)) {
                    self.callbacks.copy_to_clipboard(&mut self.screen, ty, data);
                } else {
                    self.callbacks.unhandled_osc(&mut self.screen, params);
                }
            }
            _ => self.callbacks.unhandled_osc(&mut self.screen, params),
        }
    }
}

/// Parameter `i`'s first value, if given.
fn first(params: &vte::Params, i: usize) -> Option<u16> {
    params.iter().nth(i).and_then(|p| p.first().copied())
}
