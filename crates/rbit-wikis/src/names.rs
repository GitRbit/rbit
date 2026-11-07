//! NFC, case-sensitive logical names. Neither type is an OS path or safe HTML.
mod path;
mod title;

use std::fmt;

pub use path::WikiPath;
pub use title::WikiTitle;

/// Stable presentation-independent reasons; UI adapters localize by variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WikiNameError {
    Empty,
    RawLimit,
    TooLong,
    TooDeep,
    InvalidCharacter,
    InvalidBoundary,
    ReservedPath,
}

impl fmt::Display for WikiNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "wiki name must not be empty",
            Self::RawLimit => "wiki name input exceeds the raw byte limit",
            Self::TooLong => "wiki name exceeds its normalized length limit",
            Self::TooDeep => "wiki path has too many components",
            Self::InvalidCharacter => "wiki name contains an unsupported character",
            Self::InvalidBoundary => "wiki path has an invalid component boundary",
            Self::ReservedPath => "wiki path contains a reserved component",
        })
    }
}
impl std::error::Error for WikiNameError {}

fn unsupported_display_control(character: char) -> bool {
    character.is_control()
        || matches!(character, '\u{061c}' | '\u{200b}' | '\u{200e}' | '\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
}
