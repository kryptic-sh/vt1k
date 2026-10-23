//! Rows handed to [`Callbacks::on_scroll`](crate::Callbacks::on_scroll).

use alloc::borrow::Cow;

use crate::Cell;
use crate::packed::PackedRow;
use crate::row::Row;

/// A row the screen is losing: one scrolled off a screen that keeps no
/// history, or off the top of a full history. Reading its cells costs an
/// unpacking for a row of history, so a callback that does not need them
/// pays nothing.
pub struct RowContents<'a>(Source<'a>);

enum Source<'a> {
    Row(&'a Row),
    Packed(&'a PackedRow),
}

impl<'a> RowContents<'a> {
    pub(crate) const fn row(row: &'a Row) -> Self {
        Self(Source::Row(row))
    }

    pub(crate) const fn packed(row: &'a PackedRow) -> Self {
        Self(Source::Packed(row))
    }

    /// The row's cells; a row of history holds only those up to its last
    /// written one.
    #[must_use]
    pub fn cells(&self) -> Cow<'a, [Cell]> {
        match self.0 {
            Source::Row(row) => Cow::Borrowed(row.as_slice()),
            Source::Packed(row) => Cow::Owned(row.unpack().into_cells()),
        }
    }

    /// The row's text goes on into the next row.
    #[must_use]
    pub fn wrapped(&self) -> bool {
        match self.0 {
            Source::Row(row) => row.wrapped(),
            Source::Packed(row) => row.wrapped(),
        }
    }
}
