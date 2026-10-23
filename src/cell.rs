//! A cell of the screen.

use unicode_width::{UnicodeWidthChar as _, UnicodeWidthStr as _};

use crate::Color;
use crate::attrs::Attrs;

/// The most bytes of text a cell keeps: a base character and what combines
/// with it, as vt100 keeps (later codepoints past it are dropped).
const CONTENT_BYTES: usize = 20;
const IS_WIDE: u8 = 0b1000_0000;
const IS_WIDE_CONTINUATION: u8 = 0b0100_0000;
const LEN_BITS: u8 = 0b0001_1111;

/// One cell: its text, how it is drawn and the hyperlink it was written
/// under.
#[derive(Clone, Debug, Eq)]
pub struct Cell {
    contents: [u8; CONTENT_BYTES],
    /// The content's byte count, and the wide flags.
    len: u8,
    attrs: Attrs,
    /// The OSC 8 hyperlink, as an index into the screen's links plus one;
    /// 0 for none.
    link: u16,
}

// The screen's own rows hold full cells; the history packs them.
const _: () = assert!(core::mem::size_of::<Cell>() == 36);

impl PartialEq for Cell {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len
            && self.attrs == other.attrs
            && self.link == other.link
            && self.contents_bytes() == other.contents_bytes()
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::new()
    }
}

impl Cell {
    pub(crate) const fn new() -> Self {
        Self {
            contents: [0; CONTENT_BYTES],
            len: 0,
            attrs: Attrs::new(),
            link: 0,
        }
    }

    /// Nothing was written to it.
    pub(crate) fn is_default(&self) -> bool {
        self.len == 0 && self.link == 0 && self.attrs == Attrs::new()
    }

    /// Text, its contents.
    #[must_use]
    pub fn contents(&self) -> &str {
        // Only whole characters are ever written into `contents`.
        core::str::from_utf8(self.contents_bytes()).unwrap_or_default()
    }

    /// It holds text.
    #[must_use]
    pub fn has_contents(&self) -> bool {
        self.len & LEN_BITS != 0
    }

    /// Its character is two columns wide.
    #[must_use]
    pub const fn is_wide(&self) -> bool {
        self.len & IS_WIDE != 0
    }

    /// It is the second column of the wide character before it.
    #[must_use]
    pub const fn is_wide_continuation(&self) -> bool {
        self.len & IS_WIDE_CONTINUATION != 0
    }

    #[must_use]
    pub const fn fgcolor(&self) -> Color {
        self.attrs.fg
    }

    #[must_use]
    pub const fn bgcolor(&self) -> Color {
        self.attrs.bg
    }

    #[must_use]
    pub const fn bold(&self) -> bool {
        self.attrs.bold()
    }

    #[must_use]
    pub const fn dim(&self) -> bool {
        self.attrs.dim()
    }

    #[must_use]
    pub const fn italic(&self) -> bool {
        self.attrs.italic()
    }

    /// It is underlined, in any style.
    #[must_use]
    pub const fn underline(&self) -> bool {
        self.attrs.underline()
    }

    #[must_use]
    pub fn underline_style(&self) -> crate::UnderlineStyle {
        self.attrs.underline_style()
    }

    /// Its underline's colour; `Default` for the text's own.
    #[must_use]
    pub const fn underline_color(&self) -> Color {
        self.attrs.ul
    }

    #[must_use]
    pub const fn inverse(&self) -> bool {
        self.attrs.inverse()
    }

    /// The OSC 8 hyperlink it was written under, if any, as an id for
    /// [`Screen::hyperlink`](crate::Screen::hyperlink).
    #[must_use]
    pub const fn hyperlink_id(&self) -> Option<core::num::NonZeroU16> {
        core::num::NonZeroU16::new(self.link)
    }

    pub(crate) const fn attrs(&self) -> &Attrs {
        &self.attrs
    }

    pub(crate) const fn link(&self) -> u16 {
        self.link
    }

    pub(crate) const fn set_link(&mut self, link: u16) {
        self.link = link;
    }

    pub(crate) fn contents_bytes(&self) -> &[u8] {
        &self.contents[..usize::from(self.len & LEN_BITS)]
    }

    pub(crate) const fn raw_len(&self) -> u8 {
        self.len
    }

    /// The content's byte count a length byte holds.
    pub(crate) const fn content_len(raw_len: u8) -> usize {
        (raw_len & LEN_BITS) as usize
    }

    /// A cell from what [`Self::raw_len`], [`Self::contents_bytes`],
    /// [`Self::attrs`] and [`Self::link`] gave.
    pub(crate) fn from_parts(contents: &[u8], raw_len: u8, attrs: Attrs, link: u16) -> Self {
        let mut cell = Self::new();
        let n = contents.len().min(CONTENT_BYTES);
        cell.contents[..n].copy_from_slice(&contents[..n]);
        cell.len = raw_len;
        cell.attrs = attrs;
        cell.link = link;
        cell
    }

    /// Write `c`, alone, with `attrs`, and no link.
    pub(crate) fn set(&mut self, c: char, attrs: Attrs) {
        self.len = 0;
        self.push_char(0, c);
        self.set_wide(c.width().unwrap_or(1) > 1);
        self.attrs = attrs;
        self.link = 0;
    }

    /// Write `s` as one cell's text.
    pub(crate) fn set_contents(&mut self, s: &str, attrs: Attrs) {
        let mut chars = s.chars();
        let Some(c) = chars.next() else {
            return;
        };
        self.set(c, attrs);
        for c in chars {
            self.append(c);
        }
    }

    /// Add `c`, a character that combines with what is there (a space when
    /// there is nothing), while there is room.
    pub(crate) fn append(&mut self, c: char) {
        let len = usize::from(self.len & LEN_BITS);
        if len >= CONTENT_BYTES - 4 {
            return;
        }
        if len == 0 {
            self.contents[0] = b' ';
            self.len += 1;
        }
        self.push_char(usize::from(self.len & LEN_BITS), c);
    }

    fn push_char(&mut self, at: usize, c: char) {
        c.encode_utf8(&mut self.contents[at..]);
        // A char's UTF-8 is at most four bytes.
        self.len += c.len_utf8() as u8;
        // Emoji presentation can make a narrow base character wide.
        if c == '\u{FE0F}' {
            self.set_wide(self.contents().width() > 1);
        }
    }

    /// Empty it, drawn with `attrs` and no link.
    pub(crate) const fn clear(&mut self, attrs: Attrs) {
        self.len = 0;
        self.attrs = attrs;
        self.link = 0;
    }

    const fn set_wide(&mut self, wide: bool) {
        if wide {
            self.len |= IS_WIDE;
        } else {
            self.len &= !IS_WIDE;
        }
    }

    pub(crate) const fn set_wide_continuation(&mut self, on: bool) {
        if on {
            self.len |= IS_WIDE_CONTINUATION;
        } else {
            self.len &= !IS_WIDE_CONTINUATION;
        }
    }
}
