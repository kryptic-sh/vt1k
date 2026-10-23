//! The pen: what text is written with.

use crate::{Color, UnderlineStyle};

const BOLD: u8 = 0b0000_0001;
const DIM: u8 = 0b0000_0010;
const INTENSITY: u8 = BOLD | DIM;
const ITALIC: u8 = 0b0000_0100;
const INVERSE: u8 = 0b0000_1000;
/// The underline style's number ([`UnderlineStyle::sgr`]), in the top bits.
const UNDERLINE: u8 = 0b1110_0000;
const UNDERLINE_SHIFT: u32 = 5;

/// Colours and text attributes, as SGR sets them.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Attrs {
    pub fg: Color,
    pub bg: Color,
    /// The underline's colour (SGR 58), `Default` for the text's.
    pub ul: Color,
    pub mode: u8,
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
        UnderlineStyle::from_sgr(u16::from((self.mode & UNDERLINE) >> UNDERLINE_SHIFT))
            .unwrap_or_default()
    }

    pub fn set_underline_style(&mut self, style: UnderlineStyle) {
        self.mode = (self.mode & !UNDERLINE) | (style.sgr() << UNDERLINE_SHIFT);
    }

    fn set(&mut self, bit: u8, on: bool) {
        if on {
            self.mode |= bit;
        } else {
            self.mode &= !bit;
        }
    }
}
