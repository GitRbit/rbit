//! CLDR plural categories for whole numbers, so a translation picks the right noun form.
//!
//! Counts in the catalog are unsigned integers (signed values pass their magnitude), so
//! only the integer half of each CLDR rule applies: there are no fractions or compact
//! exponents. The rules follow CLDR 46 (`plurals.xml`):
//!
//! | Locale | Categories (integers) |
//! |---|---|
//! | `en`, `de` | one (1), other |
//! | `es` | one (1), many (multiples of a million), other |
//! | `fr`, `pt-BR` | one (0 and 1), many (multiples of a million), other |
//! | `ru` | one (1, 21, 31 …; not 11), few (2–4, 22–24 …; not 12–14), many (the rest) |
//! | `zh-Hans`, `ja`, `ko` | other |
//!
//! The diagnostics (`en-XA`, `ar-XB`) keep English grammar.
use crate::locale::Locale;

/// A CLDR plural category. A translation matches on it and, as in CLDR, uses `Other` (or a
/// `_` arm) for every category its language does not distinguish.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Plural {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

const MILLION: u64 = 1_000_000;

impl Plural {
    /// The category of the whole number `count` in `locale`.
    pub const fn of(locale: Locale, count: u64) -> Self {
        match locale {
            Locale::English | Locale::German | Locale::PseudoExpanded | Locale::PseudoRtl => {
                if count == 1 {
                    Self::One
                } else {
                    Self::Other
                }
            }
            Locale::Spanish => match count {
                1 => Self::One,
                _ if count != 0 && count.is_multiple_of(MILLION) => Self::Many,
                _ => Self::Other,
            },
            Locale::French | Locale::PortugueseBrazil => match count {
                0 | 1 => Self::One,
                _ if count.is_multiple_of(MILLION) => Self::Many,
                _ => Self::Other,
            },
            Locale::Russian => match (count % 10, count % 100) {
                (1, tens) if tens != 11 => Self::One,
                (2..=4, tens) if !matches!(tens, 12..=14) => Self::Few,
                _ => Self::Many,
            },
            Locale::ChineseSimplified | Locale::Japanese | Locale::Korean => Self::Other,
        }
    }

    /// The categories a locale distinguishes among whole numbers, for checks and tooling.
    pub const fn categories(locale: Locale) -> &'static [Self] {
        match locale {
            Locale::English | Locale::German | Locale::PseudoExpanded | Locale::PseudoRtl => &[Self::One, Self::Other],
            Locale::Spanish | Locale::French | Locale::PortugueseBrazil => &[Self::One, Self::Many, Self::Other],
            Locale::Russian => &[Self::One, Self::Few, Self::Many],
            Locale::ChineseSimplified | Locale::Japanese | Locale::Korean => &[Self::Other],
        }
    }
}
