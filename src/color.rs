//! Colours, as a cell or the pen keeps them.

/// A cell's foreground, background or underline colour.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Color {
    /// The terminal's own colour.
    #[default]
    Default,
    /// An entry of the 256-colour palette.
    Idx(u8),
    /// A 24-bit colour.
    Rgb(u8, u8, u8),
}

/// How text is underlined, as SGR 4 (4:1 to 4:5) and 21 set it.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum UnderlineStyle {
    /// Not underlined.
    #[default]
    None,
    /// A single line (SGR 4, 4:1).
    Single,
    /// A double line (SGR 21, 4:2).
    Double,
    /// A curly line (SGR 4:3).
    Curly,
    /// A dotted line (SGR 4:4).
    Dotted,
    /// A dashed line (SGR 4:5).
    Dashed,
}

impl UnderlineStyle {
    /// The style SGR 4:`n` sets; `None` for a number it does not name.
    #[must_use]
    pub const fn from_sgr(n: u16) -> Option<Self> {
        Some(match n {
            0 => Self::None,
            1 => Self::Single,
            2 => Self::Double,
            3 => Self::Curly,
            4 => Self::Dotted,
            5 => Self::Dashed,
            _ => return None,
        })
    }

    /// Its number after SGR 4: (0 for none).
    #[must_use]
    pub const fn sgr(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Single => 1,
            Self::Double => 2,
            Self::Curly => 3,
            Self::Dotted => 4,
            Self::Dashed => 5,
        }
    }
}
