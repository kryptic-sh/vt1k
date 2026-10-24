//! A grid of rows: the screen's, and for the normal screen the history
//! above it.

mod reflow;

use alloc::{borrow::Cow, collections::VecDeque, string::String, vec::Vec};

pub use reflow::Reflow;

use crate::Cell;
use crate::attrs::Attrs;
use crate::capture::RowContents;
use crate::packed::PackedRow;
use crate::row::Row;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Size {
    pub rows: core::num::NonZeroU16,
    pub cols: core::num::NonZeroU16,
}

impl Size {
    pub const fn rows(self) -> u16 {
        self.rows.get()
    }

    pub const fn cols(self) -> u16 {
        self.cols.get()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Pos {
    pub row: u16,
    pub col: u16,
}

/// The rows that scrolled off the top, oldest first, packed.
#[derive(Clone, Debug)]
struct History {
    rows: VecDeque<PackedRow>,
    capacity: usize,
    /// How far the view is scrolled back into it.
    offset: usize,
    /// How many rows it would hold without a capacity, for taking rows
    /// back as the screen grows taller.
    unbounded_len: usize,
    /// The rows a scrolled-back view shows, unpacked and as wide as the
    /// screen; empty while the view is at the bottom.
    view: Vec<Row>,
}

#[derive(Clone, Debug)]
pub struct Grid {
    size: Size,
    pos: Pos,
    saved_pos: Pos,
    rows: Vec<Row>,
    scroll_top: u16,
    scroll_bottom: u16,
    origin_mode: bool,
    saved_origin_mode: bool,
    history: Option<History>,
}

impl Grid {
    /// A grid of `size`; with `history`, keeping up to that many rows that
    /// scroll off its top.
    pub fn new(size: Size, history: Option<usize>) -> Self {
        Self {
            size,
            pos: Pos::default(),
            saved_pos: Pos::default(),
            rows: Vec::new(),
            scroll_top: 0,
            scroll_bottom: size.rows() - 1,
            origin_mode: false,
            saved_origin_mode: false,
            history: history.map(|capacity| History {
                rows: VecDeque::new(),
                capacity,
                offset: 0,
                unbounded_len: 0,
                view: Vec::new(),
            }),
        }
    }

    /// Give the grid its rows, which an alternate screen gets only once it
    /// is first shown.
    pub fn allocate_rows(&mut self) {
        if self.rows.is_empty() {
            let cols = self.size.cols();
            self.rows
                .resize_with(usize::from(self.size.rows()), || Row::new(cols));
        }
    }

    pub fn clear(&mut self) {
        self.pos = Pos::default();
        self.saved_pos = Pos::default();
        for row in &mut self.rows {
            row.reset();
        }
        self.scroll_top = 0;
        self.scroll_bottom = self.size.rows() - 1;
        self.origin_mode = false;
        self.saved_origin_mode = false;
    }

    pub const fn size(&self) -> Size {
        self.size
    }

    /// Resize, keeping the cursor's place from the bottom: fewer rows take
    /// the blank ones below the cursor first, then push rows off the top
    /// (into the history, `on_row` told of each the history loses); more
    /// rows take rows back from the history first.
    pub fn set_size(&mut self, size: Size, on_row: impl FnMut(RowContents<'_>)) {
        if size == self.size {
            return;
        }
        self.scroll_top = 0;
        self.scroll_bottom = self.size.rows() - 1;
        if let Some(history) = &mut self.history {
            history.offset = 0;
            history.view.clear();
        }
        let new_row = || Row::new(size.cols());
        let diff = self.size.rows().abs_diff(size.rows());
        // An alternate screen not yet shown has no rows.
        let is_empty = self.rows.is_empty();
        let pos_diff = if is_empty {
            0
        } else if size.rows() < self.size.rows() {
            let excess_bottom = self.size.rows() - self.pos.row - 1;
            let pushed = diff.saturating_sub(excess_bottom);
            self.scroll_up(pushed, on_row);
            self.rows.truncate(usize::from(size.rows()));
            pushed
        } else if let Some(history) = &mut self.history {
            let pulled = u16::try_from(history.unbounded_len)
                .unwrap_or(u16::MAX)
                .min(diff);
            let from_history = history.rows.len().min(usize::from(pulled));
            let blank = usize::from(pulled) - from_history;
            let at = history.rows.len() - from_history;
            let taken: Vec<Row> = history.rows.drain(at..).map(|r| r.unpack()).collect();
            self.rows.splice(
                0..0,
                core::iter::repeat_with(new_row).take(blank).chain(taken),
            );
            self.rows.resize_with(usize::from(size.rows()), new_row);
            history.unbounded_len -= usize::from(pulled);
            pulled
        } else {
            self.rows.resize_with(usize::from(size.rows()), new_row);
            0
        };

        if let Some(row) = self.rows.last_mut() {
            row.wrap(false);
        }
        for row in &mut self.rows {
            row.resize(size.cols(), Cell::new());
        }

        if size.rows() < self.size.rows() {
            self.pos.row = self.pos.row.saturating_sub(pos_diff);
            self.saved_pos.row = self
                .saved_pos
                .row
                .saturating_sub(pos_diff)
                .min(size.rows() - 1);
        } else {
            self.pos.row += pos_diff;
            self.saved_pos.row += pos_diff;
        }
        self.pos.col = self.pos.col.min(size.cols() - 1);
        self.saved_pos.col = self.saved_pos.col.min(size.cols() - 1);
        self.scroll_bottom = size.rows() - 1;
        self.size = size;
    }

    pub const fn pos(&self) -> Pos {
        self.pos
    }

    pub fn set_pos(&mut self, mut pos: Pos) {
        if self.origin_mode {
            pos.row = pos.row.saturating_add(self.scroll_top);
        }
        self.pos = pos;
        self.row_clamp_top(self.origin_mode);
        self.row_clamp_bottom(self.origin_mode);
        self.col_clamp();
    }

    pub const fn save_cursor(&mut self) {
        self.saved_pos = self.pos;
        self.saved_origin_mode = self.origin_mode;
    }

    pub const fn restore_cursor(&mut self) {
        self.pos = self.saved_pos;
        self.origin_mode = self.saved_origin_mode;
    }

    /// The rows the view shows: those of the history it is scrolled back
    /// over, then the screen's.
    pub fn visible_rows(&self) -> impl Iterator<Item = &Row> {
        self.history
            .iter()
            .flat_map(|h| h.view.iter())
            .chain(&self.rows)
            .take(self.rows.len())
    }

    pub fn visible_row(&self, row: u16) -> Option<&Row> {
        let row = usize::from(row);
        if row >= self.rows.len() {
            return None;
        }
        let shown = self.history.as_ref().map_or(0, |h| h.view.len());
        match &self.history {
            Some(h) if row < shown => h.view.get(row),
            _ => self.rows.get(row - shown),
        }
    }

    pub fn drawing_row(&self, row: u16) -> Option<&Row> {
        self.rows.get(usize::from(row))
    }

    pub fn drawing_row_mut(&mut self, row: u16) -> Option<&mut Row> {
        self.rows.get_mut(usize::from(row))
    }

    fn current_row_mut(&mut self) -> &mut Row {
        let row = usize::from(self.pos.row);
        // The cursor's row is always one of the grid's.
        &mut self.rows[row]
    }

    pub fn visible_cell(&self, pos: Pos) -> Option<&Cell> {
        self.visible_row(pos.row).and_then(|r| r.get(pos.col))
    }

    pub fn drawing_cell(&self, pos: Pos) -> Option<&Cell> {
        self.drawing_row(pos.row).and_then(|r| r.get(pos.col))
    }

    pub fn drawing_cell_mut(&mut self, pos: Pos) -> Option<&mut Cell> {
        self.drawing_row_mut(pos.row)
            .and_then(|r| r.get_mut(pos.col))
    }

    /// Line `index` of the history and the screen taken together, the
    /// oldest history row first, wherever the view is scrolled to; a row of
    /// history holds only its used cells.
    pub fn line(&self, index: usize) -> Option<Cow<'_, Row>> {
        let history = self.history_len();
        match &self.history {
            Some(h) if index < history => h.rows.get(index).map(|r| Cow::Owned(r.unpack())),
            _ => self.rows.get(index - history).map(Cow::Borrowed),
        }
    }

    pub fn line_wrapped(&self, index: usize) -> bool {
        let history = self.history_len();
        match &self.history {
            Some(h) if index < history => h.rows.get(index).is_some_and(PackedRow::wrapped),
            _ => self.rows.get(index - history).is_some_and(Row::wrapped),
        }
    }

    pub fn history_capacity(&self) -> usize {
        self.history.as_ref().map_or(0, |h| h.capacity)
    }

    pub fn history_len(&self) -> usize {
        self.history.as_ref().map_or(0, |h| h.rows.len())
    }

    pub fn history_offset(&self) -> usize {
        self.history.as_ref().map_or(0, |h| h.offset)
    }

    /// Scroll the view back `rows` rows into the history (0 for the bottom).
    pub fn set_history_offset(&mut self, rows: usize) {
        let size = self.size;
        if let Some(history) = &mut self.history {
            history.offset = rows.min(history.rows.len());
            Self::refresh_view(history, size);
        }
    }

    /// Unpack the rows the view shows, as wide as the screen.
    fn refresh_view(history: &mut History, size: Size) {
        history.view.clear();
        let first = history.rows.len() - history.offset;
        history.view.extend(
            history
                .rows
                .iter()
                .skip(first)
                .take(usize::from(size.rows()))
                .map(|packed| {
                    let mut row = packed.unpack();
                    row.grow(size.cols(), Cell::new());
                    row
                }),
        );
    }

    pub fn write_contents(&self, out: &mut String) {
        let mut wrapping = false;
        for row in self.visible_rows() {
            row.write_contents(out, 0, self.size.cols(), wrapping);
            if !row.wrapped() {
                out.push('\n');
            }
            wrapping = row.wrapped();
        }
        while out.ends_with('\n') {
            out.pop();
        }
    }

    pub fn erase_all(&mut self, attrs: Attrs) {
        for row in &mut self.rows {
            row.clear(attrs);
        }
    }

    pub fn erase_all_forward(&mut self, attrs: Attrs) {
        let below = usize::from(self.pos.row) + 1;
        for row in self.rows.iter_mut().skip(below) {
            row.clear(attrs);
        }
        self.erase_row_forward(attrs);
    }

    pub fn erase_all_backward(&mut self, attrs: Attrs) {
        let above = usize::from(self.pos.row);
        for row in self.rows.iter_mut().take(above) {
            row.clear(attrs);
        }
        self.erase_row_backward(attrs);
    }

    /// Drop the whole history, `on_row` told of each row.
    pub fn erase_all_history(&mut self, mut on_row: impl FnMut(RowContents<'_>)) {
        if let Some(history) = &mut self.history {
            for row in history.rows.drain(..) {
                on_row(RowContents::packed(&row));
            }
            history.unbounded_len = 0;
            history.offset = 0;
            history.view.clear();
        }
    }

    pub fn erase_row(&mut self, attrs: Attrs) {
        self.current_row_mut().clear(attrs);
    }

    pub fn erase_row_forward(&mut self, attrs: Attrs) {
        let (cols, col) = (self.size.cols(), self.pos.col);
        let row = self.current_row_mut();
        for c in col..cols {
            row.erase(c, attrs);
        }
    }

    pub fn erase_row_backward(&mut self, attrs: Attrs) {
        let (cols, col) = (self.size.cols(), self.pos.col);
        let row = self.current_row_mut();
        for c in 0..=col.min(cols - 1) {
            row.erase(c, attrs);
        }
    }

    pub fn insert_cells(&mut self, count: u16) {
        let col = self.pos.col;
        self.current_row_mut().insert(col, count);
    }

    pub fn delete_cells(&mut self, count: u16) {
        let (cols, col) = (self.size.cols(), self.pos.col);
        self.current_row_mut()
            .remove(col..col.saturating_add(count).min(cols));
    }

    pub fn erase_cells(&mut self, count: u16, attrs: Attrs) {
        let (cols, col) = (self.size.cols(), self.pos.col);
        let row = self.current_row_mut();
        for c in col..col.saturating_add(count).min(cols) {
            row.erase(c, attrs);
        }
    }

    pub fn insert_lines(&mut self, count: u16) {
        if count == 0 || !(self.scroll_top..=self.scroll_bottom).contains(&self.pos.row) {
            return;
        }
        let between = &mut self.rows[usize::from(self.pos.row)..=usize::from(self.scroll_bottom)];
        let n = usize::from(count).min(between.len());
        between.rotate_right(n);
        for row in &mut between[..n] {
            row.reset();
        }
        self.rows[usize::from(self.scroll_bottom)].wrap(false);
    }

    pub fn delete_lines(&mut self, count: u16) {
        if !(self.scroll_top..=self.scroll_bottom).contains(&self.pos.row) {
            return;
        }
        let between = &mut self.rows[usize::from(self.pos.row)..=usize::from(self.scroll_bottom)];
        let n = usize::from(count).min(between.len());
        for row in &mut between[..n] {
            row.reset();
        }
        between.rotate_left(n);
    }

    /// Scroll the scroll region up `count` rows. From the top of the grid,
    /// the rows go into the history (`on_row` told of each the history
    /// loses past its capacity, or of each row when it keeps none).
    pub fn scroll_up(&mut self, count: u16, mut on_row: impl FnMut(RowContents<'_>)) {
        // Told of rows lost even without a history, from the grid's top.
        let from_top = self.scroll_top == 0;
        let size = self.size;
        let between =
            &mut self.rows[usize::from(self.scroll_top)..=usize::from(self.scroll_bottom)];
        let count = usize::from(count).min(between.len());
        let removed = count;

        if let Some(history) = self.history.as_mut().filter(|_| from_top) {
            history.unbounded_len = history.unbounded_len.saturating_add(removed);
            let pushed = count.min(history.capacity);
            let overflow = (history.rows.len() + pushed).saturating_sub(history.capacity);
            for row in history.rows.drain(..overflow) {
                on_row(RowContents::packed(&row));
            }
            let reset = removed - pushed;
            for row in &mut between[..reset] {
                on_row(RowContents::row(row));
                row.reset();
            }
            for row in &mut between[reset..removed] {
                history.rows.push_back(PackedRow::take(row));
            }
            if history.offset > 0 {
                history.offset = history.offset.saturating_add(count).min(history.rows.len());
                Self::refresh_view(history, size);
            }
        } else {
            for row in &mut between[..removed] {
                if from_top {
                    on_row(RowContents::row(row));
                }
                row.reset();
            }
        }
        between.rotate_left(removed);
    }

    pub fn scroll_down(&mut self, count: u16) {
        if count == 0 {
            return;
        }
        let between =
            &mut self.rows[usize::from(self.scroll_top)..=usize::from(self.scroll_bottom)];
        let n = usize::from(count).min(between.len());
        between.rotate_right(n);
        for row in &mut between[..n] {
            row.reset();
        }
        self.rows[usize::from(self.scroll_bottom)].wrap(false);
    }

    pub fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        let bottom = bottom.min(self.size.rows() - 1);
        if top < bottom {
            self.scroll_top = top;
            self.scroll_bottom = bottom;
        } else {
            self.scroll_top = 0;
            self.scroll_bottom = self.size.rows() - 1;
        }
        self.pos = Pos {
            row: self.scroll_top,
            col: 0,
        };
    }

    const fn in_scroll_region(&self) -> bool {
        self.pos.row >= self.scroll_top && self.pos.row <= self.scroll_bottom
    }

    pub fn set_origin_mode(&mut self, on: bool) {
        self.origin_mode = on;
        self.set_pos(Pos::default());
    }

    pub fn row_inc_clamp(&mut self, count: u16) {
        let in_region = self.in_scroll_region();
        self.pos.row = self.pos.row.saturating_add(count);
        self.row_clamp_bottom(in_region);
    }

    /// Down `count` rows, scrolling the region past its bottom; how many
    /// rows it scrolled.
    pub fn row_inc_scroll(&mut self, count: u16, on_row: impl FnMut(RowContents<'_>)) -> u16 {
        let in_region = self.in_scroll_region();
        self.pos.row = self.pos.row.saturating_add(count);
        let lines = self.row_clamp_bottom(in_region);
        if in_region {
            self.scroll_up(lines, on_row);
            lines
        } else {
            0
        }
    }

    pub fn row_dec_clamp(&mut self, count: u16) {
        let in_region = self.in_scroll_region();
        self.pos.row = self.pos.row.saturating_sub(count);
        self.row_clamp_top(in_region);
    }

    pub fn row_dec_scroll(&mut self, count: u16) {
        let in_region = self.in_scroll_region();
        let extra = count.saturating_sub(self.pos.row);
        self.pos.row = self.pos.row.saturating_sub(count);
        let lines = self.row_clamp_top(in_region);
        self.scroll_down(lines + extra);
    }

    pub fn row_set(&mut self, row: u16) {
        self.pos.row = row;
        self.row_clamp();
    }

    pub const fn col_inc(&mut self, count: u16) {
        self.pos.col = self.pos.col.saturating_add(count);
    }

    pub fn col_inc_clamp(&mut self, count: u16) {
        self.pos.col = self.pos.col.saturating_add(count);
        self.col_clamp();
    }

    pub const fn col_dec(&mut self, count: u16) {
        self.pos.col = self.pos.col.saturating_sub(count);
    }

    pub fn col_set(&mut self, col: u16) {
        self.pos.col = col;
        self.col_clamp();
    }

    /// Before writing a character `width` wide: past the row's end, on to
    /// the next row's start (scrolling if need be), the row left marked as
    /// wrapping into it when `wrap` says.
    pub fn col_wrap(&mut self, width: u16, wrap: bool, on_row: impl FnMut(RowContents<'_>)) {
        if self.pos.col <= self.size.cols() - width {
            return;
        }
        let mut prev = self.pos;
        self.pos.col = 0;
        let scrolled = self.row_inc_scroll(1, on_row);
        if prev.row == 0 && scrolled > 0 {
            // A grid one row high: it scrolls, but there is no row above
            // to wrap from.
            return;
        }
        prev.row -= scrolled;
        let now = self.pos;
        if let Some(row) = self.drawing_row_mut(prev.row) {
            row.wrap(wrap && prev.row + 1 == now.row);
        }
    }

    const fn row_clamp_top(&mut self, to_region: bool) -> u16 {
        if to_region && self.pos.row < self.scroll_top {
            let rows = self.scroll_top - self.pos.row;
            self.pos.row = self.scroll_top;
            rows
        } else {
            0
        }
    }

    const fn row_clamp_bottom(&mut self, to_region: bool) -> u16 {
        let bottom = if to_region {
            self.scroll_bottom
        } else {
            self.size.rows() - 1
        };
        if self.pos.row > bottom {
            let rows = self.pos.row - bottom;
            self.pos.row = bottom;
            rows
        } else {
            0
        }
    }

    fn row_clamp(&mut self) {
        self.pos.row = self.pos.row.min(self.size.rows() - 1);
    }

    fn col_clamp(&mut self) {
        self.pos.col = self.pos.col.min(self.size.cols() - 1);
    }
}
