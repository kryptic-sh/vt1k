//! A terminal emulator's screen: a program's output in, cells, history,
//! cursor and modes out.
//!
//! [`Parser`] reads escape sequences (with [`vte`]) into a [`Screen`]: the
//! normal screen and its history, and the alternate screen. Rows that
//! scroll into the history are packed small. Text is kept with its colours,
//! attributes, underline style and colour, and OSC 8 hyperlink; the normal
//! screen can be rewrapped on a width change ([`Screen::set_reflow`]).
//! What the screen does not keep is handed to [`Callbacks`].
//!
//! ```
//! use core::num::NonZeroU16;
//!
//! let rows = NonZeroU16::new(24).unwrap();
//! let cols = NonZeroU16::new(80).unwrap();
//! let mut parser = vt1k::Parser::new(rows, cols, 1000);
//! parser.process(b"\x1b[1;31mhello\x1b[m world");
//! let screen = parser.screen();
//! assert_eq!(screen.contents(), "hello world");
//! assert!(screen.cell(0, 0).unwrap().bold());
//! ```

#![no_std]

extern crate alloc;

mod attrs;
mod callbacks;
pub mod capture;
mod cell;
mod color;
mod grid;
mod packed;
mod parser;
mod row;
mod screen;

pub use callbacks::Callbacks;
pub use cell::Cell;
pub use color::{Color, UnderlineStyle};
pub use grid::Reflow;
pub use parser::Parser;
pub use screen::{CursorStyle, Hyperlink, MouseProtocolEncoding, MouseProtocolMode, Screen};
