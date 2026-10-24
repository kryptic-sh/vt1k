//! The pen: what text is written with.

use crate::{Color, UnderlineStyle};

const BOLD: u16 = 0b0000_0001;
const DIM: u16 = 0b0000_0010;
const INTENSITY: u16 = BOLD | DIM;
const ITALIC: u16 = 0b0000_0100;
const INVERSE: u16 = 0b0000_1000;
/// The underline style's number ([`UnderlineStyle::sgr`]).
const UNDERLINE: u16 = 0b1110_0000;
const UNDERLINE_SHIFT: u32 = 5;
const BLINK: u16 = 1 << 8;
const HIDDEN: u16 = 1 << 9;
const STRIKETHROUGH: u16 = 1 << 10;
const OVERLINE: u16 = 1 << 11;

/// Colours and text attributes, as SGR sets them.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Attrs {
    pub fg: Color,
    pub bg: Color,
    /// The underline's colour (SGR 58), `Default` for the text's.
    pub ul: Color,
    pub mode: u16,
}

impl Attrs {
    pub const fn new() -> Self {
        Self {
            fg: Color::Default,
            bg: Color::Default,
            ul: Color::Default,
            mode: 0,
        }
    }

    pub const fn bold(&self) -> bool {
        self.mode & BOLD != 0
    }

    pub const fn dim(&self) -> bool {
        self.mode & DIM != 0
    }

    /// Bold and dim exclude each other, as SGR 1 and 2 do in vt100.
    pub fn set_bold(&mut self) {
        self.mode = (self.mode & !INTENSITY) | BOLD;
    }

    pub fn set_dim(&mut self) {
        self.mode = (self.mode & !INTENSITY) | DIM;
    }

    pub fn set_normal_intensity(&mut self) {
        self.mode &= !INTENSITY;
    }

    pub const fn italic(&self) -> bool {
        self.mode & ITALIC != 0
    }

    pub fn set_italic(&mut self, on: bool) {
        self.set(ITALIC, on);
    }

    pub const fn inverse(&self) -> bool {
        self.mode & INVERSE != 0
    }

    pub fn set_inverse(&mut self, on: bool) {
        self.set(INVERSE, on);
    }

    pub const fn underline(&self) -> bool {
        self.mode & UNDERLINE != 0
    }

    pub fn underline_style(&self) -> UnderlineStyle {
        UnderlineStyle::from_sgr((self.mode & UNDERLINE) >> UNDERLINE_SHIFT).unwrap_or_default()
    }

    pub fn set_underline_style(&mut self, style: UnderlineStyle) {
        self.mode = (self.mode & !UNDERLINE) | (u16::from(style.sgr()) << UNDERLINE_SHIFT);
    }

    pub const fn blink(&self) -> bool {
        self.mode & BLINK != 0
    }

    pub fn set_blink(&mut self, on: bool) {
        self.set(BLINK, on);
    }

    pub const fn hidden(&self) -> bool {
        self.mode & HIDDEN != 0
    }

    pub fn set_hidden(&mut self, on: bool) {
        self.set(HIDDEN, on);
    }

    pub const fn strikethrough(&self) -> bool {
        self.mode & STRIKETHROUGH != 0
    }

    pub fn set_strikethrough(&mut self, on: bool) {
        self.set(STRIKETHROUGH, on);
    }

    pub const fn overline(&self) -> bool {
        self.mode & OVERLINE != 0
    }

    pub fn set_overline(&mut self, on: bool) {
        self.set(OVERLINE, on);
    }

    fn set(&mut self, bit: u16, on: bool) {
        if on {
            self.mode |= bit;
        } else {
            self.mode &= !bit;
        }
    }
}
