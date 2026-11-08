//! Allowlisted locale identities; tags are static and never copied from a header.

/// A catalog supported by this version. Pseudo locales are opt-in test catalogs.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Locale {
    /// Default language and fallback for feature regions without a translated catalog.
    #[default]
    English,
    /// Spanish catalog; untranslated feature regions retain explicit English fallback.
    Spanish,
    /// Simplified Chinese (`zh-Hans`), as used in mainland China and Singapore.
    ChineseSimplified,
    /// Brazilian Portuguese (`pt-BR`), also offered to other Portuguese readers.
    PortugueseBrazil,
    /// Japanese.
    Japanese,
    /// German.
    German,
    /// Russian.
    Russian,
    /// French.
    French,
    /// Korean.
    Korean,
    /// Expanded, accented English for truncation and untranslated-label testing.
    PseudoExpanded,
    /// English in an RTL presentation, with isolated interpolated values.
    PseudoRtl,
}

impl Locale {
    /// The languages people can choose, in the order the language picker lists them.
    /// Diagnostic catalogs are not languages and are never offered.
    pub const LANGUAGES: [Self; 9] = [
        Self::English,
        Self::German,
        Self::Spanish,
        Self::French,
        Self::PortugueseBrazil,
        Self::Russian,
        Self::Japanese,
        Self::ChineseSimplified,
        Self::Korean,
    ];

    /// Versioned presentation identity for validators and browser history namespaces.
    pub const fn catalog_key(self) -> &'static str {
        match self {
            Self::English => "1.en",
            Self::Spanish => "1.es",
            Self::ChineseSimplified => "1.zh-Hans",
            Self::PortugueseBrazil => "1.pt-BR",
            Self::Japanese => "1.ja",
            Self::German => "1.de",
            Self::Russian => "1.ru",
            Self::French => "1.fr",
            Self::Korean => "1.ko",
            Self::PseudoExpanded => "1.en-XA",
            Self::PseudoRtl => "1.ar-XB",
        }
    }

    /// The canonical BCP 47-style presentation tag.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Spanish => "es",
            Self::ChineseSimplified => "zh-Hans",
            Self::PortugueseBrazil => "pt-BR",
            Self::Japanese => "ja",
            Self::German => "de",
            Self::Russian => "ru",
            Self::French => "fr",
            Self::Korean => "ko",
            Self::PseudoExpanded => "en-XA",
            Self::PseudoRtl => "ar-XB",
        }
    }

    /// The language's own name for itself (its autonym), as the language picker shows it
    /// whatever the page's language. Diagnostic catalogs are named by their tag.
    pub const fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Spanish => "Español",
            Self::ChineseSimplified => "简体中文",
            Self::PortugueseBrazil => "Português (Brasil)",
            Self::Japanese => "日本語",
            Self::German => "Deutsch",
            Self::Russian => "Русский",
            Self::French => "Français",
            Self::Korean => "한국어",
            Self::PseudoExpanded => "en-XA",
            Self::PseudoRtl => "ar-XB",
        }
    }

    /// Whether the original exhaustive families (`Message`, `ChromeMessage`, the Actions,
    /// theme and language-settings messages) have this locale's own text. They are complete
    /// in English and Spanish, and the diagnostics derive from English; in every other
    /// locale they show their English text, which pages must mark `lang="en"`.
    pub const fn has_original_catalog(self) -> bool {
        matches!(self, Self::English | Self::Spanish | Self::PseudoExpanded | Self::PseudoRtl)
    }

    /// Text flow for the document's `dir` attribute.
    pub const fn direction(self) -> Direction {
        match self {
            Self::PseudoRtl => Direction::RightToLeft,
            Self::English
            | Self::Spanish
            | Self::ChineseSimplified
            | Self::PortugueseBrazil
            | Self::Japanese
            | Self::German
            | Self::Russian
            | Self::French
            | Self::Korean
            | Self::PseudoExpanded => Direction::LeftToRight,
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
