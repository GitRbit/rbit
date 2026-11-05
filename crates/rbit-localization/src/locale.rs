//! Allowlisted locale identities; tags are static and never copied from a header.

/// A catalog supported by this version. Pseudo locales are opt-in test catalogs.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Locale {
    /// The only translated language currently supplied.
    #[default]
    English,
    /// Expanded, accented English for truncation and untranslated-label testing.
    PseudoExpanded,
    /// English in an RTL presentation, with isolated interpolated values.
    PseudoRtl,
}

impl Locale {
    /// The canonical BCP 47-style presentation tag.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::PseudoExpanded => "en-XA",
            Self::PseudoRtl => "ar-XB",
        }
    }

    /// Text flow for the document's `dir` attribute.
    pub const fn direction(self) -> Direction {
        match self {
            Self::PseudoRtl => Direction::RightToLeft,
            Self::English | Self::PseudoExpanded => Direction::LeftToRight,
        }
    }
}

/// A closed set of HTML direction values, independent of untrusted preferences.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    /// Left-to-right text flow.
    LeftToRight,
    /// Right-to-left text flow.
    RightToLeft,
}

impl Direction {
    /// Safe static attribute value; interpolate through the template normally.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LeftToRight => "ltr",
            Self::RightToLeft => "rtl",
        }
    }
}
