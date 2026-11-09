//! Numbers as people read them in each language: digit grouping and the decimal mark.
//!
//! Only presentation text goes through here (counts, sizes, durations in sentences). Machine
//! values (ids, URLs, attributes, `datetime`, API fields) keep their plain ASCII digits.
//! Separators follow CLDR 46 for the Latin digits every locale uses:
//!
//! | Locale | 1234567.5 | Grouping |
//! |---|---|---|
//! | `en`, `ja`, `zh-Hans`, `ko` (and the diagnostics) | `1,234,567.5` | from 4 digits |
//! | `de`, `pt-BR` | `1.234.567,5` | from 4 digits |
//! | `es` | `1.234.567,5`, but `1234` | from 5 digits (CLDR minimum grouping 2) |
//! | `fr` | `1 234 567,5` with U+202F (narrow no-break space) | from 4 digits |
//! | `ru` | `1 234 567,5` with U+00A0 (no-break space) | from 4 digits |
use std::fmt::Write;

use crate::locale::Locale;

/// The thousands separator of `locale`.
const fn group_separator(locale: Locale) -> char {
    match locale {
        Locale::German | Locale::Spanish | Locale::PortugueseBrazil => '.',
        Locale::French => '\u{202f}',
        Locale::Russian => '\u{a0}',
        Locale::English | Locale::ChineseSimplified | Locale::Japanese | Locale::Korean | Locale::PseudoExpanded | Locale::PseudoRtl => ',',
    }
}

/// The decimal mark of `locale`.
pub const fn decimal_separator(locale: Locale) -> char {
    match locale {
        Locale::German | Locale::Spanish | Locale::PortugueseBrazil | Locale::French | Locale::Russian => ',',
        Locale::English | Locale::ChineseSimplified | Locale::Japanese | Locale::Korean | Locale::PseudoExpanded | Locale::PseudoRtl => '.',
    }
}

/// The fewest digits a number needs before it is grouped at all.
const fn grouping_threshold(locale: Locale) -> usize {
    if matches!(locale, Locale::Spanish) { 5 } else { 4 }
}

/// Appends `value` to `output` with `locale`'s digit grouping.
pub fn write_integer(output: &mut String, locale: Locale, value: u64) {
    let start = output.len();
    let _ = write!(output, "{value}");
    let length = output.len() - start;
    if length < grouping_threshold(locale) {
        return;
    }
    let separator = group_separator(locale);
    let digits = output.split_off(start);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (length - index).is_multiple_of(3) {
            output.push(separator);
        }
        output.push(digit);
    }
}

/// A whole number for display: `integer(Locale::German, 1234)` is `1.234`.
pub fn integer(locale: Locale, value: u64) -> String {
    let mut output = String::new();
    write_integer(&mut output, locale, value);
    output
}

/// A signed whole number for display, with an ASCII minus sign when negative.
pub fn signed(locale: Locale, value: i64) -> String {
    let mut output = String::new();
    if value < 0 {
        output.push('-');
    }
    write_integer(&mut output, locale, value.unsigned_abs());
    output
}

/// A number with one decimal, given in tenths: `tenths(Locale::French, 15)` is `1,5`.
pub fn tenths(locale: Locale, tenths: u64) -> String {
    let mut output = integer(locale, tenths / 10);
    let _ = write!(output, "{}{}", decimal_separator(locale), tenths % 10);
    output
}
