//! Rewrapping a grid's rows to a new width, as tmux's `grid_reflow`.

use alloc::{borrow::Cow, collections::VecDeque, vec::Vec};

use crate::Cell;
use crate::packed::PackedRow;
use crate::row::Row;

/// What a reflow did to the lines.
///
/// For each line before it (the history's, oldest first, then the
/// screen's), the line its first cell is on after it, numbered the same
/// way; `None` for one that fell off the history.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reflow {
    /// Each line's new number.
    pub lines: Vec<Option<usize>>,
    /// How many lines fell off the top of the history.
    pub dropped: usize,
}

/// A line as written, before it was wrapped: its cells, and for each row
/// it was made of that row's number and its first cell's index in `cells`.
struct Logical {
    cells: Vec<Cell>,
    starts: Vec<(usize, usize)>,
}

impl super::Grid {
    /// Rewrap every row, of the history and the screen, to `cols` columns,
    /// as tmux's `grid_reflow`: the rows of each line as written (a row
    /// that wraps goes on into the next) are joined and split again at the
    /// new width, each row's used cells only. The screen is then the last
    /// rows, blank ones added below if there are too few, and the history
    /// the rest, up to its capacity. The cursor keeps its place in its line,
    /// or goes to the line's end if it was past it, as tmux's
    /// `grid_wrap_position` / `grid_unwrap_position`; on a row now in the
    /// history it goes to the top left, as tmux's `screen_resize_cursor`.
    pub fn reflow(&mut self, cols: u16) -> Reflow {
        let screen_rows = usize::from(self.size.rows());
        let packed: VecDeque<PackedRow> = self
            .history
            .as_mut()
            .map(|h| core::mem::take(&mut h.rows))
            .unwrap_or_default();
        let old_history = packed.len();
        let screen = core::mem::take(&mut self.rows);
        let old_lines = old_history + screen.len();
        let rows = packed
            .iter()
            .map(|p| Cow::Owned(p.unpack()))
            .chain(screen.iter().map(Cow::Borrowed));
        let cursor_line = old_history + usize::from(self.pos.row);
        let cursor_col = usize::from(self.pos.col);

        // The lines as written, and the cursor's place in its line: an
        // index into its cells, or `None` for past its end.
        let mut logical: Vec<Logical> = Vec::new();
        let mut cursor: (usize, Option<usize>) = (0, None);
        let mut joining = false;
        for (i, row) in rows.enumerate() {
            if !joining {
                logical.push(Logical {
                    cells: Vec::new(),
                    starts: Vec::new(),
                });
            }
            let n = logical.len() - 1;
            let line = &mut logical[n];
            let used = row.used();
            line.starts.push((i, line.cells.len()));
            if i == cursor_line {
                cursor = (
                    n,
                    (cursor_col < used).then(|| line.cells.len() + cursor_col),
                );
            }
            line.cells.extend_from_slice(&row.as_slice()[..used]);
            joining = row.wrapped();
        }
        drop(packed);
        drop(screen);

        // Each line split at the new width.
        let mut rows: Vec<Row> = Vec::new();
        let mut first_row: Vec<usize> = alloc::vec![0; old_lines];
        let mut new_cursor = (0, 0);
        for (n, line) in logical.iter().enumerate() {
            let mut placed = Placer::new(cols, rows.len());
            let mut starts = line.starts.iter().peekable();
            for (ci, cell) in line.cells.iter().enumerate() {
                let (row, col) = placed.put(cell);
                while let Some(&&(old_row, at)) = starts.peek() {
                    if at > ci {
                        break;
                    }
                    first_row[old_row] = row;
                    starts.next();
                }
                if cursor == (n, Some(ci)) {
                    new_cursor = (row, col);
                }
            }
            let (last_row, end) = placed.finish(&mut rows);
            // Rows that held nothing begin on the line's last row.
            for &(old_row, _) in starts {
                first_row[old_row] = last_row;
            }
            if cursor == (n, None) {
                new_cursor = (last_row, end);
            }
        }

        while rows.len() < screen_rows {
            rows.push(Row::new(cols));
        }
        let history = rows.len() - screen_rows;
        let screen = rows.split_off(history);
        let dropped = match &mut self.history {
            Some(h) => {
                let dropped = history.saturating_sub(h.capacity);
                h.unbounded_len = (h.unbounded_len + history).saturating_sub(old_history);
                h.offset = 0;
                h.view.clear();
                h.rows = rows.iter().skip(dropped).map(PackedRow::pack).collect();
                dropped
            }
            None => history,
        };
        self.rows = screen;

        let size = super::Size {
            rows: self.size.rows,
            cols: core::num::NonZeroU16::new(cols).unwrap_or(self.size.cols),
        };
        self.size = size;
        self.scroll_top = 0;
        self.scroll_bottom = size.rows() - 1;
        self.pos = new_cursor
            .0
            .checked_sub(history)
            .map_or_else(super::Pos::default, |row| super::Pos {
                row: u16::try_from(row).unwrap_or(0),
                col: u16::try_from(new_cursor.1).unwrap_or(0).min(cols - 1),
            });
        self.saved_pos.row = self.saved_pos.row.min(size.rows() - 1);
        self.saved_pos.col = self.saved_pos.col.min(cols - 1);

        Reflow {
            lines: first_row
                .into_iter()
                .map(|row| row.checked_sub(dropped))
                .collect(),
            dropped,
        }
    }
}

/// Cells laid out in rows `cols` wide, a wide character that would not fit
/// at a row's end going to the next (that cell left blank).
///
/// Where a row ends is counted as tmux's `grid_reflow` counts it: a wide
/// character takes two columns and the cell after it, its padding in tmux's
/// grid, one more, so a row with a wide character in it ends a column
/// sooner than it would need to.
struct Placer {
    cols: u16,
    /// The columns of this row taken, as tmux counts them.
    counted: usize,
    /// The first of these rows' numbers among all the rows.
    base: usize,
    done: Vec<Row>,
    cells: Vec<Cell>,
}

impl Placer {
    const fn new(cols: u16, base: usize) -> Self {
        Self {
            cols,
            counted: 0,
            base,
            done: Vec::new(),
            cells: Vec::new(),
        }
    }

    fn break_row(&mut self) {
        self.counted = 0;
        let mut cells = core::mem::take(&mut self.cells);
        cells.resize(usize::from(self.cols), Cell::new());
        self.done.push(Row::from_cells(cells, true));
    }

    /// Place `cell`; where it went (its row among all rows, its column).
    fn put(&mut self, cell: &Cell) -> (usize, usize) {
        let cols = usize::from(self.cols);
        // A wide character with the cell after it, or one cell.
        let need = match (cell.is_wide(), cell.is_wide_continuation()) {
            (true, _) => 3,
            (_, true) => 0,
            _ => 1,
        };
        if need > 0 && !self.cells.is_empty() && self.counted + need > cols {
            self.break_row();
        }
        self.counted += need;
        let col = self.cells.len();
        self.cells.push(cell.clone());
        (self.base + self.done.len(), col)
    }

    /// The rows into `rows`: the last row's number and where its text ends.
    fn finish(mut self, rows: &mut Vec<Row>) -> (usize, usize) {
        let end = self.cells.len();
        let mut cells = core::mem::take(&mut self.cells);
        cells.truncate(usize::from(self.cols));
        cells.resize(usize::from(self.cols), Cell::new());
        self.done.push(Row::from_cells(cells, false));
        let last = self.base + self.done.len() - 1;
        rows.append(&mut self.done);
        (last, end)
    }
}
