//! Rows of history kept small. A row of the screen is a `Vec` of full
//! cells; once it scrolls into the history it is packed into one byte
//! buffer, and unpacked again only where a whole row is wanted.
//!
//! The buffer holds, in order: a flags byte (bit 0: the row wraps), the
//! number of cells and of attribute runs (`u16` each, little-endian), each
//! cell's length byte (its content's byte count and wide flags, as a
//! [`Cell`] keeps it), the runs (each its first cell, link and attributes,
//! [`RUN_BYTES`] long), then every cell's text one after another.

use alloc::{boxed::Box, vec::Vec};

use crate::attrs::Attrs;
use crate::row::Row;
use crate::{Cell, Color};

/// A run's first cell (2 bytes), link (2) and attributes: three colours of
/// [`COLOUR_BYTES`] and the mode (2, little-endian).
const RUN_BYTES: usize = 2 + 2 + 3 * COLOUR_BYTES + 2;
const COLOUR_BYTES: usize = 4;
const HEADER_BYTES: usize = 1 + 2 + 2;
const WRAPPED: u8 = 0b1;

/// One row of history, packed.
#[derive(Clone, Debug)]
pub struct PackedRow(Box<[u8]>);

impl PackedRow {
    /// `row`'s cells up to the last that is not blank, packed, and the row
    /// left blank: only those cells need blanking, the rest already are.
    pub fn take(row: &mut Row) -> Self {
        let used = row.used();
        let packed = Self::pack_cells(&row.as_slice()[..used], row.wrapped());
        row.clear_first(used);
        packed
    }

    /// `row`'s cells up to the last that is not blank, packed.
    pub fn pack(row: &Row) -> Self {
        Self::pack_cells(&row.as_slice()[..row.used()], row.wrapped())
    }

    fn pack_cells(cells: &[Cell], wrapped: bool) -> Self {
        let mut runs: Vec<(usize, Attrs, u16)> = Vec::new();
        for (i, cell) in cells.iter().enumerate() {
            let key = (*cell.attrs(), cell.link());
            if runs.last().is_none_or(|&(_, a, l)| (a, l) != key) {
                runs.push((i, key.0, key.1));
            }
        }
        let text: usize = cells.iter().map(|c| c.contents_bytes().len()).sum();
        let mut out =
            Vec::with_capacity(HEADER_BYTES + cells.len() + runs.len() * RUN_BYTES + text);
        out.push(if wrapped { WRAPPED } else { 0 });
        out.extend_from_slice(&count(cells.len()).to_le_bytes());
        out.extend_from_slice(&count(runs.len()).to_le_bytes());
        out.extend(cells.iter().map(Cell::raw_len));
        for (start, attrs, link) in runs {
            out.extend_from_slice(&count(start).to_le_bytes());
            out.extend_from_slice(&link.to_le_bytes());
            for colour in [attrs.fg, attrs.bg, attrs.ul] {
                out.extend_from_slice(&colour_bytes(colour));
            }
            out.extend_from_slice(&attrs.mode.to_le_bytes());
        }
        for cell in cells {
            out.extend_from_slice(cell.contents_bytes());
        }
        Self(out.into_boxed_slice())
    }

    /// The row again, its cells as they were packed.
    pub fn unpack(&self) -> Row {
        let data = &self.0;
        let cells = self.cells();
        let runs = self.runs();
        let lens = &data[HEADER_BYTES..HEADER_BYTES + cells];
        let runs_at = HEADER_BYTES + cells;
        let run = |i: usize| -> (usize, Attrs, u16) {
            let r = &data[runs_at + i * RUN_BYTES..runs_at + (i + 1) * RUN_BYTES];
            let colour = |at: usize| colour_of(&r[at..at + COLOUR_BYTES]);
            let attrs = Attrs {
                fg: colour(4),
                bg: colour(4 + COLOUR_BYTES),
                ul: colour(4 + 2 * COLOUR_BYTES),
                mode: u16::from_le_bytes([r[4 + 3 * COLOUR_BYTES], r[5 + 3 * COLOUR_BYTES]]),
            };
            (
                usize::from(u16::from_le_bytes([r[0], r[1]])),
                attrs,
                u16::from_le_bytes([r[2], r[3]]),
            )
        };
        let mut text = runs_at + runs * RUN_BYTES;
        let mut out = Vec::with_capacity(cells);
        let mut next = 0;
        let mut current = (Attrs::new(), 0);
        for (i, &len) in lens.iter().enumerate() {
            if next < runs && run(next).0 == i {
                let (_, attrs, link) = run(next);
                current = (attrs, link);
                next += 1;
            }
            let bytes = Cell::content_len(len);
            out.push(Cell::from_parts(
                &data[text..text + bytes],
                len,
                current.0,
                current.1,
            ));
            text += bytes;
        }
        Row::from_cells(out, self.wrapped())
    }

    /// The row's text goes on into the next row.
    pub fn wrapped(&self) -> bool {
        self.0[0] & WRAPPED != 0
    }

    fn cells(&self) -> usize {
        usize::from(u16::from_le_bytes([self.0[1], self.0[2]]))
    }

    fn runs(&self) -> usize {
        usize::from(u16::from_le_bytes([self.0[3], self.0[4]]))
    }
}

/// A row's count of cells or runs, which a row's width (a `u16`) bounds.
fn count(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

const fn colour_bytes(c: Color) -> [u8; COLOUR_BYTES] {
    match c {
        Color::Default => [0, 0, 0, 0],
        Color::Idx(i) => [1, i, 0, 0],
        Color::Rgb(r, g, b) => [2, r, g, b],
    }
}

const fn colour_of(b: &[u8]) -> Color {
    match b[0] {
        1 => Color::Idx(b[1]),
        2 => Color::Rgb(b[1], b[2], b[3]),
        _ => Color::Default,
    }
}
