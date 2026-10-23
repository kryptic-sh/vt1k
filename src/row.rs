//! A row of the screen.

use alloc::{string::String, vec::Vec};

use crate::Cell;
use crate::attrs::Attrs;

/// A row of cells, and whether its text goes on into the next row (a
/// soft wrap).
#[derive(Clone, Debug, Default)]
pub struct Row {
    cells: Vec<Cell>,
    wrapped: bool,
}

impl Row {
    pub fn new(cols: u16) -> Self {
        Self {
            cells: alloc::vec![Cell::new(); usize::from(cols)],
            wrapped: false,
        }
    }

    /// A row holding `cells`, which goes on into the next if `wrapped`.
    pub const fn from_cells(cells: Vec<Cell>, wrapped: bool) -> Self {
        Self { cells, wrapped }
    }

    pub fn into_cells(self) -> Vec<Cell> {
        self.cells
    }

    /// How many of its cells were written: up to the last that is not
    /// blank, as tmux counts a line's used cells.
    pub fn used(&self) -> usize {
        self.cells
            .iter()
            .rposition(|c| !c.is_default())
            .map_or(0, |i| i + 1)
    }

    fn cols(&self) -> u16 {
        // A row is never wider than a `u16` of columns.
        u16::try_from(self.cells.len()).unwrap_or(u16::MAX)
    }

    pub fn as_slice(&self) -> &[Cell] {
        &self.cells
    }

    pub fn get(&self, col: u16) -> Option<&Cell> {
        self.cells.get(usize::from(col))
    }

    pub fn get_mut(&mut self, col: u16) -> Option<&mut Cell> {
        self.cells.get_mut(usize::from(col))
    }

    pub fn clear(&mut self, attrs: Attrs) {
        for cell in &mut self.cells {
            cell.clear(attrs);
        }
        self.wrapped = false;
    }

    pub fn reset(&mut self) {
        self.cells.fill(Cell::new());
        self.wrapped = false;
    }

    pub const fn wrap(&mut self, wrapped: bool) {
        self.wrapped = wrapped;
    }

    pub const fn wrapped(&self) -> bool {
        self.wrapped
    }

    /// Insert `count` blank cells at `i`, those pushed past the end lost.
    pub fn insert(&mut self, i: u16, count: u16) {
        let count = usize::from(count).min(self.cells.len().saturating_sub(usize::from(i)));
        if count == 0 {
            self.wrapped = false;
            return;
        }
        let continuation = self.get_mut(i).is_some_and(|cell| {
            let was = cell.is_wide_continuation();
            cell.set_wide_continuation(false);
            was
        });
        let tail = &mut self.cells[usize::from(i)..];
        tail.rotate_right(count);
        tail[..count].fill(Cell::new());
        if continuation && let Some(cell) = self.get_mut(i) {
            cell.set_wide_continuation(true);
        }
        self.set_truncated();
    }

    /// Delete the cells of `range`, the rest moving left and blanks coming
    /// in at the end.
    pub fn remove(&mut self, range: core::ops::Range<u16>) {
        self.wrapped = false;
        if range.start >= range.end {
            return;
        }
        self.clear_wide(range.start);
        let last = range.end - 1;
        if last != range.start {
            self.clear_wide(last);
        }
        let count = usize::from(range.end - range.start);
        let tail = &mut self.cells[usize::from(range.start)..];
        tail[..count].fill(Cell::new());
        tail.rotate_left(count);
    }

    /// Erase cell `i` (both halves of a wide character), drawn with `attrs`.
    pub fn erase(&mut self, i: u16, attrs: Attrs) {
        let wide = self.cells[usize::from(i)].is_wide();
        self.clear_wide(i);
        self.cells[usize::from(i)].clear(attrs);
        // A wide character's first half ends the row a column early.
        if i == self.cols().saturating_sub(if wide { 2 } else { 1 }) {
            self.wrapped = false;
        }
    }

    pub fn truncate(&mut self, len: u16) {
        self.cells.truncate(usize::from(len));
        self.set_truncated();
    }

    fn set_truncated(&mut self) {
        self.wrapped = false;
        if let Some(last) = self.cells.last_mut()
            && last.is_wide()
        {
            let attrs = *last.attrs();
            last.clear(attrs);
        }
    }

    /// Widen to `len` cells with copies of `cell`, if narrower.
    pub fn grow(&mut self, len: u16, cell: Cell) {
        if self.cells.len() < usize::from(len) {
            self.cells.resize(usize::from(len), cell);
            self.wrapped = false;
        }
    }

    pub fn resize(&mut self, len: u16, cell: Cell) {
        match usize::from(len).cmp(&self.cells.len()) {
            core::cmp::Ordering::Equal => {}
            core::cmp::Ordering::Less => self.truncate(len),
            core::cmp::Ordering::Greater => self.grow(len, cell),
        }
    }

    /// Clear the other half of a wide character at `col`.
    pub fn clear_wide(&mut self, col: u16) {
        let at = usize::from(col);
        let other = if self.cells[at].is_wide() {
            let Some(next) = self.cells.get_mut(at + 1) else {
                // A wide character split off its second half.
                let attrs = *self.cells[at].attrs();
                self.cells[at].clear(attrs);
                return;
            };
            next
        } else if self.cells[at].is_wide_continuation() {
            &mut self.cells[at - 1]
        } else {
            return;
        };
        let attrs = *other.attrs();
        other.clear(attrs);
    }

    /// The text of `width` columns from `start`: blanks between text kept,
    /// trailing ones not; a newline for a row that holds nothing there but
    /// goes on from one `wrapping` into it.
    pub fn write_contents(&self, out: &mut String, start: u16, width: u16, wrapping: bool) {
        let mut prev_was_wide = false;
        let mut prev_col = start;
        for (col, cell) in self
            .cells
            .iter()
            .enumerate()
            .skip(usize::from(start))
            .take(usize::from(width))
        {
            if prev_was_wide {
                prev_was_wide = false;
                continue;
            }
            prev_was_wide = cell.is_wide();
            let col = u16::try_from(col).unwrap_or(u16::MAX);
            if cell.has_contents() {
                for _ in prev_col..col {
                    out.push(' ');
                }
                prev_col = col;
                out.push_str(cell.contents());
                prev_col += if cell.is_wide() { 2 } else { 1 };
            }
        }
        if prev_col == start && wrapping {
            out.push('\n');
        }
    }
}
